//! Who releases what under reference counting: temporaries, owned locals,
//! and every exit of a function.
//!
//! A fresh value (a call's result, a literal, a concatenation) goes into a
//! temporary slot of its own, released at the end of the statement that
//! made it, unless a store takes it over first by emptying the slot. A
//! `let` local owns its slot, released when its block ends or a `break`,
//! `continue` or `return` leaves it. Every slot is emptied at the start of
//! the statement that made it (the function's entry, outside any), so a
//! path that never filled it releases nothing; a released slot is emptied,
//! so nothing is released twice. A slot is live only within its statement,
//! so slots of different statements can share stack space.

use inkwell::basic_block::BasicBlock;
use inkwell::values::{
    BasicValueEnum, FunctionValue, InstructionOpcode, InstructionValue, PointerValue,
};

use super::lower::{Lowerer, POSITIONED};
use super::value::llvm_type;
use crate::ast::ExprId;
use crate::check::Type;

/// A statement being lowered.
struct Statement<'ctx> {
    /// Where it starts: after this instruction of this block (or at the
    /// block's start).
    start: (BasicBlock<'ctx>, Option<InstructionValue<'ctx>>),
    /// The temporaries it made, released at its end.
    temps: Vec<usize>,
    /// Every slot it made, emptied at its start.
    made: Vec<usize>,
}

