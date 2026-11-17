//! Unwinding: what a panic runs in each frame on its way out. A panic is a
//! forced unwind through the system unwinder (the runtime's `unwind.c`),
//! which runs each frame's landing pad for the call in progress. A pad
//! does what a `return` there would: the pending `defer`s, innermost first,
//! then the releases of every counted value the frame holds; then the
//! unwinding goes on (`resume`).
//!
//! While a function is lowered, each statement's end gives the calls made
//! directly in it (not in a statement inside it, which got its own) the
//! statement's pad, as metadata; a function's end gives any call left over
//! the function's own. Only calls with something to clean up get one, and
//! never a call that can't unwind: an LLVM intrinsic, a count operation,
//! or C a library compiled, which a panic never unwinds through. Once the
//! passes that rewrite calls have run, `invokes` turns every call carrying
//! a pad into an `invoke` of it, splitting its block after it.

use std::collections::{HashMap, HashSet};

use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::basic_block::BasicBlock;
use inkwell::context::Context;
use inkwell::module::Linkage;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{
    BasicValue, BasicValueEnum, CallSiteValue, FunctionValue, InstructionOpcode, InstructionValue,
    PhiValue, ValueKind,
};

use super::lower::{Lowerer, POSITIONED};

/// The metadata a call carries naming its landing pad, by index.
const PAD_KIND: &str = "lugha.unwind";

/// The personality every Lugha function with a landing pad uses: libgcc's,
/// for frames that only clean up.
const PERSONALITY: &str = "__gcc_personality_v0";

/// Where a statement starts, naming its cleanup.
type Start<'ctx> = (BasicBlock<'ctx>, Option<InstructionValue<'ctx>>);

/// The landing pads made so far, and every block lowered inside one.
///
/// A pad is two parts: an entry, whose `landingpad` a call's `invoke`
/// unwinds to, and the statement's cleanup it jumps to, which `resume`s
/// with the entry's value (a phi). A statement's calls share one entry; a
/// call whose fresh arguments it let go of itself (`codegen::rc_temps`)
/// gets an entry of its own, releasing them first.
#[derive(Default)]
pub(super) struct Pads<'ctx> {
    pub(super) blocks: Vec<BasicBlock<'ctx>>,
    inside: HashSet<BasicBlock<'ctx>>,
    /// Cleanups made ahead of their statement's end, by its start.
    cleanups: HashMap<Start<'ctx>, (BasicBlock<'ctx>, PhiValue<'ctx>)>,
    /// True while a pad's cleanup is lowered: its own calls get no pad.
    lowering: bool,
}

impl<'ctx> Lowerer<'ctx> {
    /// At the end of the statement that started at `start`: its calls
    /// still without a pad get one running what a `return` here would.
    pub(super) fn unwind_statement(&mut self, start: Start<'ctx>) {
        if self.pads.lowering {
            return;
        }
        let calls = self.unpadded_calls(start);
        let early = self.pads.cleanups.remove(&start);
        let pending = self.defers_pending(0) || self.rc_holds_anything();
        if early.is_none() && (calls.is_empty() || !pending) {
            return;
        }
        let (cleanup, landed) = early.unwrap_or_else(|| self.cleanup_block());
        self.fill_cleanup(cleanup, landed);
        if !calls.is_empty() {
            let pad = self.pad_entry((cleanup, landed), |_| {});
            for call in calls {
                self.give_pad(call, pad);
            }
        }
    }

    /// At the end of a function: the calls left without a pad (outside any
    /// statement) get the function's own, if it holds anything.
    pub(super) fn unwind_function(&mut self) {
        let Some(function) = self.function else {
            return;
        };
        let Some(entry) = function.get_first_basic_block() else {
            return;
        };
        self.unwind_statement((entry, None));
    }

    /// `call`, which let go of fresh arguments itself once it returns, gets
    /// a pad of its own: `release` lets go of them, then the statement's
    /// cleanup runs.
    pub(super) fn unwind_claimed(
        &mut self,
        call: InstructionValue<'ctx>,
        release: impl FnOnce(&mut Self),
    ) {
        if self.pads.lowering {
            return;
        }
        let start = match self.rc_slots.statements.last() {
            Some(statement) => statement.start,
            None => {
                let function = self.function.expect("calls are made inside a function");
                (
                    function.get_first_basic_block().expect("an entry block"),
                    None,
                )
            }
        };
        let cleanup = match self.pads.cleanups.get(&start) {
            Some(made) => *made,
            None => {
                let made = self.cleanup_block();
                self.pads.cleanups.insert(start, made);
                made
            }
        };
        let pad = self.pad_entry(cleanup, release);
        self.give_pad(call, pad);
    }

    /// `call`, the C entry's call of `main`: if it unwinds, `cleanup` runs
    /// (letting go of `main`'s arguments), then the unwinding goes on to
    /// its end, where the runtime ends the program.
    pub(super) fn unwind_entry(
        &mut self,
        call: InstructionValue<'ctx>,
        cleanup: impl FnOnce(&mut Self),
    ) {
        let (block, landed) = self.cleanup_block();
        self.in_pad(block, |this| {
            cleanup(this);
            this.builder
                .build_resume(landed.as_basic_value())
                .expect(POSITIONED);
        });
        let pad = self.pad_entry((block, landed), |_| {});
        self.give_pad(call, pad);
    }

