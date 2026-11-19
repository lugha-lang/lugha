//! Views of C's memory: `View<T>` and `MutView<T>`.

use crate::check::Type;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// E0506: writing an element through a read-only `View<T>`.
pub(in crate::check) fn view_read_only(span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0506",
        "cannot assign through a `View`: it only reads C's memory",
        span,
    )
    .with_help("take a `MutView<T>` to write C's memory")
}

/// E0409: a view of something other than integers or floats.
pub(in crate::check) fn view_element(ty: &Type, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0409",
        format!("a view can't hold `{ty}`: C's bytes might not be a valid one"),
        span,
    )
    .with_help(
        "a view's elements are integers, floats, or extern structs holding only those: \
         `View<u8>`, `MutView<f32>`",
    )
}

/// E0483: a view used where it could outlive the call it was given to.
pub(in crate::check) fn view_escapes(what: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0483",
        format!("a view can't outlive the call it was given to: {what}"),
        span,
    )
    .with_help(
        "a view is used, indexed, sliced and passed to functions during the call; \
         copy what you need with `to_array()`",
    )
}

/// E0484 when a callback can't take buffer parameter `ty`: a `spawned`
/// callback runs after C's call returns, so it takes a copy (`T[]`); one
/// run during the call reads C's memory in place (a view).
pub(in crate::check) fn callback_buffer(
    ty: &Type,
    spawned: bool,
    span: Span,
) -> Option<Diagnostic> {
    let help = match ty {
        Type::Ref(inner) => match &**inner {
            Type::Array(e) => {
                format!("take `MutView<{e}>`: it writes C's memory in place, during the call")
            }
            _ => return None,
        },
        Type::Array(e) if !spawned => format!(
            "take `View<{e}>`, which reads C's memory in place; `to_array()` copies it to keep"
        ),
        Type::View(_, e) if spawned => format!(
            "a `spawned` callback runs after C's call returns, when C's memory may be gone: take `{e}[]`, a copy"
        ),
        _ => return None,
    };
    let message = format!("a callback can't take `{ty}` here");
    Some(Diagnostic::error("E0484", message, span).with_help(help))
}

/// E0403: `p.view(n, f)` with an `f` that doesn't take one view.
pub(in crate::check) fn view_function(ty: &Type, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0403",
        format!("`view` takes a function of one view, not `{ty}`"),
        span,
    )
    .with_help("write the view's type on its parameter: `fun(bytes: View<u8>) { … }`")
}

/// E0333: `p.view` in a package no `ffi` grant covers; `help` names the
/// program's entry to change.
pub(in crate::check) fn view_ungranted(package: &str, help: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0333",
        format!("package `{package}` reads C's memory (`p.view`) but has no `ffi` permission"),
        span,
    )
    .with_help(help.to_string())
}
