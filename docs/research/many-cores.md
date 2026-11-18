# Many cores: research before the design

Phase 6, item 5 of the v18 Draft: tasks run on one worker thread per core (Draft §4.6 rules 1, 5, 6, 10; §3.1 rules 2, 14). This note gathers what the runtime assumes today, how other runtimes solve each part, and what that suggests for Lugha. It precedes the PRPs; nothing here is decided.

Sources: an audit of `runtime/*.c` and `src/codegen/*.rs` (2026-11-18, file:line references checked where they matter); the Go runtime (`proc.go`, `runtime2.go`, `netpoll_epoll.go`, `time.go`); Tokio's multi-threaded scheduler (`worker.rs`, `queue.rs`, `idle.rs`, `park.rs`) and its 2019 scheduler post; mimalloc (Leijen, Zorn, de Moura, MSR-TR-2019-18); snmalloc (ISMM 2019); biased reference counting (Choi, Shull, Torrellas, PACT 2018); CPython's free-threaded build (PEP 703, `Python/brc.c`); Koka's Perceus (PLDI 2021, TR §2.7.2, `kklib`); Rust's `Arc`; Lê, Pop, Cohen, Zappa Nardelli, "Correct and Efficient Work-Stealing for Weak Memory Models" (PPoPP 2013); LLVM issue #98479 and the fiber suspend-point RFC.

## 1. What the runtime assumes today

