## FEATURE: Time and shutdown for scopes: `within ms` and `until t` deadlines with `s.remaining()`, `shield within ms { … }`, and Ctrl-C cancelling the root scope; milestone 199.

**Status:** approved 2026-11-18; built 2026-11-18 (milestone 199).

## OBJECTIVE
```
fun fetch_both(net: Net, a: string, b: string): Result<(string, string), Failure> {
    scope s within 2_000 {                       // the body and both tasks: two seconds
        let x = s.spawn(fetch(net, a));
        let y = s.spawn(fetch(net, b));
        (x.join().unwrap(), y.join().unwrap())
    }
}
// past the deadline: both tasks and the body are cancelled with `.Deadline`;
// the value is `Err(f)` with `f.kind` `.Deadline`

fun session(conn: Conn) {
    defer {
        shield within 1_000 {                    // cancellation waits, at most a second
            let _ = conn.write("bye\n".to_bytes());
        }
        conn.close();
    };
    …
}

fun main(console: Console, root: Scope) {
    root.spawn(serve(…));
    // Ctrl-C: the root scope's tasks and `main` are cancelled with `.Shutdown`,
    // their defers run, and the program exits 130; a second Ctrl-C exits at once
}
```

## CONTEXT
- **Draft v18 §4.2** rules 4 (`.Deadline` failure) [SCO-04], 10–11 (`within`, `until`, `remaining`, nesting only tightens) [OPN-07, SCO-08]; **§4.3** rules 3 (`.Deadline`, `.Shutdown` reasons) [CAN-03], 5 (`shield`) [CAN-05], 9 (Ctrl-C) [CAN-09]; **§3** rule 12 (Ctrl-C cancels the root scope whether or not anyone holds `Signals`) [CAP-12].
- **Asked of the user:** `until t` takes the runtime's monotonic milliseconds, the clock `task.sleep` uses (`clock.monotonic_ms()` on the real `Clock`); a test's fake `Clock` stands still, so tests use `within`.
- **Builds on:** PRP-230's header words (`scope s word… { }`) and policies; PRP-227's cancellation (reasons, safe points, scopes catching at their edge); the scheduler's sleepers (`runtime/tasks.c`).

## DESIGN

### Deadlines
1. **`within ms`** (an `i64`, evaluated as the scope starts) and **`until t`** (an `i64` of monotonic milliseconds) give the scope a deadline; both at once is E0477. A nested scope's deadline is the earlier of its own and the enclosing scope's (in the same task, or the task's scope for a task's body).
2. **When it passes,** the body and every task of the scope are cancelled with `.Deadline`, as on a failure; the scope waits for them, and its value is `Err` with `f.kind` `.Deadline` and `f.task` the scope's name. This holds for an `independent` scope too: the deadline is the scope's own failure. A failure that came first stays the scope's value.
3. **`s.remaining(): Option<i64>`** — the milliseconds left (never below 0), or `None` without a deadline.
4. **Prelude changes:** `FailureKind` gains `Deadline`; `Reason` gains `Deadline` and `Shutdown`.
5. **The runtime keeps deadlines with the sleepers:** the scheduler checks them where it wakes sleepers, so a deadline fires while tasks wait, and while a busy loop runs (at its preemption points).

