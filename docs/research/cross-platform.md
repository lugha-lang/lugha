# Cross-platform: research before the design

lughac should build programs for any platform: the system `cc` for the machine it runs on, `zig cc -target …` for any other, `$LUGHA_CC` overriding both. Two questions come before the Draft section and the PRPs: how others drive zig as a cross toolchain, and how runtimes split their OS code so that macOS and Windows don't force a rewrite. This note gathers the answers and what they suggest for Lugha. Nothing here is decided.

Sources (2026-11-22): cargo-zigbuild (`src/zig/{wrapper,linker_args,mod}.rs`, README, issues #123, #127, #318, #457, #484); Uber's hermetic_cc_toolchain (`toolchain/zig-wrapper.zig`, `zig_sdk.bzl`); zig's source (`src/target.zig`, `src/Compilation/Config.zig`) and issues #9439, #36490, PR #35933, #36530; Go (`src/runtime/netpoll*.go`, `lock_futex.go`, `lock_sema.go`, `os_darwin.go`, `os_windows.go`, `src/internal/poll/fd_windows.go`); mio (`src/sys/windows/{afd,selector,waker}.rs`, `src/sys/unix/waker/`, 0.6.9's Windows module doc); libuv (`src/unix/async.c`, `src/win/{async,tcp}.c`); wepoll (`dist` branch); Rust std (`library/std/src/sys/sync/`, `sys/pal/unix/stack_overflow/`, `sys/pal/windows/stack_overflow.rs`, rust-lang/rust#122408); corosensei (`src/arch/x86_64_windows.rs`, `src/stack/windows.rs`); Apple's `os_sync_wait_on_address` page. Claims not checked against source are marked *unverified*.

## 1. Where we are

- The runtime is C source joined into one `lugha_rt.c` and compiled by the build's compiler (`src/link/runtime.rs`: `-O2 -funwind-tables`), cached in `<cache root>/objects/`; the same compiler links (`src/link/mod.rs`: `objects… -lm [-lssl -lcrypto] -o out`).
- `--target` is two Linux triples; ARM uses `aarch64-linux-gnu-gcc`. LLVM is initialised only for x86 and AArch64 (`src/codegen/emit.rs`).
- The runtime says "Linux with glibc". OS-bound parts: `epoll` (net, jobs, rings, parking, workers), `eventfd` (rings, helpers, jobs), `futex` (rt_lock, parking, workers), `sigaltstack` (stack), seccomp and Landlock (lockdown), `getrandom`, `MAP_STACK`, and the task switch's assembly in `stacks.c` (x86-64 and AArch64, System V / AAPCS64).

## 2. zig as the cross toolchain

### How others drive it

- **cargo-zigbuild** writes a wrapper script per target, because cargo accepts only one executable path as its linker and because rustc passes flags zig rejects. It always adds `-target <zig triple>` and `-mcpu=<cpu>`, and rewrites or drops flags: `-lgcc_s` → `-lunwind`; `--target=` stripped; `-Wl,--undefined=S` → `-Wl,-u,S`; several `-Wl,` flags dropped; CPU names take underscores (`x86-64-v3` → `x86_64_v3`). On macOS it removes duplicate `-l` (zig's Mach-O linker emitted duplicate `LC_LOAD_DYLIB`, rejected by newer dyld). It caches `zig version` instead of asking per call, and runs `zig` or `python -m ziglang`.
- **hermetic_cc_toolchain** (Uber, Bazel) uses a small compiled launcher per target and tool, sets `ZIG_GLOBAL_CACHE_DIR`, `ZIG_LOCAL_CACHE_DIR` and `ZIG_LIB_DIR` on each call, and pins zig (0.15.2). Its README calls macOS support not well tested.
- **Go** accepts `CC="zig cc -target x86_64-linux"` since 1.18; earlier it needed wrappers. Cross-building for macOS hits `unsupported linker arg: -headerpad`.
- Bun and Roc: *unverified* whether either uses `zig cc` for cross C.

**For us:** lughac writes its own command line, so it needs no wrapper and no flag filtering: run `zig cc -target … -mcpu=…` as an argument list, and never emit the flags zig rejects.

### Target names

Ours stay LLVM's (codegen needs them); zig gets its own spelling from a fixed table, never string rewriting:

| `--target` | zig `-target` |
|---|---|
| `x86_64-unknown-linux-musl` | `x86_64-linux-musl` |
| `aarch64-unknown-linux-musl` | `aarch64-linux-musl` |
| `aarch64-unknown-linux-gnu` | `aarch64-linux-gnu.<glibc>` (e.g. `.2.28`) |
| `aarch64-apple-darwin` | `aarch64-macos.<min>-none` |
| `x86_64-apple-darwin` | `x86_64-macos.<min>-none` |
| `x86_64-pc-windows-gnu` | `x86_64-windows-gnu` |

zig renamed parts of these in 0.11, 0.15 and 0.18 (`i386` → `x86`; ARM float ABIs). `--cpu` maps to `-mcpu` with underscores; `baseline` when none is given.

### Per OS

- **Linux musl:** zig ships musl 1.2.5 and links statically by default. **It links no unwinder for a C program on musl**: only static glibc or libc++ pull in libunwind automatically. Panics unwind (`_Unwind_RaiseException`), so the link needs `-lunwind`, which makes zig build its bundled LLVM libunwind. Whether backtraces are complete in static musl binaries is *unverified* (LDC dropped libunwind on musl over wrong line numbers): an end-to-end panic test decides it. `-lm`, `-lpthread` are absorbed into libc.
- **Linux glibc (foreign):** a version suffix picks the oldest glibc the binary needs (zig defaults to 2.28).
- **macOS:** zig ships libSystem stubs and headers, so a program using only libSystem (pthreads included) links with no Apple SDK. Frameworks need `SDKROOT`. arm64 binaries are ad-hoc signed by zig's linker, so no `codesign` afterwards; re-signing x86-64 binaries broke them in 0.16 (too little header padding). `_Unwind_*` comes from libSystem. The minimum macOS goes in the triple.
- **Windows (MinGW):** zig ships MinGW-w64. `-o` doesn't add `.exe` (*unverified*), so lughac names the file. Unwinding through SEH with LLVM libunwind under zig is *unverified*; expect `-lunwind`. cargo-zigbuild supports only Linux and macOS officially.
- **OpenSSL:** zig has none. cargo-zigbuild's answer is "build it for the target yourself" or rustls. Linking the host's `-lssl` for a foreign target is wrong. `std.tls` and `std.crypto` on a foreign target must be refused until a target OpenSSL can be named.

### Stability and installing

- Releases every 6–8 months (0.15 2025-08, 0.16 2026-04, 0.17 2026-10; 0.17.1 is current). Every one changed something cargo-zigbuild branches on: names, sysroot handling, linker flags. Everyone pins or checks the version.
- **0.17.0** fixes a race where parallel builds on a cold cache failed at random (#36490). zig 0.17 uses LLVM 22 while lughac uses LLVM 21; that doesn't matter, since zig gets object files, never bitcode.
- MIT licence (compatible with GPL-3.0). Installed from ziglang.org tarballs, PyPI's `ziglang` (`python -m ziglang cc`) or package managers (often old).

## 3. Splitting the runtime by OS

### The lesson: draw the line at operations, not at the poller

Every runtime that reached Windows hit the same mismatch: epoll and kqueue say "this socket is **ready**, now you read"; Windows' IOCP says "the read you started is **done**".

- **Go** keeps one poller interface (`netpollinit`, `netpollopen`, `netpoll(delay)`, `netpollBreak`) with epoll, kqueue and IOCP backends, but the real split is one level up, in `internal/poll`: on Unix `FD.Read` calls `read()` and parks on `EAGAIN`; on Windows it starts an overlapped `WSARecv` and parks until it completes. The scheduler only ever hears "this waiter is done". A timeout or cancel calls `CancelIoEx` and **still waits for the completion** before the buffer may be reused.
- **mio 0.6** faked readiness on Windows by starting reads early into its own buffers; its own docs called writes "abysmal". **mio 0.7+** (and **wepoll**, and smol's `polling`) use the undocumented `\Device\Afd` poll request instead: real readiness on Windows, sockets only, stable for over ten years, but undocumented.
- **libuv** gives every platform a completion-style API (`uv_read_start` with callbacks). On Unix it waits for readiness and then reads, at little cost. On Windows TCP it starts a zero-byte read, whose completion means "readable", then reads without blocking.

**For us:** today `net.c` calls `epoll_ctl` itself. If the OS layer is "register an fd, wait for readiness", Windows needs AFD (undocumented) or a rewrite. If the OS layer is the operations themselves (`recv`, `send`, `accept`, `connect`, `cancel`, `close`), POSIX keeps exactly today's code behind them and Windows uses documented overlapped I/O. Green threads make overlapped I/O easy: the buffer lives in the parked task's frame.

### Waking the poller

| | Linux | macOS | Windows |
|---|---|---|---|
| wake from any thread | `eventfd` | `kqueue` `EVFILT_USER` + `NOTE_TRIGGER` | `PostQueuedCompletionStatus` |
| a helper's job finished | queue + wake | queue + wake | post the job straight to the port |

Go and libuv both deduplicate wakes with one atomic flag in shared code.

### Parking workers

| OS | wait on a word | |
|---|---|---|
| Linux | `futex` (private) | today |
| Windows 8+ | `WaitOnAddress` / `WakeByAddressSingle` | in-process |
| macOS 14.4+ | `os_sync_wait_on_address` | public; older versions only have the private `__ulock_wait` |
| older macOS | `dispatch_semaphore` or mutex + condition | what Rust std does on Apple |

Go parks **per thread** (futex on Linux/FreeBSD; a semaphore per thread on macOS and Windows). Our idle workers already park on their own word, so a **per-worker parker** with the futex as its fast path works everywhere. `rt_lock.c`'s futex lock becomes either built on the parker or the platform's mutex.

### Stack overflow

- **macOS:** `sigaltstack` works, but a guard-page hit arrives as **SIGBUS**, not SIGSEGV; Rust handles both.
- **Windows:** no signals. A vectored exception handler catches `EXCEPTION_STACK_OVERFLOW`; `SetThreadStackGuarantee` leaves it room. A task's stack must copy Windows' own layout (reserved range, committed part, a `PAGE_GUARD` page, a final guard), and **the task switch must save and restore the thread block's stack fields** (`StackBase`, `StackLimit`, `DeallocationStack`, `GuaranteedStackBytes`), or exceptions and stack probes break (corosensei, boost.context). Windows x64 also saves different registers (`xmm6`–`xmm15`) and unwinds through SEH tables.

### WASI

Preview 1's `poll_oneoff` takes clock and fd subscriptions in one call (readiness, no registration); preview 2 has `wasi:io/poll`. Without threads: one worker, blocking work done inline, the wake a no-op, an overflow traps the instance. Our design fits if the worker count can be 1 and helpers are optional. (*Not checked against the current spec.*)

## 4. What this suggests for Lugha

### Toolchain

1. Native target: `cc` as today. Foreign target: `zig cc -target <table entry> -mcpu=<cpu>`, an argument list, no wrapper. `$LUGHA_CC` overrides both (multi-word, so `python -m ziglang cc` works).
2. **zig 0.17.0 or newer**, checked once per build (`zig version`) with a clear error; re-tested on each zig release.
3. `ZIG_GLOBAL_CACHE_DIR` under lughac's cache root.
4. musl and Windows link with `-lunwind`; musl with an explicit `-static`; an end-to-end panic-and-unwind test per target (musl under qemu).
5. Foreign `std.tls`/`std.crypto`: an error, never the host's OpenSSL.
6. macOS: minimum version in the triple, no `codesign`; frameworks need `SDKROOT`. Windows: name the `.exe`; experimental until unwinding passes under Wine.

### Runtime (decide in C1, before any port)

1. **The OS line is the operation** (`recv`/`send`/`accept`/`connect`/`cancel`/`close`), not fd registration. Readiness is a POSIX detail inside `os/posix.c`.
2. **The poller returns "waiter done" records**, not readiness bits; helper jobs complete through the same path (`post`), not a separate eventfd.
3. **A cancel waits for the OS to let go of the buffer.** A timeout or a scope's cancellation of a socket call must allow for an acknowledgement that comes later (Windows); on POSIX it is immediate.
4. **Parking is per worker**, futex where it exists.
5. **Handles are opaque** in shared code (an `int` on POSIX, a `SOCKET`/`HANDLE` on Windows); `std.net`'s `i64` already holds either.
6. **The task switch is per CPU × OS**, with room for Windows' thread-block fields.

A minimal surface, sketched (names illustrative):

```c
/* poller */
int  rt_poller_wait(Waiter **done, int max, int64_t timeout_ns); /* epoll_wait | kevent | GetQueuedCompletionStatusEx */
void rt_poller_wake(void);            /* eventfd | EVFILT_USER | PostQueuedCompletionStatus; any thread */
void rt_poller_post(Waiter *w);       /* a helper's job is done */

/* sockets: the OS line */
int32_t rt_sock_recv(Sock *s, void *buf, size_t n, int64_t timeout_ns); /* parks the task */
int32_t rt_sock_send(Sock *s, const void *buf, size_t n, int64_t timeout_ns);
int32_t rt_sock_accept(Sock *s, Sock *out, int64_t timeout_ns);
int32_t rt_sock_connect(Sock *s, const void *addr, int len, int64_t timeout_ns);
void    rt_sock_cancel(Sock *s);      /* waits for the OS's acknowledgement */
void    rt_sock_close(Sock *s);

/* parking: per worker */
void rt_park(Parker *p, uint32_t expected, int64_t timeout_ns); /* futex | os_sync / dispatch_semaphore | WaitOnAddress */
void rt_unpark(Parker *p);

/* stacks */
int  rt_stack_alloc(size_t size, Stack *out);  /* guard included; Windows' layout */
int  rt_overflow_install(void);                /* sigaction SEGV+BUS | AddVectoredExceptionHandler */
int  rt_overflow_thread(void);                 /* sigaltstack | SetThreadStackGuarantee */

/* misc */
int      rt_random(void *buf, size_t n);  /* getrandom | getentropy (256-byte chunks) | BCryptGenRandom */
uint64_t rt_now_ns(void);                 /* CLOCK_MONOTONIC | CLOCK_UPTIME_RAW or MONOTONIC (sleep counts?) | QueryPerformanceCounter */
```

### Can wait

AFD versus overlapped I/O on Windows; `os_sync` versus a semaphore on older macOS; Windows' high-resolution timers; io_uring on Linux (fits a completion-shaped layer for free); pipes, console and processes on Windows; WASI's single-thread mode; lockdown on other OSes; whether a sleeping Mac's time counts on the monotonic clock.
