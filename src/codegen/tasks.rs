//! Green threads in codegen. `spawn call` lowers the
//! closure the parser wrapped the call in, and starts it through
//! `lugha_rt_task_spawn` with an entry `lugha_task_<enclosing>.<n>` that
//! calls it and stores the result. The call's arguments are evaluated
//! first, by the spawning thread: each becomes a hidden
//! capture, and the closure body reads it instead of evaluating the
//! argument again. `join` copies the stored result out. In a program with
//! `spawn`, every function entry and loop iteration counts down the running
//! task's budget, the top word of its stack, and calls
//! `lugha_rt_task_preempt` at zero.

use inkwell::IntPredicate;
use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, FunctionValue, ValueKind};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::scope::Local;
use super::value::Value;
use crate::ast::{Expr, ExprKind, spawned_call};
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// `s.spawn(call)`: a new green thread started in scope `s`, its
    /// `Task` handle the value; `spawn Counter { … }`, an actor.
    pub(super) fn spawn(
        &mut self,
        expr: &Expr,
        scope: Option<&Expr>,
        closure: &Expr,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let Some(scope) = scope else {
            unreachable!("checked: a task or an actor starts only in a scope")
        };
        // What its tasks fail with: `E` in `Scope<E>`.
        let scope_error = match self.ty(scope) {
            Type::Struct(_, args) => args[0].clone(),
            _ => unreachable!("checked: a scope's handle"),
        };
        let handle = self.get(scope, &self.ty(scope))?.into_struct_value();
        let scope = self
            .builder
            .build_extract_value(handle, 0, "scope")
            .expect("a scope has 1 field");
        // `s.spawn(Counter { … })` starts an actor, a task of the scope.
        if let Type::Actor(key, targs) = self.ty(expr) {
            let state = Type::Struct(key, targs);
            return self.spawn_actor(&state, spawned_call(closure), scope, &scope_error);
        }
        let Type::Task(result) = self.ty(expr) else {
            unreachable!("checked: `spawn` makes a `Task` or an actor")
        };
        let ExprKind::Closure(c) = &closure.kind else {
            unreachable!("the parser wraps a spawned call in a closure")
        };
        self.scopes.push();
        let mut hidden = Vec::new();
        for early in evaluated_first(spawned_call(closure)) {
            let ty = self
                .conversions
                .get(&early.id)
                .cloned()
                .unwrap_or_else(|| self.ty(early));
            let value = self.get(early, &ty)?;
            let name = format!("$spawn.{}", early.id.0);
            let ptr = self.entry_alloca(&ty, &name);
            self.builder.build_store(ptr, value).expect(POSITIONED);
            let local = Local {
                ptr,
                ty,
                borrowed: false,
            };
            self.scopes.declare(&name, local);
            hidden.push((early.id, name));
        }
        // Only now: while they were evaluated above, they were themselves.
        self.presets.extend(hidden.iter().cloned());
        let names: Vec<String> = hidden.iter().map(|(_, n)| n.clone()).collect();
        let pair = self.closure_value_with(closure, c, &names);
        for (id, _) in &hidden {
            self.presets.remove(id);
        }
        self.scopes.pop();
        let pair = pair?.into_struct_value();
        // Its name, for its failures: the called function's, until `name`.
        let named = task_name(spawned_call(closure));
        self.start_task(scope, &scope_error, pair, &result, named)
    }

    /// Starts a task in the scope `scope` (its record) running the closure
    /// `pair`, which returns `result`; `named` is its name for failures.
    pub(super) fn start_task(
        &mut self,
        scope: BasicValueEnum<'ctx>,
        scope_error: &Type,
        pair: inkwell::values::StructValue<'ctx>,
        result: &Type,
        named: &str,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        // The closure, and what its arguments
        // reach, cross to the new task: marked shared, its holder the task's.
        let fun = Type::Fun(Vec::new(), Box::new(result.clone()));
        self.rc_mark(pair.into(), &fun);

        let entry = self.task_entry(result, scope_error);
        let code = self
            .builder
            .build_extract_value(pair, 0, "code")
            .expect("a pair has 2 fields");
        let env = self
            .builder
            .build_extract_value(pair, 1, "env")
            .expect("a pair has 2 fields");
        let size = match result {
            Type::Unit | Type::Never => 1,
            ty => self.element_size(ty).max(1),
        };
        let ptr = self.context.ptr_type(Default::default());
        let spawn = self.runtime(
            "lugha_rt_task_spawn_in",
            &[
                Type::String,
                Type::String,
                Type::String,
                Type::String,
                Type::I64,
            ],
            Some(Type::String),
        );
        let args = [
            scope.into(),
            entry.as_global_value().as_pointer_value().into(),
            code.into(),
            env.into(),
            self.context.i64_type().const_int(size, false).into(),
        ];
        let site = self
            .builder
            .build_call(spawn, &args, "task")
            .expect(POSITIONED);
        let ValueKind::Basic(task) = site.try_as_basic_value() else {
            unreachable!("lugha_rt_task_spawn_in returns the task")
        };
        debug_assert!(task.get_type() == ptr.into());
        let visit = match result {
            Type::Unit | Type::Never => None,
            ty => self.rc_box_drop(ty),
        };
        let visit = match visit {
            Some(f) => f.as_global_value().as_pointer_value(),
            None => ptr.const_null(),
        };
        let setup = self.runtime(
            "lugha_rt_rc_task_setup",
            &[Type::String, Type::String],
            None,
        );
        self.builder
            .build_call(setup, &[task.into(), visit.into()], "")
            .expect(POSITIONED);

        let named = self.string_literal(named);
        self.rt(
            "lugha_rt_task_name",
            &[Type::String, Type::String],
            None,
            &[task, named.into()],
        );
        Ok(task)
    }

    /// The value of a spawned call's argument the spawning thread already
    /// evaluated, inside the closure the new thread runs.
    pub(super) fn preset(&mut self, expr: &Expr) -> Option<BasicValueEnum<'ctx>> {
        let name = self.presets.get(&expr.id)?.clone();
        let local = self.scopes.lookup(&name);
        Some(self.load(&local, &name))
    }

    /// `lugha_task_<enclosing>.<n>(code, env, result)`: calls the closure
    /// `{ code, env }` and stores what it returns at `result`.
    fn task_entry(&mut self, result: &Type, scope_error: &Type) -> FunctionValue<'ctx> {
        let enclosing = self.function.expect("`spawn` is inside a function");
        let name = enclosing.get_name().to_string_lossy().into_owned();
        let base = name
            .strip_prefix("lugha_fn_")
            .or_else(|| name.strip_prefix("lugha_closure_"))
            .unwrap_or(&name)
            .to_string();
        let count = self
            .closure_counts
            .entry(format!("task {base}"))
            .or_insert(0);
        let symbol = format!("lugha_task_{base}.{count}");
        *count += 1;
        let ptr = self.context.ptr_type(Default::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr.into(), ptr.into(), ptr.into()], false);
        let entry = self
            .module
            .add_function(&symbol, fn_type, Some(Linkage::Internal));
        let saved_block = self.builder.get_insert_block();
        self.builder
            .position_at_end(self.context.append_basic_block(entry, "entry"));
        let param = |i: u32| {
            entry
                .get_nth_param(i)
                .expect("declared with three parameters")
        };
        let pair_type = self.context.struct_type(&[ptr.into(), ptr.into()], false);
        let pair = pair_type.get_undef();
        let pair = self
            .builder
            .build_insert_value(pair, param(0), 0, "code")
            .expect(POSITIONED);
        let pair = self
            .builder
            .build_insert_value(pair, param(1), 1, "env")
            .expect(POSITIONED)
            .into_struct_value();
        let ret = match result {
            Type::Never => Type::Unit,
            ty => ty.clone(),
        };
        let called = self.call_pair(pair, (&[], &ret), Vec::new());
        let call = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_last_instruction())
            .expect("the call was just built");
        if let Value::Val(value) = called {
            // The result crosses to its joiners: marked here, on its own
            // task, before any of them can see it.
            self.rc_mark(value, &ret);
            self.store_element(param(2).into_pointer_value(), &ret, value);
            self.report_failure(entry, &ret, value, scope_error);
        }
        self.builder.build_return(None).expect(POSITIONED);
        // A panic or cancellation stops here: the task's own failure.
        self.catch_task(entry, call, scope_error);
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        entry
    }

    /// `$os.sleep(ms)` and `$os.yield()` in `std.task`.
    pub(super) fn task_os_call(
        &mut self,
        op: &str,
        args: &[Expr],
    ) -> Result<Value<'ctx>, CodegenError> {
        match op {
            "sleep" => {
                let ms = self.get(&args[0], &Type::I64)?;
                let sleep = self.runtime("lugha_rt_task_sleep", &[Type::I64], None);
                self.builder
                    .build_call(sleep, &[ms.into()], "")
                    .expect(POSITIONED);
            }
            _ => {
                let yield_ = self.runtime("lugha_rt_task_yield", &[], None);
                self.builder.build_call(yield_, &[], "").expect(POSITIONED);
            }
        }
        Ok(Value::Void)
    }

    /// One step of the countdown, at a function's entry or a loop's head,
    /// in a program that spawns.
    pub(super) fn preempt_point(&mut self) {
        if !self.uses_tasks && !self.call_costs() {
            return;
        }
        let i64_type = self.context.i64_type();
        let at = self.budget_word();
        let left = self
            .builder
            .build_load(i64_type, at, "budget")
            .expect(POSITIONED)
            .into_int_value();
        let left = self
            .builder
            .build_int_sub(left, i64_type.const_int(1, false), "budget")
            .expect(POSITIONED);
        self.builder.build_store(at, left).expect(POSITIONED);
        let spent = self
            .builder
            .build_int_compare(IntPredicate::SLE, left, i64_type.const_zero(), "spent")
            .expect(POSITIONED);
        let (preempt, go_on) = (self.append("preempt"), self.append("preempt.done"));
        self.builder
            .build_conditional_branch(spent, preempt, go_on)
            .expect(POSITIONED);
        self.builder.position_at_end(preempt);
        let function = self.runtime("lugha_rt_task_preempt", &[], None);
        self.builder
            .build_call(function, &[], "")
            .expect(POSITIONED);
        self.branch(go_on);
        self.builder.position_at_end(go_on);
    }
}

