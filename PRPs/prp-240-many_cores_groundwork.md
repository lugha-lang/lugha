## FEATURE: Groundwork for many cores, still on one worker: every value that reaches a second task is marked shared on the side that hands it over, waiting gets a real handoff, the scheduler's state moves into a worker record, a debug mode catches the next unmarked crossing; milestone 208.

**Status:** approved 2026-11-18; built 2026-11-18 (milestone 208).

## OBJECTIVE
Nothing changes for a program: same output, same speed within noise. What changes is what a later PRP can rely on when tasks run on several threads at once:

```
$ LUGHA_RC_CHECK=1 lughac run server.la
panic: an unshared object reached a second task (made by task 3, used by task 7)
```

`LUGHA_RC_CHECK=1` records which task made each unshared object and panics when another task touches it. Every concurrency program in `tests/programs/concurrency/` runs clean under it. On one thread such a crossing is harmless; on several it is a use after free.

## CONTEXT
- **Research:** `docs/research/many-cores.md` (committed with this PRP): §1 lists every hole fixed here, §3 items 1, 5 and 7, §4's first PRP.
- **Draft v18 §3.1** rules 2 and 14 (counts atomic only for shared objects; marking before crossing); **§4.6** rule 1 [SCH-01] (workers), which the next PRPs build.
- **Code:**
  - Counting: `runtime/rc.c`, `src/codegen/rc.rs` (the inline count), `rc_objects.rs` (the unique-array check).
  - Marking sites: `src/codegen/task_handles.rs` (join), `actors.rs` (calls), `channels.rs` (sends, already right), `tasks.rs` (spawns, already right), `consts.rs`.
  - Waiting: `runtime/tasks.c` (`block`, `wake`, `switch_to`, `Fiber.fired`), and every waiter: `chans.c`, `actors.c`, `scopes.c`, `scope_time.c`, `net.c`, `jobs.c`.
  - Unmarked handovers: task names and errors (`tasks.c`, `scopes.c`), cancellation reasons, failure handlers' environments, cells, `dir.c`'s `dir_working`, `jobs.c`'s held `Dir`.
  - `runtime/print.c` (`show_stack`).
- **Security:** no `unsafe` beyond the existing GEPs in `codegen/rc*.rs`; nothing on CLAUDE.md's list.
- **Decisions:** none open.

## DESIGN
1. **Mark where the value leaves, before it is published.**
   - A task marks its result before it finishes. A joiner no longer marks; it only copies.
   - An actor marks its answer before replying. The caller no longer marks.
   - Already right, unchanged: channel sends and spawn arguments.
   - Each handover listed in CONTEXT is marked where it is stored for another task:
     - a task's name;
     - a cancellation's reason;
     - a task's error box and panic text;
     - a failure handler's environment;
     - a cell's contents;
     - the working folder's `Dir` and a job's held `Dir`.
   - The build audits the rest of the runtime for the same pattern. Any further hole it finds is fixed and listed here.
2. **Constants become immortal once filled.** `lugha_const_init` ends by freezing each constant and everything it reaches (`lugha_rt_rc_freeze`): the same walk as marking, but setting `RC_IMMORTAL`. A constant's count is then never written, so workers reading the same constant don't fight over its cache line.
3. **Every count access is a C11 atomic.**
   - Generated code's inline count load and store become `monotonic` (relaxed). They compile to the same `mov` on x86-64 and the same `ldr`/`str` on ARM64. What they buy is a program that is correct under the C11 model and quiet under ThreadSanitizer.
   - The runtime's plain-count path does the same with `__atomic_*_n(…, __ATOMIC_RELAXED)`.
   - The atomics only on shared counts (relaxed increment, release decrement, acquire before freeing) are unchanged.
   - Measured on `bench/`: LLVM may optimise atomic loads less freely.
