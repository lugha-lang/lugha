## FEATURE: Sockets behind the OS layer at the operation level, and a cancelled socket call that waits for the OS — milestone 250

## OBJECTIVE

Nothing a Lugha program does changes. Inside the runtime, every call that touches a socket goes through `os_sock_*` operations, so the line between shared code and the system is the operation (open, listen, connect, accept, receive, send, close), not "register a descriptor and wait until it's ready". That is the line Windows needs: its sockets complete operations (IOCP) instead of reporting readiness, and with this boundary `os/windows/sockets.c` implements the same operations without touching shared code.

```
runtime/
├── io/
│   ├── net.c          SHARED: what Lugha calls (lugha_rt_net_*): addresses, timeouts, values, messages;
│   │                  the poll (poll_found, poll_sockets) and the parked-socket list
│   ├── udp.c          SHARED: lugha_rt_udp_bind/send/recv the same way
│   └── resolve.c      SHARED (unchanged): getaddrinfo, private addresses
└── os/
    ├── layer.c        + OsSocket, OS_TIMED_OUT / OS_CANCELLED / OS_CLOSED, os_sock_* (below)
    ├── posix/
    │   ├── process.c 🔒
    │   ├── sockets.c  NEW: os_sock_* for Linux and macOS: the call, then on EAGAIN wait for
    │   │              readiness (wait_fd, moved here: readiness is POSIX's model)
    │   └── udp_options.c  NEW, moved from io/udp.c: multicast groups, socket options,
    │                  interfaces (setsockopt, getifaddrs): POSIX's, Windows writes its own
    ├── linux/os.c     + the few Linux-only socket details (below)
    ├── darwin/        ← step 4: os.c gets the same details for macOS (fcntl, SO_NOSIGPIPE)
    └── windows/       ← step 5: sockets.c (overlapped WSARecv/WSASend/AcceptEx/ConnectEx on IOCP),
                       udp_options.c (Winsock options, GetAdaptersAddresses)
```

## CONTEXT

- Read first: `runtime/io/net.c` (poll, `wait_fd`, `net_waits`, every `lugha_rt_net_*`), `runtime/io/udp.c`, `runtime/os/layer.c`, `runtime/os/linux/os.c`, `src/link/runtime_parts.rs`, `src/link/mod.rs` (the boundary test).
- Implements: v18 Draft, Part 9, 9.2 rule 1's operations line and rule 2 (a cancelled socket call waits for the OS) [PRT-08, PRT-09]; step 3 of 9.3, second half. Research: `docs/research/cross-platform.md` §3 (Go's `internal/poll`, mio, libuv).
- Decisions: v18 ledger PRT-08, PRT-09 (approved). None open.
- Follows PRP-283 (the layer, the folders).

## REQUIREMENTS

### Must Do

1. **The operations**, declared in `os/layer.c`; each waits as a task does (parking the task, never the thread), honours a deadline (monotonic ns, < 0 for none), and returns a count or handle (≥ 0), or `OS_FAILED` with the system's message, `OS_TIMED_OUT`, `OS_CANCELLED` or `OS_CLOSED` (another task closed it):
   - `os_sock_open(family, type, protocol)`: non-blocking, close-on-exec, no SIGPIPE.
   - `os_sock_listen(s, addr, len, backlog)`: address reuse, bind, listen.
   - `os_sock_bind_udp(s, addr, len, shared)`: reuse (when `shared`), only its own multicast groups, bind.
   - `os_sock_connect(s, addr, len, deadline)`, `os_sock_accept(s, deadline)`.
   - `os_sock_recv(s, buf, max, deadline)`, `os_sock_send(s, buf, n, deadline)` (all of it).
   - `os_sock_recvfrom(s, buf, max, from, &len, deadline)` → the datagram's real length where the system gives it; `os_sock_sendto(s, buf, n, to, len, deadline)`.
   - `os_sock_name(s, peer, &storage, &len)`; `os_sock_close(s)`.
