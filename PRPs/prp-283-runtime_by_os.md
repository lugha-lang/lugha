## FEATURE: The runtime in folders by role, its OS and CPU code under `os/` — milestone 249

Amended before merging (the user's review): first only the OS layer (`os/linux.c`) moved out of a flat `runtime/`; now every file goes into a folder by what it does, and everything per system or per CPU goes under `os/<system>/`, laid out for the three platforms the Draft plans, so each later port fills a folder and never edits shared code.

## OBJECTIVE

Nothing a Lugha program does changes, and the joined runtime compiles to the same code. Where every file lives, now and as the ports arrive:

```
runtime/
├── core/                       shared by every platform
│   ├── lugha_rt.c              portable headers, types, the worker record
│   └── test.c                  lughac test support
├── memory/                     rc.c  rc_alloc.c  rc_check.c  cycles.c  cycle_sites.c  weak.c  cells.c
├── tasks/                      tasks.c  runq.c  timers.c  waiting.c  workers.c  parking.c  rt_lock.c
│                               stacks.c  stack.c  spawn.c  scopes.c  scope_unwind.c  scope_policy.c
│                               scope_time.c  unwind.c  callbacks.c
├── channels/                   chans.c  rings.c  select.c  actors.c
├── io/                         helpers.c  jobs.c  net.c  udp.c  resolve.c  fs.c  file.c  env.c  random.c
├── text/                       print.c  format.c  input.c  bytes.c  chars.c  cstr.c
├── openssl/                    tls.c  tls_keys.c  crypto.c  crypto_hpke.c   (only when imported)
└── os/
    ├── layer.c                 the interface every system implements (os_poll_*, os_signal_*,
    │                           os_wait_on/os_wake_on, os_cpus, os_random64, os_thread_stack)
    ├── posix/                  what Linux and macOS share
    │   └── process.c 🔒        starting programs (posix_spawn, pipes, waitpid)
    ├── linux/                  ← built now
    │   ├── os.c                the layer: epoll, eventfd, futex, getrandom, sched_getaffinity,
    │   │                       pthread_getattr_np; Linux's headers
    │   ├── x86_64.c            task switch and boot, running on another stack, a task's first
    │   ├── aarch64.c           frame, the thread pointer (each file wrapped in #if for its CPU)
    │   ├── lockdown.c 🔒       seccomp + Landlock
    │   ├── dir.c 🔒            folder confinement (openat2)
    │   └── dir_names.c 🔒
    ├── darwin/                 ← portability step 4 (macOS)
    │   ├── os.c                kqueue (EVFILT_READ/WRITE, EVFILT_USER), os_sync_wait_on_address
    │   │                       or a semaphore, getentropy, pthread_get_stackaddr_np
    │   ├── aarch64.c           Mach-O directives, `_` symbol prefix, tpidrro_el0
    │   ├── x86_64.c            Mach-O, %gs thread pointer
    │   ├── dir.c, dir_names.c  confinement without openat2 (O_NOFOLLOW_ANY, a walk per component)
    │   └── lockdown.c          what a program promises there [PRT-11]: decided in step 4
    └── windows/                ← portability step 5
        ├── os.c                IOCP, PostQueuedCompletionStatus, WaitOnAddress, BCryptGenRandom,
        │                       GetCurrentThreadStackLimits
        ├── x86_64.c            Win64 calling rules, the TEB's stack bounds, SEH
        ├── aarch64.c
        ├── process.c           CreateProcessW (in place of posix/process.c)
        ├── memory.c            VirtualAlloc (in place of mmap), guard pages
        ├── signals.c           vectored exception handler, console Ctrl-C (in place of sigaction)
        └── dir.c, dir_names.c  confinement with NtCreateFile; lockdown [PRT-11]
```

Which parts are joined: the shared folders always; `os/layer.c`; `os/posix/` for Linux and macOS; `os/<system>/` for the target's system. Today the only system is Linux, so every file above that exists is joined; selecting by system arrives with the second one (step 4). Each CPU file is wrapped in `#if defined(__x86_64__)` / `__aarch64__`, so both are joined and the compiler keeps one.

## CONTEXT

- Read first: every `runtime/*.c` header comment (each says what it is joined after); `src/link/runtime_parts.rs` (the join order), `src/link/runtime.rs` (`TLS_SOURCE`, `CRYPTO_SOURCE`), the tests in `src/link/mod.rs`; anything else naming a runtime path (`grep -rn "runtime/" src tests docs CLAUDE.md .github`).
- Implements: v18 Draft, Part 9, 9.2 rules 1 and 3 [PRT-08, PRT-10] and rule 4's task switch per CPU and OS [PRT-14]; step 3 of 9.3, first half. Research: `docs/research/cross-platform.md` §3.
- Decisions: v18 ledger PRT-08, PRT-10, PRT-14 (approved); PRT-11 deferred (lockdown and confinement on other systems: step 4).
- Follows: **PRP-284**: sockets behind the layer at the operation level (`os_sock_*`; `net.c` and `udp.c`, which use Linux's `accept4` today) and a cancelled socket call waiting for the OS's acknowledgement [PRT-08, PRT-09].

## REQUIREMENTS

### Must Do

1. **The OS layer** (done in this branch's first commit): `os/layer.c` declares it, `os/linux/os.c` implements it with Linux's headers; jobs, net, udp, helpers, workers, parking, rt_lock, random and stack call `os_*` only.
2. **Folders.** Every file moves (`git mv`, contents unchanged except comments naming another file's place) as the tree above shows; nothing is created for darwin or windows.
3. **CPU code out of shared files.** `lugha_rt_task_swap` and `lugha_rt_task_boot` (`tasks.c`), `lugha_rt_on_stack` (`stacks.c`), `fiber_frame` (`spawn.c`) and `thread_pointer` (`lugha_rt.c`, which keeps its declaration) move to `os/linux/x86_64.c` and `os/linux/aarch64.c`, joined right after `tasks.c` (`fiber_frame` needs `Fiber`).
4. **Join order** (`runtime_parts.rs`): the same sequence as today with the new paths, plus `os/layer.c` and `os/linux/os.c` right after the core and the two CPU files right after `tasks/tasks.c`. `LINUX_ONLY` names `os/linux/*`; a new `POSIX_ONLY` names `os/posix/*`.
5. **The boundary test** (`src/link/mod.rs`) extended: no shared file (outside `os/`) names a Linux call (as now), uses `__asm__`, or tests `__x86_64__`/`__aarch64__`.
6. **References updated**: `CLAUDE.md`'s security list (`runtime/os/posix/process.c`, `runtime/os/linux/lockdown.c`), the spec, CHANGELOG, TODO, and any test or tool that reads a runtime path.
7. Docs: CHANGELOG (Changed), the Draft's step 3 note, the spec's milestone 249 row, `CLAUDE.md`'s STACK line.

### Must NOT Do

- No change in behaviour or in the joined code beyond the moves: diffed before and after, the code lines differ only by the CPU blocks' new place and the layer's renamed calls.
- No content change to the 🔒 files (`process.c`, `lockdown.c`, `dir.c`, `dir_names.c`): moved only (the user's explicit approval of the move).
- No files for darwin or windows yet; no selection by system in lughac yet (one system).
- No sockets redesign (PRP-284).

## DESIGN

**Placement rule, for every future file:** shared code goes in the folder of its role; code only one system has goes in `os/<system>/`; code Linux and macOS share but Windows doesn't goes in `os/posix/`; CPU-specific code goes in `os/<system>/<cpu>.c`, since it is CPU × system in practice (assembler directives and symbol names differ between ELF and Mach-O, calling rules and thread state between Linux and Windows).

**What stays shared though POSIX-flavoured, for now:** pthreads (Windows gets them from MinGW's winpthreads, which zig ships), `getenv`, `clock_gettime`; `mmap`/`mprotect`/`madvise` in `stacks.c`, `rc_alloc.c`, `callbacks.c` and `sigaction`/`sigaltstack` in `stack.c`, `scope_time.c`, which step 5 moves behind the layer (`windows/memory.c`, `windows/signals.c` above).

**Security.** The four 🔒 files move without a content change (shown by `git diff -M --stat` as pure renames); their compiled form is unchanged (the joined-source diff). Nothing changes what a program may do.

**Judgement calls (for review).** `test.c` in `core/`; `random.c` and `env.c` in `io/`; `callbacks.c` in `tasks/` (it runs Lugha on C's threads); `posix/` holding only `process.c` until step 4 moves more there; CPU files always joined and self-selecting by `#if` rather than lughac choosing.

## TESTS

- [ ] `src/link/mod.rs`: the boundary test with the new rules, failing first on `tasks.c`, `stacks.c`, `spawn.c`, `lugha_rt.c`; the join order (core, `os/layer.c`, `os/linux/os.c`, …, `tasks/tasks.c`, `os/linux/x86_64.c`, `os/linux/aarch64.c`, …); every runtime file on disk is a part (no file forgotten by a move).
- [ ] The joined source before and after, diffed: only moved blocks and the layer's calls differ.
- [ ] As before: `cargo test --lib`, `--test programs`, `workers`, `scheduler_stress`, `spawn_stress`, `waiting_runtime`, `net`, `foreign_threads`, `callbacks`, `pinned`, `rings`, `lockdown`, `dir_race`, `arm`, `musl`, `tls` (the OpenSSL parts' new paths), `comments`.
- [ ] `bench/run.py chanping wordfreq binarytrees`: no slower.

## ACCEPTANCE

- [ ] The tree above, for what exists today; every 🔒 file a pure rename.
- [ ] The tests above pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; every file ≤ 500 lines.
- [ ] CHANGELOG, the Draft's step 3, the spec's row 249, `CLAUDE.md` updated.
