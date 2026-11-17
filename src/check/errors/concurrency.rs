//! Diagnostics for green threads, channels and `select` (E0438) and actors (E0439).

use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// `spawn xs.push(1)`, `spawn p.bump()`: the new thread changes its own copy.
pub(in crate::check) fn spawn_changes_copy(span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0438",
        "`spawn` of a call that changes its receiver or a `mut` argument: the new thread would change its own copy",
        span,
    )
    .with_help(
        "nobody would see the change; send the new value back through a channel, or keep it in a `Ref`",
    )
}

/// A malformed `select`: the arms it may not have, or lack.
pub(in crate::check) fn bad_select(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::error("E0438", message, span).with_help(
        "a `select` waits on `name = c.recv()` and `c.send(v)` arms, with at most one \
         `after(ms)` or one `default`",
    )
}

/// `Chan.new()` with nothing saying what it carries.
pub(in crate::check) fn chan_unknown(span: Span) -> Diagnostic {
    Diagnostic::error("E0412", "cannot infer what this channel carries", span)
        .with_help("annotate it: `let c: Chan<i64> = Chan.new();`")
}

/// An actor used against its rules.
pub(in crate::check) fn actor_misuse(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::error("E0439", message, span).with_help(
        "an actor is started with `spawn` and reached only through its methods; \
         what crosses its boundary is copied and may hold no `Ref`",
    )
}

/// A scope or task handle where it may not go: returned, stored, sent or
/// captured. It is passed as an argument, and nothing else.
pub(in crate::check) fn handle_kept(message: &str, span: Span) -> Diagnostic {
    Diagnostic::error("E0472", message, span).with_help(
        "a handle is passed as an argument, and nothing else: keep it a local, and finish \
         the tasks inside the scope that started them",
    )
}

/// `x.spawn(…)` where `x` isn't a scope's handle.
pub(in crate::check) fn not_a_scope(ty: &crate::check::Type, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0473",
        format!("`spawn` needs a scope's handle, not `{ty}`"),
        span,
    )
    .with_help("start the task in a scope: `scope s { s.spawn(work(x)) }`")
}

/// `spawn work(x)`: a call that isn't an actor's starts a task, which
/// starts in a scope.
pub(in crate::check) fn unscoped_task(span: Span) -> Diagnostic {
    Diagnostic::error("E0474", "a task starts in a scope", span).with_help(
        "start it through a scope's handle, `scope s { s.spawn(work(x)) }`, or through the \
         root scope `main` takes (`root: Scope`); `spawn` alone starts an actor",
    )
}
