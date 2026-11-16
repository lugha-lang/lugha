## FEATURE: Capabilities, fourth step: `Net`, taken by `main` and tests, with `Conn`, `Listener` and `Udp` as prelude interfaces; `std.net` becomes the prelude's, and `std.tls` and `std.http` take a `Net`; milestone 183, in v18's Phase 2.

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
fun main(console: Console, net: Net): i32 {
    let server = net.listen("127.0.0.1:0").unwrap();
    spawn serve_one(server);
    let c = net.connect(server.addr()).unwrap();
    let w = c.write("ping".to_bytes());
    console.println(string_from_utf8(c.read(16).unwrap()).unwrap());    // pong
    c.close();
    console.println(net.resolve("127.0.0.1").debug());                   // Ok(["127.0.0.1"])
    0
}

fun serve_one(l: Listener) {
    let c = l.accept().unwrap();
    let got = c.read(16).unwrap();
    let w = c.write("pong".to_bytes());
    c.close();
    l.close();
}
```
`import std.net;` is gone; `http.get(net, url)`, `http.serve(net, addr, handler)` and `tls.connect(net, addr)` take the capability. A fake `Net` is a type implementing `Net`, its connections types implementing `Conn`.

## CONTEXT
- Draft v18 §2.1 [CAP-01, CAP-02, CAP-07, CAP-13, CAP-17]: connecting, listening and name lookup need `Net`; capabilities are interfaces with fakes; higher-level libraries (`std.http`, `std.tls`) stay imports taking capabilities as parameters.
- Decided with the user: TLS will run over any `Conn` (OpenSSL over a byte stream), so a fake `Net` covers TLS too. That rewrite changes the C crypto code and is the next PRP, with the user's security review. Here `std.tls` takes a `Net` in its signatures but still opens its own sockets.
- Follows PRP-212 to PRP-214 (capabilities as prelude interfaces with real implementations, `main`'s parameters by type, default methods keeping fakes small).
- Measured: `std.net` imported by 6 files (`listen`, `connect`, `udp_bind`, `resolve`, …), `std.tls` by 6, `std.http` by 2; about 120 calls.
- Code: `src/std/net.la` (removed), `src/std/tls.la`, `src/std/http.la`, `src/std/http_wire.la`, new `src/prelude/net.la` (interfaces) and `src/prelude/net_real.la` (the real ones), `src/check/entry.rs`, `src/check/impls.rs`.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The interfaces**, each method behaving as the `std.net` function or method it replaces:
   ```lugha
   interface Net {
       fun connect_timeout(self, addr: string, ms: i64): Result<Conn, string>;   // 0 waits forever
       fun listen(self, addr: string): Result<Listener, string>;
       fun resolve(self, name: string): Result<string[], string>;
       fun udp_bind(self, addr: string): Result<Udp, string>;
       fun udp_bind_shared(self, addr: string): Result<Udp, string>;
       fun interfaces(self): Result<NetInterface[], string>;
       fun connect(self, addr: string): Result<Conn, string> = self.connect_timeout(addr, 0);
   }
   interface Listener { accept, addr, set_timeout, close }
   interface Conn { read, write, set_timeout, peer, addr, close }
   interface Udp { send_to, recv_from, join, leave, set_multicast_ttl, set_multicast_loop, set_multicast_interface, set_timeout, addr, close }
   ```
   `net.Interface` becomes the prelude struct `NetInterface` (a bare `Interface` would be too general a prelude name). Sendable, like the other capabilities, so a listener can go to a spawned task.
2. **`std.net` is removed**: everything in it is a method now.
3. **`std.http`** takes `net: Net` first in every function that connects or listens (`serve`, `serve_with`, `serve_tls`, `serve_tls_with`, `get`, `post`, `post_json`, `request`, `request_with`, `request_tls`, `request_tls_with`); its name lookups go through `net.resolve`. Building requests and responses (`text`, `json`, `bytes`, `limits`) needs none.
4. **`std.tls`** takes `net: Net` first in `connect`, `connect_with`, `connect_keys`, `listen` and `listen_keys`. Until the next PRP its C code still opens the sockets, so a fake `Net` doesn't yet stop TLS; the spec says so.
5. **A `Conn` and a `tls.Conn` stay different types** for now; the next PRP makes `tls` give a `Conn`.
6. **Timing inside `std.http`** (its deadlines) keeps the runtime's monotonic clock: scheduling time, which needs no capability.
7. **The real implementations** go to `src/prelude/net_real.la`, so no prelude file passes 500 lines.
8. **Migration** by hand and script in this repository, as before.

## REQUIREMENTS

### Must Do
- Prelude: `Net`, `Listener`, `Conn`, `Udp`, `NetInterface`, the real ones over the existing `$os` built-ins; `$net()` for the entry.
- Checker: `Net` an entry capability; `Listener`, `Conn`, `Udp` nominal and sendable like `File`.
- std: remove `std.net`; `std.http` and `std.tls` take a `Net`.
- Spec (`std.net` section onto `Net`; `std.http`/`std.tls` signatures; capabilities paragraph; §11 row 183; Changes; Breaking), CHANGELOG, error pages using `std.net`.

### Must NOT Do
- TLS over a `Conn`, or any change to the C TLS or socket code (`runtime/`): the next PRP, with the user's security review.
- The test runner's fake `Net`; narrowing (`net.only`, `connect_only`, `listen_only`).

## TESTS
- [ ] `tests/programs/capabilities/net/`: the objective; a fake `Net` whose `Conn` answers from memory, used by a helper that only knows `Net`; E0304 for a repeated `Net`.
- [ ] Migrated: the `std.net`, `std.tls` and `std.http` programs and Rust tests (`tests/net*.rs`, `tests/tls*.rs`, `tests/http*.rs`).
- [ ] Unit (`check/entry.rs`): `main` takes a `Net`.
- [ ] Full `cargo test`.

## ACCEPTANCE
- [ ] The objective prints `pong` and `Ok(["127.0.0.1"])` and exits 0.
- [ ] Outside the prelude, nothing opens a socket or looks up a name without a `Net`, apart from `std.tls`'s own sockets (item 4) until the next PRP.
- [ ] `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated where they change
