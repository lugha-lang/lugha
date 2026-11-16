## FEATURE: Process lockdown: once the runtime has made what `main` asked for, the kernel limits the whole process — C code and started programs included — to the kinds of capability `main` declared: no sockets without `Net`, no starting programs without `Proc`, no files without `Fs` (beyond what the held capabilities need); seccomp and Landlock on Linux, best effort; milestone 189, in v18's Phase 2. **Kernel-level confinement: needs the user's security review before any code.**

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
extern fun socket(domain: i32, kind: i32, protocol: i32): i32;
extern fun open(path: string, flags: i32): i32;

fun main(console: Console) {                  // no Net, no Fs, no Proc
    console.println(socket(2, 1, 0));          // -1: the kernel refuses (EPERM)
    console.println(open("/etc/passwd", 0));   // -1: no files without Fs (where Landlock exists)
}
```
```lugha
fun main(console: Console, proc: Proc) {      // Proc, no Net
    let r = proc.run("curl", ["-sS", "https://example.com"]);
    console.println(r.unwrap().code != 0);     // true: curl inherits the limits, so no sockets
}
```
A program taking `Fs`, `Net` and `Proc` runs as today: lockdown adds nothing it would notice.

## CONTEXT
- Draft v18 §2.2 rule 5 [SEC-05], approved: "once the runtime has opened what `main` asked for, it restricts the whole process to the kinds of capability `main` declared (seccomp and Landlock on Linux), so C code can't exceed them either: without `Net`, no sockets; without `Proc`, no starting programs; without `Fs`, no files. It is as coarse as `main`'s parameters: a `main` that holds `Fs` leaves the whole filesystem open to C, and narrowing to a `Dir` later binds Lugha code only. It works per program, not per library. Linux first; kernel versions and system-call lists verified before the PRP."
- Decided with the user: **started programs share the limits** (the kernel passes seccomp filters and Landlock rules through `fork`, `clone` and `execve`, and they can never be loosened); **best effort, quietly** on kernels or containers that can't apply a part.
- Verified against the kernel documentation ([Landlock](https://docs.kernel.org/userspace-api/landlock.html), [landlock(7)](https://man7.org/linux/man-pages/man7/landlock.7.html), [no_new_privs](https://docs.kernel.org/userspace-api/no_new_privs.html), [PR_SET_NO_NEW_PRIVS](https://man7.org/linux/man-pages/man2/PR_SET_NO_NEW_PRIVS.2const.html)): Landlock file rights from Linux 5.13 (ABI 1), `REFER` 5.19 (2), `TRUNCATE` 6.2 (3), TCP `BIND`/`CONNECT` 6.7 (4), device `IOCTL` 6.10 (5), signal and abstract-socket scoping 6.12 (6); UDP rules only much later. seccomp needs `no_new_privs` (or `CAP_SYS_ADMIN`); filters and `no_new_privs` are inherited through `fork`, `clone` and `execve` and can't be removed.
- Measured here with `strace` on Lugha programs: the collector opens `/proc/self/maps` once, at start-up, before `main` runs, and otherwise uses `mmap`, `munmap`, `mprotect` and `brk`. Name lookup reads `/etc/hosts`, `/etc/resolv.conf`, `/etc/nsswitch.conf`, `/etc/host.conf` and may load NSS libraries; `Clock.local_offset` reads `/etc/localtime` (and `/usr/share/zoneinfo` under `TZ`); `Proc` uses `clone3`/`execve`/`wait4`/`pipe2`; TLS reads the system's trusted roots (`/etc/ssl`, `/usr/lib/ssl`). `io_uring` can open sockets and files without the system calls seccomp sees.
- The collector is to be retired (Phase 3). Its one need here (`/proc/self/maps`) falls before lockdown, so nothing in lockdown depends on it; the memory system calls any allocator uses stay allowed.
- Code: new `runtime/lockdown.c`, the C `main` and the test entry (`src/codegen/lower.rs`, `test_main.rs`), `src/link/mod.rs`'s lists, `runtime/lugha_rt.c` (includes).

## DESIGN — for the security review

### When
The C `main` makes `main`'s arguments (the capabilities) as today, then calls `lugha_rt_lockdown(kinds, folder)` with a bit for each capability kind `main` took, then calls the program's `main`. A test executable does the same per test, with the test's kinds. Nothing of the program's own runs before it; constants are computed before it (they can't touch the world: E0445).

### seccomp: system calls, for `Net` and `Proc`
`prctl(PR_SET_NO_NEW_PRIVS, 1)` then a seccomp filter (`SECCOMP_SET_MODE_FILTER`, with `SECCOMP_FILTER_FLAG_TSYNC`), checking the architecture first (`AUDIT_ARCH_X86_64` or `AUDIT_ARCH_AARCH64`; anything else is refused with `EPERM`). It allows everything except, always refused with `EPERM`:
- **Always:** `io_uring_setup`, `io_uring_enter`, `io_uring_register` (they bypass seccomp's view of sockets and opens; Lugha doesn't use them).
- **Without `Net`:** `socket` and `socketpair` (any domain), so no new socket of any kind; `bind`, `connect`, `listen`, `accept`, `accept4` on descriptors that can't exist anyway.
- **Without `Proc`:** `execve`, `execveat`, `fork`, `vfork`, `clone3` (refused with `ENOSYS`, so the C library falls back to `clone`), and `clone` without `CLONE_THREAD` (a new process, not a thread: future helper threads stay possible).
A denylist, not an allowlist: the C library's choice of system calls changes between versions (`clone3`, `rseq`, `statx`), and an allowlist would break programs on the next libc. Every refusal is an error return, never a kill, so a program sees `-1`/`Err` and carries on.

### Landlock: files, for `Fs`
Without `Fs`, a Landlock ruleset handles every file right the kernel's ABI knows, and allows only:
- **Always:** nothing written anywhere; reading the console's descriptors needs no rule (already open).
- **With `Net`:** reading `/etc/hosts`, `/etc/resolv.conf`, `/etc/nsswitch.conf`, `/etc/host.conf`, `/etc/gai.conf`, `/etc/services`, `/etc/ssl`, `/usr/lib/ssl`, `/usr/share/ca-certificates`, and reading and executing (mapping) the library folders (`/lib`, `/usr/lib`, `/lib64`, `/usr/lib64`) for NSS modules.
- **With `Clock`:** reading `/etc/localtime` and `/usr/share/zoneinfo`.
- **With `Proc`:** reading and executing everything (`/`): a started program could read any file anyway (`cat`), and needs its binary and libraries. Writing stays refused.
- **A test's `Dir`:** reading and writing beneath the test's folder.
With `Fs`, no Landlock file ruleset: the whole file system is open, as SEC-05 says. With `Net` on Landlock ABI 4 or later, nothing more: TCP ports aren't limited (any port is the `Net`'s to use). Without `Net` on ABI 4+, a ruleset handling `BIND_TCP` and `CONNECT_TCP` with no rules, belt and braces beside seccomp. On ABI 6+, signals and abstract UNIX sockets are scoped to the process.

### Best effort, quietly
`landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)` gives the ABI; the ruleset asks only for rights that ABI knows. No Landlock (`ENOSYS`, `EOPNOTSUPP`), or seccomp refused by a container: that part is skipped, the rest applied, nothing printed. `LUGHA_LOCKDOWN=report` (an environment variable, read once at start) prints one line to stderr naming what was and wasn't applied, for people checking a deployment.

### What lockdown doesn't do
- **`Console`, `Env`, `Clock`'s time, `Entropy`:** not limited (panics need stderr; the environment and clocks are in the process already; the hash seed needs `getrandom`).
- **Per-library limits:** one process, one set (SEC-05: "per program, not per library").
- **Narrowings** (`Dir`, `net.only`): bind Lugha code only; the kernel limits only the kinds `main` took.

### For the security review, in one list
1. The seccomp denylist (always `io_uring`; without `Net` the socket calls; without `Proc` the process-making calls, `clone` filtered on `CLONE_THREAD`, `clone3` as `ENOSYS`); `EPERM` returns, never kills; the architecture check.
2. The Landlock read-only lists for `Net`, `Clock` and `Proc`, and the test folder's read-write rule.
3. Applied after the capabilities are made and before any program code; children inherit it; nothing can undo it.
4. Best effort, quietly, with `LUGHA_LOCKDOWN=report` to see what applied.
5. What stays open: the console, the environment, the clocks, randomness; `Fs` opens all files.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **A denylist for seccomp** (above) and **`EPERM`, not kill**.
2. **`Proc` grants reading the whole file system** (not writing): its children need their binaries and libraries, and could read anything anyway.
3. **`LUGHA_LOCKDOWN=report`** as the only way to see what applied; no command-line flag.
4. **Tests are locked down too**, by what each test takes (plain or integration).
5. **`runtime/lockdown.c`**, a new fragment; the system-call numbers come from the target's `<sys/syscall.h>` (x86-64 and ARM64 both).

## REQUIREMENTS

### Must Do
- `runtime/lockdown.c`: `lugha_rt_lockdown(kinds, folder)`, the seccomp filter, the Landlock ruleset, best effort, the report.
- Codegen: the C `main` and the test entry call it after making the capabilities.
- Spec (§2.2-style paragraph in the capabilities section; the runtime table; §11 row 189; Changes; Breaking: C code without the capability now fails), CHANGELOG, CLAUDE.md's security list.

### Must NOT Do
- Sandboxed C (`sandbox = true`), per-library limits, limiting the console, the environment or the clocks.
- Killing a process for a refused call.
- Any platform but Linux; the ARM64 build is compiled and checked under `qemu-user` where the tests already do.

## TESTS
- [ ] `tests/lockdown.rs` (each case skips with a note where the kernel lacks the feature): a program without `Net` gets `-1` from `socket()` through an `extern`; with `Net` it doesn't; without `Proc`, `fork`/`execve` through C fail and `proc` isn't there to call; with `Proc` and without `Net`, a started `sh -c 'exec 3<>/dev/tcp/127.0.0.1/1'`-style probe (or a small C helper) can't make a socket; without `Fs`, opening `/etc/passwd` through C fails (Landlock), with `Net` `/etc/hosts` still reads and `localhost` still resolves, with `Clock` `local_offset` still works; with `Fs` everything opens; `io_uring_setup` always fails; `LUGHA_LOCKDOWN=report` prints what applied.
- [ ] A plain test (through `lughac test`) can write in its folder and not outside it through C.
- [ ] Targeted runs: `--lib`, `programs`, `net`, `http`, `tls`, `fs_time`, `test_capabilities`, `ffi_permission`, the C-interop tests; CI the rest.

## ACCEPTANCE
- [ ] On a kernel with seccomp and Landlock, no code in a program — Lugha, C, or a started program — makes a socket without `Net`, starts a program without `Proc`, or opens a file outside what its capabilities need without `Fs`.
- [ ] On a kernel without them, programs still run.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass
- [ ] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated where they change
