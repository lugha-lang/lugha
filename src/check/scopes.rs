//! Scopes in the checker: `scope s { … }` is a `Result<T, Failure>`, its
//! body checked with `s` bound to a `Scope`; `s.spawn(call)` needs a scope
//! there; and scope and task handles are second-class (E0472): passed as
//! arguments, never returned, stored, sent or captured.

use super::expr::Expect;
use super::results::RESULT;
use super::{Binding, Checker, Type, errors};
use crate::ast::{Expr, ScopeExpr};
use crate::span::Span;

/// The prelude's `Scope`, a scope's handle.
pub(super) const SCOPE: &str = "$prelude.Scope";
/// The prelude's `Failure`, a failed scope's error.
pub(super) const FAILURE: &str = "$prelude.Failure";
/// The prelude's `Stopped`, a task's end without its value.
pub(super) const STOPPED: &str = "$prelude.Stopped";
/// The prelude's `Reason`, why a task was cancelled.
pub(super) const REASON: &str = "$prelude.Reason";

/// The prelude's `Error` interface, as a type.
pub(super) fn error_type() -> Type {
    Type::Interface(super::impls::ERROR.to_string(), Vec::new())
}

/// The type of the handle of a scope whose tasks' errors are `e`.
pub(super) fn scope_type(e: Type) -> Type {
    Type::Struct(SCOPE.to_string(), vec![e])
}

/// A failure of a scope whose tasks' errors are `e`.
pub(super) fn failure_type(e: Type) -> Type {
    Type::Struct(FAILURE.to_string(), vec![e])
}

/// `Scope` or `Failure` written without arguments: the plain scope's, `<Error>`.
pub(super) fn with_error_default(key: &str) -> Option<Type> {
    match key {
        SCOPE | FAILURE => Some(Type::Struct(key.to_string(), vec![error_type()])),
        _ => None,
    }
}

/// What `t.join()` gives for a task returning `result`: `Result<V,
/// Stopped>`, `V` being the result, or its `Ok` value when it is a
/// `Result` (whose `Err` the task failed with).
pub(super) fn joined(result: &Type) -> Type {
    let value = match result {
        Type::Enum(k, args) if k == RESULT && args.len() == 2 => args[0].clone(),
        Type::Never => Type::Unit,
        other => other.clone(),
    };
    Type::Enum(
        RESULT.to_string(),
        vec![value, Type::Enum(STOPPED.to_string(), Vec::new())],
    )
}

/// True if `ty` is a scope or task handle itself.
fn is_handle(ty: &Type) -> bool {
    matches!(ty, Type::Task(_)) || matches!(ty, Type::Struct(k, _) if k == SCOPE)
}

/// True if a handle is anywhere in `ty`, itself included.
pub(super) fn holds_handle(ty: &Type) -> bool {
    is_handle(ty) || nested_handle(ty)
}

/// True if a handle is inside `ty`, as an element, payload, field type
/// argument or result: somewhere a handle can't go. A function's
/// parameter may be one: a handle is passed as an argument.
fn nested_handle(ty: &Type) -> bool {
    match ty {
        Type::Task(_) => false,
        Type::Fun(params, ret) => holds_handle(ret) || params.iter().any(nested_handle),
        Type::Array(t) | Type::Option(t) | Type::Ref(t) | Type::Chan(_, t) => holds_handle(t),
        // Tuples are structs, their elements type arguments.
        Type::Struct(_, ts) | Type::Enum(_, ts) | Type::Interface(_, ts) => {
            ts.iter().any(holds_handle)
        }
        _ => false,
    }
}

impl Checker {
    /// `scope s { body }`: the body's value once every task is done, or the
    /// first failure.
    pub(super) fn scope_expr(&mut self, scope: &ScopeExpr, expect: Option<&Expect>) -> Type {
        self.uses_tasks = true;
        let inner = match expect.map(|e| &e.ty) {
            Some(Type::Enum(k, args)) if k == RESULT && args.len() == 2 => {
                Some(Expect::of(args[0].clone()))
            }
            _ => None,
        };
        let e = self.scope_error(scope);
        self.scope_words(scope, &e);
        self.push();
        self.declare(
            &scope.name,
            scope_type(e.clone()),
            Binding::Let { mutable: false },
        );
        let body = self.block(&scope.body, inner.as_ref());
        self.pop();
        let body = if body == Type::Never {
            Type::Unit
        } else {
            body
        };
        Type::Enum(RESULT.to_string(), vec![body, failure_type(e)])
    }

    /// A task returning `ret`, started in a scope whose tasks' errors are
    /// `scope_error`: when it returns a `Result`, its error must be
    /// `scope_error`, or convert to it as `?` converts (E0478); the
    /// conversion's table is made where the task ends.
    pub(super) fn task_error(&mut self, call: &Expr, ret: &Type, scope_error: &Type) {
        let Type::Enum(k, args) = ret else { return };
        if k != RESULT || args.len() != 2 || matches!(args[1], Type::Error) {
            return;
        }
        let e = &args[1];
        if e.fits(scope_error) || *scope_error == Type::Error {
            return;
        }
        let converts = match super::applied::Applied::of(scope_error) {
            Some(interface) if !matches!(e, Type::Interface(..)) => {
                let ok = self.unmet(e, &interface).is_empty();
                if ok {
                    self.conversion_uses(call, e, &interface);
                }
                ok
            }
            Some(interface) => self
                .undeclared(
                    &super::applied::Applied::of(e).expect("an interface"),
                    &interface,
                )
                .is_empty(),
            None => false,
        };
        if !converts {
            self.report(errors::task_error_unconverted(e, scope_error, call.span));
        }
    }

