## FEATURE: TLS over any `Conn`: OpenSSL runs on memory buffers and Lugha carries the bytes over a `Conn` from a `Net`, so a fake `Net` covers TLS and `https://` too; certificates, keys and CA lists are passed as PEM text, not file paths; milestone 184, in v18's Phase 2. **Changes the C crypto code: needs the user's security review before any code.**

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
import std.tls;

fun main(console: Console, net: Net, fs: Fs): i32 {
    let dir = fs.cwd();
    let cert = dir.read_file("cert.pem").unwrap();            // PEM text, read through a Dir
    let key = dir.read_file("key.pem").unwrap();
    let server = tls.listen(net, "127.0.0.1:8443", cert, key).unwrap();
    spawn answer(server);
    let mut opts = tls.options();
    opts.ca_pem = cert;                                        // trust the self-signed server
    let c = tls.connect_with(net, "localhost:8443", opts).unwrap();
    let w = c.write("ping".to_bytes());
    console.println(string_from_utf8(c.read(16).unwrap()).unwrap());   // pong
    c.close();
    // Any Conn will do: TLS over a connection the program made itself.
    let raw = net.connect("127.0.0.1:8443").unwrap();
    let secure = tls.client(raw, "localhost", opts).unwrap();
    let v = secure.write("ping".to_bytes());
    console.println(string_from_utf8(secure.read(16).unwrap()).unwrap()); // pong
    secure.close();
    0
}