4. **Waiting becomes a handoff.**
   - A task's wait state becomes one atomic word: `RUNNING`, `PARKING`, `WAITING`, or woken with a reason (`fired`, as today).
   - `block()` sets `PARKING`, then switches. Once the switch has saved its registers, the task landing on the worker turns `PARKING` into `WAITING`. That is the first point at which the task can be resumed.
   - `wake()` is a compare-and-swap:
     - `WAITING` to the reason: it queues the task;
     - `PARKING` to the reason: the landing side queues it instead, once the context is saved;
     - anything else: already woken, so it does nothing (as today).
   - On one worker `PARKING` is never seen by a waker. The protocol is in place and tested so the workers PRP doesn't have to touch every wait.
5. **A worker record.** The scheduler's per-thread state moves into one `Worker` struct, reached through one pointer (`worker`): `current`, `zombie`, `on_c_stack`, the ready queue, `last_spawned`, the marking worklist and `select`'s random state. There is one static `Worker`.
   - Shared state (sleepers, the live list, the stack pool, the room queue) stays global for now. It moves or gets locks in the workers PRP.
   - The two values generated code reads at every call, the step budget and the stack limit, stay plain globals. Which of the three ways to make them per worker is a measured decision for the workers PRP (research §3 item 7).
6. **Printing a `Ref` remembers its boxes per task.** `show_stack` moves into the `Fiber`, so a task preempted while printing can't corrupt another task's print. This is a real bug today, on one thread.
7. **`LUGHA_RC_CHECK=1`, the owner check:**
   - At start, every new object's count is written in a checked form: negative, outside `RC_IMMORTAL` and `RC_FREED`, with the count in the low 32 bits and the making task's number in the bits above.
   - Generated code's inline path already sends every negative count to `lugha_rt_rc_slow`, so no codegen change is needed. The slow path:
     - panics when the current task differs from the maker;
     - otherwise counts as for a plain count.
   - Marking turns a checked count into an ordinary shared one.
   - The unique-array check sees a checked count as not unique and copies first. That changes only speed.
   - The runtime's own retains and releases (`lugha_rt_rc_retain`/`_release`, the reaper) skip the owner check: the runtime legitimately frees a finished task's leftovers from the next task.
   - Like `LUGHA_RC_POISON`, it is read once at start, documented in the spec's §9 and CLAUDE.md's commands, and costs nothing when unset.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Groundwork as its own PRP,** before the allocator and the workers, so each later PRP changes one thing and is measured alone.
2. **The landing side completes the park** (design 4), rather than Go's waker spinning on `PARKING`. No spin loop, and a waker never waits on a worker that is mid-switch.
3. **Immortal constants** over marking them shared: no counting at all on the hottest shared values.
4. **The owner check is a run-time switch** riding on the existing slow path for negative counts. No compiler flag, no second build of the program, and no codegen change.
5. **Relaxed atomics in the inline path now,** while there is one worker, so any cost shows up in this PRP's measurements and not mixed into the workers PRP.
6. **The hot values (budget, stack limit) stay as they are.** Choosing among local-exec TLS with a guarded accessor, a task pointer, or Go's guard word needs quiet-machine measurements and belongs with the workers.

