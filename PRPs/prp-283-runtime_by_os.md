## FEATURE: The runtime's OS calls go behind one layer in `runtime/os/linux.c` — milestone 249

## OBJECTIVE

Nothing a Lugha program does changes. Inside the runtime, every event, wake-up, parking and system call the shared code makes goes through a small set of `os_*` functions, implemented for Linux in one new fragment:

```
runtime/
  lugha_rt.c          the core: portable includes, types, declarations of the os_* layer
  os/linux.c          NEW: Linux's includes and the os_* layer (epoll, eventfd, futex, getrandom, …)
  workers.c, net.c, jobs.c, parking.c, rt_lock.c, …   call os_*, never epoll/eventfd/futex directly
  lockdown.c, dir.c, dir_names.c                       Linux-only fragments, unchanged, listed as such
```

A new test fails if a Linux-only name (`epoll_`, `eventfd`, `futex`, `SYS_`, `getrandom`, `sched_getaffinity`, `pthread_getattr_np`, `MAP_STACK`) appears in a shared fragment. Porting to macOS (step 4) then means writing `os/darwin.c` and its own versions of the three Linux-only fragments, never editing shared code.

## CONTEXT

- Read first: `runtime/lugha_rt.c` (the core, every include), `runtime/rt_lock.c` (`futex_wait`/`futex_wake`), `runtime/jobs.c` (epoll set, helper and wake eventfds), `runtime/net.c` (`poll_found`, `poll_sockets`, `wait_fd`'s arm and disarm), `runtime/parking.c`, `runtime/workers.c` (`cpus_usable`), `runtime/helpers.c` (the helper eventfd write), `runtime/random.c`, `runtime/stack.c`, `runtime/stacks.c`, `runtime/rc_alloc.c`, `src/link/runtime_parts.rs` (the join order) and its tests in `src/link/mod.rs`.
- Creates: `runtime/os/linux.c`.
- Implements: v18 Draft, Part 9, 9.2 rule 1 (the split, the poller handing back finished waiters) and rule 3 (per-worker parking behind the layer) [PRT-08, PRT-10]; step 3 of 9.3, first half. Research: `docs/research/cross-platform.md` §3.
- Decisions: v18 ledger PRT-08, PRT-09, PRT-10, PRT-14 (approved). None open.
- Follows: **PRP-284** (not this one): sockets behind the layer at the operation level (`os_sock_*`, `net.c` and `udp.c`), and a cancelled socket call waiting for the OS's acknowledgement [PRT-08's operations line, PRT-09].

## REQUIREMENTS

### Must Do