Everything runs on one OS thread; nothing locks (`runtime/tasks.c`'s header says so). Helper threads (milestone 206) are the only other threads, and they never touch Lugha values.

**State that must become per worker:** `current`, `zombie`, `on_c_stack`, the step budget `lugha_rt_task_budget`, the stack limit `lugha_rt_stack_limit`, the C stack (`c_stack_top`, `stack_low`, the 64 KiB `sigaltstack`, all the main thread's), `last_spawned`, the marking worklist (`rc.c`), the allocator's free lists and carve pointers (`rc_alloc.c`), the report counters, `select`'s random state (`chans.c`).

**State that becomes shared and needs locks or atomics:** the ready queue, the sleeper list, the live-fiber list and count, the stack pool, the allocator's region bump, deadlines and shields (`scope_time.c`), the root scope, every `LughaScope` (a record in its owner's frame that children update), channels (`select` waits on several at once), actor mailboxes, joiner lists, the helper room queue and `jobs_out`, `net_waiting`, the `epoll` set's lazy creation, Ctrl-C's `interrupts` and `shutting_down`.

**Read-only after start (safe):** the scheduling mode and seed, the hash key, `argc`/`argv`, the report flags, the region bounds, the helper limit, constant tables.

**Generated code touches four kinds of global:**
- `lugha_rt_task_budget`: load, decrement, store and compare at **every function entry and every loop head** in programs that start tasks (`codegen/tasks.rs` `preempt_point`). The hottest site there is.
- `lugha_rt_stack_limit`: one load and compare at each non-inlined function's entry (`codegen/call_costs.rs`).
- `lugha_rt_hash_key`: read-only, safe.
- `lugha_const_<key>`: constants, filled once and then retained by every reader (see the marking holes below); `lugha_cbv_<n>`, a callback site's closure slot, written on each evaluation (a race between workers).

**Reference counts.** The inline fast path (`codegen/rc.rs`) is a plain load and a plain store of the count; shared objects (bit 62), immortal ones and freed ones take the slow path, where shared counts change atomically (relaxed increment, release decrement, acquire fence before freeing — the right orderings). Marking (`lugha_rt_rc_mark`) sets the bit with a plain store and walks what the object reaches; it is correct only before the object is published. The scheme is Koka's (Perceus): one sticky shared bit, one fast-path test, atomics only when shared.

**Marking holes** — values that reach a second task without being marked, harmless on one thread, use-after-free on several. The first two are marked on the wrong side; the rest not at all:
1. A `join` result is marked by the joiner, after the task finished (`codegen/task_handles.rs`): two joiners on two workers race on its count.
2. An actor call's result is marked by the caller (`codegen/actors.rs`) while the actor may still hold it.
3. Constants (`lugha_const_*`) keep plain counts and are retained by every task.
4. A computed task name; a cancellation's reason text; a task's error box and panic text (held by the task, its scope and its joiners); a scope's failure handler's environment; a cell's stored bytes; the working folder's `Dir`; a helper job's held `Dir`.

**Wait and wake.** `block()` marks the task WAITING, then switches; its context is saved only inside the switch (`tasks.c`). On one thread nothing can run in between. With workers, another worker can wake the task and resume it before its registers are saved. `wake()`'s check-then-set of `fired` is not atomic either. Every wait is affected: channels, `join`, actors, sockets (`EPOLLONESHOT` can fire on another worker at once), helper jobs, sleeps, scope waits.

**Bound to the main thread:** `lugha_rt_main` runs `main` on a pooled stack and returns onto the process's stack, so `main` must finish on the thread it began on; the one C stack; the `SIGSEGV` handler's bounds and alternate stack; Ctrl-C's handler, which treats `current == NULL` as "no task started" (true on an idle worker too).

**Process end and deadlock:** `exit`, `die`, the deadlock report and the `atexit` handlers (the count report, the working folder's release, `rc_stop_threads`) would run while other workers keep going; the deadlock check is made per idle loop, and must become a global decision.

**Lockdown:** Landlock and `no_new_privs` cover the calling thread and the threads it starts afterwards; seccomp is synchronised to all threads (`TSYNC`). Workers must start after `lugha_rt_lockdown`, as helpers do; `clone` for threads is allowed by the filter.

**A latent bug found on the way:** `print.c`'s `show_stack` (a guard against printing a `Ref` cycle forever) is one global stack; a task preempted mid-print lets another push and pop on it. It should be per task.

## 2. How others do each part

### Scheduler
- **Go:** each P (a worker) has a 256-slot lock-free ring (owner pushes with a store-release of `tail`; owner and thieves consume with a load-acquire of `head` and a CAS), plus `runnext`, a slot for the task just woken, which inherits the rest of the waker's time slice. Every 61st pick checks the global queue first ("two goroutines can completely occupy the local runqueue by constantly respawning each other"). An idle P steals half a victim's ring, visiting victims in a random coprime stride, four passes; only the last pass may take a victim's `runnext`, after a 3 µs pause.
- **Tokio:** the same ring of 256, steal half (one thief per victim at a time), a LIFO slot for the woken task with at most 3 uses in a row ("picked out of thin air"), not stealable; a global inject queue checked every 61 ticks (now adaptive); every 61 ticks a busy worker polls I/O without waiting. Its 2019 rewrite: `chained_spawn` 12× faster, `ping_pong` 2.3×, Hyper +34% requests/s.
- **Chase–Lev deques** are the alternative: LIFO for the owner (right for fork-join, wrong for servers), a `seq_cst` fence on every pop, and safe reclamation when they grow. Go's and Tokio's bounded FIFO ring avoids all three.

### Waking idle workers without losing a task
Both Go and Tokio use the same Dekker-style protocol. A producer enqueues, issues a full fence, then wakes a worker only if none is "spinning" (searching for work). A spinner giving up decrements the spinning count, fences, then re-scans every queue before it sleeps. At most half the busy workers spin. The last spinner to find work wakes the next, so wake-ups ramp up one at a time. Each worker sleeps on its own futex word, with a NOTIFIED token so a wake that comes before the sleep isn't lost.

### I/O and timers
- **One `epoll` set**, fds registered once and edge-triggered, with readiness kept in user space. Any worker may poll without waiting; at most one blocks in `epoll_wait`, with a timeout set to the earliest timer of all workers, and is woken through an `eventfd` (Go's `netpollBreak`). The helper pool's `eventfd` stays in the same set. `EPOLLEXCLUSIVE` is for several sets watching one fd and isn't needed.
- **Timers per worker:** Go 1.14 moved to a heap per P with a lock-free status machine. That was a long-running source of bugs, and Go 1.23 replaced it with locks (each heap locked, each timer locked). An idle worker runs other workers' expired timers while stealing.

### The values generated code reads on every call
- **TLS costs:** a local-exec thread-local read on x86-64 is one `%fs:`-relative load, the cost of a global; initial-exec adds a GOT load; ARM64 reads `tpidr_el0` first. A program's own runtime linked into a PIE may use local-exec.
- **The trap:** LLVM treats a thread-local's address as constant within a function (LLVM issue #98479; the fiber suspend-point RFC). A loop that decrements the budget may compute the address once. After a call in which the task switched out and resumed on another worker, it then decrements the old worker's budget. The same happens to `current` in the C runtime, and to `errno`'s address (`__errno_location` is `const`).
- **Go's answer:** the current goroutine stays in a register (R14 on amd64, R28 on arm64), and its `stackguard0` serves both the stack check and preemption: preemption is requested by poisoning the guard, so one load and compare does both.
- **Measured here (preliminary, a busy 4-core i5):** fib(41) with the check and the countdown at each call, best of 7:

  | how | time |
  | --- | --- |
  | plain globals (today) | 1.11 s |
  | local-exec TLS | 1.06 s |
  | initial-exec TLS | 1.43 s |
  | a task pointer passed to each call | 1.26 s |
  | the task pointer read through `asm volatile` from TLS | 1.40 s |

  Local-exec costs nothing over today; the safe variants cost 13–26% on this worst case. To be measured properly on a quiet machine.

### The allocator
- **mimalloc:**
  - Each thread owns pages, one size class each, with three free lists: `free` (allocation pops), `local_free` (the owner's frees, no atomics) and `thread_free` (frees from other threads, one CAS push).
  - The owner takes `thread_free` with one swap when `free` runs dry.
  - Remote frees into full pages need a "delayed free" path that notifies the owner once, which made xmalloc-test 2× faster.
  - It was about 3.5k lines in 2019. Its hardest part, abandoned heaps of exited threads, disappears with a fixed set of workers that never exit.
- **snmalloc** batches remote frees per destination and posts each batch to the owner's message queue: one atomic per batch.
- **tcmalloc** uses per-CPU caches with restartable sequences: no atomics, but objects migrate between CPUs, the code is Linux-specific assembly, and it clashes with glibc's own rseq.
- **jemalloc and glibc:** arenas plus per-thread caches, a mutex on the home arena when the cache overflows. glibc adds its own 8–16-byte header to ours and bloats with many threads.
- **Go** frees by sweeping, so it never has a remote free. **Zig's `SmpAllocator`** has a slot per CPU with a try-lock, and a free goes to the freeing thread's slot.

### Reference counting across threads
- **Costs:** `lock xadd` is about 18–20 cycles uncontended against 1 for a plain increment. Contended, the cache line moves.
- **On ARM64,** LSE atomics (`ldadd`, ARMv8.1+) scale far better than LL/SC loops, and relaxed or release operations are cheaper than acquire-release, so orderings matter there in a way they don't on x86.
- **Measured elsewhere:**
  - Koka: forcing every count atomic cost 5–59%.
  - BRC: Swift spends 15–42% of its time on counts, and 93% of count operations hit unshared objects.
  - CPython's free-threaded build: biased counting costs 5–8% overall.
- **CPython's biased counting:** each object has an owner thread, a local count and a shared count. A decrement by a non-owner that drives the shared count negative queues the object for its owner to merge. It is only worth it if hot shared objects show up in profiles.
- **Correct orderings,** as Rust's `Arc` uses them: increment relaxed, decrement release, acquire before freeing. Ours already match.
- **Every access to a count word must be atomic in C11** (relaxed for the unshared path, which compiles to plain loads and stores), or the code is undefined and ThreadSanitizer flags it.
- **Moving a task to another worker** (stealing) moves its unshared objects too. That is sound if the steal is a happens-before edge, which the ring's release/acquire gives. But the allocator must decide local versus remote by **the page's owning worker, not the shared bit**: a stolen task frees its own unshared objects remotely.

### Testing
- **Go:** the race detector (ThreadSanitizer) and stress tests in `proc_test.go`:
  - `TestSchedLocalQueueSteal`: no task lost or duplicated.
  - `TestSchedLocalQueueEmpty`: the `runnext` race.
  - `TestPingPongHog`: a CPU hog mustn't starve a ping-pong pair.
  - `TestTimerFairness`.
- **Tokio:** `loom`, which explores interleavings under the C11 memory model and caught more than 10 bugs that stress tests missed, run on the queue with 4 slots; and benchmarks `spawn_many`, `ping_pong`, `yield_many`, `chained_spawn`.
- **CPython:** a ThreadSanitizer build in CI. **mimalloc:** `test-stress.c`, threads freeing each other's objects.
- **For C:** GenMC model-checks C11 code directly; ThreadSanitizer must be told about task switches (`__tsan_create_fiber`, `__tsan_switch_to_fiber`), or it confuses tasks with threads.

## 3. What this suggests for Lugha

These are recommendations for the discovery, not decisions.

1. **Keep the reference-counting scheme** (Koka's): a sticky shared bit, atomics only when shared, the orderings it already uses. Make every count access a C11 atomic (relaxed when unshared: free on x86 and ARM). **Fix the marking holes** of section 1, marking always on the producing side, before publication: a task marks its result before it is done, an actor its answer before it replies; constants become immortal after they are filled. A debug mode recording the owning worker in unshared objects would catch the next hole cheaply.
2. **Write our own allocator,** mimalloc's page design without its hardest part: a 64 KiB page per size class, owned by one worker, with `free`, `local_free` and an atomic `thread_free`; local frees exactly as today's fast path; a remote free is one CAS. About 600–1,000 lines of C in two or three files, no vendored code, our header kept. mimalloc itself (`LD_PRELOAD`) serves as a benchmark baseline. Remote frees will be common, not rare (stolen tasks free their own objects remotely), so parallel binarytrees is measured early.
3. **Queues as Go and Tokio have them:** a 256-slot ring per worker, steal half, a mutex-guarded global queue checked every 61 picks, a "run next" slot with at most 3 uses that inherits the waker's budget and is the last thing stolen. Not Chase–Lev.
4. **Go's parking protocol,** exactly: the spinning count, the fences, the re-scan, a futex per worker with a NOTIFIED token; at most half the busy workers searching.
5. **A real handoff for waiting:** a task becomes wakeable only after its context is saved — a "parking" state set before the switch, which the waker spins on (as Go's `casgstatus` does) or the switching worker completes after the swap. `wake` becomes a CAS on the task's state.
6. **One `epoll` set,** one blocked poller at a time, the others asleep on futexes; per-worker timer heaps under locks (Go 1.23's lesson); global deadlock detection when every worker is idle and nothing is outstanding.
7. **The two hot values:**
   - Keep them per worker in local-exec TLS. Today's measurements say this is free.
   - Make every read after a possible switch go through an accessor LLVM can't cache: inline asm, or a call marked as a suspend point.
   - Alternatively, follow Go: one "guard" word per task, read through a pinned pointer, which the scheduler poisons to request preemption, folding the countdown into the stack check.
   - Decide by measurement on a quiet machine.
   - The C runtime's `current` needs the same discipline.
8. **Bound to a thread:**
   - `main` returns on the main thread, so the main fiber is pinned or hands control back.
   - Each worker gets its own C stack, stack bounds and `sigaltstack`.
   - Ctrl-C is delivered to one thread: workers block SIGINT, as helpers do.
   - A task with C frames on its stack (inside a callback) stays on its worker, as Go's cgo callbacks do.
   - Thread-affine C libraries (GTK, OpenGL) are `pinned` territory, a later PRP of this phase. Until then, a program using them may need one worker. This is a gap to settle in discovery.
9. **Deterministic and shuffle modes stay at one worker,** so `lughac test` is unchanged.
10. **Testing:**
    - ThreadSanitizer builds of the runtime, with the fiber annotations.
    - GenMC harnesses for the ring, the remote-free list and the parking protocol.
    - Ports of Go's queue, empty-check and ping-pong-hog tests.
    - Tokio's four benchmarks.
    - Parallel binarytrees under `LUGHA_RC_REPORT=2`, expecting no live objects at exit.
    - Runs on ARM64 (qemu) with and without LSE.

## 4. A possible order of PRPs

Each leaves the program working and measured:

1. **Ground work, still one worker:**
   - the marking holes fixed;
   - count accesses made atomic-relaxed;
   - the wait/wake handoff rebuilt with a parking state;
   - per-worker state gathered into a worker record;
   - `show_stack` per task;
   - the owner-check debug mode.

   No behaviour change, and every later PRP builds on it.
2. **The allocator:** per-worker pages with remote-free lists, exercised by a C stress harness with real threads before any task runs on them.
3. **Workers:**
   - N threads with per-worker queues, stealing and parking;
   - one poller and per-worker timers;
   - locks on channels, scopes, actors and joins;
   - the two hot values per worker;
   - the main thread, C stacks and signals;
   - global deadlock detection and coordinated exit.

   This is the largest; it may split into "workers and queues with coarse locks" and "finer locks and the measurements".
