## FEATURE: Helper threads: the runtime's blocking work (files and folders, reading input, name lookups, starting and waiting for programs) runs on a bounded pool of helper threads while its task waits, so the other tasks keep running; milestone 206.

**Status:** approved 2026-11-18; built 2026-11-18 (milestone 206).

## OBJECTIVE
```
import std.task;

fun main(console: Console, proc: Proc) {
    scope s {
        s.spawn({
            let out = proc.run("sleep", ["1"]);     // a helper thread waits for it
            console.println("slept");
        });
        for i in 0..3 {
            task.sleep(100);
            console.println("tick {i}");           // runs meanwhile
        }
    };
}
```
prints `tick 0`, `tick 1`, `tick 2`, then `slept`. Today `run` holds the only OS thread for the whole second: the ticks come after `slept`. The same goes for a file read from a FIFO, `console.read_line()` waiting on a terminal, and a slow DNS lookup.

## CONTEXT
- **Draft v18 §4.6** rule 7 (blocking work on a bounded pool of about 16 helper threads with a bounded queue, while its task waits; callers wait when it's full; helpers touch only copied data) [SCH-07], rule 6 (helpers' results arrive through the scheduler's poll) [SCH-06], rule 12 (deterministic mode takes their results only at fixed points) [SCH-12]; §4.3 rule 6 (work given to a helper finishes even when its task stopped waiting) [CAN-06]; §4.7 rule 1 (helpers have their own stacks) [STK-01]. `extern blocking` (rule 7's last item) is PRP-239.
- **Spec today:** §6 "Blocking calls" (everything but sockets blocks the one OS thread); §7 "File streams" (a FIFO or terminal stalls the other green threads); §9's runtime table; §11's out-of-scope line naming non-blocking I/O other than sockets, which the Draft now plans.
- **Code:** `runtime/net.c` (the `epoll` set and `idle_wait`), `runtime/tasks.c` (`block`, `wake`, the deadlock check), `runtime/dir.c` and `dir_names.c` (folders and files), `runtime/file.c` (streams), `runtime/input.c` (`read_line`), `runtime/resolve.c` and `net.c`'s connect (`getaddrinfo`), `runtime/process.c` (`lugha_rt_spawn`), `runtime/lockdown.c`. New: `runtime/helpers.c`.
- **Decisions:** none open. Phase 6 builds one worker first; many cores is the next PRP, so the pool is shared and locked from the start.

## DESIGN

