## FEATURE: Scopes and cancellation, the core of structured concurrency: `scope s { … }`, `s.spawn(f(x))` in place of bare `spawn`, all-or-nothing failure, invisible cancellation at safe points, task handles' `join()`, `cancel` and `name`, and `main`'s root scope; milestone 195.

**Status:** proposed 2026-11-18.

## OBJECTIVE
```
fun fetch_both(net: Net, a: string, b: string): Result<(string, string), Error> {
    scope s {
        let x = s.spawn(fetch(net, a));
        let y = s.spawn(fetch(net, b));
        Ok((x.join()?, y.join()?))
    }?
}

fun main(console: Console, root: Scope) {
    let worker = root.spawn(crunch(10));     // `main` returns, then waits for it
    match scope s {
        s.spawn(fails());                    // panics: cancels `slow`, fails the scope
        s.spawn(slow());                     // stopped at its next safe point; its defers run
        1
    } {
        .Ok(n) => console.println(n),
        .Err(f) => console.println("scope failed: {f}"),
    }
}
```
A task fails its scope, not the program; the scope cancels the others, waits for them, and evaluates to `Err(failure)`. Nothing a scope started outlives it.

## CONTEXT
- **Draft v18 §4.1** rules 1, 2, 4 [COR-01, COR-02, COR-04]; **§4.2** rules 1–4, 6, 7, 13–16 [SCO-01..05, SCO-10..14]; **§4.3** rules 1–4, 8, 10, 12 [CAN-01..04, CAN-08, CAN-10, CAN-12]. All approved in `docs/decisions/v18-ledger.md`.
- **Phase 4 (m194)** built unwinding: a panic is a forced unwind whose landing pads run each frame's `defer`s and releases. Cancellation is the same unwind started at a safe point. Today an unwind always ends at a task's first frame; a scope needs to catch one in the middle of a task.
- **Today:** `spawn f(x)` starts a green thread anywhere and returns a `Task<T>` whose `join()` gives `T`; a task's panic ends the program. Bare `spawn` appears in 41 end-to-end programs, 44 embedded ones in `tests/*.rs`, and `std.http`'s server.
- **Phase 5's plan**, one PRP at a time, each approved before it starts:
  1. **This PRP:** scopes, cancellation, task handles, the root scope.
  2. **The sendable rule** (§4.1 rule 3 [COR-03]): what may cross to another task, checked.
  3. **Scope settings** (§4.2 rules 5, 8–12; §4.3 rules 3 extras, 5, 9, 11): `independent`, `within`/`until`/`remaining`, `limit`, `scope<E>`, `shield`, Ctrl-C.
  4. **Channels** (§4.4): split ends, required capacity, `try_send`/`try_recv` results, iteration, `merge`.
  5. **Actors** (§4.5): started in scopes, bounded mailboxes, generic actors and actors behind interfaces, `join` removed.

## DESIGN

### Scopes
- **`scope s { body }`** is an expression. `scope` is contextual (a keyword only where it begins one), so `scope` stays usable as a name.
- **The body runs in the current task.** The scope's value is `Result<T, Failure>`, `T` being the body's type, ready once every task started in it has finished.
- **`Failure`** (prelude enum): `.Error(Error)` for a task returning `Err`, and `.Panic(string)` for a panic.
  - It implements `Error`, so `scope s { … }?` works in a function returning `Result<_, Error>` [SCO-13].
  - `f.task()` gives the failed task's name.
  - `.Deadline` and `.Memory` come with the settings PRP and memory limits, as variants then.
- **All-or-nothing:** the first failure, from a task or the body (a panic in the body), cancels the other tasks and the body with `.SiblingFailed`. The scope waits for them; its value is that failure. Cancellations the failure caused aren't reported as failures [CAN-04].
- **Leaving the body early** (`return`, `?`, `break`, `continue`) cancels the remaining tasks with `.Parent`, waits for them, runs the body's `defer`s, then completes the exit [SCO-14].
- **At the body's end,** the scope waits for every task, then runs the body's `defer`s [SCO-10]: a resource the body opened outlives every task using it.

### Spawning and task handles
- **`s.spawn(f(x))`** takes a call, evaluated as `spawn` evaluates one today: arguments copied by value in the spawning task [COR-04].
- **The bare `spawn` expression is removed** (breaking): E0211's message says to start the task in a scope.
- **A task handle (`Task<T>`)** has three methods:
  - `join()` gives `Result<V, Stopped>`. `V` is the task's value, or its `Ok` value when the task returns a `Result`. `Stopped` is `.Cancelled(Reason)`, `.Failed(Error)` or `.Panicked(string)` and implements `Error`, so `t.join()?` works [SCO-11].
  - `cancel(reason: string)` stops it with `.Requested(reason)`.
  - `name(text)` names it for failures and reports; the default name is the called function's.
- **Joining is optional:** an unjoined result is dropped, but an unjoined failure still fails the scope.
- **Second-class handles** [SCO-02]: a `Scope` or a `Task` may be passed as an argument and nothing else. It can't be returned, stored in a field, array or map, sent, put in an interface value or captured by a closure that may outlive it. A local is fine. New errors cover each (E0476 onwards, with pages).

