## FEATURE: Pinned handles and pinned scopes: a C type declared `pinned` never leaves the worker thread that made it, and `scope ui pinned { … }` keeps its body and every task under it on one worker, so C libraries with thread rules of their own (GTK, OpenGL) work on many workers; milestone 214, the first of three PRPs for callbacks and thread-bound values.

**Status:** approved 2026-11-20.

## OBJECTIVE
A program using a thread-bound C library runs correctly on every worker count, with no `LUGHA_WORKERS=1`:

```
extern type Ctx pinned;
extern fun ctx_new(): Ctx;
extern fun ctx_draw(c: Ctx, n: i64);       // C checks it runs on the thread that made `c`

fun draw(n: i64) {
    let c = ctx_new();                      // this task is pinned from here on
    for i in 0..n { ctx_draw(c, i); task.yield_now(); }
}

fun main(console: Console) {
    let r = scope ui pinned {               // its body and tasks: the main thread
        for _ in 0..8 { ui.spawn(draw(1000)); }
    };
    r.unwrap();
    console.println("drawn");
}
```

```
$ LUGHA_WORKERS=4 lughac run ui.la
drawn
```

Passing a pinned handle to another task is a compile error:

```
error[E0475]: a value sent to another task must be `Sendable`: `Ctx` is a pinned handle
```

## CONTEXT
- **Spec:** the Draft, §4.8 rule 1 (CBK-01); spec §7 ("needs `LUGHA_WORKERS=1` until `pinned` types arrive"), §4 (handles, scope settings, `Sendable`), §6 (switching), §8 (handles, `blocking` externs).
- **The other two PRPs**, after this one: callbacks registered against a scope (queued by default, `inline`, panics at the C boundary, events dropped after the scope ends); then callbacks on C's own threads (`Ring<T>`, `queued`, the allocation lint).
- **Code:**
  - Parser: `src/parser/` (`extern type`), `parser/tasks.rs` (scope words).
  - Checker: `check/sendable.rs`, `check/derive_meets.rs` (handles), the scope settings' checks, the `blocking` extern checks.
  - Codegen: extern calls, `lugha_rt_scope_enter`.
  - Runtime: `runq.c` (`make_ready`, `take_ready`, `home_fiber`), `workers.c` (`main_go_home`, parking, rousing), `parking.c`, `scopes.c`, `spawn.c`.
- **Decisions:** none open; ledger CBK-01 approved.
- **Security:** nothing on CLAUDE.md's list; no `unsafe`, no dependency.

## DESIGN
1. **`extern type Name pinned;`** declares a pinned handle, otherwise a handle like any other.
   - It is never `Sendable`: E0475, naming the part as "a pinned handle". So a pinned value never reaches another task: not as a spawned call's argument or capture, a channel's element, nor an actor's field or message.
   - A `blocking` extern can't take or return one (new **E0480**), since it would run on a helper thread.
2. **A task that touches a pinned handle is pinned:** before every call of an extern taking or returning a pinned type, generated code calls `lugha_rt_pin_here()`. From then on the task runs only on its current worker, until it ends. A task already pinned costs one compare.
3. **`scope s pinned { … }`**, a new scope setting (at most once, E0477 as the others), combinable with the rest:
   - The owner runs its body on one worker, and every task the scope starts runs there too.
   - A scope opened inside a pinned scope, by its body or by its tasks, is pinned to the same worker.
   - When the scope ends, the owner is free again, unless it was pinned before.
   - **The worker:** the current one; for `main`, the main thread, where `main` goes first, as it does to end the program.
4. **The runtime:**
   - A fiber may name the worker it is pinned to. A pinned fiber made ready, by anyone, goes to its worker's own inbox (a list under a small lock), never to a run queue other workers steal from. Its worker is roused if parked.
   - A worker takes from its inbox first, alternating with its own queue when both have work, so neither starves.
   - A worker never parks with its inbox full, and the deadlock check counts inboxes.
   - `main` returning to the main thread becomes the same mechanism, `main` pinned to the main worker, replacing `home_fiber`.
   - With one worker (deterministic and shuffled runs included), pinning does nothing.
5. **§7's sentence** about `LUGHA_WORKERS=1` for thread-bound libraries goes; §4 and §8 describe `pinned`.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Touching a pinned handle pins the task, at run time,** rather than a rule allowing pinned externs only inside pinned scopes. It is always safe, needs no new error, and a task that never touches one is never slowed. A static rule would need to follow calls across functions.
2. **Pinned values never cross tasks, even inside a pinned scope.** Making that allowed would need `Scope` to carry whether it is pinned in its type. A C callback still hands a pinned value to the task it runs in, which arrives with the next PRP.
3. **Pinning inherits down the scope tree,** so "everything under `ui` runs on the UI thread" holds for helpers that open scopes of their own.
4. **A pinned scope in `main` pins to the main thread:** toolkits expect the thread that started the process. Elsewhere it pins to the task's current worker.
5. **One mechanism for `main`'s return home and for pinning:** the inbox replaces `home_fiber`, so there are fewer special cases in the scheduler.
6. **E0480** for a `blocking` extern with a pinned type, rather than widening E0479 (a `blocking` extern with a callback), since the fix differs.

