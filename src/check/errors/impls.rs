//! Errors for `impl I for T` blocks: E0422 for a type without the `impl`
//! or a block that doesn't match its interface, E0459 for a block in the
//! wrong place, E0460 for an ambiguous method name, E0461 for a second
//! block of one interface.

use super::super::Type;
use super::brief;
use crate::ast::{Expr, ExprKind};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// E0422: `ty` has no `impl` of `interface`; `declared` is `ty` as an
/// `impl` names it, with its declaration's parameters (`Stack<T>`), or
/// `None` for a type that can't have one.
pub(in crate::check) fn not_implemented(
    ty: &Type,
    declared: Option<&Type>,
    interface: &str,
    span: Span,
) -> Diagnostic {
    let help = match declared {
        Some(declared) => format!("add `impl {interface} for {declared} {{ … }}`"),
        None => format!(
            "add `impl {interface} for {ty} {{ … }}` in the module that declares `{interface}`"
        ),
    };
    Diagnostic::error(
        "E0422",
        format!("`{ty}` doesn't implement `{interface}`"),
        span,
    )
    .with_help(help)
}

/// E0422: the block `impl interface for ty` lacks items, has extra ones,
/// or gives one the wrong signature: one entry of `problems` each.
pub(in crate::check) fn bad_impl(
    ty: &Type,
    interface: &str,
    problems: &[String],
    span: Span,
) -> Diagnostic {
    Diagnostic::error(
        "E0422",
        format!("`impl {interface} for {ty}` doesn't match `{interface}`"),
        span,
    )
    .with_help(problems.join("; "))
}

/// E0459: an `impl` of `interface` written where it may not be: `why`
/// says what's wrong.
pub(in crate::check) fn misplaced_impl(interface: &str, why: String, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0459",
        format!("this `impl {interface} for …` {why}"),
        span,
    )
}

/// E0460: `name` on `ty` is an item of two interfaces it implements.
/// `call` is the call as written, `x.name(…)` or `T.name(…)`, from which
/// the help spells the qualified calls.
pub(in crate::check) fn ambiguous_item(
    name: &str,
    ty: &Type,
    names: &[String; 2],
    call: &Expr,
) -> Diagnostic {
    let [a, b] = names;
    let (span, help) = match &call.kind {
        ExprKind::Call(callee, args) => match &callee.kind {
            ExprKind::Field(base, item) => {
                let more = if args.is_empty() { "" } else { ", …" };
                let receiver = brief(base);
                let help = format!(
                    "say which one you mean: `{a}.{name}({receiver}{more})` or `{b}.{name}({receiver}{more})`"
                );
                (item.span, help)
            }
            _ => (call.span, String::new()),
        },
        _ => (call.span, String::new()),
    };
    let d = Diagnostic::error(
        "E0460",
        format!("`{name}` on `{ty}` is an item of both `{a}` and `{b}`"),
        span,
    );
    if help.is_empty() {
        d
    } else {
        d.with_help(help)
    }
}

/// E0460 for an associated function `ty.name(…)` that two interfaces
/// `ty` implements declare, at `span`.
pub(in crate::check) fn ambiguous_associated(
    name: &str,
    ty: &Type,
    names: &[String; 2],
    span: Span,
) -> Diagnostic {
    let [a, b] = names;
    Diagnostic::error(
        "E0460",
        format!("`{name}` on `{ty}` is an item of both `{a}` and `{b}`"),
        span,
    )
    .with_help(format!(
        "call it through a bound that names one of them, `T: {a}`, or give `{ty}` its own `{name}`"
    ))
}

/// E0422: `ty` implements `interface` (at `span`) but not `extended`,
/// which `interface` extends.
pub(in crate::check) fn extended_missing(
    ty: &Type,
    interface: &str,
    extended: &str,
    span: Span,
) -> Diagnostic {
    Diagnostic::error(
        "E0422",
        format!(
            "`{ty}` implements `{interface}` but not `{extended}`, which `{interface}` extends"
        ),
        span,
    )
    .with_help(format!(
        "add `with {extended}`, or write `impl {extended} for {ty} {{ … }}`"
    ))
}

/// E0461: a second `impl interface for ty`; the first is at `first`.
pub(in crate::check) fn duplicate_impl(
    interface: &str,
    ty: &Type,
    span: Span,
    first: Span,
) -> Diagnostic {
    Diagnostic::error(
        "E0461",
        format!("`{interface}` is implemented for `{ty}` twice"),
        span,
    )
    .with_label(first, "first implemented here")
    .with_help("keep one of the two blocks")
}