fun answer(l: Listener) {
    for i in 0..2 {
        let c = l.accept().unwrap();
        let got = c.read(16).unwrap();
        let w = c.write("pong".to_bytes());
        c.close();
    }
}
```
And in a test, `tls.connect(FakeNet {}, "example.com:443")` asks the fake for its connection: nothing leaves the machine.

## CONTEXT
- Decided with the user (PRP-215 discovery): TLS runs over any `Conn` ("option 3"), so `Net` stays TCP, UDP and lookup, and a fake `Net` covers TLS.
- Draft v18 §2.1 [CAP-01, CAP-07, CAP-13]: no ambient authority; fakes for tests.
- Today (`runtime/tls.c`, `runtime/tls_keys.c`): each TLS connection owns a socket (`SSL_set_fd`), connected or accepted by `net.c`; `tls_drive` loops on `SSL_connect`/`SSL_accept`/`SSL_read`/`SSL_write`, parking the green thread with `wait_fd` on `WANT_READ`/`WANT_WRITE`; `SIGPIPE` is blocked around each call; certificates, keys and CA files are read by OpenSSL from paths (`SSL_CTX_use_certificate_chain_file`, `SSL_CTX_use_PrivateKey_file`, `SSL_CTX_load_verify_locations`).
- Code: `runtime/tls.c`, `runtime/tls_keys.c`, `src/link/runtime.rs` (the TLS symbol list), `src/check/float_ops.rs` and `src/codegen/net.rs` (the `$os.tls_*` built-ins), `src/std/tls.la`, `src/std/http.la`, `src/std/http_wire.la`, `tests/tls*.rs`, `tests/http.rs`.

## DESIGN — for the security review

### The C side: OpenSSL on memory BIOs, no sockets
Each TLS connection is an `SSL` with two memory BIOs (`BIO_s_mem`): `in` holds bytes that arrived from the peer, `out` holds bytes OpenSSL wants sent. `SSL_set_bio(ssl, in, out)`; `BIO_set_mem_eof_return(in, -1)` so an empty `in` means "want more", not end of stream. The C code never touches a socket, never waits and never blocks a signal: `wait_fd`, `SSL_set_fd`, `tls_quiet`/`tls_loud` (SIGPIPE) go.

New built-ins (all `malloc`ed state outside the collector, as today; a handle is an opaque `i64`):

| Built-in | Does |
| --- | --- |
| `tls_client_context(ca_pem)` | a client `SSL_CTX`: TLS 1.2 minimum, `SSL_VERIFY_PEER`, `SSL_OP_IGNORE_UNEXPECTED_EOF`, the system's roots (`SSL_CTX_set_default_verify_paths`) plus each certificate in `ca_pem` (read with `PEM_read_bio_X509` from a `BIO_new_mem_buf`, added with `X509_STORE_add_cert`) |
| `tls_server_context(cert_pem, key_pem)` | a server `SSL_CTX`: TLS 1.2 minimum; the chain from `cert_pem` (`PEM_read_bio_X509` for the leaf, `SSL_CTX_add1_chain_cert` for the rest), the key from `key_pem` (`PEM_read_bio_PrivateKey`, no passphrase callback), `SSL_CTX_check_private_key` |
| `tls_key_context(server, secret)` | unchanged from today (Ed25519 raw public keys, TLS 1.3) |
| `tls_new(ctx, server, host, accept)` | an `SSL` on a context with its two BIOs; for a client, the host checks of today (`SSL_set_tlsext_host_name` + `SSL_set1_host`, or `X509_VERIFY_PARAM_set1_ip_asc` for an IP); for by-key, today's `tls_expect` with the `accept` list |
| `tls_step(h, op, data, max)` | one call of `SSL_connect`/`SSL_accept` (op 0), `SSL_read` up to `max` (1), `SSL_write` of `data` (2) or `SSL_shutdown` (3): gives `Done(bytes or count)`, `WantInput` (`SSL_ERROR_WANT_READ`) or `Err(reason)` with today's messages (`certificate verify failed: …`, `peer key not accepted`, OpenSSL's reason). `SSL_ERROR_WANT_WRITE` can't happen on a memory BIO |
| `tls_outgoing(h)` | everything in `out`, drained (`BIO_read` into a collected `u8[]`) |
| `tls_incoming(h, bytes)` | appends `bytes` to `in` (`BIO_write`) |
| `tls_incoming_end(h)` | marks `in` as ended (`BIO_set_mem_eof_return(in, 0)`): the peer closed |
| `tls_peer_key(h)` | unchanged |
| `tls_free(h)`, `tls_free_context(c)` | `SSL_free` (which frees both BIOs), `SSL_CTX_free` |

Removed: `tls_connect`, `tls_listen`, `tls_accept`, `tls_read`, `tls_write`, `tls_fd`, `tls_close`, `tls_close_listener`, `tls_listener_fd`, `tls_connect_keys`, `tls_listen_keys` (`set_accept` stays, on a context holder).

**Bounds:** `tls_step` writes at most 16384 bytes of plaintext per call (one record), so `out` holds about one record plus the handshake; `tls_incoming` takes what one `Conn.read` gave (at most 16777216, as today's read limit); `tls_outgoing` is drained after every step, so `out` never grows past one step's output. `max` for a read is checked as today (1 to 16777216).

**Ownership:** the C side copies `incoming` bytes into OpenSSL's BIO and copies `outgoing` bytes out into a new collected array (`lugha_rt_alloc`); no Lugha array is kept by C (spec §7–8). PEM text is copied into a temporary BIO and the BIO freed before returning; OpenSSL keeps its own key objects, cleared when the context is freed (as today). A private key passed as PEM text lives in collected memory, which Lugha can't wipe: the same as `Keys.secret` today, documented.

**Use after free:** Lugha holds each handle in a `Ref<i64>`, set to `-1` on close, and every method checks it first (as `tls.Conn` does today), so C never sees a freed handle.

### The Lugha side: the pump
`std.tls` drives each operation: call `tls_step`; send whatever `tls_outgoing` gives with the underlying `conn.write`; on `WantInput`, `conn.read(65536)` and `tls_incoming` it (or `tls_incoming_end` on `[]`), and step again. Timeouts: an operation's deadline is the `timeout_ms` given (connect and handshake) or the connection's `set_timeout`, measured with the runtime's monotonic clock (scheduling time); before each read or write the remaining time goes to the underlying `conn.set_timeout`, and none left is `Err("timed out")`. Waiting is the underlying `Conn`'s: with a real one only the green thread waits, as today.

### The API (`std.tls`)
- New: `client(conn: Conn, host: string, opts: Options): Result<Conn, string>` and `client_keys(conn: Conn, keys: Keys, timeout_ms: i64)`: TLS over a connection the caller made; `server(conn: Conn, cert_pem, key_pem)` for one accepted connection.
- Kept, over the `Net`: `connect(net, addr)`, `connect_with(net, addr, opts)`, `connect_keys(net, addr, keys, timeout_ms)` (`net.connect_timeout`, then `client`); `listen(net, addr, cert_pem, key_pem)`, `listen_keys(net, addr, keys)` (`net.listen`, and `accept` handshakes each connection).
- `tls.Conn` and `tls.Listener` implement the prelude's `Conn` and `Listener` (`tls.Conn` keeps `peer_key()`, `tls.Listener` keeps `set_accept`); `peer()`/`addr()` are the underlying connection's.
- **Breaking:** certificates, keys and trusted CAs are PEM text, not paths: `listen(net, addr, cert_pem, key_pem)`, `Options.ca_pem` (was `ca_file`), `http.serve_tls(net, addr, cert_pem, key_pem, handler)`, `http.request_tls`'s `opts.ca_pem`. Reading them is the caller's, through a `Dir`, so `std.tls` reads no file. The system's roots stay OpenSSL's own reading of `/etc/ssl` (the machine's trust store, not program data).
- `std.http`'s `Stream` enum goes: both kinds are a `Conn`.

### For the security review, in one list
1. Memory BIOs instead of sockets; no `wait_fd`, no `SIGPIPE` handling in TLS any more.
2. PEM parsing from memory (`PEM_read_bio_X509`, `PEM_read_bio_PrivateKey`, chain via `SSL_CTX_add1_chain_cert`, CAs via `X509_STORE_add_cert`) replacing OpenSSL's file loaders; no passphrase prompts.
3. Verification unchanged: the same verify mode, host and IP checks, minimum versions and by-key DANE checks.
4. The bounds and ownership above; handles checked in Lugha before each call.
5. `src/link/runtime.rs`'s TLS symbol list replaced.
6. Private keys as PEM text in collected memory (not wiped), as `Keys.secret` already is.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **PEM text, not paths** (above): otherwise `std.tls` would read files without an `Fs`.
2. **Memory BIOs, not a BIO pair or a custom BIO method:** the simplest OpenSSL pattern for driving TLS from outside, with no callbacks into Lugha.
3. **16384-byte writes per step** to keep `out` small.
4. **`server(conn, cert_pem, key_pem)`** builds a context per call; `listen` builds one context and reuses it for every accepted connection, as today.

## REQUIREMENTS

### Must Do
- C: the built-ins above in `runtime/tls.c` and `runtime/tls_keys.c` (each ≤ 500 lines); the old ones removed; `src/link/runtime.rs`, `src/check/float_ops.rs`, `src/codegen/net.rs` updated.
- std: `std.tls` pumping over a `Conn`, the new functions, PEM text; `std.http` over `Conn` only; `serve_tls`/`request_tls` with PEM text.
- Tests, spec (`std.tls`, `std.http`, the capabilities paragraph's note that TLS escapes a fake goes), CHANGELOG.

### Must NOT Do
- Change what is verified, the versions spoken or the by-key protocol.
- Any other C file; the test runner's fakes; narrowing.

## TESTS
- [ ] `tests/tls.rs`, `tests/tls_keys.rs`, `tests/http.rs`: every existing case passes, migrated to PEM text (certificate failures, host and IP checks, timeouts, by-key accept and refuse, `set_accept`, large transfers both ways, a peer closing early).
- [ ] New: TLS over a fake `Net` whose `Conn` is an in-memory pipe to a TLS server in the same program (a real handshake, no socket); `tls.connect(FakeNet {}, …)` refused by the fake reaches no network; `client` over a `Conn` the program made.
- [ ] `src/link/runtime.rs`'s own tests (the TLS part defines its symbols).
- [ ] Full `cargo test`.

## ACCEPTANCE
- [ ] The objective prints `pong` twice and exits 0.
- [ ] No C TLS code opens, reads or waits on a socket or a file.
- [ ] A test given a fake `Net` can't reach the network through `std.tls` or `std.http`.
- [ ] `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated where they change
