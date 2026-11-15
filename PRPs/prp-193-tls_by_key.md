## FEATURE: Mutual TLS by Ed25519 key in `std.tls` (raw public keys, RFC 7250), with OpenSSL 3.2 as the floor; milestone 161 (v17).

**Status:** proposed 2026-11-15, from rapidkoi's request "What Lugha needs for Rapidkoi's sync" (item 3). The user chose OpenSSL 3.2 as the floor on 2026-11-15. Approved and built 2026-11-15. TLS: the user reviews it before it merges (security rule).

## OBJECTIVE
```lugha
import std.tls;

// Every device: its Ed25519 seed (from std.crypto) and the keys on the signed device list.
let me = tls.Keys { secret: seed, accept: [laptop_key, phone_key] };

// Listening:
let listener = tls.listen_keys("0.0.0.0:7400", me)?;
let conn = listener.accept()?;            // the handshake refused any other key
println(conn.peer_key() == phone_key);    // true: the 32-byte Ed25519 public key
listener.set_accept([laptop_key, phone_key, tablet_key]);   // a device was paired

// Connecting:
let conn = tls.connect_keys("192.168.1.20:7400", me, 5000)?;   // timeout in ms, 0 waits forever
```

## CONTEXT
- `std.tls` (milestone 112, PRP-144; `address` milestone 121, PRP-153): `src/std/tls.la` over `$os.tls_*` built-ins (`os_signature` in `check/float_ops.rs`, codegen through `codegen/net.rs`), C in `runtime/tls.c` (321 lines), `TLS_SYMBOLS` in `src/link/runtime.rs`, tests in `tests/tls.rs` (372 lines).
- Spec v17, `std.tls`: "Certificates are always verified … and nothing turns that off … No client certificates, pinning or session resumption." This PRP changes that sentence: by-key connections verify something different and stricter (an exact key out of a list), and certificate connections stay as they are.
- OpenSSL 3.2's raw public keys: `SSL_CTX_set1_client_cert_type` / `_server_cert_type` (`TLSEXT_cert_type_rpk`), `SSL_add_expected_rpk`, `SSL_get0_peer_rpk`, the key loaded with `SSL_CTX_use_PrivateKey` from `EVP_PKEY_new_raw_private_key(EVP_PKEY_ED25519, …)`.
- E0332 (`docs/errors/E0332.md`), `src/link/mod.rs` (`missing_openssl`).
- DECISIONS: none open. The user's answer on the floor becomes a DECISIONS entry with this PRP.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **OpenSSL 3.2 or newer for every program that loads `std.tls`, `std.http` or `std.crypto`** (the user's call). `runtime/tls.c` and `runtime/crypto.c` start with `#if OPENSSL_VERSION_NUMBER < 0x30200000L` / `#error "Lugha needs OpenSSL 3.2 or newer"`. The driver recognises that message and reports E0332, whose text and page then cover "missing or older than 3.2" (same code: in both cases the machine's OpenSSL can't be used, and the fix is the same).
2. **Raw public keys, not self-signed certificates.** Nothing to generate or parse, and the key is the identity, which is what rapidkoi checks.
3. **Names**: `Keys { secret, accept }`, `listen_keys(addr, keys)`, `connect_keys(addr, keys, timeout_ms)`, `Conn.peer_key()`, `Listener.set_accept(keys)`. `Keys` has no `timeout_ms` field: connect takes it as a parameter, and a listener uses `set_timeout`, as today.
4. **Both sides prove a key.** The server asks for the client's key and refuses a client that sends none or one not in `accept` (`SSL_VERIFY_PEER | SSL_VERIFY_FAIL_IF_NO_PEER_CERT`). The client refuses a server key not in its `accept`. The check happens in the handshake through `SSL_add_expected_rpk`, before any application data. If that call turns out not to refuse in the handshake on 3.2, the fallback is a verify callback comparing `SSL_get0_peer_rpk` with the list. Either way, a refused peer never gets a `Conn`.
5. **TLS 1.3 only** for by-key connections. TLS 1.3 encrypts the client's key on the wire, so a device's identity isn't visible to others on the Wi-Fi; TLS 1.2 sends it in clear. Certificate connections keep 1.2 as their minimum.
6. **No host name**: `addr` is only where to connect. No SNI, no name check.
7. **TLS 1.3's order of events** (documented): the client finishes its handshake before the server has checked the client's key. A client the server refuses gets its `Conn` and then an error (or the end of the stream) on its first read. The server never hands a refused client to `accept`, and the client has always checked the server's key before `connect_keys` returns.
8. **Keys from memory only**: `secret` is the 32-byte Ed25519 seed `std.crypto` makes; each `accept` entry a 32-byte public key. Wrong lengths are `Err("secret key must be 32 bytes")` / `Err("accepted key 2 must be 32 bytes")` (counting from 0), checked in Lugha first. The C side wipes nothing it doesn't copy: OpenSSL holds the key object and clears it when the context is freed. An empty `accept` is allowed and refuses everyone.
9. **`set_accept`** replaces a by-key listener's list for connections accepted after it, so pairing a device doesn't mean listening again. On a certificate listener it is `Err("not a by-key listener")`; once closed, `Err("closed")`. The list lives in the C listener (`EVP_PKEY` array, freed on close and on replace).
10. **`peer_key()`** gives `[]` on a certificate connection and once closed.
11. **`accept()` on a by-key listener** is the same method as today; the listener knows its kind.
12. **Not**: `std.http` over by-key TLS, mixing certificates and keys on one listener, session resumption, keys from files, one-sided by-key (server only), a callback deciding which keys to accept.
13. **Tests in a new file `tests/tls_keys.rs`**: `tests/tls.rs` is at 372 lines.

