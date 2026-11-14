//! `std.crypto`'s sealing and key exchange end to end: RFC 8439's
//! ChaCha20-Poly1305 vector and RFC 7748's X25519 vectors, bit for bit,
//! through Lugha programs; every tampering refused, low-order keys refused,
//! and every length checked.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Hex helpers and a short `CryptoError` reader every program here uses.
const HELPERS: &str = "import std.crypto;

fun hex(bytes: u8[]): string {
    let digits = \"0123456789abcdef\";
    let mut out = \"\";
    for b of bytes {
        let hi = (b / 16) as i64;
        let lo = (b % 16) as i64;
        out = out + digits.slice(hi, hi + 1) + digits.slice(lo, lo + 1);
    }
    out
}

fun nibble(c: u8): u8 = if c >= 97 { c - 87 } else { c - 48 };

fun unhex(s: string): u8[] {
    let raw = s.to_bytes();
    let mut out: u8[] = [];
    let mut i = 0;
    while i + 1 < raw.len {
        out.push(nibble(raw[i]) * 16 + nibble(raw[i + 1]));
        i += 2;
    }
    out
}

fun first(bytes: u8[], n: i64): u8[] {
    let mut out: u8[] = [];
    for i in 0..n {
        out.push(bytes[i]);
    }
    out
}

fun show(r: Result<u8[], crypto.CryptoError>): string {
    match r {
        Ok(bytes) => hex(bytes),
        Err(e) => e.message(),
    }
}
";

