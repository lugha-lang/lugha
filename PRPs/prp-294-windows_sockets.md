## FEATURE: Sockets on Windows, through overlapped I/O on the completion port — milestone 262

## OBJECTIVE

`std.net` works on Windows as on Linux and macOS. That covers TCP listen, connect, accept, read and write, UDP and multicast, interface listing and name lookup, with deadlines, cancelled scopes and closing a socket under a waiting task all behaving the same way. Today every socket operation on Windows fails closed with `Err("sockets aren't supported on Windows yet")`.

```lugha
fun echo(server: Listener) {
    let c = server.accept().unwrap();
    c.write(c.read(100).unwrap()).unwrap();
}

fun main(console: Console, net: Net) {
    let server = net.listen("127.0.0.1:0").unwrap();
    scope s {
        s.spawn(echo(server));
        let c = net.connect(server.addr()).unwrap();
        c.write("hi".to_bytes()).unwrap();
        console.println(c.read(100).debug());          // Ok([104, 105])
    };
    match net.connect("127.0.0.1:1") {
        .Ok(_) => console.println("connected?"),
        .Err(e) => console.println(e),                 // Connection refused
    }
}
```

Windows waits on a socket by completion, not readiness. An operation is started, and the task parks until its completion arrives on the I/O completion port the runtime already has for helper jobs and wake-ups. This is the model the approved Draft text describes (9.2 rules 1–2), and the one Go and libuv use.

## CONTEXT

- **Read first:**
  - `runtime/os/layer.c`: the socket contract (`os_sock_*`, `OS_CANCELLED` only once the buffer is let go);
  - `runtime/os/posix/sockets.c` and `udp_options.c`, what's promised;
  - `runtime/io/net.c`, `udp.c` and `resolve.c`, the shared callers, the poll and `net_waits`;
  - `runtime/os/windows/os.c`, the port and `os_poll_wait`;
  - `runtime/os/windows/missing.c`, whose socket refusals are replaced.
- **Spec:**
  - the `Net` and `std.net` sections, which this PRP leaves unchanged;
  - §11 row 262.
- **Implements:** Draft 9.3 step 5's sockets.
- **Resolves PRT-13's first half:** overlapped I/O, chosen over the AFD poll on 2026-11-26. Precompiled runtimes stay deferred.
- **Follows:** PRP-293 (milestone 260). **Next:** processes and lockdown on Windows, both needing security review.

## REQUIREMENTS

### Must Do

**One operation at a time**
- Every `os_sock_*` in a new `runtime/os/windows/sockets.c`.
- A socket is made with `WSASocketW(…, WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT)` and tied to the port with a reserved completion key, `OS_TOKEN_IO`.
- Each operation runs as follows:
  - its `OVERLAPPED` sits in a small record on the task's own stack, which holds the waiting fiber and its wait generation;
  - the task starts the call (`WSARecv`, `WSASend`, `AcceptEx`, `ConnectEx`, `WSARecvFrom`, `WSASendTo`) and, unless it failed at once, parks;
  - `os_poll_wait` turns an `OS_TOKEN_IO` packet into a wake-up of that record's fiber, and stores the byte count and status in the record.
- Every operation completes through the port, including one that succeeds at once. This keeps a single path, and the record is never left to a packet that would come later.
- `WSAStartup` runs once at start.

**Cancelling (PRT-09)**
- A deadline, a cancelled scope or `close` from another task stops the wait.
- The task then calls `CancelIoEx` on its own operation and parks again, uncancellably, until that operation's packet arrives.
- Only then does it return `OS_TIMED_OUT`, `OS_CANCELLED` or `OS_CLOSED`. No buffer or stack record is ever written after the task leaves.
- If the operation finished before the cancel took hold, the bytes it moved still count:
  - a receive that completed gives its data, not a timeout;
  - a send that completed counts as sent.

**`close`**
- `lugha_rt_net_close` wakes the tasks parked on the socket, as now, then `closesocket`, which aborts their operations.
- Each of those tasks still waits for its own aborted completion, as above, before returning `Err("closed")`.
- `net_waits` keys widen from `int` to the 64-bit `OsSocket`, since a `SOCKET` is pointer-sized.

**Accept and connect**
- `AcceptEx` into a socket made beforehand, then `SO_UPDATE_ACCEPT_CONTEXT`.
- `ConnectEx` after binding to the family's any-address, then `SO_UPDATE_CONNECT_CONTEXT`.
- Both function pointers are fetched once per family through `WSAIoctl(SIO_GET_EXTENSION_FUNCTION_POINTER)`.
- Every address a name resolves to is tried in turn, as now.