## MADE DURING THE BUILD
- **`runtime/waiting.c`** (the user's choice of split): `tasks.c` would pass 500 lines, so waiting and waking (`wake`, `block`, `switch_to`, `schedule`) moved there, with the handoff.
- **The owner check is chosen at build time as well as run time** (the user's call, changing decision 4): supporting a checked count of one in copy-on-write cost fannkuch 4% in every build, even off the hot path. `lughac` reads `LUGHA_RC_CHECK=1` too and emits that test only then (`codegen::Instruments`, formerly `CallCosts`); a program built without it still checks when run with it, copying arrays on writes.
- **Counts are `unordered` atomics, not `monotonic`** (changing design 3): a `monotonic` load can't be hoisted out of a loop, costing fannkuch 5%; `unordered` is never torn and defined under a race, which is all the fast paths need (they only write unshared counts; a shared count's bit never clears), and costs nothing.
- **The handoff compiles to plain loads and stores while one worker runs** (`LUGHA_TASKS_CROSS_THREADS`, 0 today): its compare-and-swaps cost a channel ping-pong about 20%, and even a flag tested at run time 8%. `tests/waiting_runtime.rs` builds the runtime both ways and expects the same results.
- **Marking also happens where the PRP's list didn't say:** an actor marks what it stored in its state when it stops (its state box was marked at spawn, but values stored into it later weren't: a gap in "shared reaches only shared"); a cell marks the bytes stored into it, since every copy of a handle shares the cell. Spawn arguments and channel sends were already right.
- **Constants are counted as freed when frozen,** and no longer released after `main`, so `LUGHA_RC_REPORT` still ends at zero live.
- **Marking by a task other than the maker is caught too:** that is the wrong-side marking `join` and actor calls had, which the check would otherwise miss.
- **No C test of a pointer C keeps:** an extern returning a string copies it, so C can't hand a Lugha object to another task; the `join`, actor and constant tests fail without their fixes instead.
- **`chanping` is 13% slower** (the other benchmarks within 1%): the handoff's extra state checks. Accepted for now, recorded in TODO.md and `bench/RESULTS.md`.

## REQUIREMENTS

### Must Do
- Design items 1–7.
- Spec:
  - §7 (counting across tasks: where values are marked, constants immortal);
  - §9 (`LUGHA_RC_CHECK`, the runtime table);
  - "Changes since v16";
  - §11 row 208.
- CHANGELOG; CLAUDE.md's commands line for `LUGHA_RC_CHECK`; `bench/RESULTS.md` (before and after).

### Must NOT Do
- Start a second worker, change the allocator, or touch the step budget and stack limit (PRPs 2 and 3).
- Lock channels, scopes or actors (workers PRP).
- Change a program's output or exit code.

## TESTS
- [x] `tests/rc_check.rs` (new):
  - [x] Every program under `tests/programs/concurrency/` runs under `LUGHA_RC_CHECK=1` with its expected stdout and exit code.
  - [x] So do the helper and blocking-extern cases that start tasks (run once with `LUGHA_RC_CHECK=1` exported; not a test of their own), and every other end-to-end program (387, run once).
  - [ ] ~~A C file that keeps a string from one task and gives it to another through an extern panics with the owner message.~~ Not possible (see the build notes); the tests below fail without their fixes instead.
  - [x] Joining a task's string result from two joiners passes the check (it failed before the fix).
  - [x] So does an actor call returning an array, and a constant read by several tasks.
- [x] `tests/programs/concurrency/printing/` (new): two tasks printing `Ref` boxes that hold themselves, side by side and preempted partway (an item type with its own `debug`, so the print has steps), the shorter ending while the longer runs: each prints as it would alone (`true false` before the fix).
- [x] `src/codegen/rc_tests.rs`: the inline count load and store are `monotonic`; `lugha_const_init` calls `lugha_rt_rc_freeze`; a join site no longer calls `lugha_rt_rc_mark`.
- [x] The wait handoff: `tests/rc_check.rs`, or a C harness built against the runtime, exercising `wake` on a task still `PARKING`; plus the existing waiting suites unchanged.
- [x] Targeted:
  - `--lib`, `programs`;
  - `tasks`, `channels`, `actors`, `scopes`, `helpers`, `blocking_externs`, `callbacks`;
  - `rc`, `spec`, `arm` (the waiting suites and the count suites; exact file names checked during the build).
- [x] `python3 bench/run.py --lang lugha --runs 7` before and after, recorded.

## ACCEPTANCE
- [x] Every concurrency program passes under `LUGHA_RC_CHECK=1`; outputs unchanged without it.
- [x] Benchmarks within noise of milestone 207, or the difference recorded and accepted.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [x] Spec, CHANGELOG, CLAUDE.md and `bench/RESULTS.md` are updated.
