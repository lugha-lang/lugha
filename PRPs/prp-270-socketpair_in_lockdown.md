## FEATURE: Process lockdown lets every program make an unnamed `AF_UNIX` socket pair, so a program holding `Proc` but not `Net` can start tools that use one to start their own children (uutils `timeout`, anything built on Rust's `std::process`); milestone 238.

**Status:** approved 2026-11-20 (reviewed by the user: it changes what the kernel lets a program do, `runtime/lockdown.c`).

## OBJECTIVE
```
fun main(console: Console, proc: Proc) {
    match proc.run("timeout", ["5", "echo", "hi"]) {
        .Ok(out) => console.println("{out.code} {out.stdout.trim()}"),
        .Err(e) => console.println(e),
    }
}
```
On Ubuntu 26.04, whose `timeout` is uutils (Rust), this prints today:
`126` (stderr: `timeout: failed to execute process: Operation not permitted (os error 1)`).

After this change it prints `0 hi`. A program without `Net` still can't make any socket that reaches anything: `socket()` of every domain, and `socketpair` of every domain but `AF_UNIX`, stay refused.

## CONTEXT
- **Asked for by:** the user (2026-11-20). lugha-dev's sandbox runner is a v18 program holding `Fs` and `Proc`. It wraps every compile in `timeout`, and on Ubuntu 26.04 every compile fails. `strace` shows the one refused call:
  `socketpair(AF_UNIX, SOCK_SEQPACKET|SOCK_CLOEXEC, 0, …) = -1 EPERM`.
  It was reproduced on the host, with seccomp and Landlock applied, and under gVisor, with seccomp only. GNU `timeout` (fork/exec, no sockets) works.
- **The user's call** (discovery, 2026-11-20): `AF_UNIX` `socketpair` is allowed for every program, not only those holding `Proc`.
- **Spec:**
  - §9 "Process lockdown" says seccomp refuses "`socket` and `socketpair` without `Net`". This changes to `socket`, and `socketpair` of any domain but `AF_UNIX`.
  - The v18 summary line at the top: "without `Net` it can't open a socket".
  - §11 gains row 238.
- **Decisions:**
  - SEC-05 (`docs/decisions/v18-ledger.md`): "without `Net`, no sockets".
  - PRP-221's design lists `socket` and `socketpair` "(any domain)".
  - This PRP narrows that to sockets that can reach something. An unnamed pair has no address: nothing outside the process can connect to it, and it can only be shared the way a pipe can, by inheritance or descriptor passing. A new resolved decision, 227, records the change and why. SEC-05's intent, "without `Net`, no network", holds.
- **Code:**
  - `runtime/lockdown.c`, `lock_seccomp`, where the denylist is built;
  - `tests/lockdown.rs`, end to end through `lughac run`.

## REQUIREMENTS

### Must Do
- In `lock_seccomp`, without `Net`: `socket` stays refused with `EPERM`. `socketpair` is allowed when its first argument (the domain) is `AF_UNIX`, and refused with `EPERM` otherwise. This holds with or without the other capabilities.
- The check reads only the low 32 bits of the domain, since the kernel takes `int domain`. The filter's accumulator is restored for the checks after it (the `clone` thread check reads `nr` again), or the socketpair check ends in a return on both paths.
- Both architectures that have the filter, x86-64 and ARM64, use their own `SYS_socketpair`.
- Docs:
  - the spec's §9 paragraph and the summary line;
  - §11 row 238;
  - the "Changes since" list;
  - CHANGELOG;
  - DECISIONS.md 227;
  - the comment above `lock_seccomp`;
  - PRP-221's design is history and isn't edited.

### Must NOT Do
- Allow `socket()` in any domain without `Net`. That would reach filesystem sockets such as `docker.sock`, and abstract ones.
- Allow `socketpair` of `AF_INET`, `AF_INET6` or any other domain. Linux refuses most with `EOPNOTSUPP` anyway, but the filter shouldn't depend on that.
- Change anything for programs holding `Net`, or Landlock's rules.

## DESIGN
- **The BPF.** The block sits after the `io_uring` refusals, inside `if (!(kinds & LOCK_NET))`, replacing `lock_refuse(&f, SYS_socketpair, EPERM)`:
  ```
  JEQ nr == SYS_socketpair ? next : skip 4
  LD  args[0] (low word)
  JEQ == AF_UNIX ? next : skip 1
  RET ALLOW
  RET ERRNO|EPERM
  ```
  Both paths return, so the accumulator holding `args[0]` never reaches a later check.
- **Security note for review.** An `AF_UNIX` socket pair gives two connected descriptors in this process, the same reach as `pipe2`, which is allowed. It has no name, so no other process can connect to it. It can carry descriptors (`SCM_RIGHTS`), but only ones this process already holds. It opens nothing new: no file, no network, no other process.
  - Abstract-namespace unix sockets need `socket()` + `bind`/`connect`, which stay refused.
  - Landlock's ABI 6+ scoping of abstract sockets is unaffected.
- **No new diagnostics, no ABI change, no new dependency.**

## TESTS
- [ ] `tests/lockdown.rs`, `a_unix_socket_pair_needs_no_net`: a `main` holding only `Console` and `Proc` runs:
  - `python3 -c "import socket; socket.socketpair()"`: exit 0. Skipped when there is no `python3`, as the started-programs case does.
  - `python3 -c "import socket; socket.socket()"`: still refused, exit non-zero.
  - When seccomp is applied, a uutils-or-GNU `timeout 5 true` exits 0.
- [ ] `tests/lockdown.rs`, existing: `without_capabilities_c_code_makes_no_socket_process_or_file` (`socket(2, 1, 0)` is still `-1`) and `started_programs_share_the_limits` (`socket.socket()` is still refused) pass unchanged.
- [ ] `cargo test --lib`, `--test lockdown` and `--test spec`.

## ACCEPTANCE
- [ ] The objective's program prints `0 hi` on Ubuntu 26.04, under a seccomp-capable kernel.
- [ ] Without `Net`, `socket()` of every domain and `socketpair` of every domain but `AF_UNIX` are still `EPERM`.
- [ ] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Spec, CHANGELOG, DECISIONS.md and the `lock_seccomp` comment are updated.
- [ ] Tried by hand: `lughac run` on the objective's program, and lugha-dev's sandbox runner building a program inside gVisor.
