---
title: TLS and cryptography
summary: std.tls for encrypted connections and std.crypto for hashes, signatures, key agreement and sealing.
modules: [tls, crypto]
---

# TLS and cryptography

## Overview

`import std.tls;` encrypts connections, and `import std.crypto;` gives
hashes, signatures, key agreement and authenticated encryption. Both use
OpenSSL underneath. Programs importing them link OpenSSL 3.2 or newer.

**Use what is here; never build your own.** Reach first for the high-level
calls: `tls.connect` for a secure connection, `crypto.hpke_seal` to encrypt
to someone's public key, `crypto.chacha20poly1305_seal` with a fresh nonce
for a shared key, `crypto.ed25519_sign` to sign. Draw keys from `Entropy`,
compare secrets with `crypto.equal`, and never print or log a key.

## Reference

### std.tls

| Item | What it does |
| --- | --- |
| `tls.connect(net, "example.com:443")` | `Ok(SecureConn)`, the server's certificate checked for the host |
| `tls.connect_with(net, addr, opts)` | the same with `Options` |
| `tls.options()` | the default `Options`: the system's roots, no timeout |
| `Options` | `ca_pem` (more certificates to trust, PEM text), `timeout_ms`, `address` (an IP to connect to instead of resolving; the certificate must still name the host) |
| `tls.listen(net, addr, cert_pem, key_pem)` | `Ok(SecureListener)` whose `accept` gives encrypted connections |
| `tls.client(conn, host, opts)` | TLS as the client over a `Conn` made any way |
| `tls.server(conn, cert_pem, key_pem)` | TLS as the server over one `Conn` |
| `Keys` | `secret` (this side's Ed25519 seed) and `accept` (the peers' public keys), for connections by key instead of certificates |
| `tls.connect_keys(net, addr, keys, timeout_ms)` | connects by key: the server must prove a key in `keys.accept` |
| `tls.client_keys(conn, keys, timeout_ms)` | the same over a `Conn` made any way |
| `tls.listen_keys(net, addr, keys)` | listens by key: only clients proving an accepted key get in |
| `SecureConn` | a `Conn` (read, write, close, …) that is encrypted |
| `secure.peer_key()` | the Ed25519 key the other end proved (`[]` for certificates) |
| `SecureListener` | a `Listener` whose connections are encrypted |
| `listener.accept_secure()` | `accept`, giving a `SecureConn` for its `peer_key()` |
| `listener.set_accept(keys)` | replaces the keys a listener by key accepts |

### std.crypto

| Item | What it does |
| --- | --- |
| `crypto.sha256(bytes)` | the 32-byte SHA-256 digest |
| `Sha256.new()`, `h.update(bytes)`, `h.finish()` | SHA-256 fed in pieces |
| `crypto.equal(a, b)` | compares bytes in time independent of where they differ: for secrets and tags |
| `crypto.hkdf_sha256(key, salt, info, length)` | derives `length` bytes of key from a secret (RFC 5869) |
| `crypto.ed25519_keypair(entropy)` | `Ok((secret, public))`: a signing key pair |
| `crypto.ed25519_public(secret)` | the public key of a secret seed |
| `crypto.ed25519_sign(secret, message)` | `Ok(signature)`, 64 bytes |
| `crypto.ed25519_verify(public, message, signature)` | whether the signature is valid |
| `crypto.x25519_keypair(entropy)`, `crypto.x25519_public(secret)` | a key-agreement pair, and a secret's public key |
| `crypto.x25519_shared(secret, peer_public)` | the secret two parties share; derive keys from it with `hkdf_sha256` |
| `crypto.chacha20poly1305_seal(key, nonce, plaintext, aad)` | encrypts and authenticates (32-byte key, 12-byte nonce, never reused with one key) |
| `crypto.chacha20poly1305_open(key, nonce, sealed, aad)` | the plaintext, or `Err(NotAuthentic)` if anything changed |
| `crypto.hpke_seal(public, info, plaintext, aad)` | encrypts to an X25519 public key (RFC 9180) |
| `crypto.hpke_open(secret, info, sealed, aad)` | the plaintext for the holder of the secret |
| `CryptoError` | `BadLength(what, wanted, given)`, `OpenSsl(reason)`, `NotAuthentic`, `BadKey(why)`; `e.message()` |

## Examples

```lugha
import std.crypto;

fun hex(bytes: u8[]): string = bytes.fold("", fun(s, b) = "{s}{b:02x}");

fun main(console: Console, entropy: Entropy) {
    console.println(hex(crypto.sha256("abc".to_bytes())).slice(0, 16));
    let h = crypto.Sha256.new();
    h.update("a".to_bytes());
    h.update("bc".to_bytes());
    console.println(crypto.equal(h.finish(), crypto.sha256("abc".to_bytes())));

    let (secret, key) = crypto.ed25519_keypair(entropy).unwrap();
    let message = "pay ann 5".to_bytes();
    let signature = crypto.ed25519_sign(secret, message).unwrap();
    console.println("{crypto.ed25519_verify(key, message, signature)} {crypto.ed25519_verify(key, "pay ann 500".to_bytes(), signature)}");

    let key = entropy.bytes(32);
    let nonce = entropy.bytes(12);
    let sealed = crypto.chacha20poly1305_seal(key, nonce, "secret".to_bytes(), []).unwrap();
    let opened = crypto.chacha20poly1305_open(key, nonce, sealed, []).unwrap();
    console.println(string_from_utf8(opened).unwrap());
    let mut tampered = sealed;
    tampered[0] = tampered[0] ^ 1;
    console.println(crypto.chacha20poly1305_open(key, nonce, tampered, []) == Err(crypto.CryptoError.NotAuthentic));

    let (bob_secret, bob_public) = crypto.x25519_keypair(entropy).unwrap();
    let box = crypto.hpke_seal(bob_public, "demo".to_bytes(), "for bob".to_bytes(), []).unwrap();
    console.println(string_from_utf8(crypto.hpke_open(bob_secret, "demo".to_bytes(), box, []).unwrap()).unwrap());
    match crypto.ed25519_sign([1, 2], message) {
        .Ok(_) => console.println("?"),
        .Err(e) => console.println(e.message()),
    }
}
```
```output
ba7816bf8f01cfea
true
true false
secret
true
for bob
secret key: wanted 32 bytes, got 2
```

```lugha
import std.tls;

fun fetch_head(net: Net, host: string): Result<string, string> {
    let conn = tls.connect(net, "{host}:443")?;
    defer conn.close();
    conn.write("HEAD / HTTP/1.1\r\nhost: {host}\r\nconnection: close\r\n\r\n".to_bytes())?;
    let reply = conn.read(4096)?;
    Ok(string_from_utf8(reply)?.lines()[0])
}

fun main(console: Console, net: Net) {
    // Checked, not run here: it needs the internet.
    match fetch_head(net.only("example.com:443"), "example.com") {
        .Ok(status) => console.println(status),
        .Err(e) => console.eprintln(e),
    }
}
```
