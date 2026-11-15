## FEATURE: HPKE (RFC 9180) in `std.crypto`: seal bytes to an X25519 public key; milestone 162 (v17).

**Status:** proposed 2026-11-15, from rapidkoi's request "What Lugha needs for Rapidkoi's sync" (item 2's optional part), now possible with OpenSSL 3.2 as the floor (milestone 161). Approved and built 2026-11-15. Crypto: the user reviews it before it merges (security rule).

## OBJECTIVE
```lugha
import std.crypto;

// The new device shows its X25519 public key; the old one seals the master secret to it.
let (secret, public) = crypto.x25519_keypair()?;                 // on the new device
let info = "rapidkoi pairing v1".to_bytes();
let sealed = crypto.hpke_seal(public, info, master, [])?;        // on the old device
let master = crypto.hpke_open(secret, info, sealed, [])?;        // back on the new device
// Anything changed, or the wrong secret or info: Err(CryptoError.NotAuthentic)
```

## CONTEXT
- `std.crypto` (milestones 113, 120, 160): `src/std/crypto.la`, `runtime/crypto.c` (456 lines), `CRYPTO_SYMBOLS`, `tests/crypto_seal.rs` (229 lines). Milestone 160 gave `x25519_keypair` and the `NotAuthentic`/`BadKey` variants; milestone 161 made OpenSSL 3.2 the floor (DECISIONS 219, 220).
- OpenSSL 3.2's `OSSL_HPKE_*` (`openssl/hpke.h`): `OSSL_HPKE_CTX_new`, `OSSL_HPKE_encap`, `OSSL_HPKE_seal`, `OSSL_HPKE_decap`, `OSSL_HPKE_open`.
- Spec v17, `std.crypto` (the table, "Not included", which names HPKE today), §11, "Changes since v16".
- DECISIONS: none open.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **One fixed suite**: DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305 (RFC 9180's `0x0020, 0x0001, 0x0003`). It's the same three algorithms `std.crypto` already has, and keys are the 32-byte X25519 keys `x25519_keypair` makes. No suite choice: one less thing to get wrong, and both sides are Lugha.
2. **Base mode only** (`mode_base`). The sender is anonymous; whoever needs to know who sent it signs the plaintext or the sealed bytes with Ed25519, or uses an authenticated channel (rapidkoi pairs over one). No PSK or auth modes: each adds keys to manage for no use we have.
3. **Single-shot**: one message per seal. Each `hpke_seal` makes a fresh ephemeral key, so there's no nonce for the caller to manage. No context API for several messages under one encapsulation (a caller wanting that seals a key once and uses `chacha20poly1305_seal`).
4. **One blob**: `hpke_seal(public, info, plaintext, aad): Result<u8[], CryptoError>` returns the 32-byte encapsulated key followed by the ciphertext and its 16-byte tag. `hpke_open(secret, info, sealed, aad)` takes it back. A blob can't be split wrongly; the layout is documented for anyone who needs RFC 9180's separate `enc` and `ct`.
5. **Errors**: key lengths 32 (`BadLength("public key" / "secret key", 32, n)`), `sealed` at least 48 (`BadLength("sealed bytes (at least)", 48, n)`), plaintext and aad within the 1 GiB cap; `info` at most 1024 bytes, OpenSSL's `OSSL_HPKE_MAX_INFOLEN` (`BadLength("info (at most)", 1024, n)`). A tag that doesn't match, a wrong secret or info: `NotAuthentic`. A low-order recipient key (`hpke_seal`) or encapsulated key (`hpke_open`): `BadKey("the peer's public key has low order")`, as `x25519_shared`.
6. **C side** in `runtime/crypto.c` if it stays at or under 500 lines; otherwise in a new fragment `runtime/crypto_hpke.c` appended after `crypto.c` (as `tls_keys.c` is after `tls.c`). Every `OSSL_HPKE_CTX` and `EVP_PKEY` freed on every path; plaintext only copied out once `OSSL_HPKE_open` succeeds, from a buffer wiped on every path.
7. **Tests** in `tests/crypto_seal.rs` (it has room): RFC 9180's A.2.1 test vector (this suite, base mode) opened byte for byte from its `skRm`, `enc`, `ct`, `info` and `aad`. Sealing can't be checked against the vector, because it takes a random ephemeral key and OpenSSL's way to fix it (`OSSL_HPKE_CTX_set1_ikme`) is for testing and stays out of `std`. Seal then open round trips, and tampering tests, cover it.
8. **Not**: other suites, PSK and auth modes, the multi-message context API, exporting secrets (`export`), keys other than raw 32-byte X25519.

## REQUIREMENTS

### Must Do
- C: `lugha_rt_crypto_hpke_seal(public, info, plaintext, aad, out, error)`, `lugha_rt_crypto_hpke_open(secret, info, sealed, aad, out, error)`, reporting `"not authentic"` and `"low order"` as milestone 160's functions do; `CRYPTO_SYMBOLS`; `#include <openssl/hpke.h>`.
- `$os.crypto_hpke_seal`, `$os.crypto_hpke_open` in `os_signature` (four `u8[]` in, `Result<u8[], string>` out).
- `src/std/crypto.la`: `hpke_seal`, `hpke_open` with `/** */` docs; the module doc names HPKE.
- Spec (`std.crypto` paragraph, table, "Not included"), §11 row 162, "Changes since v16"; CHANGELOG, TODO, the Draft, DECISIONS.

### Must NOT Do
- Decision 8's list; no way to fix the ephemeral key from Lugha; no `unsafe` Rust; no new crates.

## DESIGN
- Security: all inputs are the caller's bytes, lengths checked in Lugha and again in C. No files, processes or network. The ephemeral secret lives only inside OpenSSL's context, cleared when it's freed. The opened plaintext lives in collected memory, which Lugha can't wipe (as for every secret `std.crypto` returns).
- No new diagnostics; no new `CryptoError` variants.

## TESTS (`tests/crypto_seal.rs`)
- [ ] RFC 9180 A.2.1, sequence 0: `hpke_open(skRm, info, enc ‖ ct, aad)` gives the vector's plaintext.
- [ ] A round trip with a fresh `x25519_keypair`, with and without `info` and `aad`, and an empty plaintext (48 bytes sealed).
- [ ] Two seals of the same plaintext differ (fresh ephemeral key each time).
- [ ] `NotAuthentic` for one flipped byte in `enc`, the ciphertext, the tag, `info` or `aad`, and for another key pair's secret.
- [ ] `BadKey` for an all-zero recipient public key and an all-zero `enc`.
- [ ] `BadLength` for a 31-byte public key, a 33-byte secret, 47 sealed bytes and 1025 bytes of `info`, with their messages.

## BUILT (2026-11-15)
- One change from the plan: OpenSSL's `OSSL_HPKE_seal` refuses an empty plaintext ("passed invalid argument"; checked in a C prototype), though RFC 9180 allows one. `hpke_seal` refuses it first with `BadLength("plaintext (at least)", 1, 0)`, and `hpke_open` needs at least 49 sealed bytes, not 48. The round-trip test seals one byte instead.
- `runtime/crypto.c` was at 456 lines, so HPKE is its own fragment, `runtime/crypto_hpke.c`, and `CRYPTO_SOURCE` is the two joined; `crypto.c` gained `#include <openssl/hpke.h>`.
- Low-order keys are found before OpenSSL is asked: an X25519 derivation with a fixed scalar against the key comes out zero (or is refused) exactly for low-order points, whatever the scalar.
- RFC 9180's A.2.1 values were read from the RFC's text (rfc-editor.org); the vector opens byte for byte.

## ACCEPTANCE
- [ ] The tests pass; `cargo test --lib`, `--test crypto_seal --test crypto --test lazy_std --test lsp_docs --test comments`; fmt and clippy clean.
- [ ] Docs as listed; the user's security review before merging; tag `m162`.
