//! Task handles in codegen: `t.join()` builds the `Result<V, Stopped>`
//! from how the runtime says the task ended, `t.cancel(why)` and
//! `t.name(text)` call it; and a task returning `Err` tells the runtime it
//! failed, with the error as an `Error` value.

use inkwell::IntPredicate;
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::task_scopes::{ENDED_FAILED, ENDED_OK, ENDED_PANICKED};
use super::value::{Value, llvm_type};
use crate::ast::Expr;
use crate::check::Type;

/// How a task's cancellation came about, as the runtime numbers it.
const SIBLING_FAILED: u64 = 1;
const PARENT: u64 = 2;

/// The prelude's `Error` interface, as a type.
fn error_type() -> Type {
    Type::Interface("$prelude.Error".to_string(), Vec::new())
}

/// The `Ok` and `Err` types of `ty`, if it is a `Result`.
fn result_parts(ty: &Type) -> Option<(&Type, &Type)> {
    match ty {
        Type::Enum(k, args) if k == "$prelude.Result" && args.len() == 2 => {
            Some((&args[0], &args[1]))
        }
        _ => None,
    }
}

impl<'ctx> Lowerer<'ctx> {
    /// `t.join()`, `t.cancel(why)` or `t.name(text)`.
    pub(super) fn task_method(
        &mut self,
        call: &Expr,
        base: &Expr,
        name: &str,
        args: &[Expr],
    ) -> Result<Value<'ctx>, CodegenError> {
        let ty = self.ty(base);
        let task = self.get(base, &ty)?;
        let runtime = match name {
            "join" => return Ok(Value::Val(self.join(call, &ty, task))),
            "cancel" => "lugha_rt_task_cancel",
            _ => "lugha_rt_task_name",
        };
        let text = self.get(&args[0], &Type::String)?;
        self.rt(runtime, &[Type::String, Type::String], None, &[task, text]);
        Ok(Value::Void)
    }

    /// Waits for `task`, then its value or how it stopped.
    fn join(&mut self, call: &Expr, ty: &Type, task: BasicValueEnum<'ctx>) -> BasicValueEnum<'ctx> {
        let Type::Task(result) = ty else {
            unreachable!("checked: `join` has a `Task` receiver")
        };
        let joined = self.ty(call);
        let Some((value_type, stopped)) = result_parts(&joined) else {
            unreachable!("checked: `join` gives a `Result`")
        };
        let (value_type, stopped) = (value_type.clone(), stopped.clone());
        let ptr = Type::String;
        let reason = self.entry_alloca(&Type::I32, "stop.reason");
        let parts = [
            self.entry_alloca(&ptr, "stop.text"),
            self.entry_alloca(&ptr, "stop.table"),
            self.entry_alloca(&ptr, "stop.box"),
        ];
        let mut args = vec![task, reason.into()];
        args.extend(parts.iter().map(|p| BasicValueEnum::from(*p)));
        let params = [ptr.clone(), ptr.clone(), ptr.clone(), ptr.clone(), ptr];
        let status = self
            .rt("lugha_rt_task_join_status", &params, Some(Type::I32), &args)
            .expect("returns the status")
            .into_int_value();
        let slot = self.entry_alloca(&joined, "joined");
        let done = self.append("join.done");
        let ok = self.append("join.ok");
        let cancelled = self.append("join.cancelled");
        let failed = self.append("join.failed");
        let panicked = self.append("join.panicked");
        let i32_type = self.context.i32_type();
        self.builder
            .build_switch(
                status,
                cancelled,
                &[
                    (i32_type.const_int(ENDED_OK, false), ok),
                    (i32_type.const_int(ENDED_FAILED, false), failed),
                    (i32_type.const_int(ENDED_PANICKED, false), panicked),
                ],
            )
            .expect(POSITIONED);

        self.builder.position_at_end(ok);
        let value = self.joined_value(task, result, &value_type);
        let value = self.construct_values(&joined, "Ok", value.into_iter().collect());
        self.builder.build_store(slot, value).expect(POSITIONED);
        self.branch(done);

        let [text, table, boxed] = parts;
        self.builder.position_at_end(cancelled);
        let why = self.reason_value(reason, text);
        self.stopped(&joined, &stopped, ("Cancelled", why), (slot, done));
        self.builder.position_at_end(failed);
        let pair = self.pair(self.load_ptr(table), self.load_ptr(boxed));
        self.stopped(&joined, &stopped, ("Failed", pair.into()), (slot, done));
        self.builder.position_at_end(panicked);
        let message = self.load_ptr(text);
        self.stopped(
            &joined,
            &stopped,
            ("Panicked", message.into()),
            (slot, done),
        );

        self.builder.position_at_end(done);
        self.builder
            .build_load(llvm_type(self.context, &joined), slot, "joined")
            .expect(POSITIONED)
    }

    /// A copy of a finished task's value: its result, or the result's `Ok`
    /// value when it returns a `Result`. None for `()`.
    fn joined_value(
        &mut self,
        task: BasicValueEnum<'ctx>,
        result: &Type,
        value_type: &Type,
    ) -> Option<BasicValueEnum<'ctx>> {
        if matches!(result, Type::Unit | Type::Never) {
            return None;
        }
        let at = self
            .rt(
                "lugha_rt_task_join",
                &[Type::String],
                Some(Type::String),
                &[task],
            )
            .expect("returns the result's address")
            .into_pointer_value();
        let value = match result_parts(result) {
            Some(_) => {
                let (_, payloads) = self.variant(result, "Ok");
                self.load_payload(result, at, &payloads, 0, value_type)
            }
            None => self.load_element(at, result),
        };
        if matches!(value_type, Type::Unit) {
            return None;
        }
        // A copy site: joining again gives another copy. The value crosses
        // back from the task: marked shared.
        self.rc_mark(value, value_type);
        Some(self.deep_copy(value, value_type))
    }

    /// Stores `Err(Stopped.<variant>(payload))` at `slot`, then on to `done`.
    fn stopped(
        &mut self,
        joined: &Type,
        stopped: &Type,
        (variant, payload): (&str, BasicValueEnum<'ctx>),
        (slot, done): (PointerValue<'ctx>, inkwell::basic_block::BasicBlock<'ctx>),
    ) {
        let value = self.construct_values(stopped, variant, vec![payload]);
        let err = self.construct_values(joined, "Err", vec![value]);
        self.builder.build_store(slot, err).expect(POSITIONED);
        self.branch(done);
    }

    /// The `Reason` a cancelled task's `reason` number and `text` make.
    fn reason_value(
        &mut self,
        reason: PointerValue<'ctx>,
        text: PointerValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let ty = Type::Enum("$prelude.Reason".to_string(), Vec::new());
        let number = self
            .builder
            .build_load(self.context.i32_type(), reason, "reason")
            .expect(POSITIONED)
            .into_int_value();
        let slot = self.entry_alloca(&ty, "reason");
        let made = self.append("reason.made");
        let [sibling, parent, requested] = [
            self.append("reason.sibling"),
            self.append("reason.parent"),
            self.append("reason.requested"),
        ];
        let i32_type = self.context.i32_type();
        self.builder
            .build_switch(
                number,
                requested,
                &[
                    (i32_type.const_int(SIBLING_FAILED, false), sibling),
                    (i32_type.const_int(PARENT, false), parent),
                ],
            )
            .expect(POSITIONED);
        for (block, variant) in [(sibling, "SiblingFailed"), (parent, "Parent")] {
            self.builder.position_at_end(block);
            let value = self.construct_values(&ty, variant, Vec::new());
            self.builder.build_store(slot, value).expect(POSITIONED);
            self.branch(made);
        }
        self.builder.position_at_end(requested);
        let why = self.load_ptr(text);
        let value = self.construct_values(&ty, "Requested", vec![why.into()]);
        self.builder.build_store(slot, value).expect(POSITIONED);
        self.branch(made);
        self.builder.position_at_end(made);
        self.builder
            .build_load(llvm_type(self.context, &ty), slot, "reason")
            .expect(POSITIONED)
    }

    fn load_ptr(&self, at: PointerValue<'ctx>) -> PointerValue<'ctx> {
        let ptr = self.context.ptr_type(Default::default());
        self.builder
            .build_load(ptr, at, "part")
            .expect(POSITIONED)
            .into_pointer_value()
    }

    /// In task entry `entry`, after the task's `value` (of type `result`)
    /// is stored: an `Err` fails the task, its error handed to the runtime
    /// as an `Error` value with a holder of its own.
    pub(super) fn report_failure(
        &mut self,
        entry: FunctionValue<'ctx>,
        result: &Type,
        value: BasicValueEnum<'ctx>,
    ) {
        let Some((_, error)) = result_parts(result) else {
            return;
        };
        let error = error.clone();
        let saved = self.function.replace(entry);
        let slot = self.entry_alloca(result, "returned");
        self.builder.build_store(slot, value).expect(POSITIONED);
        let tag = self.tag(result, slot);
        let (err_tag, payloads) = self.variant(result, "Err");
        let is_err: IntValue = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                tag,
                self.context.i32_type().const_int(u64::from(err_tag), false),
                "failed",
            )
            .expect(POSITIONED);
        let (fails, done) = (self.append("task.failed"), self.append("task.done"));
        self.builder
            .build_conditional_branch(is_err, fails, done)
            .expect(POSITIONED);
        self.builder.position_at_end(fails);
        let e = self.load_payload(result, slot, &payloads, 0, &error);
        self.rc_retain(e, &error);
        let pair = if error == error_type() {
            e.into_struct_value()
        } else {
            self.box_as(e, &error, &error_type()).into_struct_value()
        };
        let table = self
            .builder
            .build_extract_value(pair, 0, "table")
            .expect("a pair has 2 fields");
        let boxed = self
            .builder
            .build_extract_value(pair, 1, "box")
            .expect("a pair has 2 fields");
        let params = [Type::String, Type::String];
        self.rt("lugha_rt_task_failed", &params, None, &[table, boxed]);
        self.branch(done);
        self.builder.position_at_end(done);
        self.function = saved;
    }
}
