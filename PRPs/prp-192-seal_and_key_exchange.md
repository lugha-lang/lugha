## FEATURE: ChaCha20-Poly1305 sealing (RFC 8439) and X25519 key exchange (RFC 7748) in `std.crypto`; milestone 160 (v17).

**Status:** proposed and approved 2026-11-15, built the same day, from rapidkoi's request "What Lugha needs for Rapidkoi's sync" (items 1 and 2). Crypto: the user reviews it before it merges (security rule).

## OBJECTIVE
```lugha
import std.crypto;

// Two devices agree on a key, then seal and open.
let (a_secret, a_public) = crypto.x25519_keypair()?;
let (b_secret, b_public) = crypto.x25519_keypair()?;
let shared = crypto.x25519_shared(a_secret, b_public)?;      // == x25519_shared(b_secret, a_public)
let key = crypto.hkdf_sha256(shared, [], "pairing v1".to_bytes(), 32)?;

let nonce = crypto.random_bytes(12)?;
let sealed = crypto.chacha20poly1305_seal(key, nonce, "hello".to_bytes(), [])?;   // 5 + 16 bytes
let opened = crypto.chacha20poly1305_open(key, nonce, sealed, [])?;              // "hello"
// One flipped byte anywhere in sealed, nonce or aad: Err(CryptoError.NotAuthentic)
```

## CONTEXT
- `std.crypto` (milestone 113, PRP-145; HKDF milestone 120, PRP-152): Lugha in `src/std/crypto.la` over `$os.crypto_*` built-ins (`os_signature` in the checker, `codegen/os.rs` → `codegen/net.rs`), C in `runtime/crypto.c` (274 lines; OpenSSL 3, `-lcrypto`, only for programs loading `std.crypto`), `RUNTIME_SYMBOLS` in `src/link/mod.rs`.
- Spec v17, `std.crypto` (its table and the "Not included" line, which today rules out encryption and key exchange), §11 (milestone row), "Changes since v16".
- Both algorithms are in every OpenSSL 3.0: `EVP_chacha20_poly1305()` and `EVP_PKEY_X25519`. Nothing here raises the OpenSSL floor.
- DECISIONS: none open blocks this. PRP-145's "no encryption, key exchange" was a scope line for milestone 113, not a rule against them.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Names**: `chacha20poly1305_seal`, `chacha20poly1305_open`, `x25519_keypair`, `x25519_public`, `x25519_shared`. The algorithm leads, as `ed25519_*` and `hkdf_sha256` do. `x25519_shared` rather than the request's bare `x25519`: it says what comes back.
2. **Signatures**:
   - `chacha20poly1305_seal(key, nonce, plaintext, aad): Result<u8[], CryptoError>`: ciphertext with the 16-byte tag appended.
   - `chacha20poly1305_open(key, nonce, sealed, aad): Result<u8[], CryptoError>`.
   - `x25519_keypair(): Result<(u8[], u8[]), CryptoError>` (`(secret, public)`, 32 bytes each, like `ed25519_keypair`), `x25519_public(secret)`, `x25519_shared(secret, peer_public)`: 32 bytes each.
3. **Two new `CryptoError` variants**, so a caller can tell tampering from a broken OpenSSL:
   - `NotAuthentic`: `open` found a wrong tag (changed bytes, wrong key, nonce or aad). Message: `the sealed bytes don't match their key, nonce and associated data`.
   - `BadKey(string)`: `x25519_shared` got a low-order peer key (the shared secret came out all zeros). Message: `bad key: {why}`.
   Breaking: a `match` over every `CryptoError` must add the two arms. Listed under Breaking in CHANGELOG.
