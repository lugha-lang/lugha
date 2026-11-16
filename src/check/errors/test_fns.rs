//! Diagnostics for `@test` functions (E0434).

use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// A `@test` function with parameters, type parameters or a return type.
pub(in crate::check) fn bad_test(span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0434",
        "a test takes only `Console`, `Dir`, `Clock`, `Env`, `Entropy` and `Net` (and, with `@test(integration)`, `Fs` and `Proc`), each at most once, and returns nothing",
        span,
    )
    .with_help(
        "write `@test fun name() { … }` or `@test fun name(dir: Dir) { … }`, and check results with `assert` or `assert_eq`",
    )
}

/// A plain test asking for `Fs` or `Proc`, which only an integration test
/// is given.
pub(in crate::check) fn test_needs_integration(span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0434",
        "only an integration test is given `Fs` and `Proc`: a plain test gets a fresh `Dir` and no programs",
        span,
    )
    .with_help("take a `dir: Dir` instead, or mark it `@test(integration)` (run by `lughac test --integration`)")
}

/// A test called or used as a value.
pub(in crate::check) fn test_used(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0434",
        format!("`{name}` is a test: only `lughac test` runs it"),
        span,
    )
    .with_help("move the shared code into an ordinary function and call that")
}