### `shield within ms { body }`
6. **An expression whose value is its body's.** Cancellation reaching the task inside it is held until the block ends (then it applies at once, at the next safe point) or its deadline passes (then it applies wherever the task is: a wait inside the shield is woken to unwind). A shield always has a deadline: `shield` without `within` is a parse error (E0211).
7. **Inside a shield, waits aren't cut short** by cancellation; nested shields keep the earlier deadline.
7a. **A shield entered while its task already unwinds** (in a `defer` running for a cancellation) is cut off at its deadline too (the user's call, during the build): the cut-off is a second unwind, which the shield's own landing pad stops at its edge, running the block's `defer`s; the code after the shield runs, and the first unwinding goes on. Every shield has such a pad, as a scope's body has; an unwind it stops that isn't a cut-off (a panic, a cancellation reaching a shield entered normally) goes on just past the shield.

### Ctrl-C
8. **The first Ctrl-C cancels the root scope** — its tasks and `main` — with `.Shutdown`: they unwind at their next safe point, running their defers, and the program exits with 130. A program that never started a task (no scheduler running) exits at once, as today. **A second Ctrl-C** exits with 130 at once.
9. **The handler only sets a flag** (async-signal-safe); the scheduler acts on it where it wakes sleepers, and a wait in `epoll` or `nanosleep` is interrupted by the signal. It is installed when the runtime starts, before process lockdown, so lockdown doesn't change.

## DECISIONS MADE IN DISCOVERY (mine, for review; 8–10 made during the build)
1. **`within` and `until` together is E0477**, since a scope has one deadline.
2. **A deadline fails even an independent scope**, as its body's failure does: it is the scope's own.
3. **`remaining()` gives `Option<i64>`**, `None` without a deadline, so code can tell "no deadline" from "none left".
4. **`shield` is an expression**, like `scope`, so `let x = shield within 100 { … };` works; `shield` is contextual (a word only before `within`).
5. **Exit code 130 for a shutdown** (128 + SIGINT, the shell's convention), after the unwinding; a panic stays 101.
6. **A program that never started a task exits on the first Ctrl-C at once:** there is no scheduler to reach a safe point, and a busy `main` must still stop.
7. **Deadlines use the runtime's monotonic clock** (the user's choice for `until`), like `task.sleep`, without a `Clock`: a deadline is the program's own bookkeeping, not a reading of the world.
8. **A shield whose type isn't `()`, cut off while its task unwinds, ends the program** (exit 101, with a message): there is no value to go on with, as with a panic while unwinding. A `()` shield, the cleanup case, goes on.
9. **`shield {` is E0211 only at a statement's start**, where `{` can't begin anything else; elsewhere `shield` stays a name (`if shield { … }`).
10. **A task parked in the scheduler is woken when the scheduler cancels it** (a deadline, Ctrl-C): before, a cancellation skipped the running fiber, which is the one parked while the scheduler runs, so a `main` asleep at Ctrl-C slept on.

## REQUIREMENTS

### Must Do
- `within`, `until`, `remaining`, `.Deadline` (failure and reason), nesting.
- `shield within ms { … }` with held and released cancellation.
- Ctrl-C: `.Shutdown`, exit 130, a second Ctrl-C at once.
- Spec §3–§6, §9, "Changes since v16"; §11 row 199; CHANGELOG; E0477's page; error code pages if a new one is needed.

### Must NOT Do
- `Signals` (other signals as a `Receiver`): with channels' rework.
- Virtual time, `background`, `memory n`, `stack n`: later phases.

## TESTS
- [x] Checker unit tests: the new words and their types; `within` with `until` (E0477); `remaining()`'s type; `shield` parsing and E0211 without `within`.
- [x] `tests/programs/concurrency/deadlines/`: a scope past its deadline (tasks waiting, and busy) failing with `.Deadline`, defers run; `remaining()` falling; a nested scope keeping the earlier deadline; an earlier failure winning; `until`; a shield finishing its cleanup under cancellation, and one cut off at its deadline.
- [x] Ctrl-C: an integration test sending SIGINT to a running program (defers run, exit 130), a second SIGINT exiting at once, and a program without tasks exiting at once.
- [x] Targeted: `--lib`, `programs`, `leaks`, `spec`, `explain`, `comments`, `tasks_runtime`, the new Ctrl-C test.

## ACCEPTANCE
- [x] Deadlines cancel and report; shields hold cancellation off for at most their deadline; Ctrl-C shuts the program down cleanly.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [x] Spec, CHANGELOG and error pages updated.
