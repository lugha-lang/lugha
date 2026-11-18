//! TLS over any `Conn`: a real handshake over an in-memory connection a
//! fake `Net` hands out, with no socket anywhere, and a fake that refuses
//! keeping `std.tls` and `https://` off the network. The certificate is
//! made by the `openssl` command for each run; no key is committed.

use std::path::PathBuf;
use std::process::Command;

/// The program: a fake `Net` whose connections are channel pipes to a TLS
/// server in the same program, and one with nothing on it.
const PROGRAM: &str = r#"import std.http;
import std.tls;

/** One end of an in-memory connection: what one end writes, the other
 * reads. `open` holds one token until the end is closed, so closing twice
 * closes the channel once. */
struct Pipe { inbox: Receiver<u8[]>, outbox: Sender<u8[]>, open: Receiver<bool> }

impl Conn for Pipe {
    fun read(self, max: i64): Result<u8[], string> = match self.inbox.recv() {
        .Some(bytes) => Ok(bytes),
        .None => Ok([]),
    };
    fun write(self, bytes: u8[]): Result<(), string> {
        self.outbox.send(bytes);
        Ok(())
    }
    fun set_timeout(self, ms: i64) {}
    fun peer(self): string = "pipe:1";
    fun addr(self): string = "pipe:0";
    fun close(self) {
        if let .Ready(_) = self.open.try_recv() {
            self.outbox.close();
        }
    }
}

/** A network of one TLS server, in this program: no socket anywhere. Each
 * connection's other end goes to the server through `calls`. */
struct Loopback { calls: Sender<Pipe> }

impl Net for Loopback {
    fun connect_timeout(self, addr: string, ms: i64): Result<Conn, string> {
        let (to_there, there) = Chan<u8[]>.new(64);
        let (to_back, back) = Chan<u8[]>.new(64);
        self.calls.send(Pipe { inbox: there, outbox: to_back, open: token() });
        Ok(Pipe { inbox: back, outbox: to_there, open: token() })
    }
    fun listen(self, addr: string): Result<Listener, string> = Err("loopback only");
    fun resolve(self, name: string): Result<string[], string> = Ok(["127.0.0.1"]);
    fun udp_bind(self, addr: string): Result<Udp, string> = Err("loopback only");
    fun udp_bind_shared(self, addr: string): Result<Udp, string> = Err("loopback only");
    fun interfaces(self): Result<NetInterface[], string> = Ok([]);
}

/** A channel holding one token: an end that is still open. */
fun token(): Receiver<bool> {
    let (t, r) = Chan<bool>.new(1);
    t.send(true);
    r
}

/** The server: each connection in turn, until `calls` closes. */
fun answering(calls: Receiver<Pipe>, cert: string, key: string) {
    while let .Some(conn) = calls.recv() {
        answer(conn, cert, key);
    }
}

/** One connection: TLS over its end of the pipe, answering one request. */
fun answer(conn: Conn, cert: string, key: string) {
    match tls.server(conn, cert, key) {
        .Ok(s) => {
            let got = string_from_utf8(s.read(4096).unwrap_or([])).unwrap_or("");
            let reply = if got.starts_with("GET ") { "HTTP/1.1 200 OK\r\ncontent-length: 6\r\nconnection: close\r\n\r\nsecure" } else { "pong" };
            let w = s.write(reply.to_bytes());
            s.close();
        }
        .Err(_) => {}
    }
}

/** A network with nothing on it. */
struct Offline {}

impl Net for Offline {
    fun connect_timeout(self, addr: string, ms: i64): Result<Conn, string> = Err("offline: {addr}");
    fun listen(self, addr: string): Result<Listener, string> = Err("offline");
    fun resolve(self, name: string): Result<string[], string> = Err("offline");
    fun udp_bind(self, addr: string): Result<Udp, string> = Err("offline");
    fun udp_bind_shared(self, addr: string): Result<Udp, string> = Err("offline");
    fun interfaces(self): Result<NetInterface[], string> = Ok([]);
}

fun main(console: Console, fs: Fs, root: Scope) {
    let cert = fs.cwd().read_file("cert.pem").unwrap();
    let key = fs.cwd().read_file("key.pem").unwrap();
    let (calls, incoming) = Chan<Pipe>.new(1);
    root.spawn(answering(incoming, cert, key));
    defer calls.close();
    let net = Loopback { calls: calls };
    let mut opts = tls.options();
    opts.ca_pem = cert;
    let c = tls.connect_with(net, "localhost:443", opts).unwrap();
    let w = c.write("ping".to_bytes());
    console.println(string_from_utf8(c.read(16).unwrap()).unwrap());
    c.close();
    console.println(tls.connect(net, "localhost:443").is_err());
    match http.request_tls(net, "GET", "https://localhost/page", Map {}, [], opts) {
        .Ok(r) => console.println("{r.status} {r.text().unwrap()}"),
        .Err(e) => console.println(e),
    }
    console.println(tls.connect(Offline {}, "example.com:443").debug());
    console.println(http.get(Offline {}, "https://example.com/").debug());
}
"#;

/// A temporary folder, removed on drop.
struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn tls_and_https_run_over_a_fake_net_and_a_refusing_one_keeps_them_off_the_network() {
    let dir = std::env::temp_dir().join(format!("lugha-tls-conn-{}", std::process::id()));
    // A leftover from an earlier run would change what the test sees.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let folder = Folder(dir);
    let made = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "key.pem",
            "-out",
            "cert.pem",
            "-days",
            "2",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost",
        ])
        .current_dir(&folder.0)
        .output()
        .is_ok_and(|o| o.status.success());
    if !made {
        eprintln!("skipped: no `openssl` command to make a test certificate");
        return;
    }
    std::fs::write(folder.0.join("main.la"), PROGRAM).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["run", "main.la"])
        .current_dir(&folder.0)
        .env("HOME", &folder.0)
        .output()
        .expect("lughac runs");
    assert_eq!(
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).as_ref()
        ),
        (
            Some(0),
            "pong\ntrue\n200 secure\nErr(\"offline: example.com:443\")\nErr(\"offline: example.com:443\")\n"
        ),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
