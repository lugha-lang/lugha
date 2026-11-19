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
        "nobody would see the change; send the new value back through a channel, or keep the state in an actor",
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
/// E0415: `Chan.name(…)` that isn't one of the ways to make a channel.
pub(in crate::check) fn chan_maker(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error("E0415", format!("`Chan` has no `{name}`"), span)
}

pub(in crate::check) fn chan_unknown(span: Span) -> Diagnostic {
    Diagnostic::error("E0412", "cannot infer what this channel carries", span)
        .with_help("write what it carries: `let (tx, rx) = Chan<i64>.new(4);`")
}

/// An actor used against its rules.
pub(in crate::check) fn actor_misuse(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::error("E0439", message, span).with_help(
        "an actor is started in a scope, `s.spawn(Counter { … })`, and reached only through its methods; \
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
         root scope `main` takes (`root: Scope`)",
    )
}

/// E0474: an actor started without a scope.
pub(in crate::check) fn unscoped_actor(span: Span) -> Diagnostic {
    Diagnostic::error("E0474", "an actor starts in a scope", span).with_help(
        "start it through a scope's handle, `s.spawn(Counter { count: 0 })`, or through the \
         root scope `main` takes (`root: Scope`); the scope stops it once its tasks are done",
    )
}

/// E0475: a value that isn't sendable crosses to another task: `head`
/// says which and how, `why` names its first part that isn't.
pub(in crate::check) fn not_sendable(head: &str, why: &str, span: Span) -> Diagnostic {
    let help = if head.starts_with("a `spawned` callback") {
        "make the callback `direct` in the extern (`cb: direct fun(W, data)`) to run it during C's call, or capture only sendable values"
    } else if why.contains("type parameter") {
        "add `Sendable` to its bounds: `<T: Sendable>`"
    } else if why.contains("pinned handle") {
        "do the work that needs it in this task, or start the other tasks in a `pinned` scope and make the handle in each"
    } else if why.contains("function value") {
        "pass the function itself, or take an interface that requires `Sendable`: `interface Job: fun(), Sendable {}`"
    } else {
        "keep shared state in an actor, or send copies through a channel: a `Ref` belongs to one task"
    };
    Diagnostic::error("E0475", format!("{head}: {why}"), span).with_help(help)
}

/// E0476: `impl Sendable`, `with Sendable`, or `auto interface` outside
/// the prelude: sendability is worked out from a type's parts.
pub(in crate::check) fn sendable_claimed(message: &str, span: Span) -> Diagnostic {
    Diagnostic::error("E0476", message.to_string(), span).with_help(
        "a type is sendable when every part of it is: nothing to write, and nothing to claim",
    )
}

/// E0477: a scope's setting that doesn't exist, or is said twice.
pub(in crate::check) fn scope_word(message: &str, span: Span) -> Diagnostic {
    Diagnostic::error("E0477", message.to_string(), span).with_help(
        "a scope's settings come between its name and its body, each at most once: \
         `scope conns independent limit 100 { … }`",
    )
}

/// E0478: a task's error type that doesn't convert to its scope's.
pub(in crate::check) fn task_error_unconverted(
    e: &crate::check::Type,
    scope_error: &crate::check::Type,
    span: Span,
) -> Diagnostic {
    Diagnostic::error(
        "E0478",
        format!("this task fails with `{e}`, which doesn't convert to its scope's `{scope_error}`"),
        span,
    )
    .with_help(format!(
        "return `{scope_error}` from the task, or make `{scope_error}` an interface that `{e}` implements"
    ))
}
