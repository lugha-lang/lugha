//! A scope's settings, the words between its name and its body:
//! `independent`, `independent(f)` and `limit n` (E0477 for any other, or a
//! repeat); and `scope<E>`, the error type its tasks fail with.

use super::expr::Expect;
use super::scopes::{error_type, failure_type};
use super::{Checker, Type, errors};
use crate::ast::ScopeExpr;

impl Checker {
    /// `E` in `scope<E> s`, else `Error`. It must be an error (E0422): a
    /// failed scope is one, and its value crosses from the tasks.
    pub(super) fn scope_error(&mut self, scope: &ScopeExpr) -> Type {
        let Some(written) = &scope.error else {
            return error_type();
        };
        let e = self.resolve(written);
        if e == Type::Error || e == error_type() {
            return e;
        }
        let error = super::applied::Applied::of(&error_type()).expect("an interface");
        if !matches!(e, Type::Interface(..)) && !self.satisfies(&e, &error, written.span) {
            return Type::Error;
        }
        e
    }

    /// The scope's words: each known one at most once, with its value
    /// checked (E0477 otherwise).
    pub(super) fn scope_words(&mut self, scope: &ScopeExpr, e: &Type) {
        let mut seen: Vec<&str> = Vec::new();
        for w in &scope.words {
            let name = w.name.name.as_str();
            if seen.contains(&name) {
                let message = format!("`{name}` is said twice");
                self.report(errors::scope_word(&message, w.name.span));
                continue;
            }
            seen.push(name);
            match (name, &w.arg) {
                ("independent", None) => {}
                ("independent", Some(f)) => {
                    // The handler runs in the scope's tasks: it crosses to them.
                    let wanted = Type::Fun(vec![failure_type(e.clone())], Box::new(Type::Unit));
                    let found = self.expect_type(f, &Expect::of(wanted.clone()));
                    if matches!(found, Type::Fun(..)) {
                        let to = Type::Interface(super::sendable::SENDABLE.to_string(), Vec::new());
                        self.sendable_conversion(f, &found, &to);
                    }
                }
                ("limit", Some(n)) => {
                    self.expect_type(n, &Expect::of(Type::I64));
                }
                _ => {
                    let message = format!(
                        "`{name}` isn't a scope setting here: write `independent`, `independent(f)` or `limit n`"
                    );
                    self.report(errors::scope_word(&message, w.name.span));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type, ok};

    const WORK: &str = "fun work(n: i64): i64 = n;\nfun bad(): Result<i64, string> = Err(\"x\");\n";

    fn codes(src: &str) -> Vec<&'static str> {
        errors(src).into_iter().map(|(c, _)| c).collect()
    }

    #[test]
    fn settings_are_independent_a_handler_and_a_limit() {
        ok(&format!(
            "{WORK}fun main() {{ let n = 3; let r = scope s independent limit n {{ s.spawn(work(1)); }}; }}"
        ));
        ok(&format!(
            "{WORK}fun note(f: Failure) {{}}\nfun main() {{ let r = scope s independent(note) {{ s.spawn(bad()); }}; }}"
        ));
        assert_eq!(
            codes(&format!(
                "{WORK}fun main() {{ let r = scope s quickly {{ }}; }}"
            )),
            ["E0477"]
        );
        assert_eq!(
            codes(&format!(
                "{WORK}fun main() {{ let r = scope s limit 1 limit 2 {{ }}; }}"
            )),
            ["E0477"]
        );
        // The handler runs in the failing task: what it captured must be sendable.
        assert_eq!(
            codes(&format!(
                "{WORK}fun main() {{ let c = Ref.new(0); let r = scope s independent(fun(f: Failure) {{ c.set(1); }}) {{ }}; }}"
            )),
            ["E0475"]
        );
    }

    #[test]
    fn a_typed_scope_fails_with_its_error() {
        let typed = "enum Oops { A }\nimpl Display for Oops { fun show(self): string = \"a\"; }\nimpl Error for Oops {}\n\
                     fun a(): Result<i64, Oops> = Err(Oops.A);\nfun other(): Result<i64, string> = Err(\"x\");\n";
        let oops = Type::Enum("Oops".into(), Vec::new());
        let failure = Type::Struct("$prelude.Failure".into(), vec![oops]);
        assert_eq!(
            let_type(
                &format!("{typed}fun main() {{ let r = scope<Oops> s {{ s.spawn(a()); }}; }}"),
                "r"
            ),
            Type::Enum("$prelude.Result".into(), vec![Type::Unit, failure])
        );
        assert_eq!(
            codes(&format!(
                "{typed}fun main() {{ let r = scope<Oops> s {{ s.spawn(other()); }}; }}"
            )),
            ["E0478"]
        );
    }

    #[test]
    fn scope_and_failure_alone_are_the_plain_scopes() {
        let error = Type::Interface("$prelude.Error".into(), Vec::new());
        assert_eq!(
            let_type("fun main() { let r = scope s { 1 }; }", "r"),
            Type::Enum(
                "$prelude.Result".into(),
                vec![
                    Type::I64,
                    Type::Struct("$prelude.Failure".into(), vec![error])
                ]
            )
        );
        ok("fun f(s: Scope): Result<(), Failure> = Ok(());\nfun main() {}");
    }
}
