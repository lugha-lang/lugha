# macOS: research before the rest of step 4

Hello world runs on a Mac (milestone 251). What's left needs answers macOS gives differently from Linux: folder confinement without `openat2`, what lockdown can mean, a too-long UDP datagram's length, close-on-exec pipes without `pipe2`, and the Linux-isms that are easy to miss. Nothing here is decided.

Sources (2026-11-23): XNU source (apple-oss-distributions/xnu: `bsd/sys/fcntl.h` across tags 6153, 7195, 10063 = macOS 14.4, 11215 = 15.0, 11417 = 15.4, 12377 = 26; `bsd/vfs/vfs_lookup.c`, `vfs_syscalls.c`, `vfs_vnops.c`; `bsd/kern/uipc_socket.c`, `sys_socket.c`, `kern_exec.c`, `kern_descrip.c`); Apple's `open(2)` man page; cap-std (`cap-primitives/src/rustix/fs/mod.rs`, `fs/manually/open.rs`); Go (`os/root_openat.go`, `root_unix.go`, `syscall/forkpipe.go`, `net/udpsock_posix.go`); Rust std (`sys/pipe/unix.rs`); libuv (`unix/udp.c`, `unix/process.c`, `unix/pipe.c`); CPython `subprocess.py`; Chromium `sandbox/mac/seatbelt.cc`; zig 0.17.1's macOS headers and `libSystem.tbd`. Behaviour read from source but not tried on a Mac is marked *untested*.

## 1. Folder confinement

**What macOS has, and since when:**

| Flag | macOS |
|---|---|
| `O_NOFOLLOW_ANY`, `AT_SYMLINK_NOFOLLOW_ANY`, `RENAME_NOFOLLOW_ANY` (no symlink in any component) | 11+ |
| `O_RESOLVE_BENEATH` (`open`/`openat` only) | 15.4 |
| `AT_RESOLVE_BENEATH`, `RENAME_RESOLVE_BENEATH` | 26 |

- `O_RESOLVE_BENEATH` is weaker than Linux's: `..` is refused only when the lookup stands on the starting folder itself, with no recheck for a folder renamed mid-lookup (*untested*).
- On 14.x, `O_RESOLVE_BENEATH`'s bit is the kernel's internal `FMARK`, **silently ignored**: no confinement and no error. Never rely on it without a startup probe.
- `mkdirat`, `symlinkat` and `readlinkat` take no flags at all.

**How others do it:** cap-std (and so wasmtime) walks paths one component at a time on macOS, following symlinks itself and keeping a stack of folder descriptors, never asking the kernel for `..`. Go's `os.Root` does the same, rewriting `..` lexically and restarting from the root ("dir may have moved since we opened it"), with caps of 255 steps and 8 restarts. Neither uses macOS's flags.

**Two sound designs for macOS 14.4+:**

1. **Refuse `..`, absolute paths and every symlink**, then one call from the `Dir`'s descriptor with `O_NOFOLLOW_ANY` (`AT_SYMLINK_NOFOLLOW_ANY`, `RENAME_NOFOLLOW_ANY`); `mkdirat` after opening the parent the same way. With no `..` and no symlink, a lookup only goes down from the folder: Linux's guarantee. Simple. **But** a symlink that stays inside the folder is refused, which Linux allows (`tests/programs/capabilities/dir_confinement/confined.la` checks that it works).
2. **Walk like Go's `os.Root`:** one `openat(parent, name, O_NOFOLLOW)` per component, symlinks read and followed by the runtime, `..` resolved lexically against the walk so far, capped. Same behaviour as Linux, inside links included; more code, and the known TOCTOU traps (never re-resolve a string, never ask the kernel for `..`).

Either way: hard links inside a folder can point at files elsewhere, on macOS as on Linux; APFS is case-insensitive, so no name-based allow or deny lists.

## 2. Lockdown