## REQUIREMENTS

### Must Do
- `runtime/tls.c`: the version check; `lugha_rt_tls_connect_keys`, `lugha_rt_tls_listen_keys`, `lugha_rt_tls_set_accept`, `lugha_rt_tls_peer_key`; `TlsListener` gains the accepted keys and a by-key flag; `accept` adds them to each new `SSL`. `runtime/crypto.c`: the version check. If `tls.c` passes 500 lines, the by-key part moves to its own optional fragment `runtime/tls_keys.c` (appended with `tls.c`).
- `$os.tls_connect_keys`, `$os.tls_listen_keys`, `$os.tls_set_accept`, `$os.tls_peer_key` in `os_signature` and codegen; `TLS_SYMBOLS`.
- `src/std/tls.la`: `Keys`, `listen_keys`, `connect_keys`, `Conn.peer_key`, `Listener.set_accept`, with `/** */` docs; the module doc comment updated.
- E0332: the driver's check and message, `docs/errors/E0332.md`.
- Spec (`std.tls`, E0332's row, OpenSSL 3.2 wherever "OpenSSL 3" is named as a requirement, §11 row 161, "Changes since v16"), CLAUDE.md's stack line on OpenSSL, CHANGELOG (Breaking: OpenSSL 3.2), TODO, the Draft, DECISIONS.

### Must NOT Do
- Decision 12's list. No way to turn verification off; no `unsafe` Rust; no new crates.

## DESIGN
- Security: the peer's key is external input checked by OpenSSL against an exact list; our side compares nothing by hand. Keys cross as bytes. No files, no processes. The one new outward behaviour is listening for TLS connections that need no CA.
- No new diagnostic code (E0332 widens).

## TESTS (`tests/tls_keys.rs`, loopback, in one Lugha program per case with a green thread on each side)
- [ ] Two accepted keys both connect; `peer_key()` on each side gives the other's public key.
- [ ] A third key: the server's `accept` fails and the client's first read fails or ends; no data reaches the server.
- [ ] A client whose `accept` doesn't hold the server's key: `connect_keys` fails.
- [ ] A certificate client (`tls.connect_with` with the server's address) is refused by a by-key listener.
- [ ] `set_accept` adding a key lets it in next time; removing one keeps it out.
- [ ] Wrong lengths for `secret` and an `accept` entry; `set_accept` on a certificate listener; `peer_key()` on a certificate connection is `[]`.
- [ ] E0332's message for an old OpenSSL: a unit test in `src/link/` on the stderr matcher (no old OpenSSL needed).

## BUILT (2026-11-15)
- As planned. A C prototype first showed `SSL_add_expected_rpk` fails unless DANE is on: the context gets `SSL_CTX_dane_enable`, each connection `SSL_dane_enable(ssl, NULL)`, then one `SSL_add_expected_rpk` per key. With that, a wrong or missing client key is refused in the server's handshake (alert 42 or 116 at the client's first read), a wrong server key fails `SSL_connect`, and an empty list refuses everyone.
- The by-key code is its own fragment, `runtime/tls_keys.c` (199 lines), appended after `tls.c` (351): `TLS_SOURCE` is the two joined. `TlsConn` and `TlsListener` gained a `by_key` flag (and the listener its keys), so a failed check on a by-key connection reads `peer key not accepted` instead of DANE's wording.
- `set_accept` takes effect for handshakes started after it; one in progress keeps the keys it began with.

## ACCEPTANCE
- [ ] The tests pass; `cargo test --lib`, `--test tls_keys --test tls --test crypto --test crypto_seal --test explain --test lazy_std --test lsp_docs --test comments`; fmt and clippy clean.
- [ ] Docs as listed; the user's security review before merging; tag `m161`.