**Options**
- `listen` sets `SO_EXCLUSIVEADDRUSE`, not `SO_REUSEADDR`. On Windows, `SO_REUSEADDR` lets another program bind the same port and take its connections. Windows already allows rebinding a port in `TIME_WAIT`, which is all Linux's `SO_REUSEADDR` is there for.
- UDP `shared` sets `SO_REUSEADDR`. Windows has no `SO_REUSEPORT`, and its `SO_REUSEADDR` means what `shared` asks for.
- A UDP socket sets `SIO_UDP_CONNRESET` off. Otherwise an ICMP "port unreachable" from an earlier send fails the next receive with `WSAECONNRESET`, which Linux never does.

**UDP**
- `recvfrom` receives into the caller's buffer plus the shared 64 KiB overflow behind it, as two `WSABUF`s. The real length of a longer datagram comes back, as on POSIX, so `udp_long` gives the same refusal.
- Multicast join and leave, TTL, loop, broadcast and the multicast interface go through Winsock's options of the same names, taking `DWORD` values.

**Interfaces and names**
- `net_interfaces` gives the same `name\tindex\tup\tmulticast\taddress` lines through `GetAdaptersAddresses`, using the adapter's friendly name, its `IfIndex`/`Ipv6IfIndex`, `IfOperStatus`, and whether it has `IP_ADAPTER_NO_MULTICAST`.
- `getaddrinfo` and `getnameinfo` come from `ws2_32`, still on a helper thread.

**Errors**
- Errors read as Linux's words:
  - `WSAECONNREFUSED` → "Connection refused";
  - `WSAECONNRESET` → "Connection reset by peer";
  - `WSAEADDRINUSE` → "Address already in use";
  - `WSAENETUNREACH`, `WSAEHOSTUNREACH`, `WSAEACCES`, `WSAEADDRNOTAVAIL`, `WSAEMSGSIZE` and the rest map the same way;
  - anything unknown → "Input/output error".
- Lookup failures read as glibc's `gai_strerror`, e.g. "Name or service not known". MinGW's `gai_strerror` gives Windows' localised text, so this is a fixed table.

**Build and tests**
- Link `-liphlpapi`.
- Remove the socket refusals from `missing.c`, `refusals.la` and the CLAUDE.md stack line. "Sockets and processes fail closed for now" becomes "processes fail closed for now".

### Must NOT Do

- **No AFD poll and no `\Device\Afd`:** they are undocumented (PRT-13).
- **No `FILE_SKIP_COMPLETION_PORT_ON_SUCCESS`:** one completion path. It can be added later as a measured optimisation.
- **No TLS on Windows yet.** A Windows TLS build still needs `LUGHA_OPENSSL_X86_64_PC_WINDOWS_GNU`. With `OS_TLS_TRUST 0` and no roots file, such a client trusts no server unless given `ca_pem`, which fails closed. Trusting Windows' own store (`CertGetCertificateChain`) is its own PRP.
- **No change to `std.net`'s API, the Linux or macOS runtimes, or `net.c`'s shape**, beyond the wider `net_waits` key.
- **No kernel lockdown of `Net` on Windows:** that comes with PRT-11's Windows answer, under security review. `LUGHA_LOCKDOWN=report` keeps saying it isn't applied.

## DESIGN

**Files**

| File | Change | Size |
| --- | --- | --- |
| `runtime/os/windows/sockets.c` | new: the operations, their record, cancel-then-wait | ≈350 lines |
| `runtime/os/windows/net_options.c` | new: UDP options, multicast, `net_interfaces`, the error and lookup message tables | ≈250 lines |
| `runtime/os/windows/os.c` | `os_poll_wait` hands `OS_TOKEN_IO` packets to their records; the unused `os_poll_arm` and `os_poll_disarm` stubs go | — |
| `runtime/io/net.c` | `NetWait.fd` becomes `OsSocket`; the `__attribute__((unused))` on `net_waits_add` and `net_waits_remove` goes | — |
| `runtime/io/resolve.c` | `gai_strerror` goes through a per-OS `os_lookup_message(code)` | — |
| `src/link/runtime_parts.rs`, `src/link/zig.rs` | the two new parts; `-liphlpapi` | — |

**The record and the wake-up.** The record is `{OVERLAPPED ov; uint64_t packed; DWORD bytes; DWORD status; volatile LONG done}`.

- `packed` is the same fiber-and-generation token `wait_fd` packs, so `poll_found` wakes the fiber with the same check against late events.
- `os_poll_wait` reads `bytes` and status from each `OVERLAPPED_ENTRY`, sets `done`, then reports the token.
- On the cancel path, the task stores a fresh generation into `packed` with an atomic exchange before `CancelIoEx`. It parks only if `done` is still 0, and `wake_gen`'s "one ahead" rule covers a packet that lands between that check and the park.