- **Seatbelt** (`sandbox_init`, a profile in SBPL): deprecated and private, but in libSystem (zig's stub exports `sandbox_init`, `sandbox_init_with_parameters`, `sandbox_compile_string`, `sandbox_apply`), no SDK needed; Chromium declares the functions itself ("OS X deprecated these functions, but did not provide a suitable replacement"), Firefox and Bazel use it too. A process can apply it to itself after startup; it can't be lifted; children inherit it.
- **It could deny**: new sockets (without `Net`), exec and fork (without `Proc`), file access outside granted paths (without `Fs`).
- **It can't promise**: stability (the profile language is private and Apple can change it); already-open descriptors; a syscall-level filter like seccomp; and denying reads late can break DNS (mDNSResponder), TLS trust, time zones and lazy library loads. Must be tested on real 14 and 15 machines.
- **Alternatives don't fit**: App Sandbox needs signing and entitlements; Endpoint Security needs a system extension; `RLIMIT_NPROC` stops fork but not exec, and not for root.

## 3. A too-long UDP datagram

- macOS ignores `MSG_TRUNC` as an input flag: `recvfrom` returns only what fit, so today **a long datagram is cut short silently on macOS**.
- `recvmsg` sets `MSG_TRUNC` in `msg_flags` (reliable, no length). `SO_NREAD` gives the first datagram's length but races with other readers; `FIONREAD` counts every queued datagram.
- Go, Rust and tokio truncate silently; libuv reports only "partial"; Node relies on a 64 KiB buffer.
- **Best:** one `recvmsg` with two buffers, the program's `max` bytes then a 64 KiB scratch area. A UDP payload is at most 65,527 bytes, so the call returns the real length; one call, no race, and the same on Linux, replacing its `MSG_TRUNC` trick.

## 4. Close-on-exec pipes

- `pipe2` and `dup3` are macOS 27 only (weakly linked today, so starting a program would fail before 27).
- Go: `pipe` + `fcntl` under `ForkLock`; Rust std: `pipe` + `fcntl`, no lock, the race accepted; libuv: `pipe` + `fcntl`, and every `posix_spawn` with **`POSIX_SPAWN_CLOEXEC_DEFAULT`**.
- `POSIX_SPAWN_CLOEXEC_DEFAULT` (macOS 10.7) closes, atomically at exec, every descriptor the file actions don't name: our own program starts can never leak a pipe. It also closes the child's stdin, stdout and stderr unless named (`posix_spawn_file_actions_addinherit_np` for one already in place; `adddup2(fd, fd)` doesn't work, as `FD_CLOEXEC` wins).
- Left over, as for Go, Rust and libuv: a child started by foreign C code (`system()`) in the two calls between `pipe` and `fcntl` can inherit a pipe.

## 5. Linux-isms that are easy to miss

| Linux | macOS |
|---|---|
| `CLOCK_MONOTONIC` (stops while asleep) | `CLOCK_UPTIME_RAW`; macOS's `CLOCK_MONOTONIC` keeps counting through sleep |
| `ip_mreqn` for `IP_ADD_MEMBERSHIP` | `ip_mreq`, or `MCAST_JOIN_GROUP` with `group_req` (an interface index) |
| `MSG_NOSIGNAL` | `SO_NOSIGPIPE` (done) |
| `accept4`, `SOCK_CLOEXEC`/`SOCK_NONBLOCK` | `fcntl` after (done) |
| `pthread_setname_np(thread, name)` | `pthread_setname_np(name)`, current thread only |
| `recvmmsg`, `TCP_KEEPIDLE`, `MSG_MORE` | none; `TCP_KEEPALIVE`, `TCP_NOPUSH` |
| `sun_path[108]`, abstract sockets | `sun_path[104]`, no abstract namespace |

## 6. What this suggests

- **A "POSIX gaps" PRP** needs no decision: UDP's two-buffer receive (both systems), `POSIX_SPAWN_CLOEXEC_DEFAULT` with `pipe` + `fcntl` (🔒 `process.c`), multicast through `group_req`, and the sleep-aware clock question.
- **Confinement** is a choice between design 1 (simple, refuses links that stay inside) and design 2 (Linux's behaviour, more code).
- **Lockdown** is a choice between "not applied" (honest, today) and a best-effort Seatbelt layer, labelled partial.
