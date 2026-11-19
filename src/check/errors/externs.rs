//! Diagnostics for C interop: callbacks (E0435, E0436), extern symbols and `extern struct` and
//! `extern union` declarations (E0437).

use crate::check::Type;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// A malformed callback declaration in an extern.
pub(in crate::check) fn callback_decl(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::error("E0435", message, span).with_help(
        "write the callback as `cb: fun(T, data)` and its user data as a parameter `user: data`; \
         `release` is the function C calls when it drops the callback",
    )
}

/// `data` or `release` written where no extern signature gives them a meaning.
pub(in crate::check) fn marker_outside(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0435",
        format!("`{name}` only means something in an extern's signature"),
        span,
    )
    .with_help("`data` and `release` mark the parameters C uses for a callback's user data")
}

/// A function value with captures where C keeps no user data.
pub(in crate::check) fn callback_captures(span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0436",
        "C can't keep this function's captures: this callback has no `data`",
        span,
    )
    .with_help(
        "pass a named function, or a closure that captures nothing; a callback declared with `data` takes any function",
    )
}

/// A type that can't go into or out of a callback.
pub(in crate::check) fn callback_type(span: Span) -> Diagnostic {
    Diagnostic::error("E0409", "this type can't cross into a callback", span).with_help(
        "a callback's parameters and result are numbers, `bool`, handles (`extern type`) or `Option` of a handle; \
         its parameters may also be `string` or `Option<string>`, copied from C, but it can't return one: \
         C would keep a pointer into Lugha's memory",
    )
}

/// E0481: a `spawned` callback taking `Ref<T[]>`, which C reads back when
/// its call returns, before the `spawned` callback has run.
pub(in crate::check) fn spawned_ref(span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0481",
        "a `spawned` callback can't take `Ref<T[]>`: C reads the array back before the callback runs",
        span,
    )
    .with_help("make the callback `direct` (`cb: direct fun(Ref<i64[]>, data)`), so it runs during C's call and its changes reach C")
}

/// E0482: a `realtime` callback taking a value C's call copies.
pub(in crate::check) fn realtime_param(ty: &Type, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0482",
        format!("a `realtime` callback can't take `{ty}`: C's call copies it into a new Lugha value, an allocation"),
        span,
    )
    .with_help("take numbers, `bool`, handles or extern structs, or make the callback `direct`")
}

/// E0479: a `blocking` extern's callback (or its `data` or `release`).
pub(in crate::check) fn blocking_callback(span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0479",
        "a `blocking` extern can't take a callback: it runs on a helper thread",
        span,
    )
    .with_help("Lugha code can't run on a helper thread; drop `blocking`, or call the callback's work after the call returns")
}

/// E0480: a `blocking` extern taking or giving a pinned handle.
pub(in crate::check) fn blocking_pinned(ty: &Type, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0480",
        format!("a `blocking` extern can't take or give `{ty}`, a pinned handle: it runs on a helper thread"),
        span,
    )
    .with_help("a pinned handle stays on the thread that made it; drop `blocking`, so the call runs on the task's own thread")
}

/// E0306: one C function declared `blocking` in one place and not another.
pub(in crate::check) fn blocking_mismatch(symbol: &str, span: Span, first: Span) -> Diagnostic {
    Diagnostic::error(
        "E0306",
        format!("C function `{symbol}` is declared `blocking` in one place and not in another"),
        span,
    )
    .with_label(first, "first declared here")
    .with_help("every declaration of a C function must agree on `blocking`")
}

/// `extern "symbol"` with a symbol that isn't a usable C name.
pub(in crate::check) fn bad_symbol(symbol: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0306",
        format!("`{symbol}` is not a C function name lughac can call"),
        span,
    )
    .with_help("use letters, digits and `_`, not starting with a digit; names starting with `lugha_` are reserved")
}

/// A field of an `extern struct` that C can't hold.
pub(in crate::check) fn c_struct_field(span: Span) -> Diagnostic {
    Diagnostic::error("E0437", "an `extern struct` can't hold this", span).with_help(
        "an `extern struct` holds numbers, `bool`, `char`, handles (`extern type`) and other extern structs, laid out as C lays them out",
    )
}

/// An `extern struct` with type parameters.
pub(in crate::check) fn c_struct_generic(span: Span) -> Diagnostic {
    Diagnostic::error("E0437", "an `extern struct` can't be generic", span)
        .with_help("C has no generics: declare one `extern struct` per layout")
}

/// An `extern struct` with no fields: C has no empty structs.
pub(in crate::check) fn c_struct_empty(span: Span) -> Diagnostic {
    Diagnostic::error("E0437", "an `extern struct` needs at least one field", span)
        .with_help("C has no empty structs; pass nothing instead")
}

/// A pointer (a handle, `Option` of one, `CPtr`) in an `extern union`, at
/// any depth: another field's bytes would make one up.
pub(in crate::check) fn c_union_pointer(span: Span) -> Diagnostic {
    Diagnostic::error("E0437", "a union can't hold a pointer", span).with_help(
        "reading it from another field's bytes would make one up; a union holds numbers, `bool`, `char` and extern structs and unions of those",
    )
}