**Unsafe or security notes**
- No `unsafe` Rust.
- Sockets aren't on CLAUDE.md's security-review list, and `Net` is still enforced only by Lugha's capability types on Windows, which is unchanged.
- The cancel path is where a mistake would be memory-unsafe: the kernel writing into a stack that moved on. So it gets its own stress test.

**No new crates.**

## TESTS

- [ ] **`tests/windows/tcp.la`** (run under Wine by `tests/windows.rs`, and on Windows by CI):
  - echo across tasks;
  - `addr()` and `peer()` names;
  - a read timing out;
  - a scope deadline cancelling a reader;
  - `close` from another task waking a reader with `Err("closed")`;
  - connecting to a closed port, "Connection refused";
  - a listener's port being exclusive (a second `listen` on it, "Address already in use");
  - writing 1 MiB to a reader taking it in pieces;
  - `connect` to a name, `localhost`.
- [ ] **`tests/windows/udp.la`:**
  - send and receive with the sender's address;
  - the `udp_long` refusal with the real length;
  - a receive timing out;
  - `shared` binds twice to one port;
  - a send to a closed port, then a receive that times out, not "Connection reset";
  - multicast TTL, loop and broadcast set.
- [ ] **`tests/windows/interfaces.la`:** the loopback interface listed, up, with `127.0.0.1`.
- [ ] **`tests/windows/cancel_stress.la`:** 2,000 reads cancelled by short deadlines while data trickles in on other sockets. No crash, and every byte sent is received exactly once.
- [ ] **`tests/windows/refusals.la`:** the socket lines gone. Processes still refused.
- [ ] **Lookup errors:** `net.connect("no-such-host.invalid:80")` gives `Err("Name or service not known")`.
- [ ] **CI:** the Windows job runs the new cases. Multicast joins run only there, since Wine's multicast depends on the host.

## ACCEPTANCE

- [ ] The new cases pass under Wine (`cargo test --test windows`) and on CI's Windows runner.
- [ ] Nothing changes on Linux or macOS: `cargo test --lib`, `--test macos` and the `std.net` integration tests pass.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean, and every C file stays at or under 500 lines.
- [ ] Updated: the spec's §11 row 262, CHANGELOG, TODO, the ledger's PRT-13 (overlapped I/O chosen) and CLAUDE.md's stack line.

## DECIDED HERE (for review)

1. **One completion path, no skip-on-success.** It's simpler, and the cancel rule never meets an operation that finished without a packet.
2. **The record lives on the task's stack,** not the heap. The task can't leave until the packet arrives, so the stack outlives the operation, and nothing is allocated per call.
3. **`SO_EXCLUSIVEADDRUSE` for listeners.** Windows' `SO_REUSEADDR` would let another program take the port.
4. **`SIO_UDP_CONNRESET` off,** so UDP receives behave as on Linux.
5. **Linux's words for errors,** as Windows folders already do, so programs and tests see one set of messages.
6. **TLS on Windows is out of scope.** It fails closed (no roots) until its own PRP brings Windows' trust store.
7. **New files, not growth:** `sockets.c` and `net_options.c`, each under 500 lines.

## MADE WHILE BUILDING (for review)

8. **Multicast joins use the classic `IP_ADD_MEMBERSHIP` and `IPV6_ADD_MEMBERSHIP`,** the interface given by index (IPv4's as `0.0.0.n`), not `MCAST_JOIN_GROUP`. Every Windows takes the classic ones (Go uses them), while Wine refuses `MCAST_JOIN_GROUP`. So `multicast.la` runs under Wine too.
9. **Interface names come from Windows' own `if_nametoindex`/`ConvertInterfaceLuidToNameA`** (`loopback_0`, `ethernet_32768`), not the friendly names. They're what `join` and `set_multicast_interface` look up, as POSIX names are.
   - Three small layer functions make link-local addresses carry those names, as on Linux:
     - `os_lookup_message`;
     - `os_scope_name`, which turns `%12` into `%name`;
     - `os_scope_numeric`, which turns `%name` into `%12` for `getaddrinfo`.
   - On POSIX these are `gai_strerror` and two no-ops.
10. **Two existing bugs on Linux and macOS, found while writing the cases, are recorded in BUGS.md, not fixed here:**
    - a nested scope's deadline cancels its enclosing scope's tasks;
    - two tasks waiting on one socket at once leave all but the last unwoken.
    
    The cases are written around them: `main` accepts the stress connections, and the deadline-cancelled reads aren't inside a scope with live tasks.
