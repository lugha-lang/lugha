## FEATURE: UDP sockets, multicast and network interfaces in `std.net`; milestone 163 (v17).

**Status:** proposed 2026-11-15, from rapidkoi's request "What Lugha needs for Rapidkoi's sync" (item 4). Approved and built 2026-11-15. Sockets: the user reviews it before it merges (as `std.net` was, milestone 110).

## OBJECTIVE
```lugha
import std.net;

// Announce and listen on the local network, sharing the port with Avahi.
let u = net.udp_bind_shared("0.0.0.0:5353")?;
u.join("224.0.0.251", "")?;               // "" = the system's default interface
u.set_timeout(2000);
u.send_to(announcement, "224.0.0.251:5353")?;
let (bytes, from) = u.recv_from(65536)?;   // from: "192.168.1.20:5353"
u.close();

for i of net.interfaces()? {               // name, index, up, multicast, addresses
    println("{i.name} {i.addresses}");
}
```

## CONTEXT
- `std.net` (milestone 110, PRP-142; `resolve` milestone 121): `src/std/net.la` over `$os.net_*` (`os_signature` in `check/float_ops.rs`, codegen through `codegen/net.rs`), C in `runtime/net.c` (300 lines: `wait_fd`, `net_deadline`, `net_error`) and `runtime/resolve.c`, core fragments in `RUNTIME_PARTS` (`src/link/mod.rs`, `#include`s only in the first); tests in `tests/net.rs` (275 lines).
- Spec v17, `std.net` ("no UDP yet"), §11, "Changes since v16"; spec §7's blocking-calls paragraph (sockets don't block the thread).
- DECISIONS: none open. DECISION 141–152 (`std.net`): non-blocking sockets parked through `epoll`, `Result<…, string>` errors, explicit `close()`.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **A `Udp` struct** with `udp_bind(addr)` and `udp_bind_shared(addr)`. Like `Listener`/`Conn`: an `fd` and a timeout in `Ref`s, so copies share them; `close()` closes every copy; the collector never closes it.
2. **Address reuse is a separate function, not an option.** `udp_bind_shared` sets `SO_REUSEADDR` and `SO_REUSEPORT` before binding, which mDNS's port 5353 needs alongside Avahi. Plain `udp_bind` sets neither: on Linux a shared port lets another program of the same user receive the unicast datagrams meant for this one, so sharing should be a choice that is visible in the code.
3. **`send_to(bytes, addr)` takes IP literals only** (`"192.168.1.20:5353"`, `"[ff02::fb%eth0]:5353"`), never a host name: a lookup per datagram would block the thread. A name is `Err("address must be an IP address with a port: …")`; `net.resolve` turns a name into addresses first.
4. **`recv_from(max): Result<(u8[], string), string>`**: the bytes and the sender as `ip:port` (IPv6 in brackets, with `%interface` for a link-local scope). `max` from 1 to 65536. A datagram longer than `max` is `Err("datagram of N bytes is longer than max")` and is gone (the kernel drops the rest); never silently truncated.
5. **Waits park only their green thread**, through `wait_fd`, as TCP's do. `set_timeout(ms)` covers `recv_from` and `send_to` (`0` waits forever); a timeout is `Err("timed out")`.
6. **Multicast**: `join(group, interface)` and `leave(group, interface)`; `interface` is a name (`"eth0"`) or `""` for the system's choice. IPv4 groups need an IPv4 socket and IPv6 groups an IPv6 one (`Err("an IPv6 group needs a socket bound to an IPv6 address")`); a group that isn't multicast is `Err`. `set_multicast_ttl(n)` (1–255, the IPv6 hop limit too), `set_multicast_loop(on)`, `set_multicast_interface(name)` (which interface sends multicast).
7. **`interfaces(): Result<Interface[], string>`** from `getifaddrs`: `Interface { name, index, up, multicast, addresses }`, addresses as text (`"192.168.1.20"`, `"fe80::1%eth0"`), each interface once.
8. **Not**: broadcast (`SO_BROADCAST`), `connect`ed UDP sockets, source-specific multicast, an mDNS or DNS-SD implementation (rapidkoi's choice, and it sits on top of this), non-blocking name lookup.
9. **C** in a new core fragment, `runtime/udp.c`, after `net.c`; its system headers (`<net/if.h>`, `<ifaddrs.h>`) join the core's `#include`s. Every fd is non-blocking and close-on-exec, as TCP's are.
10. **Tests in a new file `tests/udp.rs`** (`tests/net.rs` is at 275 lines).

## REQUIREMENTS

### Must Do
- `runtime/udp.c`: `lugha_rt_udp_bind(addr, shared)`, `lugha_rt_udp_send(fd, bytes, addr, timeout)`, `lugha_rt_udp_recv(fd, max, timeout, out bytes, out sender)`, `lugha_rt_udp_group(fd, group, iface, join)`, `lugha_rt_udp_option(fd, which, value)` (TTL, loop), `lugha_rt_udp_multicast_if(fd, iface)`, `lugha_rt_net_interfaces()` (text, one line per address, parsed in Lugha as `resolve` does); `RUNTIME_SYMBOLS`.
- The `$os` built-ins in `os_signature` and codegen. `recv` gives two values: either a new out-slot shape in `codegen/net.rs`, or the sender joined to the bytes and split in Lugha. I'll take whichever keeps codegen smaller and say which in BUILT.
- `src/std/net.la`: `Udp`, `udp_bind`, `udp_bind_shared`, its methods, `Interface`, `interfaces`; `/** */` docs; the module doc updated (it says TCP).
- Spec (`std.net`, §11 row 163, "Changes since v16"), CHANGELOG, TODO (rapidkoi's sync line goes), the Draft, DECISIONS.

### Must NOT Do
- Decision 8's list; no `unsafe` Rust; no new crates.

## DESIGN
- Security: datagrams are external input: lengths bounded (`max` ≤ 65536), never truncated silently, the sender's address reported for the caller to check (UDP senders can be forged; the module doc says so). Addresses are parsed numerically only (`AI_NUMERICHOST`). Shared ports are opt-in (decision 2). No processes or files.
- No new diagnostics.

## TESTS (`tests/udp.rs`)
- [ ] Two sockets on `127.0.0.1:0`: send and receive both ways; `recv_from` names the sender's address; `addr()` names the port.
- [ ] IPv6 on `[::1]:0` (skipped when the machine has no IPv6 loopback).
- [ ] `recv_from` with nothing sent and a 200 ms timeout: `Err("timed out")`; another green thread runs meanwhile.
- [ ] A 100-byte datagram read with `max` 10: `Err("datagram of 100 bytes is longer than max")`.
- [ ] `send_to("localhost:9")` is `Err("address must be an IP address with a port: localhost:9")`; `max` 0 and 65537 refused; using a closed socket is `Err("closed")`.
- [ ] Two `udp_bind_shared` sockets on one port both bind; a plain `udp_bind` on a taken port fails.
- [ ] Multicast: both shared sockets join `239.255.0.1` with loop on; one packet sent to the group reaches both; after `leave`, the one that left hears nothing more (timeout). Skipped, with a message, when the machine has no multicast-capable interface (`join` fails with `No such device`).
- [ ] Joining an IPv6 group on an IPv4 socket and a non-multicast group are `Err`.
- [ ] `interfaces()` includes `lo` with `up`, index ≥ 1 and `127.0.0.1` among its addresses.

## BUILT (2026-11-15)
- As planned, plus one thing found by the multicast test: Linux's `IP_MULTICAST_ALL` (on by default) gives a socket bound to `0.0.0.0` every group any socket on the machine joined, so after `leave` it still heard the group. `udp_bind` turns it off (and `IPV6_MULTICAST_ALL` where the kernel has it), so membership is per socket.
- `recv_from`'s two values: the out-slot holds the `(u8[], string)` tuple and C fills in its two fields; `codegen/io.rs`'s `out_slot` now zero-fills any type (`const_zero`) instead of only pointers and integers. No other codegen change.
- `net_resolve` in `net.c` takes `getaddrinfo` flags instead of a `passive` flag, so UDP asks for `AI_NUMERICHOST`. The new core fragment is `runtime/udp.c`; `<net/if.h>` and `<ifaddrs.h>` joined the core's includes.
- `interface` is a keyword, so the methods' parameter is `iface`.

## ACCEPTANCE
- [ ] The tests pass; `cargo test --lib`, `--test udp --test net --test lazy_std --test lsp_docs --test comments`; fmt and clippy clean.
- [ ] Docs as listed; the user's review before merging; tag `m163`.
