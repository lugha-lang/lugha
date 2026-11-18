## FEATURE: A scheduler for many cores: a run queue per worker with stealing and a run-next slot, Go's protocol for waking idle workers, timers per worker, and no lock held across a task switch; milestone 211, the first of two "fast workers" PRPs.

**Status:** proposed 2026-11-19.

## OBJECTIVE
Programs print what they printed. What changes is speed:
- `chanping` comes back within 10% of milestone 209 (it is 51% slower at 210);
- many tasks spawned at once spread over every worker without fighting over one queue;
- a task woken by a message usually runs next on its waker's worker, so a ping-pong stays on one core.

```
$ python3 bench/run.py chanping --lang lugha     # 0.28 s at m210 → about 0.18 s
```

## CONTEXT
- **Research:** `docs/research/many-cores.md` §2 (Go's and Tokio's queues, parking, timers), §3 items 3–6. The user split the "fast" half in two: this PRP is the scheduler; the next takes channels, scopes, actors and joins off the global lock, onto locks of their own, and adds the scaling benchmarks.
- **Draft v18 §4.6** rules:
  - 1 [SCH-01]: a queue per worker, a shared queue for new and overflowing work, stealing half;
  - 5 [SCH-05]: first in first out, a woken task running next at most three times in a row, finishing the waker's slice, the shared queue checked at least every 61 picks;
  - 6 [SCH-06]: a busy worker polls without waiting every few switches;
  - 10 [SCH-10]: timers in a heap per worker, checked at every scheduling decision.
- **Built on:** PRP-240 (the parking handoff, written for this, compiled out until now), PRP-242 (workers, the idle loop, the poller, the lock).
- **Code:** `runtime/workers.c`, `waiting.c`, `tasks.c` (the ready queue, sleepers), `spawn.c`, `rt_lock.c`, `net.c` (`poll_sockets`), `scope_time.c` (`check_times`), `lugha_rt.c` (`Worker`, `worker_now`).
- **Security:** nothing on CLAUDE.md's list; no `unsafe`, no dependency.
- **Decisions:** the user's: this split. None open.

## DESIGN
1. **No lock across a switch.**
   - **Blocking:** `block()` remembers how deep its fiber is in runtime sections, closes them all (letting go of `rt_lock`) and switches. When the fiber resumes, on any worker, `block()` reopens them.
   - **What still takes the lock:** only the shared objects (channels, scopes, actors, joins, deadlines, shields, the job room) stay under `rt_lock`. Scheduling itself never takes it.
   - **Waking:** a waker may now meet a fiber still parking, the window PRP-240's handoff covers. Its compare-and-swaps are switched on (`LUGHA_TASKS_CROSS_THREADS` 1).
   - **Starting:** a new fiber starts with no section open and no lock to let go of.
