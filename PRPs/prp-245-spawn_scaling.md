## FEATURE: Spawning that scales: a per-worker cache of task stacks, so a finished task's stack is reused without a system call, and a lock per scope for its own spawn and finish bookkeeping, off the shared scopes lock; milestone 213.

**Status:** proposed 2026-11-20.

## OBJECTIVE
Programs print what they printed. Starting and finishing many small tasks gets cheaper, and stops getting slower with workers:

```
$ python3 bench/measure/scaling.py
spawn_many 200000    1.38 s on 1 worker, 1.54 s on 4 (today)
spawners  200000     (new: eight tasks each spawning their share, in scopes of their own)
```

Measured before writing this PRP (`bench/measure/scaling.la`'s pattern, and one spawning from eight tasks, each in its own scope):

| case | 1 worker | 4 workers |
|---|---|---|
| one spawner, today | 1.22 s | 1.37 s |
| eight spawners, today | 1.35 s | 1.53 s |
| one spawner, no `madvise` | 0.78 s | 0.58 s |
| eight spawners, no `madvise` | 0.65 s | 1.02–1.14 s |

## CONTEXT
- **The cost of a spawn today** (about 7 µs):
  - **A finished task's stack** is given back to the kernel (`madvise(MADV_DONTNEED)`, 8 MiB), about 40% of the time. On several threads that call also interrupts every core to flush its TLB.
  - **Every spawn and every task end** takes the shared scopes lock (its scope's counts and task list, the live-task list) and the stack pool's lock. That's why eight spawners in eight scopes get slower with workers.
- **The user's choice:** both fixes. The scope split stays partial: only a scope's own spawn and finish bookkeeping move to its lock. Cancelling, failing, deadlines, shields and Ctrl-C stay on the shared lock, as PRP-244 decided.
- **Built on:** PRP-244 (milestone 212): object locks, parking holding a lock, the lock order.
- **Code:** `runtime/stacks.c` (the pool), `spawn.c` (`task_create`, `fiber_exit`), `scopes.c` (`lugha_rt_task_spawn_in`, `scope_task_done`, `scope_wait_until`, `cancel_tasks`, `scope_pop`), `scope_policy.c` (`scope_room`), `actors.c` (`lugha_rt_actor_started`, `scope_stop_actors`), `tasks.c` (the live list), `bench/measure/scaling.la`.
- **Security:** nothing on CLAUDE.md's list; no `unsafe`, no dependency.
- **Decisions:** none open.

## DESIGN
1. **A stack cache per worker:**
   - **Giving back:** a finished task's stack goes into its worker's cache, up to 16 stacks, with its pages kept. A full cache sends the stack to the shared pool as now, its pages given back (`madvise`).
   - **Taking:** a new task takes a stack from its worker's cache first, else from the shared pool.
   - **The cost:** memory up to 16 stacks' touched pages per worker while it is busy. A worker that parks (nothing to run) gives its cached stacks' pages back first, off the hot path, so an idle program holds no more than today.
2. **A lock per scope** (rt_lock.c's kind) over its task list, its running and actor counts, its owner's wait (`owner_waiting`, `wait_left`) and `limit n`'s waiter.
   - **Spawning:** `s.spawn` takes only that lock, and waits for room under `limit n` parked holding it.
   - **Task end:** a task's end takes its scope's lock to count itself out and wake the owner or a waiting spawner.
   - **The scope's end:** the owner waits for its tasks parked holding the lock.
   - **Failing and cancelling** keep the shared lock and take the scope's lock only to walk its task list.
3. **The lock order** becomes: the scopes lock, then a scope's lock (parent before child), then an actor's, a task's, and channels' locks, then the leaf locks. A spawn that finds its scope failed or its owner cancelled (read atomically) lets go of the scope's lock, then cancels the new task under the shared lock.
4. **The live-task list** (deadlock reports, `main`'s end) gets a leaf lock of its own. The fiber count and fibers started become atomic.
5. **`bench/measure/scaling.la` gains `spawners`:** eight tasks, each in its own scope, spawning N/8 empty tasks.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **16 stacks per worker, their pages given back when the worker parks:** Go caches stacks per worker the same way. How deep a finished task went isn't known cheaply, so pages are kept while the worker is busy and given back once it idles: a burst of deep tasks costs memory only while it runs.
2. **Parent before child** for scope locks, the only nesting (cancelling walks down). A task's end and a spawn hold one scope's lock only.
3. **The failed and cancelled checks read atomically** at spawn, and cancel under the shared lock afterwards, rather than taking the shared lock on every spawn.
4. **No change to what a scope does:** the same counts, order and policies; only which lock guards them.

## REQUIREMENTS

### Must Do
- Design items 1–5.
- Spec §9 (the runtime's locks and stacks), "Changes since v16", §11 row 213; CHANGELOG; TODO (remove the spawn item; the scope-lock item narrows to cancelling); `bench/RESULTS.md`; `bench/README.md` (the new pattern).

### Must NOT Do
- Move cancelling, failing, deadlines, shields or Ctrl-C off the shared lock.
- Change a scope's behaviour, limits or failure policies.

## TESTS
- [ ] `tests/scheduler_stress.rs`, plain and under ThreadSanitizer:
  - [ ] Eight spawners in eight scopes, 10,000 tasks each, all finished: each scope ends once all its tasks are done.
  - [ ] A scope failing while another task keeps spawning into it: every task spawned is cancelled or finishes; none outlives the scope.
  - [ ] `limit 2` with 1,000 spawns from four workers' tasks: never more than two running (a running count each task checks).
- [ ] The stack cache, through the C harness: a stack given back and taken again on one worker is the same stack, with no system call between (counted by a hook the harness reads); after the worker parks, its cache's pages are given back (the cache's count of stacks still holding pages drops to 0).
- [ ] `tests/workers.rs`'s sweep, `tests/programs/`, `leaks`, `rc_check`, `tasks_runtime`, unchanged.
- [ ] `bench/measure/scaling` before and after; the six single-task benchmarks and chanping within 3%.
- [ ] Targeted: `--lib`, `programs`, `leaks`, `rc_check`, `workers`, `scheduler_stress`, `tasks_runtime`, `waiting_runtime`, `rc_alloc_threads`, `helpers`, `blocking_externs`, `callbacks`, `spec`, `arm`, `codegen`, `comments`.

## ACCEPTANCE
- [ ] Spawning many tasks is faster on one worker and no slower on four; eight spawners scale with workers.
- [ ] The stress tests pass under ThreadSanitizer.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Spec, CHANGELOG, TODO and the bench docs are updated.
