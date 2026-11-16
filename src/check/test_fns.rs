//! Tests: a `@test` function's shape (E0434), the rule that nothing calls
//! a test, and `skip(reason)`. The assertions are prelude functions.

use super::errors;
use super::expr::Expect;
use super::{Checker, Entry, Type, maker};
use crate::ast::{Expr, Item, Program};
use crate::span::Span;

/// `skip(reason)`, which ends a test as skipped: an intrinsic, since it
/// talks to the test runner. The assertions are prelude functions.
pub(super) const ASSERTIONS: [&str; 1] = ["skip"];

/// The names of `program`'s integration tests (`@test(integration)`).
pub(super) fn integration_tests_of(program: &Program) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fun(f) if f.test.is_some() && f.integration => Some(f.name.name.clone()),
            _ => None,
        })
        .collect()
}

/// The names of `program`'s tests, in declaration order.
pub(super) fn tests_of(program: &Program) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fun(f) if f.test.is_some() => Some(f.name.name.clone()),
            _ => None,
        })
        .collect()
}

impl Checker {
    /// E0434 for every test of `program` that takes parameters, type
    /// parameters or returns something.
    pub(super) fn test_shapes(&mut self, program: &Program) {
        for item in &program.items {
            if let Item::Fun(f) = item
                && f.test.is_some()
            {
                let key = self.key(&f.name.name);
                let params = self
                    .functions
                    .get(&key)
                    .map(|s| s.params.clone())
                    .unwrap_or_default();
                let entry = Entry::Test {
                    integration: f.integration,
                };
                let supplied = self.entry_params((&key, entry), &params, f.name.span);
                let shaped = f.type_params.is_empty() && f.ret.is_none();
                // Only the real `Fs` or `Proc` missing: say how to get them.
                let real = !f.integration
                    && params.iter().all(|p| match p {
                        Type::Interface(k, _) => {
                            maker(k, Entry::Test { integration: true }).is_some()
                        }
                        _ => false,
                    });
                if !supplied && shaped && real {
                    self.report(errors::test_needs_integration(f.name.span));
                } else if !supplied || !shaped {
                    self.report(errors::bad_test(f.name.span));
                }
            }
        }
    }

    /// E0434 when the function `key`, named at `at`, is a test: tests are
    /// compiled only by `lughac test`, which alone runs them.
    pub(super) fn not_a_test(&mut self, key: &str, at: Span) {
        if self.functions.get(key).is_some_and(|sig| sig.test) {
            let name = key.rsplit('.').next().unwrap_or(key);
            self.report(errors::test_used(name, at));
        }
    }

    /// `skip(reason)`: ends the test, so nothing after it runs.
    pub(super) fn assertion(&mut self, call: &Expr, name: &str, args: &[Expr]) -> Type {
        if args.len() != 1 {
            self.report(errors::arity(name, 1, args.len(), call.span));
            self.unchecked_args(args);
            return Type::Unit;
        }
        self.expect_type(&args[0], &Expect::of(Type::String));
        Type::Never
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{errors, ok, test_mode};

    #[test]
    fn a_test_takes_nothing_and_returns_nothing() {
        let main = "fun main() {}\n";
        assert_eq!(
            errors(&format!("{main}@test fun t(n: i64) {{}}")),
            [("E0434", "t")]
        );
        assert_eq!(
            errors(&format!("{main}@test fun t(): i64 = 1;")),
            [("E0434", "t")]
        );
        assert_eq!(
            errors(&format!("{main}@test fun t<T>() {{}}")),
            [("E0434", "t")]
        );
        ok(&format!("{main}@test fun t() {{ assert(true); }}"));
    }

    #[test]
    fn a_plain_test_takes_stand_ins_and_an_integration_test_the_real_world() {
        let main = "fun main() {}\n";
        ok(&format!(
            "{main}@test fun t(d: Dir, c: Clock, e: Env, r: Entropy, n: Net, o: Console) {{}}"
        ));
        ok(&format!(
            "{main}@test(integration) fun t(f: Fs, p: Proc, d: Dir, n: Net) {{}}"
        ));
        for params in ["f: Fs", "p: Proc", "a: string[]", "d: Dir, e: Dir"] {
            assert_eq!(
                errors(&format!("{main}@test fun t({params}) {{}}")),
                [("E0434", "t")],
                "{params}"
            );
        }
    }

    #[test]
    fn nothing_calls_a_test() {
        let src = "@test fun t() {}\nfun main() { t(); let f = t; }";
        assert_eq!(errors(src), [("E0434", "t"), ("E0434", "t")]);
    }

    #[test]
    fn assertions_are_the_preludes_and_skip_takes_a_reason() {
        use crate::check::modules::test_util::prelude_codes;
        let codes = |body: &str| prelude_codes(&[("main.la", &format!("fun main() {{ {body} }}"))]);
        let none: Vec<&str> = Vec::new();
        assert_eq!(
            codes(
                "assert(1 < 2); assert_eq(\"a\", \"a\"); assert_ne([1], [2]); let x: u8 = 3; assert_eq(x, 3);"
            ),
            none
        );
        assert_eq!(codes("assert(1);"), ["E0401"]);
        assert_eq!(codes("assert_eq(1, \"a\");")[0], "E0401");
        assert_eq!(codes("assert_eq(1);"), ["E0405"]);
        assert_eq!(codes("let r = Ref.new(1); assert_eq(r, r);")[0], "E0419");
        assert_eq!(errors("fun main() { skip(1); }"), [("E0401", "1")]);
    }

    #[test]
    fn test_mode_needs_no_main_and_lists_the_tests_in_order() {
        let src = "@test fun b() {}\nfun helper() {}\n@test fun a() {}";
        let checked = test_mode(src).expect("no errors");
        assert_eq!(checked.tests, Some(vec!["b".to_string(), "a".to_string()]));
        assert_eq!(errors(src), [("E0303", "")]);
        assert_eq!(ok("fun main() {}\n@test fun a() {}").tests, None);
    }
}