2. **A run queue per worker.**
   - **The ring:** each worker has a 256-slot ring. The owner pushes and pops at its ends without a lock; other workers steal half of it with one compare-and-swap on its head (Go's and Tokio's design, not Chase–Lev). A full ring moves half its tasks to the **shared queue**, a list under its own small mutex, which also takes tasks made ready off a worker (the poller's wake-ups, timers).
   - **Run next:** a task made ready by a running task (a send, a join, an actor's reply) goes into its waker's **run-next slot**, taken before the ring. It inherits what is left of the waker's slice, and at most three times in a row before the ring's turn. Stealers take it only from a worker not running a task.
   - **Fairness:** every 61st pick checks the shared queue first.
   - **Spawning:** a spawned task goes to the spawner's ring.
3. **Waking idle workers: Go's protocol.**
   - **Spinning:** an idle worker spins, looking for work: its own queue, the shared queue, a non-blocking poll, then stealing from every other worker in a random order. Only then does it park on its futex. At most half the busy workers spin at once.
   - **Waking:** making a task ready wakes a parked worker only when no worker is spinning. The spinner that finds work wakes another before running it. The counts and the re-check before parking use Go's ordering (`nmspinning`, a StoreLoad fence, a re-scan), so a task queued just as the last worker parks is never stranded.
   - **The poller:** one parked worker at a time waits in `epoll_wait`, as at milestone 210, kicked by the wake-up event.
   - **Busy workers poll too:** a busy worker polls without waiting every 61 picks.
4. **Timers per worker.**
   - **The heap:** each worker's sleepers are a binary heap under its own mutex. The earliest time is kept in an atomic word, which the poller and spinning workers read to time their waits; a worker that steals runs another's due timers too (Go 1.23's design).
   - **Cancelling:** a cancelled sleep removes its fiber from whichever heap holds it.
   - **What stays:** deadlines and shields stay under `rt_lock` with their scopes until the next PRP, checked only when an atomic count says one exists.
5. **Deadlock and exit.** A deadlock is all workers parked with no timer, socket wait, helper job or Ctrl-C outstanding. The last worker to park checks, under the shared queue's mutex, so the check can't race a wake-up.
6. **Less per call.**
   - **The fresh thread-pointer read:** with the lock no longer held across switches, it is needed only after `lugha_rt_task_swap`, in `block()` and `switch_to`; everywhere else `worker` is one `fs`-relative access again. The rule moves into one accessor, used where a function can come back on another thread.
   - **Sections:** scheduler-only entries (yield, preempt, sleep, spawn's queueing) take no section.
   - **The measure:** chanping with one worker and with the default workers.
7. **Deterministic and shuffled runs:** one worker, its ring as the ready queue. A shuffled pick takes a random slot, as now.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The lock off the switch first** (design 1). Without it, per-worker queues would still serialise on the switch. It needs only `block()`'s change and the handoff, which was built for exactly this.
2. **Go's ring and steal-half over Chase–Lev,** as the research recommends: a single compare-and-swap per steal, and half the queue moves at once.
3. **Run-next not stealable from a running worker:** a simpler version of Go's short delay, keeping a ping-pong's partner on its core.
4. **Per-worker timer heaps with a mutex each,** per the Draft and Go 1.23's lesson that lock-free timers weren't worth their bugs.
5. **Deadlines and shields wait for the next PRP,** with the scopes they belong to.
6. **Targets:**
   - chanping within 10% of milestone 209;
   - the single-task benchmarks within 3% of milestone 210;
   - the objective of PRP-242 at least as fast as at milestone 210.

   A miss is recorded and reported, not hidden.

## REQUIREMENTS

### Must Do
- Design items 1–7.
- Spec:
  - §6: a woken task may run next on its waker's worker; tasks spread over workers;
  - §9: the scheduler's description and the runtime table;
  - "Changes since v16";
  - §11 row 211.
- CHANGELOG, TODO, `bench/RESULTS.md`.

### Must NOT Do
- Locks per channel, scope, actor or join (the next PRP).
- `background` scopes (a later PRP of this phase).
- A second worker in deterministic or shuffled runs.

## TESTS
- [ ] `tests/workers.rs`'s C harness grows Go's scheduler tests, each run plain and under ThreadSanitizer, clean:
  - [ ] **Local queue steal:** two workers, one pushing and popping its ring while another steals, over many rounds. Every task is run exactly once: none lost, none twice.
  - [ ] **Empty queue:** a worker waking a task into run-next while a stealer looks. The task runs once.
  - [ ] **Ping-pong next to a hog:** two tasks ping-ponging over a channel while a busy task spins. The ping-pong keeps making progress.
  - [ ] **Many at once:** 10,000 tasks spawned from one, each joined.
  - [ ] **The last worker to park:** a task readied just as every worker parks still runs.
- [ ] `tests/workers.rs`'s sweep of every concurrency program on four workers, and its other cases, unchanged.
- [ ] `tests/waiting_runtime.rs` with the handoff's atomics on (the only build now).
- [ ] Benchmarks against milestone 210: the six single-task ones, chanping on 1 and 4 workers, and PRP-242's objective on 1, 2 and 4 workers.
- [ ] Targeted: `--lib`, `programs`, `leaks`, `rc_check`, `workers`, `tasks_runtime`, `waiting_runtime`, `rc_alloc_threads`, `helpers`, `blocking_externs`, `callbacks`, `spec`, `arm`, `codegen`, `comments`.

## ACCEPTANCE
- [ ] No task is lost or run twice under the stress tests; ThreadSanitizer is clean.
- [ ] Benchmarks meet decision 6's targets, or each miss is recorded and accepted.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Spec, CHANGELOG, TODO and `bench/RESULTS.md` are updated.