/// Runs `lughac <command> main.la` on `main` (with the helpers).
fn lughac(command: &str, main: &str) -> Output {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir: PathBuf =
        std::env::temp_dir().join(format!("lugha-crypto-seal-{}-{n}", std::process::id()));
    // A leftover from an earlier run would change what the test sees.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.la"), format!("{HELPERS}\n{main}")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args([command, "main.la"])
        .current_dir(&dir)
        .env("HOME", &dir)
        .output()
        .expect("lughac runs");
    // Best effort: a leftover temp dir must not mask the real failure.
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// Runs `main` and returns its stdout, expecting exit 0.
fn run(main: &str) -> String {
    let out = lughac("run", main);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// RFC 8439's AEAD test vector: key, nonce, aad, and the sealed result (ciphertext then tag).
const RFC_8439: &str = "
fun key(): u8[] = unhex(\"808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f\");
fun nonce(): u8[] = unhex(\"070000004041424344454647\");
fun aad(): u8[] = unhex(\"50515253c0c1c2c3c4c5c6c7\");
fun plain(): u8[] = \"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.\".to_bytes();
fun sealed(): u8[] = unhex(\"d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b61161ae10b594f09e26a7e902ecbd0600691\");

fun flipped(bytes: u8[], at: i64): u8[] {
    let mut out = bytes;
    out[at] = out[at] ^ 1;
    out
}
";

#[test]
fn chacha20poly1305_matches_rfc_8439_both_ways() {
    let out = run(&format!(
        "{RFC_8439}
fun main() {{
    println(hex(crypto.chacha20poly1305_seal(key(), nonce(), plain(), aad()).unwrap()) == hex(sealed()));
    let opened = crypto.chacha20poly1305_open(key(), nonce(), sealed(), aad()).unwrap();
    println(opened == plain());
}}"
    ));
    assert_eq!(out, "true\ntrue\n");
}

#[test]
fn any_changed_byte_or_wrong_key_is_not_authentic() {
    let out = run(&format!(
        "{RFC_8439}
fun main() {{
    let n = sealed().len;
    println(show(crypto.chacha20poly1305_open(key(), nonce(), flipped(sealed(), 0), aad())));
    println(show(crypto.chacha20poly1305_open(key(), nonce(), flipped(sealed(), n - 1), aad())));
    println(show(crypto.chacha20poly1305_open(key(), nonce(), sealed(), flipped(aad(), 3))));
    println(show(crypto.chacha20poly1305_open(key(), flipped(nonce(), 11), sealed(), aad())));
    println(show(crypto.chacha20poly1305_open(flipped(key(), 5), nonce(), sealed(), aad())));
    println(show(crypto.chacha20poly1305_open(key(), nonce(), sealed(), [])));
    match crypto.chacha20poly1305_open(key(), nonce(), flipped(sealed(), 7), aad()) {{
        Err(NotAuthentic) => println(\"refused\"),
        _ => println(\"accepted\"),
    }}
}}"
    ));
    let refused = "the sealed bytes don't match their key, nonce and associated data\n";
    assert_eq!(out, format!("{}refused\n", refused.repeat(6)));
}

#[test]
fn empty_plaintext_and_aad_and_every_bad_length() {
    let out = run(&format!(
        "{RFC_8439}
fun main() {{
    let tag_only = crypto.chacha20poly1305_seal(key(), nonce(), [], []).unwrap();
    println(tag_only.len);
    println(crypto.chacha20poly1305_open(key(), nonce(), tag_only, []).unwrap().len);
    let short_key = first(key(), 31);
    println(show(crypto.chacha20poly1305_seal(short_key, nonce(), plain(), [])));
    println(show(crypto.chacha20poly1305_open(short_key, nonce(), sealed(), [])));
    println(show(crypto.chacha20poly1305_seal(key(), first(nonce(), 11), plain(), [])));
    println(show(crypto.chacha20poly1305_open(key(), nonce(), first(sealed(), 15), [])));
}}"
    ));
    assert_eq!(
        out,
        "16\n0\n\
         key: wanted 32 bytes, got 31\n\
         key: wanted 32 bytes, got 31\n\
         nonce: wanted 12 bytes, got 11\n\
         sealed bytes (at least): wanted 16 bytes, got 15\n"
    );
}

#[test]
fn x25519_matches_rfc_7748() {
    let out = run("fun main() {
    // RFC 7748's scalar and u-coordinate vectors.
    println(show(crypto.x25519_shared(
        unhex(\"a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4\"),
        unhex(\"e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c\"))));
    println(show(crypto.x25519_shared(
        unhex(\"4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d\"),
        unhex(\"e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493\"))));
    // RFC 7748's Alice and Bob.
    let alice = unhex(\"77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a\");
    let bob = unhex(\"5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb\");
    let alice_public = crypto.x25519_public(alice).unwrap();
    let bob_public = crypto.x25519_public(bob).unwrap();
    println(hex(alice_public));
    println(hex(bob_public));
    println(show(crypto.x25519_shared(alice, bob_public)));
    println(show(crypto.x25519_shared(bob, alice_public)));
}");
    assert_eq!(
        out,
        "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552\n\
         95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957\n\
         8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a\n\
         de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f\n\
         4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742\n\
         4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742\n"
    );
}

#[test]
fn x25519_keypairs_agree_and_bad_keys_are_refused() {
    let out = run("fun main() {
    let (a, a_public) = crypto.x25519_keypair().unwrap();
    let (b, b_public) = crypto.x25519_keypair().unwrap();
    println(a.len == 32 && a_public.len == 32 && a != b);
    println(crypto.x25519_public(a).unwrap() == a_public);
    println(crypto.x25519_shared(a, b_public).unwrap() == crypto.x25519_shared(b, a_public).unwrap());
    // Low-order points: zero, and RFC 7748's point of order 8.
    println(show(crypto.x25519_shared(a, [0 as u8; 32])));
    let order_8 = unhex(\"e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b800\");
    match crypto.x25519_shared(a, order_8) {
        Err(BadKey(why)) => println(\"refused: {why}\"),
        _ => println(\"accepted\"),
    }
    println(show(crypto.x25519_public(first(a, 31))));
    println(show(crypto.x25519_shared(first(a, 31), b_public)));
    println(show(crypto.x25519_shared(a, [1 as u8; 33])));
}");
    assert_eq!(
        out,
        "true\ntrue\ntrue\n\
         bad key: the peer's public key has low order\n\
         refused: the peer's public key has low order\n\
         secret key: wanted 32 bytes, got 31\n\
         secret key: wanted 32 bytes, got 31\n\
         peer public key: wanted 32 bytes, got 33\n"
    );
}

#[test]
fn a_match_must_name_the_new_variants() {
    let out = lughac(
        "check",
        "fun main() {
    match crypto.random_bytes(1) {
        Ok(_) => {}
        Err(BadLength(_, _, _)) => {}
        Err(OpenSsl(_)) => {}
    }
}",
    );
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("NotAuthentic"), "stderr: {stderr}");
}