### Cancellation
- **Reasons** (`Reason`, a prelude enum): `.SiblingFailed`, `.Parent`, `.Requested(string)` now; `.Deadline` and `.Shutdown` come with deadlines and Ctrl-C [CAN-03].
- **Invisible in the cancelled task** [CAN-01]. Cancelling marks the task; at its next safe point it unwinds as a panic does, running its `defer`s and releasing what it holds, and stops with `.Cancelled(reason)`. Nothing is printed.
- **Safe points** [CAN-02]: every pause point (a channel operation, `join`, `select`, sleep, a socket wait, a scope waiting for its tasks) and every preemption point (function entries and loop iterations, the countdown that already exists for tasks). A cancelled task waiting at a pause point is woken to unwind.
- **Never inside C** [CAN-07]: a task in a C call or a callback is stopped once it is back in Lugha.

### Catching at the scope's edge
- A failure or cancellation of the body unwinds the current task only as far as the scope: the scope catches it there and goes on with its `Result`.
- **Within a scope's body, a landing pad's cleanup stops at the scope.** It runs the `defer`s and releases of the blocks and statements inside the body, then branches to the scope's failure path instead of resuming. Calls outside the body keep their pads as now.
- **The runtime records each active scope** (its tasks, its state, its first failure) in the current task. A panic or cancellation whose innermost scope is in this task stops being an unwind there. The scope records the failure or cancellation, cancels its tasks, waits for them, and its value is the failure.
- **A task's own panic,** at its first frame, fails the scope that started it (the scope's record) and ends the task, instead of ending the program.

### The root scope and `main`
- `main` may take a `Scope` (a capability, by type, like `Console`), the root scope [SCO-12]. When `main` returns, the program waits for the root scope's tasks.
- A failure in a root-scope task cancels the others and `main`, then the program prints it and exits 101, as an unhandled panic does today [CAN-08]. Tests may take a `Scope` too.
- Programs without a `Scope` can still use `scope s { … }` anywhere.

### Unchanged here
- Actors are still started with `spawn Counter { … }` until the actors PRP. It is the one `spawn` left, and its actors are the root scope's: a panic ends only the actor, as now.
- Channels keep their v17 API until the channels PRP. Any value may still cross, until the sendable rule.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The phase in five PRPs, in the order above.** Scopes and cancellation first, since every other part leans on them.
2. **`scope` is contextual,** as the Draft lists it [OPN-08]; `spawn` stays a keyword for actors until their PRP.
3. **A scope catches in place,** with cleanup pads that stop at its edge, not by running the body on a separate stack or as a closure. Being an ordinary block keeps `return`, `break` and `?` inside the body plain control flow.
4. **`Failure.task()` and `Task.name`'s default** come from the called function's name, so reports say which task failed without extra code.
5. **The settings PRP adds `Failure.Deadline`;** this one only defines what exists now.
6. **Error codes:** E0476 onwards for second-class handles and a `spawn` outside `s.spawn`; E0211 changes its message.

## REQUIREMENTS

### Must Do
- `scope s { … }`, `s.spawn(…)`, `Scope`, `Task<T>` with `join`/`cancel`/`name`, `Failure`, `Stopped`, `Reason`.
- Cancellation at safe points, all-or-nothing scopes, catching at the scope's edge, the root scope, early exits cancelling with `.Parent`.
- Second-class checks for `Scope` and `Task`.
- Remove bare `spawn` for calls; rewrite every program using it (end-to-end, embedded tests, `std.http`, the spec's examples).
- Spec §3–§6, §9, §10 where they show `spawn`; "Changes since v16"; §11 row 195; CHANGELOG (Breaking); `docs/errors/` pages.

### Must NOT Do
- The sendable rule, scope settings (`independent`, deadlines, `limit`, `scope<E>`, `shield`, Ctrl-C), the channel and actor reworks: their own PRPs, in the order above.
- Several OS threads: tasks stay on one thread until phase 6.

## TESTS
- [ ] Checker unit tests: `scope`'s type; `s.spawn` only takes a call; bare `spawn f()` is E0211; every second-class misuse (return, field, array, capture by an escaping closure, send) has its error.
- [ ] `tests/programs/concurrency/scopes/` (new):
  - a scope's value and `Ok`;
  - a task's panic failing its scope and cancelling a busy sibling (its `defer` runs);
  - a task's `Err` as `Failure.Error`;
  - `join()`'s three `Stopped` cases;
  - `cancel` and `.Requested`;
  - `return` and `?` out of a body cancelling with `.Parent`;
  - the body's `defer`s after its tasks;
  - nested scopes (cancellation stops at the inner scope's edge);
  - a task waiting on a channel when cancelled;
  - `main`'s root scope waiting at exit;
  - a root-scope task's panic ending the program with 101.
- [ ] `tests/leaks.rs`: every scope program frees everything, cancelled tasks included; drop now-fixed entries from its known list (spawned-task panics, deadlock cases) where scopes unwind the waiting tasks.
- [ ] Every rewritten program prints what it did, `join()` results aside.
- [ ] Targeted runs: `--lib`, `programs`, `leaks`, `codegen`, `tasks_runtime`, `spec`, `http`, `net`, `comments`, `explain`.

## ACCEPTANCE
- [ ] No task outlives the scope that started it; failures reach their scope; cancelled tasks unwind and free what they hold.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Every `pub` item documented; spec, CHANGELOG, error pages, CLAUDE.md's known issues updated.
