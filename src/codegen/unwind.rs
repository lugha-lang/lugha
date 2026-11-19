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
//! passes that rewrite calls have run, `invokes` (`codegen::invoke`) turns
//! every call carrying a pad into an `invoke` of it.

use std::collections::{HashMap, HashSet};

use inkwell::basic_block::BasicBlock;
use inkwell::module::Linkage;
use inkwell::values::{
    CallSiteValue, FunctionValue, InstructionOpcode, InstructionValue, PhiValue,
};

use super::lower::{Lowerer, POSITIONED};

/// The metadata a call carries naming its landing pad, by index.
pub(super) const PAD_KIND: &str = "lugha.unwind";

/// The personality every Lugha function with a landing pad uses: libgcc's,
/// for frames that only clean up.
const PERSONALITY: &str = "__gcc_personality_v0";

/// Where a statement starts, naming its cleanup.
pub(super) type Start<'ctx> = (BasicBlock<'ctx>, Option<InstructionValue<'ctx>>);

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
    /// The function's last block when each statement started, by its
    /// start: blocks up to it were made before the statement, and may
    /// follow its first block yet belong to an enclosing one.
    last_before: HashMap<Start<'ctx>, Option<BasicBlock<'ctx>>>,
    /// True while a pad's cleanup is lowered: its own calls get no pad.
    pub(super) lowering: bool,
    /// The scopes whose bodies are being lowered, innermost last.
    pub(super) catches: Vec<super::task_scopes::Catch<'ctx>>,
    /// True while a block's own end runs its `defer`s.
    pub(super) block_end: bool,
}

impl<'ctx> Lowerer<'ctx> {
    /// A statement starts at `start`: which blocks are its own is known
    /// from what was there already.
    pub(super) fn unwind_begins(&mut self, start: Start<'ctx>) {
        let last = self.function.and_then(|f| f.get_last_basic_block());
        self.pads.last_before.insert(start, last);
    }

    /// What a statement starting at `start` recorded, put back by
    /// [`Self::unwind_restore`]: a construct's body may start at the same
    /// place, after blocks the construct made for itself, and record those.
    pub(super) fn unwind_record(&self, start: Start<'ctx>) -> Option<Option<BasicBlock<'ctx>>> {
        self.pads.last_before.get(&start).copied()
    }

    /// Puts back what [`Self::unwind_record`] gave.
    pub(super) fn unwind_restore(
        &mut self,
        start: Start<'ctx>,
        record: Option<Option<BasicBlock<'ctx>>>,
    ) {
        match record {
            Some(last) => self.pads.last_before.insert(start, last),
            None => self.pads.last_before.remove(&start),
        };
    }

    /// At the end of the statement that started at `start`: its calls
    /// still without a pad get one running what a `return` here would.
    pub(super) fn unwind_statement(&mut self, start: Start<'ctx>) {
        if self.pads.lowering {
            return;
        }
        let calls = self.unpadded_calls(start);
        let early = self.pads.cleanups.remove(&start);
        // Inside a scope's body, every call's unwind is caught.
        let pending =
            self.defers_pending(0) || self.rc_holds_anything() || !self.pads.catches.is_empty();
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
        let start = match (self.scope_start(), self.rc_slots.statements.last()) {
            (Some(start), _) => start,
            (None, Some(statement)) => statement.start,
            (None, None) => {
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
        (main, call): (FunctionValue<'ctx>, CallSiteValue<'ctx>),
        cleanup: impl FnOnce(&mut Self),
    ) {
        let saved = self.function.replace(main);
        let (block, landed) = self.cleanup_block();
        self.in_pad(block, |this| {
            cleanup(this);
            this.builder
                .build_resume(landed.as_basic_value())
                .expect(POSITIONED);
        });
        let pad = self.pad_entry((block, landed), |_| {});
        self.give_pad(super::invoke::instruction(call), pad);
        self.function = saved;
    }

    /// `call` unwinds to a pad running `body` (which ends its block), and
    /// the unwind stops there: a task's entry catching its own.
    pub(super) fn catch_call(
        &mut self,
        call: InstructionValue<'ctx>,
        body: impl FnOnce(&mut Self),
    ) {
        let (block, landed) = self.cleanup_block();
        self.in_pad(block, body);
        let pad = self.pad_entry((block, landed), |_| {});
        self.give_pad(call, pad);
    }

    /// In a function C calls (a callback's entry): a panic never unwinds
    /// into C, so every call here that may unwind gets a pad running
    /// `stop`, which must end with a return or `unreachable`.
    pub(super) fn unwind_stops_here(&mut self, stop: impl FnOnce(&mut Self)) {
        let Some(entry) = self.function.and_then(|f| f.get_first_basic_block()) else {
            return;
        };
        let calls = self.unpadded_calls((entry, None));
        if calls.is_empty() {
            return;
        }
        let (block, landed) = self.cleanup_block();
        self.in_pad(block, stop);
        let pad = self.pad_entry((block, landed), |_| {});
        for call in calls {
            self.give_pad(call, pad);
        }
    }

    /// The calls from `start` on (that instruction's block after it, then
    /// every later block of the function) that can unwind and have no pad
    /// yet, outside the pads' own blocks.
    fn unpadded_calls(&self, (block, after): Start<'ctx>) -> Vec<InstructionValue<'ctx>> {
        let kind = self.context.get_kind_id(PAD_KIND);
        // Blocks made before the statement aren't its own, wherever they lie;
        // a function's own start (no entry recorded) owns them all. Kept,
        // not removed: statements starting at one place share the entry.
        let mut skip_until = self
            .pads
            .last_before
            .get(&(block, after))
            .copied()
            .flatten();
        if skip_until == Some(block) {
            skip_until = None;
        }
        let mut calls = Vec::new();
        let mut next_block = Some(block);
        let mut first = true;
        while let Some(b) = next_block {
            next_block = b.get_next_basic_block();
            let earlier = !first && skip_until.is_some();
            if skip_until == Some(b) {
                skip_until = None;
            }
            if earlier || self.pads.inside.contains(&b) {
                first = false;
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
        // What the pad's code registers to release (a hand-back's slots, a
        // `defer`'s) is released by the pad itself: no other way out of the
        // statement may release slots only the pad writes. They stay among
        // the slots emptied at the statement's start.
        let temps: Vec<usize> = self
            .rc_slots
            .statements
            .iter()
            .map(|s| s.temps.len())
            .collect();
        let outside = self.rc_slots.outside.len();
        let blocks: Vec<usize> = self.rc_slots.blocks.iter().map(Vec::len).collect();
        let was = std::mem::replace(&mut self.pads.lowering, true);
        body(self);
        self.pads.lowering = was;
        for (statement, n) in self.rc_slots.statements.iter_mut().zip(temps) {
            statement.temps.truncate(n);
        }
        self.rc_slots.outside.truncate(outside);
        for (block, n) in self.rc_slots.blocks.iter_mut().zip(blocks) {
            block.truncate(n);
        }
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
    /// frame's releases, as a `return` here, then `resume`; inside a
    /// scope's body, only the body's, then on past it.
    fn fill_cleanup(&mut self, block: BasicBlock<'ctx>, landed: PhiValue<'ctx>) {
        self.in_pad(block, |this| {
            if this.scope_cleanup() {
                return;
            }
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

#[cfg(test)]
#[path = "unwind_tests.rs"]
mod tests;
