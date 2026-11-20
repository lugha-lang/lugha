## FEATURE: Three fixes from the v18 spec review: a task-free program's first Ctrl-C cancels `main` (CAN-13), callbacks without `data` run whenever C calls them (CBK-08, spec only), and registering a `realtime` callback reserves a thread record so no `realtime` call allocates (CBK-09); milestone 226.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
fun main(console: Console) {
    defer console.println("cleaned up");
    let line = console.read_line();     // Ctrl-C here: prints "cleaned up", exits 130
}
```
(It used to exit at once, skipping the `defer`. A loop that never pauses ends at the second Ctrl-C, as before.)

```
extern fun audio_start(cb: realtime fun(MutView<f32>, data), u: data);
audio_start(fill);    // reserves a thread record: C's audio thread's first call allocates nothing
```

## CONTEXT
- **Ledger:** CAN-09, CAN-13, CBK-04…07, CBK-08, CBK-09 (2026-11-20).
- **Spec:**
  - §6 "Ctrl-C" (L1314: "…or one in a program that never started a task, exits with 130 at once");
  - §8 "The scope's end" (L1714) and §6's closures-handed-to-C bullet ("calling it after its scope's body ended is dropped");
  - "`realtime fun(…)`" (L1715) and "Threads C made" (L1716).
- **Code:**
  - `runtime/scope_time.c` `on_interrupt` (exits when `!tasks_started`), with `check_times` applying a held Ctrl-C once the scheduler runs;
  - `runtime/callbacks.c` `foreign_attach`: a thread C made gets a `Fiber` (`lugha_rc_internal`), a C stack (`c_stack_make`), a place on `workers` under `workers_lock`, and the pthread key (`pthread_once`), all on its first callback;
  - `src/codegen/callbacks.rs` (the extern call site that registers a callback).
- **Security:** none of CLAUDE.md's list (the signal handler and the callback runtime aren't on it). No `unsafe`, and no new dependency.

## DESIGN
1. **CAN-13.**
   - `on_interrupt` exits at once only on the second Ctrl-C.
   - The first is held, as it already is once tasks run. `main`'s next pause (any wait, which starts the scheduler) reaches `check_times`, which cancels `main` with `.Shutdown`; it unwinds through its `defer`s and the program exits with 130.
   - Before the scheduler exists, the handler only counts. It wakes no poller, since there is none yet.
2. **CBK-08.** Spec only:
   - "The scope's end" covers callbacks with `data`. One without `data` is a capture-free function: it runs whenever C calls it, after its scope too.
   - §6's bullet is reworded to match.
3. **CBK-09.**
   - `lugha_rt_cb_reserve()` runs before an extern call that passes a `realtime` callback. It makes sure the pthread key exists and that one spare stands ready: a `Fiber` and a C stack, made then.
   - `foreign_attach` first claims the spare with one compare-and-swap. If it gets one, it allocates nothing.
   - The thread is listed for `LUGHA_RC_REPORT` by a lock-free push onto a separate list of C threads, which `foreign_exit` and the report walk under `workers_lock` as today. So the realtime path takes no lock.
   - With no spare (several new threads before another registration), it allocates as today. The spec says so.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **One spare, refilled at each registration,** not a pool sized by a guess: audio and sensor libraries call from one thread they start after registration. A second thread is the documented case.
2. **The reserve is emitted only at call sites passing a `realtime` callback,** so other programs pay nothing.
3. **The C-thread list is separate** from `workers`, so a lock-free push never races a locked unlink of the same list.

## REQUIREMENTS
### Must Do
- Design 1–3.
- Spec: §6 Ctrl-C, §8 "The scope's end" and the §6 bullet, "Threads C made" and "`realtime fun(…)`", §11 row 226, and a "Changes since v17" bullet.
- CHANGELOG.
### Must NOT Do
- Add preemption checks to programs without tasks.
- Change callbacks with `data`.

## TESTS
- [ ] `tests/cli.rs` (or the signals test file): a task-free program in `read_line`, sent one SIGINT, prints its `defer` and exits 130; a busy task-free loop exits 130 on the second SIGINT.
- [ ] `tests/foreign_threads.rs`: a `realtime` callback registered, then called first from a thread C made, takes the spare. A counter of system allocations made on that path (`LUGHA_RC_REPORT`'s count for that thread, or a test hook) is zero.
- [ ] `tests/scope_callbacks.rs`: a callback without `data`, called by C after its scope ended, runs.
- [ ] Targeted: `--lib`, `foreign_threads`, `scope_callbacks`, `rings`, `callbacks`, the signals test, `spec`, `comments`.

## ACCEPTANCE
- [ ] A task-free program's first Ctrl-C runs `main`'s `defer`s when `main` pauses.
- [ ] The spec's callback rules match the code.
- [ ] A `realtime` callback's first call on C's thread allocates nothing after a registration.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
