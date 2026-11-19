//! Scopes in codegen. `scope s { … }` asks the runtime for a record
//! (`lugha_rt_scope_enter`) that the body's tasks start in, lowers the body
//! as a block, and ends with `lugha_rt_scope_exit`, whose status picks the
//! `Result`.
//!
//! Every way out of the body waits for its tasks before the body's
//! `defer`s run: its end (`lugha_rt_scope_end`), an early exit
//! (`lugha_rt_scope_leave`, cancelling them first). An unwind is caught at
//! the scope: within the body, a landing pad's cleanup runs the body's
//! `defer`s and releases only, after telling the runtime it caught the
//! unwind (`lugha_rt_scope_caught`), and jumps past the body instead of
//! resuming.

use inkwell::IntPredicate;
use inkwell::basic_block::BasicBlock;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, PointerValue, ValueKind};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::rc::RcOp;
use super::scope::Local;
use super::unwind::Start;
use super::value::{Value, llvm_type};
use crate::ast::{Expr, ScopeExpr};
use crate::check::Type;

/// How a scope ended, as `lugha_rt_scope_exit` and `lugha_rt_task_join_status` say.
pub(super) const ENDED_OK: u64 = 0;
pub(super) const ENDED_FAILED: u64 = 2;
pub(super) const ENDED_PANICKED: u64 = 3;

/// The 8-byte words a scope's record takes in its frame (the runtime's
/// `LughaScope` checks that it fits).
const SCOPE_RECORD_WORDS: u32 = 24;

/// A scope or shield whose body is being lowered: where its pads stop.
pub(super) struct Catch<'ctx> {
    /// The scope's runtime record; none for a shield.
    pub(super) rec: Option<PointerValue<'ctx>>,
    /// Where a caught unwind goes on, once cleaned up.
    pub(super) caught: BasicBlock<'ctx>,
    /// The depth of the body's block among the blocks with `defer`s.
    pub(super) defers: usize,
    /// How many statements enclosed the scope.
    pub(super) statements: usize,
    /// Where the scope started: calls after it still without a pad when
    /// the body ends get the scope's.
    pub(super) start: Start<'ctx>,
}

impl<'ctx> Lowerer<'ctx> {
    /// `scope s { body }`: `Ok` with the body's value once every task it
    /// started is done, or `Err` with the first failure.
    pub(super) fn scope_expr(
        &mut self,
        expr: &Expr,
        scope: &ScopeExpr,
    ) -> Result<Value<'ctx>, CodegenError> {
        let result = self.ty(expr);
        let Type::Enum(_, args) = &result else {
            unreachable!("checked: a scope is a `Result`")
        };
        let (body_type, failure) = (args[0].clone(), args[1].clone());
        // Owned by the scope until its value is taken: empty until set.
        let slot = self.entry_alloca(&result, "scope.value");
        let empty = llvm_type(self.context, &result).const_zero();
        self.builder.build_store(slot, empty).expect(POSITIONED);
        let ptr = Type::String;
        // The record lives in the frame: an exit can't leave it behind.
        let words = self.context.i64_type().array_type(SCOPE_RECORD_WORDS);
        let record = self.entry_slot(words.into(), "scope.record");
        // Every use names the slot itself, so its frame keeps it to the end.
        let rec = record;
        let mut enter = vec![rec.into()];
        enter.extend(self.scope_settings(scope)?);
        let params = [
            ptr.clone(),
            ptr.clone(),
            Type::I32,
            ptr.clone(),
            ptr.clone(),
            Type::I64,
            Type::I64,
            Type::I64,
            Type::I32,
        ];
        self.rt("lugha_rt_scope_enter", &params, None, &enter);

        // `Scope<E>`, `E` being what its tasks fail with, as in `Failure<E>`.
        let Type::Struct(_, failure_args) = &failure else {
            unreachable!("checked: a scope fails with a `Failure`")
        };
        let handle_type = Type::Struct("$prelude.Scope".to_string(), failure_args.clone());
        let handle = self.entry_alloca(&handle_type, &scope.name.name);
        let shell = llvm_type(self.context, &handle_type).into_struct_type();
        let value = self
            .builder
            .build_insert_value(shell.get_undef(), rec, 0, "scope")
            .expect("a scope has 1 field");
        self.builder.build_store(handle, value).expect(POSITIONED);
        self.scopes.push();
        let local = Local {
            ptr: handle,
            ty: handle_type,
            borrowed: true,
        };
        self.scopes.declare(&scope.name.name, local);