    /// The calls from `start` on (that instruction's block after it, then
    /// every later block of the function) that can unwind and have no pad
    /// yet, outside the pads' own blocks.
    fn unpadded_calls(&self, (block, after): Start<'ctx>) -> Vec<InstructionValue<'ctx>> {
        let kind = self.context.get_kind_id(PAD_KIND);
        let mut calls = Vec::new();
        let mut next_block = Some(block);
        let mut first = true;
        while let Some(b) = next_block {
            next_block = b.get_next_basic_block();
            if self.pads.inside.contains(&b) {
                continue;
            }
            let mut next = match (first, after) {
                (true, Some(i)) => i.get_next_instruction(),
                _ => b.get_first_instruction(),
            };
            first = false;
            while let Some(i) = next {
                next = i.get_next_instruction();
                if i.get_opcode() == InstructionOpcode::Call
                    && i.get_metadata(kind).is_none()
                    && unwinds(i)
                {
                    calls.push(i);
                }
            }
        }
        calls
    }

    /// Runs `body` with the builder at the end of `block`, then puts it back
    /// where it was; every block `body` makes belongs to the pads.
    fn in_pad(&mut self, block: BasicBlock<'ctx>, body: impl FnOnce(&mut Self)) {
        let function = self.function.expect("pads are made inside a function");
        let back = self.builder.get_insert_block();
        let before = function.get_last_basic_block();
        self.builder.position_at_end(block);
        let was = std::mem::replace(&mut self.pads.lowering, true);
        body(self);
        self.pads.lowering = was;
        let mut made = before.and_then(|b| b.get_next_basic_block());
        while let Some(b) = made {
            self.pads.inside.insert(b);
            made = b.get_next_basic_block();
        }
        self.pads.inside.insert(block);
        if let Some(back) = back {
            self.builder.position_at_end(back);
        }
    }

    /// A new, empty cleanup: a block starting with the phi of the entries'
    /// `landingpad` values.
    fn cleanup_block(&mut self) -> (BasicBlock<'ctx>, PhiValue<'ctx>) {
        let block = self.append("unwind.cleanup");
        let ty = self.caught();
        let mut phi = None;
        self.in_pad(block, |this| {
            phi = Some(this.builder.build_phi(ty, "unwinding").expect(POSITIONED));
        });
        (block, phi.expect("built"))
    }

    /// Lowers the cleanup into `block`: the pending `defer`s and the
    /// frame's releases, as a `return` here, then `resume`.
    fn fill_cleanup(&mut self, block: BasicBlock<'ctx>, landed: PhiValue<'ctx>) {
        self.in_pad(block, |this| {
            if let Err(e) = this.run_defers(0) {
                // Lowered at every other exit already, so a failure there
                // would have stopped the build first; kept for the end.
                this.pad_error.get_or_insert(e);
            }
            if this
                .builder
                .get_insert_block()
                .is_some_and(|b| b.get_terminator().is_none())
            {
                this.builder
                    .build_resume(landed.as_basic_value())
                    .expect(POSITIONED);
            }
        });
    }

    /// A new pad entry: `landingpad`, then `extra`, then on to `cleanup`.
    fn pad_entry(
        &mut self,
        (cleanup, landed): (BasicBlock<'ctx>, PhiValue<'ctx>),
        extra: impl FnOnce(&mut Self),
    ) -> usize {
        let entry = self.append("unwind");
        let ty = self.caught();
        let personality = self.personality();
        self.in_pad(entry, |this| {
            let value = this
                .builder
                .build_landing_pad(ty, personality, &[], true, "unwinding")
                .expect(POSITIONED);
            extra(this);
            let from = this.builder.get_insert_block().expect(POSITIONED);
            this.builder
                .build_unconditional_branch(cleanup)
                .expect(POSITIONED);
            landed.add_incoming(&[(&value, from)]);
        });
        if let Some(function) = self.function {
            function.set_personality_function(personality);
        }
        self.pads.blocks.push(entry);
        self.pads.blocks.len() - 1
    }

    /// The type a `landingpad` gives: the exception and its selector.
    fn caught(&self) -> inkwell::types::StructType<'ctx> {
        let ptr = self.context.ptr_type(Default::default());
        self.context
            .struct_type(&[ptr.into(), self.context.i32_type().into()], false)
    }

    /// Records `pad` as `call`'s landing pad.
    fn give_pad(&self, call: InstructionValue<'ctx>, pad: usize) {
        let kind = self.context.get_kind_id(PAD_KIND);
        let index = self.context.i64_type().const_int(pad as u64, false);
        let node = self.context.metadata_node(&[index.into()]);
        call.set_metadata(node, kind)
            .expect("a call takes metadata");
    }

    /// `__gcc_personality_v0`, declared once.
    pub(super) fn personality(&self) -> FunctionValue<'ctx> {
        self.module.get_function(PERSONALITY).unwrap_or_else(|| {
            let ty = self.context.i32_type().fn_type(&[], true);
            self.module
                .add_function(PERSONALITY, ty, Some(Linkage::External))
        })
    }

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

/// True if a panic may unwind out of `call`: not an LLVM intrinsic, a
/// count operation or allocation (which never panic), or C (a panic never
/// unwinds into C: a callback's entry ends the program first).
fn unwinds(call: InstructionValue<'_>) -> bool {
    let Ok(site) = CallSiteValue::try_from(call) else {
        return false;
    };
    let Some(callee) = site.get_called_fn_value() else {
        // Through a function value or a table: Lugha code.
        return true;
    };
    let name = callee.get_name().to_string_lossy();
    if name.starts_with("llvm.")
        || name.starts_with("lugha_rc_")
        || name.starts_with("lugha_rt_rc_")
    {
        return false;
    }
    name.starts_with("lugha_") && name != "lugha_rt_alloc"
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