    /// `s.spawn(call)` starts a task, and `s.spawn(Counter { … })` an
    /// actor; `spawn` without a scope is E0474.
    pub(super) fn spawn_in(
        &mut self,
        expr: &Expr,
        scope: Option<&Expr>,
        closure: &Expr,
        expect: Option<&Expect>,
    ) -> Type {
        let scope_error = match scope {
            Some(scope) => self.spawn_scope(scope),
            None => error_type(),
        };
        let ty = self.spawn(closure, expect, &scope_error);
        // A task's failure may be printed: by an `independent` scope, or the root.
        if scope.is_some() {
            self.used(expr, "$prelude.$report_failure", &[]);
        }
        match (&ty, scope) {
            (Type::Actor(_), None) => {
                self.report(errors::unscoped_actor(expr.span));
                Type::Error
            }
            (Type::Task(_), None) => {
                self.report(errors::unscoped_task(expr.span));
                Type::Error
            }
            _ => ty,
        }
    }

    /// The scope `s` in `s.spawn(…)`: a `Scope`, or E0473.
    /// Its tasks' error type, `Error` for a plain scope.
    fn spawn_scope(&mut self, scope: &Expr) -> Type {
        let ty = self.expr(scope, None);
        match &ty {
            Type::Struct(k, args) if k == SCOPE => args.first().cloned().unwrap_or_else(error_type),
            Type::Error => Type::Error,
            _ => {
                self.report(errors::not_a_scope(&ty, scope.span));
                Type::Error
            }
        }
    }

    /// A value of type `ty` made by `expr`: a handle inside it (an array of
    /// tasks, a tuple holding a scope, a closure returning one) is E0472.
    pub(super) fn second_class(&mut self, span: Span, ty: &Type) {
        if nested_handle(ty) {
            self.report(errors::handle_kept(
                "a scope or task handle can't be stored in a value",
                span,
            ));
        }
    }

    /// A field or payload of type `ty`: a handle there is E0472.
    pub(super) fn stored_handle(&mut self, ty: &Type, span: Span) {
        if holds_handle(ty) {
            self.report(errors::handle_kept(
                "a scope or task handle can't be stored in a field",
                span,
            ));
        }
    }

    /// `name`, a local of type `ty`, captured by a closure: E0472 for a
    /// handle, which the closure could outlive or carry to another task.
    pub(super) fn captured_handle(&mut self, ty: &Type, span: Span) {
        if holds_handle(ty) {
            self.report(errors::handle_kept(
                "a scope or task handle can't be captured by a closure or sent to another task",
                span,
            ));
        }
    }

    /// A function's declared result holding a handle: E0472.
    pub(super) fn returned_handle(&mut self, ty: &Type, span: Span) {
        if holds_handle(ty) {
            self.report(errors::handle_kept(
                "a scope or task handle can't be returned",
                span,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type, ok};

    const WORK: &str = "fun work(n: i64): i64 = n * n;\n";

    #[test]
    fn a_scope_is_a_result_of_its_body() {
        let src = format!("{WORK}fun main() {{ let r = scope s {{ s.spawn(work(1)); 2 }}; }}");
        let error = Type::Interface("$prelude.Error".into(), Vec::new());
        let failure = Type::Struct("$prelude.Failure".into(), vec![error]);
        assert_eq!(
            let_type(&src, "r"),
            Type::Enum("$prelude.Result".into(), vec![Type::I64, failure])
        );
        ok(&format!(
            "{WORK}fun main(root: Scope) {{ root.spawn(work(1)); let scope = 3; }}"
        ));
        ok(&format!(
            "{WORK}fun f(): Result<i64, Error> {{ let n = scope s {{ s.spawn(work(1)).join()? }}?; Ok(n) }}\nfun main() {{}}"
        ));
    }

    #[test]
    fn spawn_needs_a_scope_and_a_task_needs_spawn_through_one() {
        assert_eq!(
            errors(&format!(
                "{WORK}fun main() {{ let x = 1; x.spawn(work(1)); }}"
            )),
            [("E0473", "x")]
        );
        assert_eq!(
            errors(&format!("{WORK}fun main() {{ let t = spawn work(1); }}")),
            [("E0474", "spawn work(1)")]
        );
    }

    #[test]
    fn handles_go_only_where_arguments_go() {
        let take = "fun take(s: Scope, t: Task<i64>) {}\n";
        ok(&format!(
            "{WORK}{take}fun main() {{ let _ = scope s {{ let t = s.spawn(work(1)); take(s, t); }}; }}"
        ));
        for (bad, at) in [
            ("fun keep(s: Scope): Scope = s;", "Scope"),
            ("fun all(t: Task<i64>): Task<i64>[] = [t];", "Task<i64>[]"),
            ("struct Keeps { t: Task<i64> }", "Task<i64>"),
        ] {
            let codes: Vec<_> = errors(&format!("{WORK}{bad}\nfun main() {{}}"))
                .into_iter()
                .map(|(c, _)| c)
                .collect();
            assert!(codes.contains(&"E0472"), "{bad}: {codes:?} at {at}");
        }
        assert_eq!(
            errors(&format!(
                "{WORK}fun main() {{ let _ = scope s {{ let ts = [s.spawn(work(1))]; }}; }}"
            )),
            [("E0472", "[s.spawn(work(1))]")]
        );
        assert_eq!(
            errors(&format!(
                "{WORK}fun main() {{ let _ = scope s {{ let f = fun() {{ s.spawn(work(1)); }}; }}; }}"
            )),
            [("E0472", "s")]
        );
    }

    #[test]
    fn a_task_error_must_be_an_error() {
        let src = "struct Oops { code: i64 }\nfun bad(): Result<i64, Oops> = Ok(1);\n\
                   fun main() { let _ = scope s { s.spawn(bad()); }; }";
        assert_eq!(errors(src)[0].0, "E0478");
    }
}
