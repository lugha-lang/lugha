## FEATURE: Tasks on every core: one worker thread per core runs tasks in parallel, sharing one run queue, with the runtime's shared state under one lock; milestone 210, the first of two workers PRPs.

**Status:** approved 2026-11-18; built 2026-11-19 (milestone 210).

## OBJECTIVE
```
fun busy(n: i64): i64 { let mut s = 0; for i in 0..n { s += i % 7; } s }

fun main(console: Console) {
    let r = scope s {
        let ts = [s.spawn(busy(400_000_000)), s.spawn(busy(400_000_000)),
                  s.spawn(busy(400_000_000)), s.spawn(busy(400_000_000))];
        let mut sum = 0;
        for t of ts { sum += t.join().unwrap(); }
        sum
    };
    console.println(r.unwrap());
}
```
Today this takes four times as long as one `busy` call. With this PRP it takes about as long as one on a machine with four free cores. `LUGHA_WORKERS=1` restores today's single thread.

## CONTEXT
- **Research:** `docs/research/many-cores.md`: §1 (what assumes one thread), §3 items 5–9, §4's third PRP. The user split it in two: this PRP makes many cores **correct**; the next (per-worker queues, stealing, Go's parking protocol, timers per worker, finer locks, scaling benchmarks) makes them **fast**.
- **Draft v18 §4.6** rules 1 [SCH-01] (one worker per core), 2 (step preemption), 6 (I/O and helper results through the poll), 11 [SCH-11] (deterministic and shuffle runs on one worker); **§4.7** rule 1 [STK-01] (C on each worker's C stack).
- **Built on:** PRP-240 (marking, the owner check, the waiting handoff, the `Worker` record), PRP-241 (the allocator, the thread-local `Worker`).
- **Code:**
  - The scheduler: `runtime/tasks.c`, `waiting.c`, `spawn.c`, `stacks.c`, `net.c` (the idle wait, polling, the deadlock check), `jobs.c`.
  - The shared state: `scopes.c`, `scope_policy.c`, `scope_time.c` (deadlines, shields, Ctrl-C), `chans.c`, `actors.c`, `unwind.c`.
  - Process start and end: `lugha_rt.c`, `stack.c` (the `SIGSEGV` handler), `lockdown.c`.
  - Codegen: `codegen/tasks.rs` (`preempt_point`), `call_costs.rs` (the entry check), `callbacks.rs` (`lugha_cbv_<n>`).
- **Security:** `runtime/lockdown.c` is read, not changed. Workers start only after lockdown, as helpers do, so Landlock and `no_new_privs` cover them, and seccomp is synchronised to every thread. No `unsafe`, no new dependency.
- **Decisions:** the user's: two PRPs; `LUGHA_WORKERS=1` for thread-bound C libraries until `pinned`. None open.

## DESIGN
1. **Workers.**
   - **How many:** `LUGHA_WORKERS=n` (1 to 256; anything else ends the program at start: `panic: LUGHA_WORKERS must be from 1 to 256`), else one per CPU the process may run on (`sched_getaffinity`).
   - **Which thread is which:** the main thread is worker 0. The others start together when the first task is spawned after lockdown, so a program that never spawns never starts a thread.
   - **Deterministic and shuffled runs** (`lughac test`) always use one worker.
   - **Lifetime:** a worker never exits, since blocks it owns are freed back to it. Each has its own C stack, its own alternate signal stack, and SIGINT blocked.
2. **One run queue, one lock.**
   - **What the lock guards:** the ready queue, sleepers, live fibers, the stack pool, scopes, channels, actor mailboxes, joins, deadlines and shields, the poller, the job room and the room queue are all guarded by one mutex, `rt_lock`.
   - **When it's held:** every runtime entry that touches them takes it, and it stays held across a task switch: the fiber landing on the thread releases it when it returns to generated code. A new fiber releases it before running its entry.
   - **What runs in parallel:** Lugha code, counting, allocation, printing and C calls run without it. Only scheduling serialises.
   - Finer locks are the next PRP.
3. **Idle workers and the poller.**
   - **Going idle:** a worker with nothing to run releases the lock. One idle worker at a time is the **poller**: it waits in `epoll_wait` for sockets, helper jobs and a wake-up `eventfd`, timed out at the next sleeper's deadline. The rest sleep on a condition variable.
   - **Waking an idle worker:** making a task ready wakes one idle worker only when more tasks are ready than the waking worker will take itself. A channel ping-pong therefore stays on one thread instead of bouncing between cores.
   - **Deadlock:** declared only when every worker is idle with nothing outstanding: no sleeper, socket wait or helper job.
4. **The two hot values leave thread-local storage altogether.**
   - **Stack check:** every stack Lugha code runs on is aligned to its size (a power of two, `lugha_rt_stack_mask` + 1, fixed at start). Task stacks, `main`'s and each worker's C stack all come from aligned regions. Each function's entry check becomes `(sp & mask) < STACK_RESERVE`: two instructions, no memory load. An unwinding overflow's extra room is granted by `lugha_rt_stack_low` itself.
   - **Step budget:** the countdown lives in the top word of the running task's stack, at `(sp | mask) - 15`, already unused. Each task has its own and moves with it.
   - **Why:** generated code reads no thread-local value, so LLVM can't keep one from before a task moved (LLVM #98479). It's measured against milestone 209 on `fib` and `nbody`; if it costs more than 3%, I stop and report.
5. **The runtime's own thread-local reads after a switch.**
   - **The problem:** C code that reads `worker` after `lugha_rt_task_swap` may be on another thread. GCC treats a thread-local address as fixed within a function, so it may reuse one computed before the swap.
   - **The fix:** after a swap, `worker` and `current` are read through `worker_now()`, which computes the address in `asm volatile`, so it can't be reused. Only the functions that switch (`switch_to`, `block`'s return, a new fiber's start) are affected.
6. **`main` ends where it began.** `main`'s task may move between workers. When it returns, it hands itself to worker 0, the main thread, and waits there, so `lugha_rt_main` returns on the process's own stack. The root scope is waited for and the program exits from the main thread, as today.
7. **C and signals.**
   - **C stacks:** each worker's C stack is an aligned 8 MiB stack with a guard page below it. The `SIGSEGV` handler reports a fault on the faulting thread's own C stack guard: an overflow in C still ends the program, now on any worker.
   - **Ctrl-C:** SIGINT reaches only the main thread. Its handler sets the flag as now and wakes the poller through the `eventfd`.
   - **A callback-site closure** (`lugha_cbv_<n>`), always the same value, becomes a constant initializer instead of a store, so workers don't race on it.
8. **Thread-bound C libraries.** Until `pinned` (a later PRP of this phase), a program using a C library that needs one thread runs with `LUGHA_WORKERS=1`. This is documented in the spec's C section and on `extern`.
9. **What stays single-threaded:**
   - The wait handoff's atomics stay compiled out (`LUGHA_TASKS_CROSS_THREADS` 0): under one lock nothing races on `fired`.
   - Virtual time and shuffling only exist with one worker.
   - The owner check works unchanged, since task numbers are taken under the lock.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The hot values come from the stack pointer** (design 4), not thread-local storage, a task pointer or Go's guard word. It's the only option with neither a stale-read hazard nor a register or call cost. It needs aligned stacks, which take only rounding at mapping time.
2. **The main thread is a worker,** not a thread that waits for the others. `main`'s own task and C code keep running there as today, and nothing changes for a program with no tasks.
3. **Workers start at the first spawn after lockdown,** not at start-up, so a program with no tasks has one thread and lockdown covers every worker.
4. **Waking only for surplus work** (design 3). Waking on every ready task would bounce `chanping` across cores at a futex wake per message. The next PRP replaces this with Go's spinning protocol.
5. **The test suite's order.**
   - `tests/programs/` keeps comparing exact output, run with `LUGHA_WORKERS=1` set by its runner, since many programs print from several tasks and depend on today's order.
   - A new `tests/workers.rs` runs every concurrency program with 4 workers, 5 times each, comparing exit codes and stdout as a sorted set of lines. A program whose lines themselves depend on order is listed with its reason.
   - On multiple cores the order really is free. The spec says so where it now describes the order.
6. **ThreadSanitizer** builds of a C harness, with the fiber annotations (`__tsan_switch_to_fiber`) compiled in only under `__SANITIZE_THREAD__`.

## MADE DURING THE BUILD
- **The lock is atomics and a futex, not a pthread mutex** (design 2): it is taken by one fiber and let go of by another on the same thread, which a pthread mutex doesn't allow and ThreadSanitizer reports as an unlock by the wrong thread. An ownerless lock has neither problem, and ThreadSanitizer follows its atomics across fibers. Idle workers sleep on a futex word of their own for the same reason.
- **The lock is free until the workers start:** taking and letting go of it does nothing while one thread runs; at the first spawn after lockdown the spawning thread takes it for real, inside its open sections, before any other worker exists.
- **A blocking fiber with nothing ready switches to its worker's idle loop** (a fiber of its own, on a pooled stack), instead of waiting on its own stack, so any worker can take it once woken.
- **A task is queued only once it is set up** (`lugha_rt_task_go`, a new runtime symbol): with several workers, a task could fail before its spawner had named it (a crash, reading a null name) or before an actor's record was attached.
- **A new timer wakes the poller** (`timers_changed`): a sleep, deadline or shield added while another worker waited in `epoll_wait` with a longer timeout would otherwise sleep past it (found as a hang).
- **`println` prints its line whole** (`lugha_rt_print_begin` and `_end`, stdout's lock), and `eprintln` likewise: two tasks on two workers interleaved inside lines.
- **Program ends:** every `exit` in the runtime goes through `program_exit`, which lets only the first thread end the process; a C stack overflow on several workers at once is reported once.
- **`tests/callbacks.rs` runs on one worker:** its case comparing C's stack addresses from `main` and from a task assumes one C stack.
- **`scope_policy.c` holds what a task handle does** (`join`'s status, `cancel`, `name`), moved out of `scopes.c`, which the sections took past 500 lines (the user's choice of split).
- **`tests/workers.rs` lists one order-dependent program:** `scope_policies/handler.la` names a task after starting it.
- **`chanping` is 51% slower,** on one worker too: the sections and the fresh thread-pointer reads cost on every scheduler call. Recorded in TODO.md and `bench/RESULTS.md`; the PRP asked only that it be recorded.

## REQUIREMENTS

### Must Do
- Design items 1–9.
- **Spec:**
  - §6: tasks run on several threads, on any worker; order across tasks isn't fixed; `LUGHA_WORKERS`.
  - §7: the shared bit now matters.
  - §8: C runs on its worker's C stack; thread-bound libraries and `LUGHA_WORKERS=1`.
  - §9: `lugha_rt_stack_mask`; the budget word; the runtime table; `lugha_rt_task_budget` and `lugha_rt_stack_limit` gone.
  - Also "Changes since v16" and §11 row 210.
- CHANGELOG (under Breaking: task order is no longer fixed), CLAUDE.md, `bench/RESULTS.md`, TODO (the next PRP's items).

### Must NOT Do
- Per-worker queues, stealing, spinning, timer heaps or finer locks (the next PRP).
- `pinned`, callbacks from C's own threads (PRP 6).
- Several workers in deterministic or shuffled runs.

## TESTS
- [x] `tests/workers.rs` (new):
  - [x] Four tasks calling a C function that returns `pthread_self()` (a `[link]` C file) record at least two different threads under `LUGHA_WORKERS=4`, and exactly one under `LUGHA_WORKERS=1`.
  - [x] The objective's program prints the right sum with 1 and 4 workers.
  - [x] Every concurrency program under `LUGHA_WORKERS=4`, 5 runs each, as decision 5 says.
  - [x] `LUGHA_WORKERS=0` and `=x` end the program with the message.
  - [x] A deadlock with 4 workers is reported as with one.
  - [x] Ctrl-C with tasks on several workers: the root scope is cancelled and the program exits 130.
  - [x] `main` blocking on another worker's task returns and exits normally (every multi-worker program run exercises `main`'s return home; not a test of its own).
  - [x] A C stack overflow on a worker other than the main thread ends the program with the C-stack message.
- [x] A C harness with 4 workers (in `tests/workers.rs`: 64 tasks passing a token round a ring of channels, then joined), plain and under ThreadSanitizer, clean.
- [x] With 4 workers, no unmarked crossing and nothing leaked: checked in `tests/workers.rs`'s sweep (its last run of each program).
- [x] `src/codegen` unit tests: the entry check is an `and` and a compare on the stack pointer; the countdown reads the stack's top word; no `lugha_rt_task_budget` or `lugha_rt_stack_limit` global.
- [x] Benchmarks against milestone 209: fib, fannkuch, mandelbrot, nbody, binarytrees, wordfreq within 3%. Also chanping, and the objective's program with 1, 2 and 4 workers.
- [x] Targeted: `--lib`, `programs`, `leaks`, `rc_check`, `workers`, `tasks_runtime`, `waiting_runtime`, `rc_alloc_threads`, `helpers`, `blocking_externs`, `callbacks`, `spec`, `arm`, `codegen`, `comments`.

## ACCEPTANCE
- [x] CPU-bound tasks run in parallel; every concurrency program behaves with 4 workers; ThreadSanitizer is clean on the harness.
- [x] Single-task benchmarks within 3% of milestone 209; chanping's change recorded.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [x] Spec, CHANGELOG, CLAUDE.md, TODO and `bench/RESULTS.md` are updated.