4. **Lengths**: key 32, nonce 12, X25519 secret and public 32; `sealed` at least 16; otherwise `BadLength(what, wanted, given)`, checked in Lugha before any C call (the C side checks again). Plaintext and sealed at most 1 GiB (OpenSSL's update calls take `int`; a round cap is easier to state than 2³¹ − 17); `BadLength("plaintext (at most)", 1073741824, n)`. Empty plaintext and empty aad are allowed.
5. **The caller picks the nonce.** Never the same nonce twice with one key. The doc comment says so and says that `random_bytes(12)` is safe for up to about 2³² messages per key. No random-nonce helper and no XChaCha20 (OpenSSL doesn't have it).
6. **Never partial plaintext**: `open` decrypts into an OpenSSL buffer, checks the tag (`EVP_DecryptFinal_ex`), and copies into a Lugha array only if the tag matches. The buffer is wiped and freed (`OPENSSL_clear_free`) on every path.
7. **X25519 secrets** come from the same CSPRNG path as `ed25519_keypair` (`$os.crypto_seed`). OpenSSL clamps internally, so any 32 bytes are a valid secret (as RFC 7748 says). The all-zero check is done in C with `CRYPTO_memcmp` even though OpenSSL 3 also refuses it, so the error is `BadKey` and doesn't depend on the OpenSSL version.
8. **C side**: a new fragment-style section in `runtime/crypto.c` (about +110 lines, under 500). Every `EVP_CIPHER_CTX` and `EVP_PKEY` freed on every path, the error queue read or cleared, derived secrets wiped from C buffers. `RUNTIME_SYMBOLS` gains the four new symbols.
9. **Tests in a new file `tests/crypto_seal.rs`**: `tests/crypto.rs` is at 299 lines and would pass 450.
10. **Not in this PRP**: HPKE (RFC 9180). It needs OpenSSL 3.2's `OSSL_HPKE_*` or a composition written in `std`, and raising the OpenSSL floor is a separate question for the user. Also not: AES-GCM, other ciphers, streaming encryption, password hashing.

## REQUIREMENTS

### Must Do
- `runtime/crypto.c`: `lugha_rt_crypto_seal`, `lugha_rt_crypto_open`, `lugha_rt_crypto_x25519_public`, `lugha_rt_crypto_x25519_shared`, with the `(…, out, error)` shape and `int32_t` return used by the existing built-ins.
- `$os.crypto_seal`, `$os.crypto_open`, `$os.crypto_x25519_public`, `$os.crypto_x25519_shared` in the checker's `os_signature` and in codegen. `open` must tell "wrong tag" from "OpenSSL failed": its C function returns 2 for a wrong tag, and the built-in gives `Result<u8[], string>` with a fixed string the Lugha side maps to `NotAuthentic`. The same for `x25519_shared` → `BadKey`.
- `src/std/crypto.la`: the five functions and two variants with `/** */` docs; the module doc comment names the new algorithms and nonce rule.
- Spec: the `std.crypto` paragraph and table, "Not included" (drops encryption and key exchange, keeps password hashing; adds HPKE), §11 row 160, "Changes since v16". CHANGELOG (with Breaking), TODO, the Draft's decision list, a DECISIONS entry.

### Must NOT Do
- Decision 10's list. No `unsafe` Rust, no new crates, no OpenSSL version above 3.0.
- No variant or flag that skips the tag check.

## DESIGN
- Security: all inputs are the caller's bytes; lengths are checked twice. No file, process or network access. Secrets in C buffers are wiped; OpenSSL clears its contexts on free; bytes handed to Lugha live in collected memory and aren't wiped, as the module doc already says.
- No new diagnostics.

## TESTS (`tests/crypto_seal.rs`)
- [ ] RFC 8439 §2.8.2 AEAD vector: `seal` gives the ciphertext and tag byte for byte; `open` gives the plaintext back.
- [ ] One flipped byte in the ciphertext, the tag, the aad or the nonce: `NotAuthentic` from `open`; a wrong key: `NotAuthentic`.
- [ ] Empty plaintext seals to 16 bytes and opens to empty; empty aad works.
- [ ] `BadLength` for a 31-byte key, an 11-byte nonce and a 15-byte sealed input, with their messages.
- [ ] RFC 7748 §5.2 X25519 vectors (both scalar/u-coordinate pairs) and §6.1 (Alice's and Bob's public keys and their shared secret, both ways).
- [ ] `x25519_keypair` twice: both sides compute the same shared secret; `x25519_public(secret)` equals the keypair's public.
- [ ] A low-order peer key (all zeros, and the order-8 point from RFC 7748's list): `BadKey`.
- [ ] `BadLength` for a 31-byte X25519 secret and a 33-byte peer key.
- [ ] A `match` over `CryptoError` without the new arms is rejected (the existing non-exhaustive match error); the message of each new variant.

## BUILT (2026-11-15)
- As planned, with one change: instead of a return code of 2, the runtime reports a wrong tag and a low-order key with fixed messages (`"not authentic"`, `"low order"`) that `std.crypto`'s `failed` maps to `NotAuthentic` and `BadKey`, so codegen's flag-and-out-pointer call shape stays as it is.
- `runtime/crypto.c` grew to 450 lines. `crypto_feed` takes a pointer and a length, because `LughaArray` ends in a flexible array member and can't be viewed shorter by copying the header. When `EVP_PKEY_derive` fails after both keys were read, that counts as the all-zero result (OpenSSL 3's only reason for it), so it reports `BadKey`.
- Test programs match bare variant names (`Err(NotAuthentic)`); there's no array `slice`, so the tests use a `first(bytes, n)` helper.

## ACCEPTANCE
- [ ] The tests above pass; `cargo test --lib`, `--test crypto_seal --test crypto --test lazy_std --test lsp_docs --test comments`; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every `pub` item documented; spec, Draft, DECISIONS, CHANGELOG and TODO updated.
- [ ] The user's security review before merging; tag `m160`.
