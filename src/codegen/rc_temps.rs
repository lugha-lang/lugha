//! Temporaries that need no slot: one taken over
//! in the block that made it, and a fresh argument, released right after
//! the call it is passed to. Nothing but the temporary's own instructions
//! reached its slot, so they go with it, and the frame stays small.

use inkwell::basic_block::BasicBlock;
use inkwell::values::{BasicValueEnum, InstructionOpcode, InstructionValue, PointerValue};

use super::lower::Lowerer;
use super::rc_slots::Made;
use crate::ast::ExprId;
use crate::check::Type;

/// A fresh argument's temporary, claimed for release after its call.
pub(super) struct Claim<'ctx> {
    slot: PointerValue<'ctx>,
    made: Made<'ctx>,
    value: BasicValueEnum<'ctx>,
    ty: Type,
    /// The argument's own slot, when it is passed through one: the
    /// callee never changes it, so the release reads only what it needs.
    pub(super) at: Option<PointerValue<'ctx>>,
}

impl<'ctx> Lowerer<'ctx> {
    /// Removes the temporary in `slot`, as `made`: its instructions, its
    /// slot, and its place in the lists of what to empty and release.
    pub(super) fn rc_dissolve(&mut self, slot: PointerValue<'ctx>, made: Made<'ctx>) {
        let (index, block, before, store) = made;
        erase_after(block, before, store);
        slot.as_instruction()
            .expect("a slot is an alloca")
            .erase_from_basic_block();
        for list in self
            .rc_slots
            .statements
            .iter_mut()
            .flat_map(|s| [&mut s.temps, &mut s.made])
        {
            list.retain(|&i| i != index);
        }
        self.rc_slots.outside.retain(|&i| i != index);
    }

    /// True if leaving the function now would release anything: a
    /// temporary, a local or an owned parameter.
    pub(super) fn rc_holds_anything(&self) -> bool {
        let slots = &self.rc_slots;
        slots.statements.iter().any(|s| !s.temps.is_empty())
            || !slots.outside.is_empty()
            || slots.blocks.iter().any(|b| !b.is_empty())
            || !slots.params.is_empty()
    }

    /// The temporary the argument `id`'s value was just put in, made in
    /// the current block, claimed so the call can release it.
    pub(super) fn rc_claim(
        &mut self,
        id: ExprId,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
    ) -> Option<Claim<'ctx>> {
        let made = self.rc_slots.made?;
        match self.rc_slots.last {
            Some((last, slot)) if last == id && made.1 == self.current_block() => {
                self.rc_slots.last = None;
                self.rc_slots.made = None;
                Some(Claim {
                    slot,
                    made,
                    value,
                    ty: ty.clone(),
                    at: None,
                })
            }
            _ => None,
        }
    }

    /// After a call, in the block its claimed arguments were made in: each
    /// is released here instead of with its statement, and its temporary
    /// goes. In another block (an argument branched), they stay as made.
    /// Last first: a later argument's range starts after an earlier one's.
    ///
    /// `call` is the call they were passed to: if it unwinds, its own pad
    /// lets go of them (`codegen::unwind`).
    pub(super) fn rc_release_claims(
        &mut self,
        call: InstructionValue<'ctx>,
        claims: Vec<Claim<'ctx>>,
    ) {
        let mut released = Vec::new();
        for claim in claims.into_iter().rev() {
            if claim.made.1 != self.current_block() {
                continue;
            }
            self.rc_dissolve(claim.slot, claim.made);
            let what = (claim.at, claim.value, claim.ty);
            self.rc_let_go_claimed(&what);
            released.push(what);
        }
        if !released.is_empty() {
            self.unwind_claimed(call, |this| {
                for what in &released {
                    this.rc_let_go_claimed(what);
                }
            });
        }
    }

    /// Lets go of a claimed argument: from its slot, or its value.
    fn rc_let_go_claimed(
        &mut self,
        (at, value, ty): &(Option<PointerValue<'ctx>>, BasicValueEnum<'ctx>, Type),
    ) {
        match at {
            Some(at) => self.rc_value_at(*at, ty, super::rc::RcOp::Release),
            None => self.rc_release(*value, ty),
        }
    }
}

/// Erases `block`'s instructions after `before` up to `last`, users
/// before what they use. With no `before` the range starts past the
/// slots at the block's head: an entry block gets every slot there, the
/// temporary's own among them.
fn erase_after<'ctx>(
    block: BasicBlock<'ctx>,
    before: Option<InstructionValue<'ctx>>,
    last: InstructionValue<'ctx>,
) {
    let mut range = Vec::new();
    let mut next = match before {
        Some(i) => i.get_next_instruction(),
        None => block
            .get_instructions()
            .find(|i| i.get_opcode() != InstructionOpcode::Alloca),
    };
    while let Some(i) = next {
        range.push(i);
        if i == last {
            break;
        }
        next = i.get_next_instruction();
    }
    for i in range.into_iter().rev() {
        i.erase_from_basic_block();
    }
}