1. **The layer.** Declared in the core (`lugha_rt.c`), implemented in `os/linux.c`, each `static` (one translation unit, so inlined, no cost):
   - Events: `os_poll_open()`; `os_poll_arm(fd, want, token)` (one-shot readiness, `want` a mask of `OS_READ`/`OS_WRITE`); `os_poll_disarm(fd)`; `os_poll_wait(OsEvent *out, int max, int timeout_ms)` → count, each event carrying back its `token` (`uint64_t`).
   - Signals between threads (today's two eventfds): `os_signal_new(token)` registered with the poll set; `os_signal_raise(signal)`; `os_signal_take(signal)`.
   - Parking: `os_wait_on(uint32_t *word, uint32_t expected)`, `os_wake_on(uint32_t *word, int n)` (today's `futex_wait`/`futex_wake`).
   - System: `os_cpus()`; `os_random64()` (today's getrandom, else `/dev/urandom`); `os_thread_stack(char **low, size_t *size)` (`pthread_getattr_np`); `OS_MAP_STACK` (the `mmap` flag for task stacks, 0 where none).
2. **Shared code uses only the layer.** `jobs.c`, `net.c`, `helpers.c`, `rings.c`, `parking.c`, `workers.c`, `rt_lock.c`, `random.c`, `stack.c` and `stacks.c` call `os_*` instead of the Linux calls; `poll_found` reads `OsEvent` tokens, deciding what each means (a fiber's wait, the helper signal, the wake signal) exactly as today.
3. **Includes.** The core keeps the portable (POSIX and C) headers; Linux's (`sys/epoll.h`, `sys/eventfd.h`, `linux/futex.h`, `linux/openat2.h`, `linux/landlock.h`, `linux/seccomp.h`, `linux/audit.h`, `linux/filter.h`, `sys/prctl.h`, `sys/syscall.h`, `sys/random.h`) move to `os/linux.c`, joined right after the core. The fragment rule becomes "only the core and `os/` files have `#include` lines".
4. **Join order** (`runtime_parts.rs`): core, `os/linux.c`, then the rest as today. `RUNTIME_PARTS` marks `os/linux.c`, `lockdown.c`, `dir.c` and `dir_names.c` as Linux's, the rest as shared; while Linux is the only OS, the joined source is the same list.
5. **The boundary test** (`src/link/mod.rs`): no shared fragment names `epoll_`, `eventfd`, `futex`, `SYS_`, `syscall(`, `getrandom`, `sched_getaffinity`, `pthread_getattr_np`, `MAP_STACK`, `openat2`, `RESOLVE_`, `landlock`, `seccomp` or `prctl` outside comments.
6. Docs: the core's header (no longer "Linux with glibc" as a whole: shared code, plus `os/` per OS), CHANGELOG (Changed, internal), TODO (step 3's first half done), the Draft's Part 9 step 3 noted as split into PRP-283 and PRP-284.

### Must NOT Do

- No change in behaviour, timing or symbols: generated code calls the same `lugha_rt_*` functions.
- No edits to `lockdown.c`, `process.c`, `dir.c` or `dir_names.c` (security-sensitive or confinement): they are only *listed* as Linux's.
- No sockets redesign (PRP-284), no macOS or Windows code (steps 4–5), no `@cfg`.
- No new crate.

## DESIGN

**Why these functions.** They are exactly the Linux calls shared code makes today (inventory: 15 files), grouped by what macOS and Windows will need to supply: an event poll (kqueue, IOCP), a cross-thread signal (`EVFILT_USER`, `PostQueuedCompletionStatus`), waiting on a word (`os_sync_wait_on_address`/semaphore, `WaitOnAddress`), and four system facts. `os_poll_*` keeps today's readiness shape; PRP-284 adds the operation-level socket calls on top, so Windows can implement them with completions without the shared code changing again.

**Token instead of `epoll_event.data`.** Today `poll_found` reads `data.ptr == &helper_tag` / `&wake_tag` or a packed fiber+generation in `data.u64`. The layer carries one `uint64_t token` both ways; the helper and wake signals get fixed tokens (1 and 2, never a fiber address, which is aligned and above 4096), fibers keep their packed value.

**Security.** No change to what any program may do; `lockdown.c`, `process.c`, `dir.c` untouched. The includes `lockdown.c` and `dir.c` need move from the core to `os/linux.c`, joined earlier than both, so they compile the same; the joined runtime is diffed before and after to show only the moved lines and the renamed calls differ.

**Performance.** All in one translation unit as `static` functions: the compiler inlines them. Checked with `bench/run.py chanping wordfreq binarytrees` before and after (no change beyond noise).

**Judgement calls (for review).**
- Two PRPs for step 3, so each stays reviewable; the Draft's step 3 is both.
- Linux-only fragments are listed, not moved into `os/linux/`: moving security-sensitive files is a change to them in git's eyes, and gains nothing until a second OS exists.
- `os/linux.c` holds what POSIX systems share too (the `/dev/urandom` fallback, `mmap` flags) until macOS shows what really is common; `os/posix.c` comes with step 4.
- Fixed tokens 1 and 2 for the two signals.

## TESTS

- [ ] `src/link/mod.rs`: the boundary test (Must Do 5), failing today on the 13 fragments that make Linux calls; the fragment rule's test updated to allow `os/` includes; the join order begins core, `os/linux.c`.
- [ ] Unchanged behaviour: `cargo test --lib`; `--test programs`; `--test workers`, `scheduler_stress`, `spawn_stress`, `waiting_runtime` (they include `lugha_rt.c` itself and call the scheduler's functions); `--test net`, `foreign_threads`, `callbacks`, `pinned`, `rings`, `lockdown`, `dir_race`; `--test arm` and `--test musl` (zig still compiles the joined runtime).
- [ ] `bench/run.py chanping wordfreq binarytrees --runs 5` before and after.

## ACCEPTANCE

- [ ] No Linux call outside `os/linux.c` and the three listed Linux fragments, enforced by the test.
- [ ] The tests above pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; every runtime fragment ≤ 500 lines.
- [ ] Benchmarks unchanged within noise.
- [ ] CHANGELOG, TODO and the Draft's step 3 note updated.