### The pool
1. **Helper threads start at the first job, at most 16** (`LUGHA_HELPERS=n`, 1 to 256; any other value ends the program at start with `panic: LUGHA_HELPERS must be from 1 to 256`). A new thread is started only when a job is queued and none is idle. Each has its own 8 MiB stack (the C library's default) and blocks every signal: Ctrl-C and `SIGPIPE` reach only the worker.
2. **At most 256 jobs at once,** queued or running; a task submitting the 257th waits, first come first served, until one finishes.
3. **A job is a C function and a block of memory from `malloc`:** the worker copies the job's inputs into it (paths, the input text of a program, a host name), the helper runs the system calls and puts plain C results there (bytes from `malloc`, a number, an error number). Only the worker turns them into Lugha strings and arrays, so helpers never touch counted objects or the runtime's allocator (`lugha_rt_alloc` isn't thread-safe yet, and counts aren't atomic).
4. **Finished jobs come back through the scheduler's `epoll` set:** the helper adds the job to a locked list and writes an `eventfd` in the set; the poll that wakes tasks for sockets wakes the jobs' tasks too. The deadlock check counts running jobs, as it counts parked sockets.
5. **Cancelled while waiting** (a deadline, a sibling's failure, Ctrl-C), the task unwinds at once; its job runs to the end (CAN-06), and the worker frees the results nobody took. A started program is still waited for, so no zombie is left.

### What moves to helpers
6. **Every call that reads or changes the file system:** a `Dir`'s reads, writes, listings, opens, `make`, `remove`, `rename`, `kind`, `size`, `chmod`, `sync`, `path`; a `File`'s `read` and `write` (not `close`, which returns nothing and is quick). **Reading input:** `read_line`. **Name lookups:** `Net`'s `resolve` and the lookup in `connect`. **Programs:** `Proc`'s `run`, start to exit (`posix_spawnp`, feeding input, draining output, `waitpid`), unchanged in what it does.
7. **What stays inline:** writing to the console (quick, and order matters), the environment, clocks, randomness, sockets (already non-blocking), and the runtime's start (making `main`'s capabilities opens folders before lockdown).

### Lockdown and determinism
8. **No helper starts before lockdown:** Landlock limits a thread and the threads it starts afterwards, not threads already running, so a helper started earlier would escape it. Jobs submitted before lockdown (none today: only the program's code submits) run inline. seccomp's filter is synchronised to every thread already (`TSYNC`).
9. **In deterministic and shuffle runs every job runs inline**, on the task, at the point it is asked for: no other task runs meanwhile and no virtual time passes, so a run repeats exactly (SCH-12) and `lughac test` is unchanged.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Threads start on demand, up to the limit**, not 16 at start: a program that never blocks never has a second thread.
2. **The bound is 256 jobs in flight**, running or queued: enough to keep 16 helpers busy, and a task flood waits rather than piling up memory.
3. **`LUGHA_HELPERS`**, like `LUGHA_SCHED`, configures the pool. A project setting can follow with `stack n`'s project default (PRP 8 of the phase).
4. **Deterministic runs keep everything inline** rather than queueing jobs and taking results at chosen points: same order, no threads, and tests stay as fast as today.
5. **`close` and console writes stay inline**; everything else that touches the disk moves, `stat`-like calls included (a network file system can hang on any of them).
6. **A fault in a helper (C crashing) ends the program**, as a crash in C does today; helpers have no alternate signal stack.
7. **Measured:** a small file read 100,000 times, inline against a helper, recorded in the PRP and `bench/RESULTS.md`. A helper's round trip costs microseconds per call; a miss is recorded with its fix (quick calls tried inline first), as with PRP-236 and PRP-237.

## SECURITY (for your review)
- **`runtime/process.c`:** `lugha_rt_spawn`'s body moves to a helper thread unchanged (no shell, close-on-exec descriptors, `posix_spawnp`); its input is a copy, and the `SIGPIPE` juggling becomes the helper's permanent signal mask.
- **`runtime/lockdown.c`:** unchanged, but the pool's start depends on it (decision 8 of the design), and a test checks a helper is refused what lockdown refuses: a program without `Fs` started from a helper can't read a file outside its capabilities.
- Every descriptor the runtime opens is already close-on-exec, which matters more now that two programs can start at once from two helpers.

## MEASUREMENTS
A 13-byte file read 100,000 times, one task, `-O2`: inline 0.30–0.31 s (~3 µs a read), on a helper 1.93–2.35 s (~21 µs). The cost is the two wake-ups across threads per call. Recorded (`bench/RESULTS.md`) and accepted, as decision 7 says; the fix is in TODO.md: each call tried in place first (`RESOLVE_CACHED`, `RWF_NOWAIT`), handed over only when it would wait.

## MADE DURING THE BUILD
- **A started program's signal mask:** a child takes its starter's mask, and a helper blocks every signal, so `spawn_job` gives the child an empty mask (`POSIX_SPAWN_SETSIGMASK`), as a child started by the worker had. Without it the child would ignore Ctrl-C and `SIGTERM`.
- **File streams use a copy of the descriptor** (`F_DUPFD_CLOEXEC`): a task cancelled mid-read may close its `File`, and the number could otherwise go to a new file while the helper still reads or writes it. A `Dir`'s jobs keep the `Dir` itself alive instead (a count held until the job comes back).
- **A descriptor a job opened for a task that stopped waiting is closed** when the job comes back (a job's `drop`), as a `getaddrinfo` list is freed.
- **Numeric-only lookups stay inline** (`AI_NUMERICHOST`, UDP's addresses): they never wait.
- **A C callback's work runs inline**, since it can't wait.
- **`jobs.c` owns the scheduler's `epoll` set**, which net.c's sockets now share; it's joined after unwind.c, which it needs for `cancel_unwind`.
- **Lockdown's test:** `tests/lockdown.rs`'s `started_programs_share_the_limits` now starts its programs from a helper thread, so it is the check that helpers are limited; no new case was needed.

## REQUIREMENTS

### Must Do
- `runtime/helpers.c` (the pool, the job queue, the `eventfd`); the moves of the design's item 6; the deadlock check and cancellation as above; `LUGHA_HELPERS`.
- Every runtime file stays at or under 500 lines.
- Spec §6 (blocking calls), §7 (file streams), §9 (the runtime table, `LUGHA_HELPERS`), §11's out-of-scope line and row 206, "Changes since v16"; CHANGELOG; the `File` docs in `src/prelude/capabilities.la`; TODO's non-blocking item (its several-OS-threads half stays).

### Must NOT Do
- `extern blocking` (PRP-239); several workers (PRP 5); `io_uring` (deferred, DEF-10); DNS without threads or `pidfd` (deferred, DEF-11).

## TESTS
- [x] `tests/helpers.rs`: a program's ticks print while another task's `proc.run("sleep", …)` waits; while `read_line` waits on a pipe the test fills late; while a `File` read waits on a FIFO.
- [x] `tests/helpers.rs`: a task waiting on `run` cancelled by `within` ends its scope at the deadline; a hundred tasks each reading a file all succeed (the queue bound and on-demand threads); `LUGHA_HELPERS=0` and `=x` fail at start with the message.
- [x] `tests/lockdown.rs`: a file read on a helper is refused what lockdown refuses.
- [x] `tests/test_modes.rs`: a deterministic run with files and `run` repeats exactly.
- [x] Targeted: `--lib`, `programs`, `leaks`, `spec`, `tasks_runtime`, `test_modes`, `lockdown`, `net`, `dir_race`, `fs_time`, `interrupt`, `test_capabilities`.

## ACCEPTANCE
- [x] Blocking runtime work no longer stops other tasks; deterministic runs repeat; cancelled tasks don't wait for their jobs.
- [x] The measurement recorded.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [x] Spec, CHANGELOG, TODO and the prelude's docs updated.