impl<'ctx> Lowerer<'ctx> {
    /// The running task's budget: the word at `(sp | mask) - 15`, the top
    /// of whichever stack the code runs on (every one is aligned to its
    /// size, `lugha_rt_stack_mask` + 1). No thread's storage is read, so a
    /// task moved to another thread keeps counting its own.
    fn budget_word(&mut self) -> inkwell::values::PointerValue<'ctx> {
        let i64_type = self.context.i64_type();
        let sp = self.stack_pointer();
        let mask = self.stack_mask();
        let top = self
            .builder
            .build_or(sp, mask, "stack.top")
            .expect(POSITIONED);
        let word = self
            .builder
            .build_int_sub(top, i64_type.const_int(15, false), "budget.at")
            .expect(POSITIONED);
        self.builder
            .build_int_to_ptr(
                word,
                self.context.ptr_type(Default::default()),
                "budget.ptr",
            )
            .expect(POSITIONED)
    }

    /// The stack pointer, read where it is (`llvm.read_register`), which
    /// neither makes a frame nor counts as a side effect.
    fn stack_pointer(&mut self) -> inkwell::values::IntValue<'ctx> {
        let i64_type = self.context.i64_type();
        let name = match self.target {
            super::Target::X86_64Linux => "rsp",
            super::Target::Aarch64Linux => "sp",
        };
        let read = inkwell::intrinsics::Intrinsic::find("llvm.read_register")
            .and_then(|i| i.get_declaration(&self.module, &[i64_type.into()]))
            .expect("LLVM has llvm.read_register");
        let register = self
            .context
            .metadata_node(&[self.context.metadata_string(name).into()]);
        let site = self
            .builder
            .build_call(read, &[register.into()], "sp")
            .expect(POSITIONED);
        match site.try_as_basic_value() {
            ValueKind::Basic(v) => v.into_int_value(),
            ValueKind::Instruction(_) => unreachable!("llvm.read_register returns the register"),
        }
    }

    /// `lugha_rt_stack_mask`, a stack's size less one: set before any Lugha
    /// code runs and never changed.
    fn stack_mask(&mut self) -> inkwell::values::IntValue<'ctx> {
        let i64_type = self.context.i64_type();
        let global = match self.module.get_global("lugha_rt_stack_mask") {
            Some(global) => global,
            None => {
                let global = self
                    .module
                    .add_global(i64_type, None, "lugha_rt_stack_mask");
                global.set_linkage(Linkage::External);
                global
            }
        };
        self.builder
            .build_load(i64_type, global.as_pointer_value(), "stack.mask")
            .expect(POSITIONED)
            .into_int_value()
    }
}

