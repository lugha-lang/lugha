## FEATURE: Locks of their own for the hot paths: each channel (and `select`), each actor's mailbox and each task's `join` takes its own lock instead of the shared one, every wait claimed by compare-and-swap before a value moves, and a wait's generation guarding against stale wake-ups; milestone 212, the second "fast workers" PRP.

**Status:** approved 2026-11-20; built 2026-11-20 (milestone 212).

## OBJECTIVE
Programs print what they printed. Tasks passing messages on different channels, or calling different actors, no longer queue for one lock:

For example, eight ping-pong pairs, each pair on its own two channels: today every send and receive of all eight takes the one shared lock, so four workers barely beat one. After this PRP, the pairs only meet in the scheduler's per-worker queues, and four workers should run them several times faster. `bench/measure/scaling` (new) measures this and four other patterns on 1, 2 and 4 workers.

## CONTEXT
- **Built on:** PRP-243 (milestone 211): per-worker queues, the lock off the switch, the parking handoff.
- **The user's choice:** hot paths only. Scopes, cancelling, deadlines, shields, the live-task list and the helper-job room keep one shared lock, renamed the **scopes lock**. Splitting it is a later PRP, only if a measurement shows contention, following Go's `context` design.
- **Research:** `docs/research/many-cores.md` §2 (Go's channels).
- **Draft v18 §4.6** rule 1 [SCH-01] (workers run independently).
- **Code:** `runtime/chans.c`, `actors.c`, `spawn.c` (join, task end), `waiting.c` (`wake`, `block`, `switch_away`, `landed`), `rt_lock.c`, `timers.c`, `net.c` (`wait_fd`, the poll), `scope_policy.c` (join status), `bench/measure/`.
- **Security:** nothing on CLAUDE.md's list; no `unsafe`, no dependency.
- **Decisions:** the user's: hot paths only. None open.

## DESIGN
1. **Lock order:** the scopes lock, then an actor's lock, then a task's lock, then channel locks in address order, then the scheduler's leaf locks (run queues, timer heaps, the stack pool). Nothing takes a lock earlier in the order while holding a later one: a cancellation point runs before a channel or actor lock is taken, never under one.
2. **Parking under a lock.** A fiber waiting on a channel, an actor or a join enqueues itself and parks *while still holding that object's lock(s)*. The fiber landing on its worker lets them go once the switch has saved it, as Go's `selparkcommit` does. So a waker holding the lock never meets a fiber still parking. A scheduling point inside such a switch skips deadlines and shields, which would take the scopes lock out of order.
3. **Claim, then move.** `wake` becomes two steps:
   - **Claim:** a compare-and-swap of the fiber's wait word from the exact value its waker saw.
   - **Ready:** queuing the fiber.

   A sender copies its value into a waiting receiver only after its claim succeeds. A receiver claims a waiting sender before taking its value. A claim that fails (a `select`'s other case won, or a timer, or a cancellation) drops that waiter and goes on to the next.
4. **A wait's generation.** The wait word gains a 32-bit generation, bumped at each `block`. Timers and socket events remember the generation they were armed for and claim only that one. A timeout or socket event delivered after its fiber was already woken some other way, and has since begun another wait, misses instead of waking the wrong wait. That was possible before too, rarely, with the shared lock: a socket event in the poller's batch could wake a fiber already waiting on a channel. An `epoll` event carries the fiber and its generation packed in its 64 bits (user addresses take 48).
5. **Channels:** a lock per channel guards its ring, waiters and `closed`. Send, receive, `try_send`, `try_recv`, `close` and a channel's release take it. `select` takes every case's channel lock in address order (each once), checks, enqueues one waiter per case and parks holding them all. Woken, it takes them again to drop its other waiters.
6. **Actors:** a lock per actor guards its mailbox, status, room waiters, idle fiber and the message being handled. Calls, casts, `stop` and the actor's loop take it. An actor's end (`actor_ended`, from its task's end under the scopes lock) takes it after the scopes lock, as the order says.
7. **Joins:** a lock per task record guards `done` and its joiners. `join` takes it. A task's end takes the scopes lock (its scope's bookkeeping), then its own record's lock to set `done` and wake its joiners.
8. **What stays under the scopes lock:** everything else that took the shared lock: scopes, cancelling, deadlines, shields, Ctrl-C, the live-task list, task names and outcomes, the job room, socket waits' bookkeeping and cells. A timer fires with no lock but its heap's, since a claim of the exact generation needs none.
9. **Scaling measures:** `bench/measure/scaling.la` and its driver, run by hand like the other measures, Lugha only. They are Tokio's four (spawn many, yield many, chained spawn, many ping-pong pairs) and a parallel binarytrees (trees built by separate tasks), each on 1, 2 and 4 workers, recorded against milestone 211.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Go's park-holding-the-lock** (design 2), over waiters that are visible while still parking. It keeps every claim simple: no waker ever meets a half-saved fiber.
2. **The generation in the wait word** (design 4), not a sequence beside it, so one compare-and-swap decides.
3. **Locks for task records too,** since `join` is on the message path for fan-out and fan-in.
4. **No lock-free channels:** a lock per channel, as Go has. A fast path that skips the lock for an empty ring is a later measurement's question.
5. **chanping won't speed up much here:** it is one pair on two channels, so it takes as many lock operations as before, just on other locks. The gain is in programs where many tasks talk at once. Its ideas in TODO (passing the worker down the hot paths) stay there.

## MADE DURING THE BUILD
- **`select` keeps its one random draw:** among the cases that look ready under their locks, then the rest in turn if another thread took a waiter meanwhile, so a shuffled run's seed gives the same order as before.
- **Generic `wake` stays for lists kept under their own lock** (joins, actors' room and idle fiber, answers): a waiter there leaves the list under that lock before it can wait on anything else. Timers, socket events and channel waiters claim by generation.
- **Bugs found and fixed:**
  - A worker thread could park before `workers_start` counted it, so the parked count matched the running count while `main` still ran: a false deadlock (since milestone 210). Each thread is now counted before it is created.
  - `main` finishing on another worker just as the main thread's worker parked was stranded: the main worker parked without checking `home_fiber`, and the deadlock check deferred to it (a hang, since milestone 211). Parking now checks it under the same lock.
  - A cast cancelled while waiting for mailbox room read its message after `message_done` had freed it.
  - The stack pool wrote the stack mask again with every region of stacks, while other threads read it: a data race ThreadSanitizer found. It is now set once.
- **The stale-event test** is in `tests/waiting_runtime.rs` (a wake for an earlier generation misses the next wait), not a socket program: it tests the one check timers and sockets share.
- **The actor stress test** is a Lugha program (an actor's handlers are generated code), in `tests/scheduler_stress.rs`.
- **`tests/workers.rs`'s sweep kills a run past 30 seconds** and reports it as hung, instead of waiting forever.
- **chanping on one worker went from 0.29–0.35 s to 0.36–0.39 s:** channel locks are real atomics even on one thread, where the shared lock was free. On four workers it is within 5% of milestone 211, as the target asked.

## REQUIREMENTS

### Must Do
- Design items 1–9.
- Spec §6 (several tasks passing messages don't wait on each other; a wake-up meant for an earlier wait is dropped), §9 (the runtime's locks), "Changes since v16", §11 row 212; CHANGELOG; TODO; `bench/RESULTS.md`; `bench/README.md` (the new measure).

### Must NOT Do
- A lock per scope, or moving deadlines and shields (a later PRP, if ever).
- Lock-free channels.
- Changing what any program prints or how a channel behaves.

## TESTS
- [x] `tests/scheduler_stress.rs` grows, each case plain and under ThreadSanitizer:
  - [x] **A select racing its cases:** many tasks each selecting on two channels with a short timeout, while two senders feed both. Every value sent is received exactly once; a timeout loses none.
  - [x] **Actors under load:** four actors, each called by eight tasks at once. Every call is answered once, with the right result.
  - [x] **Join fan-in:** 1,000 tasks, each joined by two joiners on other workers. Every joiner gets the result.
  - [x] **A stale socket event:** as a check of the generation itself (`tests/waiting_runtime.rs`): a wake armed for an earlier wait misses the next.
- [x] `tests/workers.rs`'s sweep, `tests/programs/` (one worker, exact output), `leaks`, `rc_check`, unchanged.
- [x] `bench/measure/scaling` on 1, 2 and 4 workers against milestone 211, recorded; the six single-task benchmarks and chanping against milestone 211.
- [x] Targeted: `--lib`, `programs`, `leaks`, `rc_check`, `workers`, `scheduler_stress`, `tasks_runtime`, `waiting_runtime`, `rc_alloc_threads`, `helpers`, `blocking_externs`, `callbacks`, `spec`, `arm`, `codegen`, `comments`.

## ACCEPTANCE
- [x] Many ping-pong pairs, actor calls and joins scale with workers instead of slowing down; the stress tests pass under ThreadSanitizer.
- [x] Single-task benchmarks within 3% of milestone 211; chanping within 5% of it on the default workers (one worker: 20% slower, recorded).
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [x] Spec, CHANGELOG, TODO, `bench/README.md` and `bench/RESULTS.md` are updated.
