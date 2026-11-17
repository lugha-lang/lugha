//! Scope settings and failures in codegen.
//!
//! A scope's settings become the arguments of `lugha_rt_scope_enter`: its
//! name, its policy (all-or-nothing, `independent`, `independent(f)` with
//! the handler's code and environment) and its `limit` (-1 for none). A
//! failed scope's `Failure { task, kind }` is built from what
//! `lugha_rt_scope_exit` hands over. A task fails in its own entry: a
//! returned `Err`, or a panic its entry caught, becomes a `Failure` there,
//! handed to the runtime and, as the scope's policy says, printed or given
//! to the scope's handler.

use inkwell::IntPredicate;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::rc::RcOp;
use super::value::llvm_type;
use crate::ast::ScopeExpr;
use crate::check::Type;

/// `lugha_rt_task_policy`'s answers (`runtime/scope_policy.c`).
const POLICY_PRINT: u64 = 1;
const POLICY_HANDLER: u64 = 2;
const POLICY_ROOT: u64 = 3;

/// How a task failed, in its entry.
pub(super) enum TaskFailure<'ctx> {
    /// It returned `Err` with this value, of this type.
    Error(BasicValueEnum<'ctx>, Type),
    /// It panicked with this message (a new holder).
    Panic(PointerValue<'ctx>),
}

/// The prelude's `Error` interface, as a type.
fn error_type() -> Type {
    Type::Interface("$prelude.Error".to_string(), Vec::new())
}

/// `Failure<e>` and `FailureKind<e>`.
fn failure_types(e: &Type) -> (Type, Type) {
    (
        Type::Struct("$prelude.Failure".to_string(), vec![e.clone()]),
        Type::Enum("$prelude.FailureKind".to_string(), vec![e.clone()]),
    )
}

impl<'ctx> Lowerer<'ctx> {
    /// The scope's settings as `lugha_rt_scope_enter`'s arguments after the
    /// record: its name, policy, handler code and environment, and limit.
    pub(super) fn scope_settings(
        &mut self,
        scope: &ScopeExpr,
    ) -> Result<Vec<BasicValueEnum<'ctx>>, CodegenError> {
        let ptr = self.context.ptr_type(Default::default());
        let i32_type = self.context.i32_type();
        let name = self.string_literal(&scope.name.name);
        let (mut policy, mut code, mut env) = (0, ptr.const_null(), ptr.const_null());
        let mut limit = self.context.i64_type().const_all_ones();
        for w in &scope.words {
            match (w.name.name.as_str(), &w.arg) {
                ("independent", None) => policy = POLICY_PRINT,
                ("independent", Some(f)) => {
                    policy = POLICY_HANDLER;
                    let pair = self.get(f, &self.ty(f))?.into_struct_value();
                    let part = |i| self.builder.build_extract_value(pair, i, "handler");
                    code = part(0).expect("a pair has 2 fields").into_pointer_value();
                    env = part(1).expect("a pair has 2 fields").into_pointer_value();
                }
                ("limit", Some(n)) => limit = self.get(n, &Type::I64)?.into_int_value(),
                _ => unreachable!("checked: E0477 for any other setting"),
            }
        }
        Ok(vec![
            name.into(),
            i32_type.const_int(policy, false).into(),
            code.into(),
            env.into(),
            limit.into(),
        ])
    }

    /// The `Failure` of type `failure` that a failed scope's `status` and
    /// handed-over `parts` (the error's table and box, a panic's message,
    /// the failing task's name) make.
    pub(super) fn scope_failure(
        &mut self,
        failure: &Type,
        status: inkwell::values::IntValue<'ctx>,
        [table, boxed, panic, task]: [PointerValue<'ctx>; 4],
    ) -> BasicValueEnum<'ctx> {
        let Type::Struct(_, args) = failure else {
            unreachable!("checked: a `Failure` struct")
        };
        let e = args[0].clone();
        let kind_type = failure_types(&e).1;
        let ptr = self.context.ptr_type(Default::default());
        let load = |this: &Self, at: PointerValue<'ctx>| {
            this.builder
                .build_load(ptr, at, "part")
                .expect(POSITIONED)
                .into_pointer_value()
        };
        let is_panic = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                status,
                self.context
                    .i32_type()
                    .const_int(super::task_scopes::ENDED_PANICKED, false),
                "panicked",
            )
            .expect(POSITIONED);
        let slot = self.entry_alloca(&kind_type, "failure.kind");
        let (panicked, errored) = (self.append("failure.panic"), self.append("failure.error"));
        let made = self.append("failure.made");
        self.builder
            .build_conditional_branch(is_panic, panicked, errored)
            .expect(POSITIONED);
        self.builder.position_at_end(panicked);
        let message = load(self, panic);
        let kind = self.construct_values(&kind_type, "Panic", vec![message.into()]);
        self.builder.build_store(slot, kind).expect(POSITIONED);
        self.branch(made);
        self.builder.position_at_end(errored);
        let value = self.handed_error(&e, load(self, table), load(self, boxed));
        let kind = self.construct_values(&kind_type, "Error", vec![value]);
        self.builder.build_store(slot, kind).expect(POSITIONED);
        self.branch(made);
        self.builder.position_at_end(made);
        let kind = self
            .builder
            .build_load(llvm_type(self.context, &kind_type), slot, "kind")
            .expect(POSITIONED);
        let name = load(self, task);
        self.failure_struct(failure, name, kind)
    }

    /// The error of type `e` a scope was handed: an interface value's table
    /// and box, or a box holding the value, let go of once it is read.
    fn handed_error(
        &mut self,
        e: &Type,
        table: PointerValue<'ctx>,
        boxed: PointerValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        if matches!(e, Type::Interface(..)) {
            return self.pair(table, boxed).into();
        }
        let value = self.load_element(boxed, e);
        self.rc_retain(value, e);
        let release = self.runtime("lugha_rt_rc_release", &[Type::String], None);
        self.builder
            .build_call(release, &[boxed.into()], "")
            .expect(POSITIONED);
        value
    }

    /// `Failure { task: name, kind }` of type `failure`, both handed over.
    fn failure_struct(
        &mut self,
        failure: &Type,
        name: PointerValue<'ctx>,
        kind: BasicValueEnum<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let shape = llvm_type(self.context, failure).into_struct_type();
        let value = self
            .builder
            .build_insert_value(shape.get_undef(), name, 0, "task")
            .expect("a `Failure` has 2 fields");
        self.builder
            .build_insert_value(value, kind, 1, "kind")
            .expect("a `Failure` has 2 fields")
            .into_struct_value()
            .into()
    }

    /// In task entry `entry`, whose scope's tasks fail with `e`: the task
    /// failed. The runtime records it (failing an all-or-nothing scope or
    /// the root); then, as the scope's policy says, it is printed or given
    /// to the scope's handler.
    pub(super) fn task_failure(
        &mut self,
        entry: FunctionValue<'ctx>,
        e: &Type,
        how: TaskFailure<'ctx>,
    ) {
        let saved = self.function.replace(entry);
        let ptr = Type::String;
        let i32_type = self.context.i32_type();
        let policy = self
            .rt("lugha_rt_task_policy", &[], Some(Type::I32), &[])
            .expect("returns the policy")
            .into_int_value();
        let is = |this: &Self, p: u64, name: &str| {
            this.builder
                .build_int_compare(IntPredicate::EQ, policy, i32_type.const_int(p, false), name)
                .expect(POSITIONED)
        };
        // All-or-nothing (0) and the root fail the scope.
        let all = self
            .builder
            .build_int_compare(IntPredicate::EQ, policy, i32_type.const_zero(), "all")
            .expect(POSITIONED);
        let fails = self
            .builder
            .build_or(all, is(self, POLICY_ROOT, "root"), "fails")
            .expect(POSITIONED);
        let fails = self
            .builder
            .build_int_z_extend(fails, i32_type, "fails")
            .expect(POSITIONED);
        let (failure_type, kind_type) = failure_types(e);
        let kind = match how {
            TaskFailure::Error(value, x) => {
                self.rc_retain(value, &x);
                let as_e = if matches!(e, Type::Interface(..)) && x != *e {
                    self.box_as(value, &x, e)
                } else {
                    value
                };
                // The runtime keeps a holder for `join` and the scope.
                let (table, boxed) = if matches!(e, Type::Interface(..)) {
                    let (t, b) = self.parts(as_e.into_struct_value());
                    self.rc_retain(as_e, e);
                    (t, b)
                } else {
                    self.rc_retain(as_e, e);
                    (
                        self.context.ptr_type(Default::default()).const_null(),
                        self.new_box(e, as_e),
                    )
                };
                let args = [table.into(), boxed.into(), fails.into()];
                self.rt(
                    "lugha_rt_task_failed",
                    &[ptr.clone(), ptr.clone(), Type::I32],
                    None,
                    &args,
                );
                self.construct_values(&kind_type, "Error", vec![as_e])
            }
            TaskFailure::Panic(text) => {
                self.rc_retain(text.into(), &Type::String);
                self.rt(
                    "lugha_rt_task_panicked",
                    &[ptr.clone(), Type::I32],
                    None,
                    &[text.into(), fails.into()],
                );
                self.construct_values(&kind_type, "Panic", vec![text.into()])
            }
        };
        let name = self
            .rt("lugha_rt_task_own_name", &[], Some(ptr.clone()), &[])
            .expect("returns the name")
            .into_pointer_value();
        let failure = self.failure_struct(&failure_type, name, kind);
        let (report, handler, done) = (
            self.append("failure.report"),
            self.append("failure.handler"),
            self.append("failure.done"),
        );
        let cases = [
            (i32_type.const_int(POLICY_PRINT, false), report),
            (i32_type.const_int(POLICY_ROOT, false), report),
            (i32_type.const_int(POLICY_HANDLER, false), handler),
        ];
        self.builder
            .build_switch(policy, done, &cases)
            .expect(POSITIONED);
        self.builder.position_at_end(report);
        self.report_failure_value(failure, e);
        self.branch(done);
        self.builder.position_at_end(handler);
        self.call_handler(failure, &failure_type);
        self.branch(done);
        self.builder.position_at_end(done);
        self.rc_release(failure, &failure_type);
        self.function = saved;
    }

    /// Prints `failure` (of `Failure<e>`) through the prelude's
    /// `$report_failure`, as a `Failure<Error>`.
    fn report_failure_value(&mut self, failure: BasicValueEnum<'ctx>, e: &Type) {
        let plain = failure_types(&error_type()).0;
        let shown = if *e == error_type() {
            self.rc_retain(failure, &plain);
            failure
        } else {
            self.as_plain_failure(failure, e)
        };
        let function = self.monos["$prelude.$report_failure"].signature.function;
        let at = self.entry_alloca(&plain, "report");
        self.builder.build_store(at, shown).expect(POSITIONED);
        self.builder
            .build_call(function, &[at.into()], "")
            .expect(POSITIONED);
        self.rc_value_at(at, &plain, RcOp::Release);
    }

    /// `failure`, a `Failure<e>`, as a new `Failure<Error>`: its error boxed
    /// as an `Error` value.
    fn as_plain_failure(
        &mut self,
        failure: BasicValueEnum<'ctx>,
        e: &Type,
    ) -> BasicValueEnum<'ctx> {
        let kind_type = failure_types(e).1;
        let (plain, plain_kind) = failure_types(&error_type());
        let whole = failure.into_struct_value();
        let name = self
            .builder
            .build_extract_value(whole, 0, "task")
            .expect("a `Failure` has 2 fields");
        self.rc_retain(name, &Type::String);
        let kind = self
            .builder
            .build_extract_value(whole, 1, "kind")
            .expect("a `Failure` has 2 fields");
        let at = self.entry_alloca(&kind_type, "kind");
        self.builder.build_store(at, kind).expect(POSITIONED);
        let tag = self.tag(&kind_type, at);
        let (error_tag, payloads) = self.variant(&kind_type, "Error");
        let slot = self.entry_alloca(&plain_kind, "plain.kind");
        let (errored, panicked, made) = (
            self.append("plain.error"),
            self.append("plain.panic"),
            self.append("plain.made"),
        );
        let is_error = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                tag,
                self.context
                    .i32_type()
                    .const_int(u64::from(error_tag), false),
                "error",
            )
            .expect(POSITIONED);
        self.builder
            .build_conditional_branch(is_error, errored, panicked)
            .expect(POSITIONED);
        self.builder.position_at_end(errored);
        let value = self.load_payload(&kind_type, at, &payloads, 0, e);
        self.rc_retain(value, e);
        let boxed = self.box_as(value, e, &error_type());
        let k = self.construct_values(&plain_kind, "Error", vec![boxed]);
        self.builder.build_store(slot, k).expect(POSITIONED);
        self.branch(made);
        self.builder.position_at_end(panicked);
        let (_, panic_payloads) = self.variant(&kind_type, "Panic");
        let text = self.load_payload(&kind_type, at, &panic_payloads, 0, &Type::String);
        self.rc_retain(text, &Type::String);
        let k = self.construct_values(&plain_kind, "Panic", vec![text]);
        self.builder.build_store(slot, k).expect(POSITIONED);
        self.branch(made);
        self.builder.position_at_end(made);
        let k = self
            .builder
            .build_load(llvm_type(self.context, &plain_kind), slot, "kind")
            .expect(POSITIONED);
        self.failure_struct(&plain, name.into_pointer_value(), k)
    }

    /// Calls the scope's handler with `failure`, of type `failure_type`.
    fn call_handler(&mut self, failure: BasicValueEnum<'ctx>, failure_type: &Type) {
        let ptr = self.context.ptr_type(Default::default());
        let code = self.entry_alloca(&Type::String, "handler.code");
        let env = self.entry_alloca(&Type::String, "handler.env");
        self.rt(
            "lugha_rt_task_handler",
            &[Type::String, Type::String],
            None,
            &[code.into(), env.into()],
        );
        let load = |this: &Self, at: PointerValue<'ctx>| {
            this.builder.build_load(ptr, at, "part").expect(POSITIONED)
        };
        let shape = self.context.struct_type(&[ptr.into(), ptr.into()], false);
        let pair = self
            .builder
            .build_insert_value(shape.get_undef(), load(self, code), 0, "code")
            .expect("a pair has 2 fields");
        let pair = self
            .builder
            .build_insert_value(pair, load(self, env), 1, "env")
            .expect("a pair has 2 fields")
            .into_struct_value();
        let at = self.entry_alloca(failure_type, "failure");
        self.builder.build_store(at, failure).expect(POSITIONED);
        let arg: BasicMetadataValueEnum = at.into();
        self.call_pair(
            pair,
            (std::slice::from_ref(failure_type), &Type::Unit),
            vec![arg],
        );
    }
}