/// A task's name before `t.name(…)`: the called function's simple name,
/// `closure` for a closure called on the spot.
fn task_name(call: &Expr) -> &str {
    let ExprKind::Call(callee, _) = &call.kind else {
        return "task";
    };
    match &callee.kind {
        ExprKind::Name(name) => name,
        ExprKind::Field(_, name) => &name.name,
        ExprKind::Closure(_) => "closure",
        _ => "task",
    }
}

/// The parts of a spawned call that the spawning thread evaluates first
/// (each is lowered through `expr`, where `preset` finds it): every
/// argument that computes something, a receiver that is a call
/// (`make().run()`), and a callee that is neither a name, a member nor a
/// closure literal (`fs[i](x)`, `pick()(x)`). Literals, names and closure
/// literals evaluate the same in either thread (captures are copies), so
/// they stay where they are.
fn evaluated_first(call: &Expr) -> Vec<&Expr> {
    let ExprKind::Call(callee, args) = &call.kind else {
        unreachable!("the parser checked that `spawn` has a call")
    };
    let mut early = Vec::new();
    match &callee.kind {
        ExprKind::Field(base, _) if matches!(base.kind, ExprKind::Call(..)) => early.push(&**base),
        ExprKind::Field(..) | ExprKind::Name(_) | ExprKind::Closure(_) => {}
        _ => early.push(&**callee),
    }
    early.extend(args.iter().filter(|a| computed(a)));
    early
}

