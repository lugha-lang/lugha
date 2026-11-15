//! `std.tls` by key end to end: Lugha programs on loopback where both sides
//! prove an Ed25519 key from the other's list (raw public keys, TLS 1.3), keys
//! refused in the handshake, lists replaced while listening, and the
//! boundary with certificate connections. Keys are made by each run.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder with one program, removed on drop.
struct Case(PathBuf);

impl Case {
    fn new(main: &str) -> Case {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-tls-keys-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.la"), format!("{HELPERS}\n{main}")).unwrap();
        Case(dir)
    }

    /// A self-signed certificate for 127.0.0.1 as `self.pem`/`self.key`;
    /// `false` when there is no `openssl` command to make it with.
    fn certificate(&self) -> bool {
        Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "ed25519",
                "-nodes",
                "-keyout",
                "self.key",
                "-out",
                "self.pem",
                "-days",
                "2",
                "-subj",
                "/CN=localhost",
                "-addext",
                "subjectAltName=IP:127.0.0.1",
            ])
            .current_dir(&self.0)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    fn runs(&self, stdout: &str) {
        let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(["run", "main.la"])
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .output()
            .expect("lughac runs");
        assert_eq!(
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).as_ref()
            ),
            (Some(0), stdout),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A device's key pair, a server that answers each client with the key it
/// saw, and a client that says whether it got in.
const HELPERS: &str = "import std.tls;
import std.crypto;

struct Device {
    secret: u8[],
    key: u8[],
}

fun device(): Device {
    let (secret, key) = crypto.ed25519_keypair().unwrap();
    Device { secret: secret, key: key }
}

fun serve(l: tls.Listener) {
    while true {
        match l.accept() {
            Ok(conn) => {
                let _ = conn.write(conn.peer_key());
                conn.close();
            }
            Err(_) => {}
        }
    }
}

fun listening(server: Device, accept: u8[][]): tls.Listener {
    let l = tls.listen_keys(\"127.0.0.1:0\", tls.Keys { secret: server.secret, accept: accept }).unwrap();
    spawn serve(l);
    l
}

// `in` when the server named this client's key back, `refused: …` otherwise.
fun try(addr: string, client: Device, server_key: u8[]) {
    match tls.connect_keys(addr, tls.Keys { secret: client.secret, accept: [server_key] }, 5000) {
        Ok(c) => {
            let ours = c.peer_key() == server_key;
            match c.read(100) {
                Ok(seen) => println(if seen == client.key && ours { \"in\" } else { \"refused\" }),
                Err(_) => println(\"refused\"),
            }
            c.close();
        }
        Err(e) => println(\"refused: {e}\"),
    }
}
";

#[test]
fn accepted_keys_get_in_and_others_are_refused() {
    Case::new(
        "fun main() {
    let (server, a, b, c) = (device(), device(), device(), device());
    let l = listening(server, [a.key, b.key]);
    try(l.addr(), a, server.key);
    try(l.addr(), b, server.key);
    try(l.addr(), c, server.key);
    // A client that doesn't accept the server's key stops at connect.
    try(l.addr(), a, c.key);
    // Nobody is accepted by an empty list.
    let empty = listening(server, []);
    try(empty.addr(), a, server.key);
}",
    )
    .runs("in\nin\nrefused\nrefused: peer key not accepted\nrefused\n");
}

#[test]
fn set_accept_changes_who_gets_in_next() {
    Case::new(
        "fun main() {
    let (server, a, c) = (device(), device(), device());
    let l = listening(server, [a.key]);
    try(l.addr(), c, server.key);
    l.set_accept([a.key, c.key]).unwrap();
    try(l.addr(), c, server.key);
    l.set_accept([]).unwrap();
    try(l.addr(), a, server.key);
    l.close();
    println(l.set_accept([]));
}",
    )
    .runs("refused\nin\nrefused\nErr(\"closed\")\n");
}

#[test]
fn wrong_key_lengths_are_refused() {
    Case::new(
        "fun main() {
    let a = device();
    let short = [1 as u8; 31];
    println(tls.listen_keys(\"127.0.0.1:0\", tls.Keys { secret: short, accept: [] }).is_err());
    match tls.listen_keys(\"127.0.0.1:0\", tls.Keys { secret: short, accept: [] }) {
        Err(e) => println(e),
        Ok(_) => println(\"listening\"),
    }
    match tls.listen_keys(\"127.0.0.1:0\", tls.Keys { secret: a.secret, accept: [a.key, short] }) {
        Err(e) => println(e),
        Ok(_) => println(\"listening\"),
    }
    match tls.connect_keys(\"127.0.0.1:1\", tls.Keys { secret: a.secret, accept: [short] }, 1000) {
        Err(e) => println(e),
        Ok(_) => println(\"connected\"),
    }
    let l = tls.listen_keys(\"127.0.0.1:0\", tls.Keys { secret: a.secret, accept: [] }).unwrap();
    println(l.set_accept([short]));
}",
    )
    .runs(
        "true\nsecret key must be 32 bytes\naccepted key 1 must be 32 bytes\n\
         accepted key 0 must be 32 bytes\nErr(\"accepted key 0 must be 32 bytes\")\n",
    );
}

#[test]
fn certificates_and_keys_do_not_mix() {
    let case = Case::new(
        "fun main() {
    let (server, a) = (device(), device());
    // A certificate client at a by-key listener.
    let by_key = listening(server, [a.key]);
    let trust = tls.Options { ca_file: \"self.pem\", ..tls.options() };
    println(tls.connect_with(by_key.addr(), trust).is_err());
    // A by-key client at a certificate listener, which has no list to change.
    let by_cert = tls.listen(\"127.0.0.1:0\", \"self.pem\", \"self.key\").unwrap();
    spawn serve(by_cert);
    try(by_cert.addr(), a, server.key);
    println(by_cert.set_accept([a.key]));
    // A certificate connection has no peer key.
    let c = tls.connect_with(by_cert.addr(), trust).unwrap();
    println(c.peer_key().len);
    c.close();
}",
    );
    if !case.certificate() {
        eprintln!("skipped: no `openssl` command to make a test certificate");
        return;
    }
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["run", "main.la"])
        .current_dir(&case.0)
        .env("HOME", &case.0)
        .output()
        .expect("lughac runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(lines.len(), 4, "stdout: {stdout}");
    assert_eq!(lines[0], "true");
    assert!(lines[1].starts_with("refused"), "stdout: {stdout}");
    assert_eq!(lines[2..], ["Err(\"not a by-key listener\")", "0"]);
}