2. **The cancel contract** [PRT-09], stated in `os/layer.c`: `OS_CANCELLED` is returned only once the system has let go of the operation's buffer. On POSIX that is at once (nothing outstanding after a readiness wait); Windows' implementation will cancel the overlapped operation and wait for its completion first. Shared code frees what it holds and unwinds only after `OS_CANCELLED` comes back — today's order, now a rule.
3. **`os/posix/sockets.c`** implements them as today's code does (the call; on `EAGAIN`, `wait_fd`; `EINTR` retried; `connect`'s `EINPROGRESS` then `SO_ERROR`), moved, not rewritten. `wait_fd` moves there with them.
4. **Linux's details in `os/linux/os.c`**, so `os/posix/` holds nothing macOS lacks: `os_sock_raw(family, type, protocol)` (`SOCK_NONBLOCK | SOCK_CLOEXEC`), `os_accept_raw(s)` (`accept4`), `OS_SEND_FLAGS` (`MSG_NOSIGNAL`), `OS_RECV_FLAGS` (`MSG_TRUNC`, for the real length), `os_udp_own_groups(s, family)` (`IP_MULTICAST_ALL` off).
5. **Shared `io/net.c` and `io/udp.c`** keep only what isn't the system's: resolving and formatting addresses, deadlines from timeouts, Lugha values and messages, the poll and the parked-socket list (closing still wakes the tasks parked on a socket with `Err("closed")` before `os_sock_close`).
6. **UDP options moved**: `lugha_rt_udp_group`, `lugha_rt_udp_option`, `lugha_rt_udp_multicast_if`, `lugha_rt_net_interfaces` and their helpers move unchanged to `os/posix/udp_options.c`.
7. **The boundary test** extended: Linux's list gains `accept4`, `SOCK_NONBLOCK`, `SOCK_CLOEXEC`, `MSG_NOSIGNAL`, `MSG_TRUNC`, `IP_MULTICAST_ALL` (outside `os/linux/`); and no shared part outside `os/` calls `socket(`, `accept(`, `connect(`, `bind(`, `listen(`, `recv(`, `send(`, `recvfrom(`, `sendto(`, `setsockopt(`, `getsockopt(`, `getsockname(` or `getpeername(`.
8. Docs: CHANGELOG (Changed), the spec's milestone 250 row, the Draft's step 3, TODO (step 3 done), and the step-4 notes below in TODO.

### Must NOT Do

- No change in behaviour, messages, timeouts or what a cancelled call does.
- No edit to `os/posix/process.c` or any other 🔒 file.
- No darwin or windows files; no io_uring (DEF-10).

## DESIGN

**Why the operation, not the descriptor.** Go draws its OS line in `internal/poll` (`FD.Read` is a read-then-wait on Unix and an overlapped read on Windows), mio first faked readiness on Windows and called it "abysmal", libuv made its whole API completion-shaped. Readiness stays POSIX's private detail inside `os/posix/sockets.c`; shared code only ever hears "done", "timed out", "cancelled" or "closed".

**Handles.** `OsSocket` is `int64_t`: a descriptor on POSIX; Windows' `SOCKET` fits. `std.net`'s values already carry an `i64`.

**Errors.** `OS_FAILED` with `*message` the system's words (`strerror` on POSIX), so shared code builds the same `Err("…")` as today.

**Noted for step 4 (macOS), in TODO:** macOS has no `MSG_TRUNC` length from `recvfrom`, so "datagram of N bytes is longer than max" needs `recvmsg`'s truncation flag and a spec decision about the number; `pipe2` in `os/posix/process.c` (🔒) needs a macOS form.

**Judgement calls (for review).** One PRP for TCP and UDP together (they share the waiting and the cancel rule); UDP options moved whole to `os/posix/` rather than split, since Windows' options differ enough to be written anew; `wait_fd` in `os/posix/` (only readiness waits use it); result codes as negative numbers beside counts, as the system calls do.

## TESTS

- [ ] `src/link/mod.rs`: the boundary test's new lists, failing first on `io/net.c` and `io/udp.c`.
- [ ] Unchanged behaviour: `--test net`, `udp`, `http`, `tls`, `tls_over_conn`, `tls_keys`, `scope_callbacks`, `programs`, `scheduler_stress`, `workers` (a cancelled socket call, closing a socket under a parked task, timeouts), `lockdown` (no socket without `Net`), `arm`, `musl`; `cargo test --lib`.
- [ ] `bench/run.py chanping wordfreq binarytrees`: no slower (none uses sockets; checks the joined runtime).

## ACCEPTANCE

- [ ] No socket call in a shared part; no Linux-only socket detail outside `os/linux/`; both enforced by the test.
- [ ] The tests above pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; every file ≤ 500 lines.
- [ ] CHANGELOG, the spec's row 250, the Draft's step 3, TODO updated.