/// Whether evaluating `e` could do something or differ by when it runs.
fn computed(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Char(_)
        | ExprKind::Str(_)
        | ExprKind::Name(_)
        | ExprKind::Closure(_) => false,
        // `-1` is one literal.
        ExprKind::Unary(crate::ast::UnOp::Neg, inner) => {
            !matches!(inner.kind, ExprKind::Int(_) | ExprKind::Float(_))
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::ir;

    #[test]
    fn spawn_starts_an_entry_that_calls_the_closure() {
        let ir = ir("fun work(n: i64): i64 = n * n;
            fun main() { let _ = scope s { let t = s.spawn(work(3)); let n = t.join(); }; }");
        assert!(
            ir.contains("define internal void @lugha_task_main.0(ptr"),
            "{ir}"
        );
        assert!(
            ir.contains("@lugha_rt_task_spawn_in(ptr %")
                && ir.contains("ptr @lugha_task_main.0, ptr @lugha_closure_main.0"),
            "{ir}"
        );
        assert!(ir.contains("@lugha_rt_task_join_status("), "{ir}");
    }

    #[test]
    fn arguments_are_evaluated_by_the_spawning_thread() {
        let ir = ir("fun next(): i64 = 1;\nfun work(n: i64): i64 = n;
            fun main() { let _ = scope s { let t = s.spawn(work(next())); }; }");
        let main = &ir[ir.find("define void @lugha_fn_main()").unwrap()..];
        let main = &main[..main.find("\n}\n").unwrap()];
        assert!(main.contains("call i64 @lugha_fn_next()"), "{main}");
        let body = &ir[ir
            .find("define internal i64 @lugha_closure_main.0")
            .unwrap()..];
        let body = &body[..body.find("\n}\n").unwrap()];
        assert!(!body.contains("@lugha_fn_next"), "{body}");
    }

    #[test]
    fn only_programs_that_spawn_count_down() {
        let plain = ir("fun main() { let mut i = 0; while i < 3 { i += 1; } }");
        assert!(!plain.contains("budget.at"), "{plain}");
        let spawning = ir(
            "fun f() {}\nfun main() { let _ = scope s { s.spawn(f()); }; let mut i = 0; while i < 3 { i += 1; } }",
        );
        assert!(
            spawning.contains("call void @lugha_rt_task_preempt()"),
            "{spawning}"
        );
        // The countdown is the stack's top word, found from the stack
        // pointer: no thread's storage, nothing a moved task could miss.
        assert!(spawning.contains("@llvm.read_register.i64"), "{spawning}");
        assert!(
            spawning.contains("%budget.at = sub i64 %stack.top, 15"),
            "{spawning}"
        );
        assert!(!spawning.contains("lugha_rt_task_budget"), "{spawning}");
    }
}
