//! `std.tls` and `https://` end to end: Lugha
//! programs on loopback with certificates the `openssl` command makes for each
//! run — a test CA, a server certificate signed by it, a self-signed one, an
//! expired one, one for another host. No key is ever committed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder with the certificates and one program, removed on drop.
struct Case(PathBuf);

impl Case {
    /// `None` when there is no `openssl` command to make certificates with.
    fn new(main: &str) -> Option<Case> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-tls-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let case = Case(dir);
        if !case.certificates() {
            eprintln!("skipped: no `openssl` command to make test certificates");
            return None;
        }
        std::fs::write(case.0.join("main.la"), main).unwrap();
        Some(case)
    }

    fn openssl(&self, args: &[&str]) -> bool {
        Command::new("openssl")
            .args(args)
            .current_dir(&self.0)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// `ca.pem`; `cert.pem`/`key.pem` for 127.0.0.1 and localhost signed by it;
    /// `self.pem`/`self.key`, self-signed; `old.pem` (expired) and
    /// `other.pem` (for other.test), both signed by the CA with `key.pem`.
    fn certificates(&self) -> bool {
        let req = |key: &str, cn: &str, out: &str| {
            self.openssl(&[
                "req", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", out, "-subj", cn,
            ])
        };
        let sign = |csr: &str, ext: &str, out: &str, dates: &[&str]| {
            let mut args = vec![
                "x509",
                "-req",
                "-in",
                csr,
                "-CA",
                "ca.pem",
                "-CAkey",
                "ca.key",
                "-CAcreateserial",
                "-out",
                out,
                "-extfile",
                ext,
            ];
            args.extend_from_slice(dates);
            self.openssl(&args)
        };
        std::fs::write(
            self.0.join("local.ext"),
            "subjectAltName=DNS:localhost,IP:127.0.0.1\n",
        )
        .unwrap();
        std::fs::write(self.0.join("other.ext"), "subjectAltName=DNS:other.test\n").unwrap();
        self.openssl(&[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
            "-days",
            "2",
            "-subj",
            "/CN=Lugha Test CA",
        ]) && req("key.pem", "/CN=localhost", "local.csr")
            && sign("local.csr", "local.ext", "cert.pem", &["-days", "2"])
            && sign("local.csr", "other.ext", "other.pem", &["-days", "2"])
            && sign(
                "local.csr",
                "local.ext",
                "old.pem",
                &[
                    "-not_before",
                    "20200101000000Z",
                    "-not_after",
                    "20200102000000Z",
                ],
            )
            && self.openssl(&[
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
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

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const SERVE: &str = "fun serve(l: tls.SecureListener) {
    while true {
        match l.accept() {
            .Ok(conn) => {
                if let .Ok(got) = conn.read(100) {
                    let _ = conn.write(got);
                }
                conn.close();
            }
            .Err(_) => {}
        }
    }
}

fun trusting(fs: Fs): tls.Options = tls.Options { ca_pem: pem(fs, \"ca.pem\"), ..tls.options() };

fun try(net: Net, console: Console, addr: string, opts: tls.Options) {
    match tls.connect_with(net, addr, opts) {
        .Ok(c) => {
            c.write(\"x\".to_bytes()).unwrap();
            console.println(string_from_utf8(c.read(10).unwrap()).unwrap());
            c.close();
        }
        .Err(e) => console.println(e),
    }
}

fun listening(s: Scope, fs: Fs, net: Net, cert: string, key: string): string {
    let l = tls.listen(net, \"127.0.0.1:0\", pem(fs, cert), pem(fs, key)).unwrap();
    let addr = l.addr();
    s.spawn(serve(l));
    addr
}
";

#[test]
fn a_verified_connection_carries_bytes_and_bad_certificates_are_refused() {
    let Some(case) = Case::new(&format!(
        "import std.tls;\n\n/** A PEM file's text, empty when it can't be read. */\nfun pem(fs: Fs, name: string): string = fs.cwd().read_file(name).unwrap_or(\"\");\n\n{SERVE}
fun main(console: Console, net: Net, fs: Fs) {{
    let servers = scope s {{
        let good = listening(s, fs, net, \"cert.pem\", \"key.pem\");
        try(net, console, good, trusting(fs));
        let port = good.split(\":\")[1];
        try(net, console, \"localhost:{{port}}\", trusting(fs));
        try(net, console, good, tls.options());
        try(net, console, listening(s, fs, net, \"self.pem\", \"self.key\"), trusting(fs));
        let trust_self = tls.Options {{ ca_pem: pem(fs, \"self.pem\"), ..tls.options() }};
        try(net, console, listening(s, fs, net, \"self.pem\", \"self.key\"), trust_self);
        try(net, console, listening(s, fs, net, \"old.pem\", \"key.pem\"), trusting(fs));
        try(net, console, listening(s, fs, net, \"other.pem\", \"key.pem\"), trusting(fs));
        console.println(tls.listen(net, \"127.0.0.1:0\", pem(fs, \"cert.pem\"), pem(fs, \"missing.pem\")).is_err());
        let plain = net.listen(\"127.0.0.1:0\").unwrap();
        let plain_addr = plain.addr();
        s.spawn(refuse(plain));
        console.println(tls.connect_with(net, plain_addr, trusting(fs)).is_err());
        // Leaving early cancels the servers, which never end.
        return;
    }};
}}

fun refuse(l: Listener) {{
    let c = l.accept().unwrap();
    let _ = c.write(\"HTTP/1.1 400 Bad Request\\r\\n\\r\\n\".to_bytes());
    c.close();
}}
"
    )) else {
        return;
    };
    let _ = case.path();
    case.runs(
        "x\nx\ncertificate verify failed: unable to get local issuer certificate\n\
         certificate verify failed: self-signed certificate\nx\n\
         certificate verify failed: certificate has expired\n\
         certificate verify failed: IP address mismatch\ntrue\ntrue\n",
    );
}

#[test]
fn https_serves_and_fetches() {
    let Some(case) = Case::new(
        "import std.http;
import std.task;
import std.tls;

/** A PEM file's text, empty when it can't be read. */
fun pem(fs: Fs, name: string): string = fs.cwd().read_file(name).unwrap_or(\"\");

fun handle(req: http.Request): http.Response = http.text(200, \"secure {req.method} {req.path}\");

fun server(fs: Fs, net: Net, addr: string) {
    http.serve_tls(net, addr, pem(fs, \"cert.pem\"), pem(fs, \"key.pem\"), handle).unwrap();
}

fun main(console: Console, net: Net, fs: Fs, root: Scope) {
    let probe = tls.listen(net, \"127.0.0.1:0\", pem(fs, \"cert.pem\"), pem(fs, \"key.pem\")).unwrap();
    let addr = probe.addr();
    probe.close();
    let serving = root.spawn(server(fs, net, addr));
    defer serving.cancel(\"done\");
    task.yield_now();
    let port = addr.split(\":\")[1];
    let opts = tls.Options { ca_pem: pem(fs, \"ca.pem\"), ..tls.options() };
    let res = http.request_tls(net, \"POST\", \"https://localhost:{port}/x\", Map {}, \"b\".to_bytes(), opts).unwrap();
    console.println(\"{res.status} {res.text().unwrap()}\");
    match http.get(net, \"https://localhost:{port}/\") {
        .Ok(_) => console.println(\"?\"),
        .Err(e) => console.println(e),
    }
    console.println(http.get(net, \"http://localhost:{port}/\").is_err());
}
",
    ) else {
        return;
    };
    case.runs(
        "200 secure POST /x\ncertificate verify failed: unable to get local issuer certificate\ntrue\n",
    );
}

#[test]
fn request_tls_with_takes_limits_and_trust() {
    let Some(case) = Case::new(
        "import std.http;
import std.task;
import std.tls;

/** A PEM file's text, empty when it can't be read. */
fun pem(fs: Fs, name: string): string = fs.cwd().read_file(name).unwrap_or(\"\");

fun handle(req: http.Request): http.Response = http.bytes(200, [7 as u8; 17000000]);

fun server(fs: Fs, net: Net, addr: string) {
    http.serve_tls(net, addr, pem(fs, \"cert.pem\"), pem(fs, \"key.pem\"), handle).unwrap();
}

fun silent(l: tls.SecureListener) {
    let c = l.accept().unwrap();
    task.sleep(10000);
    c.close();
}

fun fetch(fs: Fs, net: Net, console: Console, url: string, body: i64, ca_file: string) {
    let mut limits = http.limits();
    limits.body = body;
    let opts = tls.Options { ca_pem: pem(fs, ca_file), ..tls.options() };
    match http.request_tls_with(net, \"GET\", url, Map {}, [], limits, opts) {
        .Ok(res) => console.println(\"{res.status} {res.body.len} {res.body[16999999]}\"),
        .Err(e) => console.println(e),
    }
}

fun main(console: Console, clock: Clock, net: Net, fs: Fs, root: Scope) {
    let probe = tls.listen(net, \"127.0.0.1:0\", pem(fs, \"cert.pem\"), pem(fs, \"key.pem\")).unwrap();
    let addr = probe.addr();
    probe.close();
    let serving = root.spawn(server(fs, net, addr));
    defer serving.cancel(\"done\");
    task.yield_now();
    let url = \"https://localhost:{addr.split(\":\")[1]}/app.rkb\";
    fetch(fs, net, console, url, 33554432, \"ca.pem\");
    fetch(fs, net, console, url, 16777216, \"ca.pem\");
    fetch(fs, net, console, url, 33554432, \"\");

    let quiet = tls.listen(net, \"127.0.0.1:0\", pem(fs, \"cert.pem\"), pem(fs, \"key.pem\")).unwrap();
    let port = quiet.addr().split(\":\")[1];
    let quiet_task = root.spawn(silent(quiet));
    defer quiet_task.cancel(\"done\");
    let mut limits = http.limits();
    limits.timeout_ms = 60000;
    let opts = tls.Options { ca_pem: pem(fs, \"ca.pem\"), timeout_ms: 200, address: \"\" };
    let started = clock.monotonic_ms();
    match http.request_tls_with(net, \"GET\", \"https://localhost:{port}/\", Map {}, [], limits, opts) {
        .Ok(_) => console.println(\"?\"),
        .Err(e) => console.println(e),
    }
    console.println(clock.monotonic_ms() - started < 5000);
}
",
    ) else {
        return;
    };
    case.runs(
        "200 17000000 7\nresponse too large\n\
         certificate verify failed: unable to get local issuer certificate\ntimed out\ntrue\n",
    );
}

#[test]
fn an_address_is_connected_to_and_the_name_still_verified() {
    let Some(case) = Case::new(
        "import std.http;
import std.task;
import std.tls;

/** A PEM file's text, empty when it can't be read. */
fun pem(fs: Fs, name: string): string = fs.cwd().read_file(name).unwrap_or(\"\");

fun handle(req: http.Request): http.Response = http.text(200, \"pinned {req.path}\");

fun server(fs: Fs, net: Net, addr: string) {
    http.serve_tls(net, addr, pem(fs, \"cert.pem\"), pem(fs, \"key.pem\"), handle).unwrap();
}

fun fetch(fs: Fs, net: Net, console: Console, url: string, address: string) {
    let opts = tls.Options { ca_pem: pem(fs, \"ca.pem\"), address: address, ..tls.options() };
    match http.request_tls_with(net, \"GET\", url, Map {}, [], http.limits(), opts) {
        .Ok(res) => console.println(\"{res.status} {res.text().unwrap()}\"),
        .Err(e) => console.println(e),
    }
}

fun main(console: Console, net: Net, fs: Fs, root: Scope) {
    let probe = tls.listen(net, \"127.0.0.1:0\", pem(fs, \"cert.pem\"), pem(fs, \"key.pem\")).unwrap();
    let addr = probe.addr();
    probe.close();
    let serving = root.spawn(server(fs, net, addr));
    defer serving.cancel(\"done\");
    task.yield_now();
    let port = addr.split(\":\")[1];
    fetch(fs, net, console, \"https://localhost:{port}/a\", \"127.0.0.1\");
    fetch(fs, net, console, \"https://wrong.example:{port}/b\", \"127.0.0.1\");
    fetch(fs, net, console, \"https://localhost:{port}/c\", \"localhost\");
    let opts = tls.Options { ca_pem: pem(fs, \"ca.pem\"), address: \"127.0.0.1\", ..tls.options() };
    let c = tls.connect_with(net, \"localhost:{port}\", opts).unwrap();
    c.close();
    console.println(tls.connect_with(net, \"wrong.example:{port}\", opts).is_err());
}
",
    ) else {
        return;
    };
    case.runs(
        "200 pinned /a\n\
         certificate verify failed: hostname mismatch\n\
         address must be an IP address: localhost\ntrue\n",
    );
}
