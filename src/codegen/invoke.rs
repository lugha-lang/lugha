//! Calls carrying a landing pad (`codegen::unwind`) become `invoke`s,
//! once the passes rewriting calls have run (they carry pads over,
//! `carry_pad`) and before slots are shared, which follows unwind edges.

use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::basic_block::BasicBlock;
use inkwell::context::Context;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{
    BasicValue, BasicValueEnum, CallSiteValue, InstructionOpcode, InstructionValue, ValueKind,
};

use super::lower::{Lowerer, POSITIONED};
use super::unwind::PAD_KIND;

impl<'ctx> Lowerer<'ctx> {
    /// Turns every call carrying a pad into an `invoke` of it, and gives
    /// every function unwind tables. Run after the passes that rewrite
    /// calls, before slots are shared, which follows the unwind edges.
    pub(super) fn invokes(&self) {
        let kind = self.context.get_kind_id(PAD_KIND);
        let uwtable = self
            .context
            .create_enum_attribute(Attribute::get_named_enum_kind_id("uwtable"), 2);
        for function in self.module.get_functions() {
            if function.count_basic_blocks() == 0 {
                continue;
            }
            function.add_attribute(AttributeLoc::Function, uwtable);
            let mut calls = Vec::new();
            for block in function.get_basic_block_iter() {
                // The passes rebuilding definitions don't carry it over.
                if block
                    .get_first_instruction()
                    .is_some_and(|i| i.get_opcode() == InstructionOpcode::LandingPad)
                {
                    function.set_personality_function(self.personality());
                }
                for i in block.get_instructions() {
                    if i.get_opcode() == InstructionOpcode::Call
                        && let Some(pad) = pad_of(i, kind)
                    {
                        calls.push((i, pad));
                    }
                }
            }
            for (call, pad) in calls {
                invoke(self.context, call, self.pads.blocks[pad]);
            }
        }
    }
}

/// The instruction a call site is.
pub(super) fn instruction(site: CallSiteValue<'_>) -> InstructionValue<'_> {
    match site.try_as_basic_value() {
        ValueKind::Basic(v) => v.as_instruction_value().expect("a call is an instruction"),
        ValueKind::Instruction(i) => i,
    }
}

/// Gives `to`, a call rebuilt from `from`, `from`'s landing pad if it has
/// one: the passes rewriting calls keep them.
pub(super) fn carry_pad<'ctx>(
    context: &'ctx Context,
    from: InstructionValue<'ctx>,
    to: CallSiteValue<'ctx>,
) {
    let kind = context.get_kind_id(PAD_KIND);
    let Some(node) = from.get_metadata(kind) else {
        return;
    };
    let to = match to.try_as_basic_value() {
        ValueKind::Basic(v) => v.as_instruction_value().expect("a call is an instruction"),
        ValueKind::Instruction(i) => i,
    };
    to.set_metadata(node, kind).expect("a call takes metadata");
}

/// The pad index `call` carries, if any.
fn pad_of(call: InstructionValue<'_>, kind: u32) -> Option<usize> {
    let node = call.get_metadata(kind)?;
    let index = node.get_node_values()?.first()?.into_int_value();
    usize::try_from(index.get_zero_extended_constant()?).ok()
}

/// Replaces `call` with an `invoke` unwinding to `pad`, its block split
/// after it: what followed moves to a new block, which successors' phis
/// now come from.
fn invoke<'ctx>(context: &'ctx Context, call: InstructionValue<'ctx>, pad: BasicBlock<'ctx>) {
    let block = call.get_parent().expect("a call is in a block");
    let rest = context.insert_basic_block_after(block, "unwound.not");
    let builder = context.create_builder();
    builder.position_at_end(rest);
    let mut next = call.get_next_instruction();
    while let Some(i) = next {
        next = i.get_next_instruction();
        i.remove_from_basic_block();
        builder.insert_instruction(&i, None);
    }
    for successor in super::split_leaves::successors(rest) {
        repoint_phis(context, successor, block, rest);
    }
    let site = CallSiteValue::try_from(call).expect("a call");
    let count = call.get_num_operands() - 1;
    let args: Vec<BasicValueEnum<'ctx>> = (0..count)
        .map(|i| {
            call.get_operand(i)
                .and_then(|o| o.value())
                .expect("a call's arguments are values")
        })
        .collect();
    builder.position_at_end(block);
    let invoked = match site.get_called_fn_value() {
        Some(function) => builder
            .build_direct_invoke(function, &args, rest, pad, "")
            .expect(POSITIONED),
        None => {
            let callee = call
                .get_operand(count)
                .and_then(|o| o.value())
                .expect("a call's callee is a value")
                .into_pointer_value();
            let params: Vec<BasicMetadataTypeEnum> =
                args.iter().map(|a| a.get_type().into()).collect();
            let ty = match BasicTypeEnum::try_from(call.get_type()) {
                Ok(ret) => ret.fn_type(&params, false),
                Err(_) => context.void_type().fn_type(&params, false),
            };
            builder
                .build_indirect_invoke(ty, callee, &args, rest, pad, "")
                .expect(POSITIONED)
        }
    };
    invoked.set_call_convention(site.get_call_convention());
    for loc in std::iter::once(AttributeLoc::Function)
        .chain(std::iter::once(AttributeLoc::Return))
        .chain((0..count).map(AttributeLoc::Param))
    {
        for attribute in site.attributes(loc) {
            invoked.add_attribute(loc, attribute);
        }
    }
    let invoked = match invoked.try_as_basic_value() {
        ValueKind::Basic(v) => v
            .as_instruction_value()
            .expect("an invoke is an instruction"),
        ValueKind::Instruction(i) => i,
    };
    call.replace_all_uses_with(&invoked);
    call.erase_from_basic_block();
}

/// `block`'s phis taking a value from `from` take it from `to` instead.
fn repoint_phis<'ctx>(
    context: &'ctx Context,
    block: BasicBlock<'ctx>,
    from: BasicBlock<'ctx>,
    to: BasicBlock<'ctx>,
) {
    let builder = context.create_builder();
    let phis: Vec<_> = block
        .get_instructions()
        .take_while(|i| i.get_opcode() == InstructionOpcode::Phi)
        .collect();
    for old in phis {
        let phi = inkwell::values::PhiValue::try_from(old).expect("a phi");
        let incoming: Vec<_> = (0..phi.count_incoming())
            .filter_map(|n| phi.get_incoming(n))
            .map(|(v, b)| (v, if b == from { to } else { b }))
            .collect();
        builder.position_before(&old);
        let new = builder
            .build_phi(phi.as_basic_value().get_type(), "")
            .expect(POSITIONED);
        for (v, b) in &incoming {
            new.add_incoming(&[(v, *b)]);
        }
        old.replace_all_uses_with(&new.as_instruction());
        old.erase_from_basic_block();
    }
}