/// The slots of the function being lowered.
#[derive(Default)]
pub(super) struct RcSlots<'ctx> {
    /// Every slot.
    all: Vec<(PointerValue<'ctx>, Type)>,
    /// Parameters an owning instance keeps: released when it returns,
    /// holding the argument from the start.
    params: Vec<(PointerValue<'ctx>, Type)>,
    /// The statements being lowered, innermost last.
    statements: Vec<Statement<'ctx>>,
    /// Temporaries made outside any statement (a function's tail), emptied
    /// at its entry and released when it returns.
    outside: Vec<usize>,
    /// Per block being lowered (in step with the `defer` lists), its owned
    /// locals.
    blocks: Vec<Vec<usize>>,
    /// The expression whose value was last put in a temporary, and where:
    /// a store of that very value takes it instead of retaining it.
    last: Option<(ExprId, PointerValue<'ctx>)>,
    /// True while a `let` pattern binds: its bindings belong to the block,
    /// not the statement.
    let_pattern: bool,
}

impl<'ctx> Lowerer<'ctx> {
    /// Records `slot`, emptied when the statement making it starts.
    fn rc_record(&mut self, slot: PointerValue<'ctx>, ty: &Type) -> usize {
        self.rc_slots.all.push((slot, ty.clone()));
        let index = self.rc_slots.all.len() - 1;
        match self.rc_slots.statements.last_mut() {
            Some(statement) => statement.made.push(index),
            None => self.rc_slots.outside.push(index),
        }
        index
    }

    /// Makes `index` a temporary of the statement being lowered. Outside
    /// any, `rc_record` listed it to release at the function's exits.
    fn rc_temp_of_statement(&mut self, index: usize) {
        if let Some(statement) = self.rc_slots.statements.last_mut() {
            statement.temps.push(index);
        }
    }

    /// A new slot of type `ty`, emptied here: in a loop, what it held from
    /// the last time round is released first.
    fn rc_slot(&mut self, ty: &Type, name: &str) -> (PointerValue<'ctx>, usize) {
        let slot = self.entry_alloca(ty, name);
        let index = self.rc_record(slot, ty);
        self.rc_empty(slot, ty);
        (slot, index)
    }

    /// Releases what `slot` holds and empties it.
    fn rc_empty(&mut self, slot: PointerValue<'ctx>, ty: &Type) {
        let held = self
            .builder
            .build_load(llvm_type(self.context, ty), slot, "held")
            .expect(POSITIONED);
        self.rc_release(held, ty);
        self.builder
            .build_store(slot, llvm_type(self.context, ty).const_zero())
            .expect(POSITIONED);
    }

    /// Puts `value`, owned, in a temporary released when the statement
    /// being lowered ends.
    pub(super) fn rc_temporary(
        &mut self,
        id: Option<ExprId>,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
    ) {
        let (slot, index) = self.rc_slot(ty, "temp");
        self.builder.build_store(slot, value).expect(POSITIONED);
        self.rc_temp_of_statement(index);
        self.rc_slots.last = id.map(|id| (id, slot));
    }

    /// A slot for a loop's result, a temporary of the statement holding
    /// the loop: what `break value` stores in it is owned.
    pub(super) fn rc_result_slot(&mut self, slot: PointerValue<'ctx>, ty: &Type) {
        let index = self.rc_record(slot, ty);
        self.rc_empty(slot, ty);
        self.rc_temp_of_statement(index);
    }

    /// Makes `slot` a temporary of the statement being lowered, emptied
    /// here, under reference counting when `ty` is counted.
    pub(super) fn rc_statement_slot(&mut self, slot: PointerValue<'ctx>, ty: &Type) {
        if self.rc() && self.counted(ty) {
            self.rc_result_slot(slot, ty);
        }
    }

    /// Stores `value`, owned, into `slot`, releasing what it held.
    pub(super) fn rc_replace(
        &mut self,
        slot: PointerValue<'ctx>,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
    ) {
        self.rc_empty(slot, ty);
        self.builder.build_store(slot, value).expect(POSITIONED);
    }

    /// One more holder of `value` until the statement ends, under reference
    /// counting: for a value whose place may let go of it meanwhile.
    pub(super) fn rc_keep(&mut self, value: BasicValueEnum<'ctx>, ty: &Type) {
        if self.rc() && self.counted(ty) {
            self.rc_retain(value, ty);
            self.rc_temporary(None, value, ty);
        }
    }

    /// Forgets which value was last put in a temporary.
    pub(super) fn rc_forget_last(&mut self) {
        self.rc_slots.last = None;
    }

    /// Takes over the temporary holding `id`'s value, if the value just
    /// lowered for it is one: its slot is emptied and the caller owns it.
    pub(super) fn rc_take(&mut self, id: ExprId, ty: &Type) -> bool {
        match self.rc_slots.last.take() {
            Some((last, slot)) if last == id => {
                self.builder
                    .build_store(slot, llvm_type(self.context, ty).const_zero())
                    .expect(POSITIONED);
                true
            }
            _ => false,
        }
    }

    /// Marks whether a `let` pattern is binding, giving the previous mark.
    pub(super) fn rc_let_pattern(&mut self, on: bool) -> bool {
        std::mem::replace(&mut self.rc_slots.let_pattern, on)
    }

    /// A new slot for a pattern's binding, holding `value` with one more
    /// holder: released with the statement (a `match` arm, `if let`), or
    /// with the block for a `let` pattern.
    pub(super) fn rc_binding(
        &mut self,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
        name: &str,
    ) -> PointerValue<'ctx> {
        let (slot, index) = self.rc_slot(ty, name);
        self.rc_retain(value, ty);
        self.builder.build_store(slot, value).expect(POSITIONED);
        if self.rc_slots.let_pattern {
            if let Some(block) = self.rc_slots.blocks.last_mut() {
                block.push(index);
            }
        } else {
            self.rc_temp_of_statement(index);
        }
        slot
    }

    /// An or-pattern's later alternative binding the slot an earlier one
    /// made: what it held is released, and `value` held.
    pub(super) fn rc_rebind(
        &mut self,
        slot: PointerValue<'ctx>,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
    ) {
        self.rc_empty(slot, ty);
        self.rc_retain(value, ty);
        self.builder.build_store(slot, value).expect(POSITIONED);
    }

    /// A `let` local now owns `slot`: released when its block ends.
    pub(super) fn rc_own_local(&mut self, slot: PointerValue<'ctx>, ty: &Type) {
        if !self.rc() || !self.counted(ty) {
            return;
        }
        let index = self.rc_record(slot, ty);
        if let Some(block) = self.rc_slots.blocks.last_mut() {
            block.push(index);
        }
    }

    /// A parameter an owning instance keeps now owns `slot`: what is left
    /// in it is released when the function returns.
    pub(super) fn rc_own_param(&mut self, slot: PointerValue<'ctx>, ty: &Type) {
        if self.rc() && self.counted(ty) {
            self.rc_slots.params.push((slot, ty.clone()));
        }
    }

    /// Starts a statement's slots, where the builder is now.
    pub(super) fn rc_statement_start(&mut self) {
        if !self.rc() {
            return;
        }
        let block = self.current_block();
        let start = (block, block.get_last_instruction());
        self.rc_slots.statements.push(Statement {
            start,
            temps: Vec::new(),
            made: Vec::new(),
        });
    }

    /// Ends a statement: the slots it made are emptied at its start, and
    /// its temporaries released here.
    pub(super) fn rc_statement_end(&mut self) {
        self.rc_statement_end_also(None);
    }

    /// `rc_statement_end`, its temporaries released at the start of `also`
    /// too: a pattern's test may bind some names, then fail, and jump there.
    pub(super) fn rc_statement_end_also(&mut self, also: Option<BasicBlock<'ctx>>) {
        if !self.rc() {
            return;
        }
        let Some(statement) = self.rc_slots.statements.pop() else {
            return;
        };
        let (block, after) = statement.start;
        let mut next = match after {
            Some(i) => i.get_next_instruction(),
            None => block.get_first_instruction(),
        };
        // Past the entry block's slots, which come first.
        while let Some(i) = next.filter(|i| i.get_opcode() == InstructionOpcode::Alloca) {
            next = i.get_next_instruction();
        }
        let builder = self.context.create_builder();
        match next {
            Some(i) => builder.position_before(&i),
            None => builder.position_at_end(block),
        }
        for &index in &statement.made {
            let (slot, ty) = &self.rc_slots.all[index];
            builder
                .build_store(*slot, llvm_type(self.context, ty).const_zero())
                .expect(POSITIONED);
        }
        let resume = self.builder.get_insert_block();
        for at in [None].into_iter().chain(also.map(Some)) {
            if let Some(block) = at {
                match block.get_first_instruction() {
                    Some(first) => self.builder.position_before(&first),
                    None => self.builder.position_at_end(block),
                }
            }
            for &index in statement.temps.iter().rev() {
                let (slot, ty) = self.rc_slots.all[index].clone();
                self.rc_empty(slot, &ty);
            }
        }
        if let (Some(block), Some(_)) = (resume, also) {
            self.builder.position_at_end(block);
        }
    }

    /// How many statements are being lowered: a loop's body starts at this
    /// depth, and a jump out of it releases the temporaries of those inside.
    pub(super) fn rc_statement_depth(&self) -> usize {
        self.rc_slots.statements.len()
    }

    /// Releases the temporaries of the statements from depth `to` up,
    /// innermost first, for a jump out of them.
    pub(super) fn rc_leave_statements(&mut self, to: usize) {
        if !self.rc() {
            return;
        }
        let leaving: Vec<usize> = self.rc_slots.statements
            [to.min(self.rc_slots.statements.len())..]
            .iter()
            .rev()
            .flat_map(|s| s.temps.iter().rev().copied())
            .collect();
        for index in leaving {
            let (slot, ty) = self.rc_slots.all[index].clone();
            self.rc_empty(slot, &ty);
        }
    }

    /// At an exit of the function: every statement's temporaries, those
    /// made outside any, every block's locals and the kept parameters are
    /// released. A `return` value already holds its own.
    pub(super) fn rc_exit_function(&mut self) {
        if !self.rc() {
            return;
        }
        self.rc_leave_statements(0);
        let outside = self.rc_slots.outside.clone();
        for index in outside.into_iter().rev() {
            let (slot, ty) = self.rc_slots.all[index].clone();
            self.rc_empty(slot, &ty);
        }
        self.rc_leave_blocks(0);
        for (slot, ty) in self.rc_slots.params.clone() {
            self.rc_empty(slot, &ty);
        }
    }

    /// A block's value, about to outlive the block's locals and defers:
    /// one more holder, released with the statement's temporaries.
    pub(super) fn rc_outlive_block(
        &mut self,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
    ) -> BasicValueEnum<'ctx> {
        let depth = self.defers.len() - 1;
        let owns = self.rc_slots.blocks.last().is_some_and(|b| !b.is_empty());
        if self.counted(ty) && (owns || self.defers_pending(depth)) {
            self.rc_retain(value, ty);
            self.rc_temporary(None, value, ty);
        }
        value
    }

    /// Starts a block's owned locals.
    pub(super) fn rc_block_start(&mut self) {
        self.rc_slots.blocks.push(Vec::new());
    }

    /// Ends a block, whose owned locals its exit released.
    pub(super) fn rc_block_end(&mut self) {
        // Its exit already released them, with its defers.
        self.rc_slots.blocks.pop();
    }

    /// Releases the owned locals of the blocks from depth `to` up, innermost
    /// first, for a jump out of them.
    pub(super) fn rc_leave_blocks(&mut self, to: usize) {
        if !self.rc() {
            return;
        }
        let leaving: Vec<usize> = self.rc_slots.blocks[to..]
            .iter()
            .rev()
            .flat_map(|block| block.iter().rev().copied())
            .collect();
        for index in leaving {
            let (slot, ty) = self.rc_slots.all[index].clone();
            self.rc_empty(slot, &ty);
        }
    }

    /// Starts a function's slots, keeping the enclosing function's (a
    /// closure is lowered in the middle of another function).
    pub(super) fn rc_enter_function(&mut self) -> RcSlots<'ctx> {
        std::mem::take(&mut self.rc_slots)
    }

    /// Ends `function`: the slots made outside any statement are emptied
    /// at its entry. Then the enclosing function's slots are back.
    pub(super) fn rc_finish_function(
        &mut self,
        function: FunctionValue<'ctx>,
        outer: RcSlots<'ctx>,
    ) {
        let slots = std::mem::replace(&mut self.rc_slots, outer);
        if !self.rc() || slots.outside.is_empty() {
            return;
        }
        let entry = function
            .get_first_basic_block()
            .expect("functions start with an entry block");
        let first = entry
            .get_instructions()
            .find(|i| i.get_opcode() != InstructionOpcode::Alloca);
        let builder = self.context.create_builder();
        match first {
            Some(first) => builder.position_before(&first),
            None => builder.position_at_end(entry),
        }
        for &index in &slots.outside {
            let (slot, ty) = &slots.all[index];
            builder
                .build_store(*slot, llvm_type(self.context, ty).const_zero())
                .expect(POSITIONED);
        }
    }

    /// After the entry returns, under reference counting: its arguments
    /// and every constant are released, so the report at exit counts only
    /// what the program lost.
    pub(super) fn rc_after_entry(
        &mut self,
        params: &[Type],
        args: &[inkwell::values::BasicMetadataValueEnum<'ctx>],
    ) {
        if !self.rc() {
            return;
        }
        for (arg, ty) in args.iter().zip(params) {
            if let Ok(value) = BasicValueEnum::try_from(*arg) {
                self.rc_release(value, ty);
            }
        }
        let globals: Vec<_> = self.const_globals.values().cloned().collect();
        for (global, ty) in globals {
            let value = self
                .builder
                .build_load(
                    llvm_type(self.context, &ty),
                    global.as_pointer_value(),
                    "const",
                )
                .expect(POSITIONED);
            self.rc_release(value, &ty);
        }
    }
}