## REQUIREMENTS

### Must Do
- Design items 1–5; E0480 with its `docs/errors/` page.
- Spec §4 (handles, `Sendable`, scope settings), §6 (switching), §7, §8, §9 (`lugha_rt_pin_here`), "Changes since v16", §11 row 214; CHANGELOG; TODO (the "Green threads on several OS threads" line, if pinning was what it waited on); the language server's keyword lists and the VS Code grammar if they list scope settings (bumping the extension's version if it changes).

### Must NOT Do
- Callbacks queued as tasks, `inline` and `queued` marks, panics caught at the C boundary, or `Ring<T>`: the next two PRPs.
- Change what any program without `pinned` does or prints.

## TESTS
- [x] `tests/programs/`:
  - [x] `extern type X pinned;` parses, and checks (one worker, exact output).
  - [x] E0475 for a pinned handle sent each way: a spawn argument, a capture, a channel, an actor field.
  - [x] E0480 for a `blocking` extern taking one.
  - [x] E0477 for `pinned` twice.
- [x] A C library through `[link]`, on four workers, ten runs each: every call on a pinned handle runs on the thread that made it, with tasks that yield, sleep and wait on channels in between:
  - [x] in a task pinned by touching the handle;
  - [x] in `scope ui pinned` in `main` (the main thread) and in another task (its worker), with nested scopes;
  - [x] an actor started in a pinned scope.
- [x] `tests/scheduler_stress.rs`, plain and under ThreadSanitizer: pinned fibers woken from other workers, by channels, timers and yields, run only on their worker (joins, and that unpinned work is still stolen, not covered: see the build notes); a worker with pinned work never parks.
- [x] `tests/workers.rs`'s sweep, `tests/programs/`, `leaks`, `rc_check`, unchanged.
- [x] Benchmarks: the seven and `bench/measure/scaling` within 3% of milestone 213.
- [x] Targeted: `--lib`, `programs`, `leaks`, `rc_check`, `workers`, `scheduler_stress`, `spawn_stress`, `tasks_runtime`, `waiting_runtime`, `callbacks`, `blocking_externs`, `explain`, `spec`, `arm`, `codegen`, `comments`, `lsp*`.

## ACCEPTANCE
- [x] A thread-bound C library runs correctly on four workers, through pinned handles and pinned scopes.
- [x] Programs without `pinned` print what they printed; benchmarks within 3%.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [x] Spec, CHANGELOG, TODO and the editor tooling are updated.

## BUILD NOTES
- **Measured** (`bench/RESULTS.md`): the seven benchmarks within 1.5% of milestone 213; the scaling patterns within run-to-run noise. One run of the scaling script stalled past 25 minutes; 360 timed runs of every pattern on both builds afterwards found no hang, so its cause is unknown.
- **The tests discriminate:** without `pinned`, the same programs on four workers make 1,200–1,800 calls on the wrong thread each run; with it, none in ten runs each.
- **A lost wake-up, found by the new stress case and fixed** (`parking.c`): a worker roused between parking and saying it polls waited in `epoll_wait`, its rouser having bumped the futex word it doesn't wait on. The pinned case hung in 57 of 100 runs; none in 100 after. It could strand `main` going home before. The `park` case's false deadlock under ThreadSanitizer (about 1 in 100 runs at milestones 212 and 213) didn't occur in 600 runs since, where six were expected: the same lost wake-up, so its TODO.md entry is gone.
- **Where the pinned set lives:** `Checked::pinned`, read by codegen to mark externs; the C-stack pass calls `lugha_rt_pin_here` before each call of a marked one.
- **A handle pins for good:** a task pinned by touching a handle stays pinned when a pinned scope it was in ends (the handle may outlive the scope); a scope's pin is undone only when no handle pinned the task.
- **Splits (the user's choices):** `codegen/lower.rs` (at 500 lines) gave the program's source text to a new `codegen/source.rs`; `runtime/scopes.c` (526 lines with this) gave the root scope to `scope_policy.c`.
- **The stress case covers** channels, timers and yields waking pinned tasks from other workers, not joins; that unpinned work is still stolen isn't asserted, only that every task finishes.
- **No editor list of scope settings exists,** so only `extern type … pinned` gained highlighting (language server and grammar, extension 0.36.0).