        let block = self.current_block();
        let start = (block, block.get_last_instruction());
        self.unwind_begins(start);
        let caught = self.append("scope.caught");
        let end = self.append("scope.end");
        self.pads.catches.push(Catch {
            rec: Some(rec),
            caught,
            defers: self.defers.len(),
            statements: self.rc_statement_depth(),
            start,
        });
        let body = self.block(&scope.body);
        if let Ok(value) = &body {
            match value {
                Value::Never => {
                    self.builder.build_unreachable().expect(POSITIONED);
                }
                value => {
                    let values = match value {
                        Value::Val(v) => {
                            self.rc_retain(*v, &body_type);
                            vec![*v]
                        }
                        _ => Vec::new(),
                    };
                    let ok = self.construct_values(&result, "Ok", values);
                    self.builder.build_store(slot, ok).expect(POSITIONED);
                    self.branch(end);
                }
            }
            // The body's calls still without a pad (its tail's) are caught here too.
            self.unwind_statement(start);
        }
        self.pads.catches.pop();
        self.scopes.pop();
        body?;

        self.builder.position_at_end(caught);
        self.branch(end);
        self.builder.position_at_end(end);
        let parts = [
            self.entry_alloca(&ptr, "failed.table"),
            self.entry_alloca(&ptr, "failed.box"),
            self.entry_alloca(&ptr, "failed.panic"),
            self.entry_alloca(&ptr, "failed.task"),
        ];
        let mut args: Vec<BasicValueEnum> = vec![rec.into()];
        args.extend(parts.iter().map(|p| BasicValueEnum::from(*p)));
        let status = self
            .rt(
                "lugha_rt_scope_exit",
                &[
                    ptr.clone(),
                    ptr.clone(),
                    ptr.clone(),
                    ptr.clone(),
                    ptr.clone(),
                ],
                Some(Type::I32),
                &args,
            )
            .expect("returns the status")
            .into_int_value();
        let i32_type = self.context.i32_type();
        let (failed, done) = (self.append("scope.failed"), self.append("scope.done"));
        let ok = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                status,
                i32_type.const_int(ENDED_OK, false),
                "ok",
            )
            .expect(POSITIONED);
        self.builder
            .build_conditional_branch(ok, done, failed)
            .expect(POSITIONED);
        self.builder.position_at_end(failed);
        // The body may have finished before a task failed.
        self.rc_value_at(slot, &result, RcOp::Release);
        let err = self.scope_failure(&failure, status, parts);
        let err = self.construct_values(&result, "Err", vec![err]);
        self.builder.build_store(slot, err).expect(POSITIONED);
        self.branch(done);
        self.builder.position_at_end(done);
        Ok(Value::Val(
            self.builder
                .build_load(llvm_type(self.context, &result), slot, "scope")
                .expect(POSITIONED),
        ))
    }

    /// The scopes an exit to block depth `to` leaves, outermost first,
    /// each with its body's depth: none from a pad, whose scope caught it.
    pub(super) fn scopes_left(&self, to: usize) -> Vec<(usize, PointerValue<'ctx>)> {
        if self.pads.lowering {
            return Vec::new();
        }
        self.pads
            .catches
            .iter()
            .filter(|c| c.defers >= to)
            .filter_map(|c| Some((c.defers, c.rec?)))
            .collect()
    }

    /// Leaves the scope `rec` before its body's `defer`s run: at the body's
    /// `end`, waiting for its tasks; else cancelling them first.
    pub(super) fn scope_left(&mut self, rec: PointerValue<'ctx>, end: bool) {
        let name = if end {
            "lugha_rt_scope_end"
        } else {
            "lugha_rt_scope_leave"
        };
        self.rt(name, &[Type::String], None, &[rec.into()]);
    }

    /// A pad's cleanup inside the innermost scope's or shield's body: the
    /// runtime takes the unwind over, the body's `defer`s and releases run,
    /// then on past the body.
    pub(super) fn scope_cleanup(&mut self) -> bool {
        let Some(catch) = self.pads.catches.last() else {
            return false;
        };
        let (rec, caught) = (catch.rec, catch.caught);
        let (defers, statements) = (catch.defers, catch.statements);
        match rec {
            Some(rec) => {
                self.rt(
                    "lugha_rt_scope_caught",
                    &[Type::String],
                    None,
                    &[rec.into()],
                );
            }
            None => {
                self.rt("lugha_rt_shield_caught", &[], None, &[]);
            }
        }
        if let Err(e) = self.run_defers(defers) {
            // Lowered at every other exit already, so a failure there
            // would have stopped the build first; kept for the end.
            self.pad_error.get_or_insert(e);
        }
        self.rc_leave_statements(statements);
        if self
            .builder
            .get_insert_block()
            .is_some_and(|b| b.get_terminator().is_none())
        {
            self.branch(caught);
        }
        true
    }

    /// Where a call made now unwinds to when a scope catches it without
    /// a statement of the body around: the scope's start.
    pub(super) fn scope_start(&self) -> Option<Start<'ctx>> {
        self.pads
            .catches
            .last()
            .filter(|c| c.statements == self.rc_statement_depth())
            .map(|c| c.start)
    }

    /// A slot of LLVM type `ty` at the top of the function's entry block.
    fn entry_slot(
        &self,
        ty: inkwell::types::BasicTypeEnum<'ctx>,
        name: &str,
    ) -> PointerValue<'ctx> {
        let entry = self
            .function
            .and_then(|f| f.get_first_basic_block())
            .expect("a scope is inside a function");
        let builder = self.context.create_builder();
        match entry.get_first_instruction() {
            Some(first) => builder.position_before(&first),
            None => builder.position_at_end(entry),
        }
        builder.build_alloca(ty, name).expect(POSITIONED)
    }

    /// At the end of a block's tail: when the block is a scope's body, the
    /// calls of its tail get the scope's pad while its locals are in reach.
    pub(super) fn scope_body_tail(&mut self) {
        let depth = self.defers.len() - 1;
        if let Some(start) = self
            .pads
            .catches
            .last()
            .filter(|c| c.defers == depth)
            .map(|c| c.start)
        {
            self.unwind_statement(start);
        }
    }

    /// An entry's argument `value`, of type `ty`, as passed: a struct (the
    /// root scope's handle) through a slot of the entry's own.
    pub(super) fn entry_passed(
        &mut self,
        ty: &Type,
        value: BasicValueEnum<'ctx>,
    ) -> BasicMetadataValueEnum<'ctx> {
        if !super::value::is_aggregate(ty) {
            return value.into();
        }
        let slot = self
            .builder
            .build_alloca(value.get_type(), "entry.argument")
            .expect(POSITIONED);
        self.builder.build_store(slot, value).expect(POSITIONED);
        slot.into()
    }

    /// `main` unwound: the root scope's tasks stop first if it was
    /// cancelled, then its arguments are let go of.
    pub(super) fn main_unwound(&mut self, params: &[Type], args: &[BasicMetadataValueEnum<'ctx>]) {
        self.rt("lugha_rt_scope_root_unwound", &[], None, &[]);
        self.rc_after_entry(params, args);
    }

    /// Once `main` (or a test) returns: the root scope's tasks finish, then
    /// its arguments are let go of. True (an `i1`) if a root task failed.
    pub(super) fn finish_entry(
        &mut self,
        params: &[Type],
        args: &[BasicMetadataValueEnum<'ctx>],
    ) -> inkwell::values::IntValue<'ctx> {
        let failed = self
            .rt("lugha_rt_scope_root_finish", &[], Some(Type::I32), &[])
            .expect("returns whether a task failed")
            .into_int_value();
        self.rc_after_entry(params, args);
        self.builder
            .build_int_compare(
                IntPredicate::NE,
                failed,
                failed.get_type().const_zero(),
                "failed",
            )
            .expect(POSITIONED)
    }

    /// The program's exit code: 101 when a root task failed, else `code`.
    pub(super) fn exit_code(
        &mut self,
        code: inkwell::values::IntValue<'ctx>,
        failed: inkwell::values::IntValue<'ctx>,
    ) -> inkwell::values::IntValue<'ctx> {
        let panicked = code.get_type().const_int(101, false);
        self.builder
            .build_select(failed, panicked, code, "exit")
            .expect(POSITIONED)
            .into_int_value()
    }

    /// Calls runtime function `name`; its value, if it returns one.
    pub(super) fn rt(
        &mut self,
        name: &str,
        params: &[Type],
        returns: Option<Type>,
        args: &[BasicValueEnum<'ctx>],
    ) -> Option<BasicValueEnum<'ctx>> {
        let function = self.runtime(name, params, returns);
        let args: Vec<BasicMetadataValueEnum> = args.iter().map(|a| (*a).into()).collect();
        let call = self
            .builder
            .build_call(function, &args, "")
            .expect(POSITIONED);
        match call.try_as_basic_value() {
            ValueKind::Basic(value) => Some(value),
            ValueKind::Instruction(_) => None,
        }
    }
}
