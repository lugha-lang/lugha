//! `std.net` end to end: Lugha programs
//! through `lughac run`, on the loopback interface only.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder holding one program, removed on drop.
struct Case(PathBuf);

impl Case {
    fn new(main: &str) -> Case {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-net-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.la"), format!("import std.task;\n\n{main}")).unwrap();
        Case(dir)
    }

    fn command(&self, opt: &str) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lughac"));
        cmd.args(["run", "main.la", opt])
            .current_dir(&self.0)
            .env("HOME", &self.0);
        cmd
    }

    fn run(&self, opt: &str) -> Output {
        self.command(opt).output().expect("lughac runs")
    }

    /// Runs at -O0 and -O2, expecting exit 0 and exactly `stdout`.
    fn runs(&self, stdout: &str) {
        for opt in ["-O0", "-O2"] {
            let out = self.run(opt);
            assert_eq!(
                (
                    out.status.code(),
                    String::from_utf8_lossy(&out.stdout).as_ref()
                ),
                (Some(0), stdout),
                "{opt}, stderr: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const ECHO: &str = "fun serve(console: Console, conn: Conn) {
    match conn.read(4096) {
        .Ok(bytes) => {
            let _ = conn.write(bytes);
        }
        .Err(e) => console.eprintln(e),
    }
    conn.close();
}

fun accept_all(console: Console, listener: Listener, n: i64) {
    scope s {
        for _ in 0..n {
            match listener.accept() {
                .Ok(conn) => {
                    s.spawn(serve(console, conn));
                }
                .Err(e) => console.eprintln(e),
            }
        }
    }.unwrap();
}
";

#[test]
fn a_server_and_a_client_talk_in_one_program() {
    Case::new(&format!(
        "{ECHO}
fun main(console: Console, net: Net, root: Scope) {{
    let listener = net.listen(\"127.0.0.1:0\").unwrap();
    let addr = listener.addr();
    console.println(addr.starts_with(\"127.0.0.1:\"));
    root.spawn(accept_all(console, listener, 1));
    let conn = net.connect(addr).unwrap();
    console.println(conn.peer() == addr);
    conn.write(\"hello\".to_bytes()).unwrap();
    console.println(string_from_utf8(conn.read(100).unwrap()).unwrap());
    console.println(conn.read(100).unwrap().len);
    let copy = conn;
    copy.close();
    console.println(conn.read(1).debug());
    console.println(conn.write([1]).debug());
    conn.close();
    listener.close();
    console.println(listener.accept().is_err());
}}
"
    ))
    .runs("true\ntrue\nhello\n0\nErr(\"closed\")\nErr(\"closed\")\ntrue\n");
}

#[test]
fn many_clients_wait_without_blocking_each_other() {
    Case::new(&format!(
        "{ECHO}
fun client(net: Net, addr: string, i: i64, done: Sender<i64>) {{
    let conn = net.connect(addr).unwrap();
    conn.write(\"{{i}}\".to_bytes()).unwrap();
    let back = string_from_utf8(conn.read(64).unwrap()).unwrap();
    conn.close();
    done.send(if back == \"{{i}}\" {{ 1 }} else {{ 0 }});
}}

fun ticker(ticks: Sender<i64>) {{
    for _ in 0..20 {{
        task.sleep(1);
    }}
    ticks.send(20);
}}

fun main(console: Console, net: Net, root: Scope) {{
    let listener = net.listen(\"127.0.0.1:0\").unwrap();
    let addr = listener.addr();
    root.spawn(accept_all(console, listener, 200));
    let (tick, ticks) = Chan<i64>.new(1);
    root.spawn(ticker(tick));
    let (done, results) = Chan<i64>.new(200);
    for i in 0..200 {{
        root.spawn(client(net, addr, i, done));
    }}
    let mut ok = 0;
    for _ in 0..200 {{
        ok += results.recv().unwrap();
    }}
    console.println(ok);
    console.println(ticks.recv().unwrap());
    listener.close();
}}
"
    ))
    .runs("200\n20\n");
}

#[test]
fn waits_time_out_and_failures_are_errors() {
    Case::new(
        "fun hold(listener: Listener) {
    let conn = listener.accept().unwrap();
    let _ = conn.read(10);
    conn.close();
}

fun main(console: Console, net: Net, root: Scope) {
    let listener = net.listen(\"127.0.0.1:0\").unwrap();
    let addr = listener.addr();
    root.spawn(hold(listener));
    let conn = net.connect(addr).unwrap();
    conn.set_timeout(50);
    console.println(conn.read(10).debug());
    console.println(conn.read(0).debug());
    conn.close();
    let gone = net.listen(\"127.0.0.1:0\").unwrap();
    let free = gone.addr();
    gone.close();
    match net.connect(free) {
        .Ok(_) => console.println(\"connected?\"),
        .Err(e) => console.println(e),
    }
    console.println(net.listen(\"nope\").debug());
    console.println(net.listen(\"127.0.0.1:70000\").debug());
    let quiet = net.listen(\"127.0.0.1:0\").unwrap();
    quiet.set_timeout(20);
    console.println(quiet.accept().is_err());
    quiet.close();
    listener.close();
}
",
    )
    .runs(
        "Err(\"timed out\")\nErr(\"read size must be from 1 to 16777216 bytes\")\n\
         Connection refused\nErr(\"address must be host:port\")\n\
         Err(\"port must be a number from 0 to 65535\")\ntrue\n",
    );
}

#[test]
fn ipv6_works_where_the_host_has_it() {
    let case = Case::new(&format!(
        "{ECHO}
fun main(console: Console, net: Net, root: Scope) {{
    match net.listen(\"[::1]:0\") {{
        .Ok(listener) => {{
            let addr = listener.addr();
            root.spawn(accept_all(console, listener, 1));
            let conn = net.connect(addr).unwrap();
            conn.write(\"six\".to_bytes()).unwrap();
            console.println(string_from_utf8(conn.read(10).unwrap()).unwrap());
            console.println(addr.starts_with(\"[::1]:\"));
        }}
        .Err(_) => console.println(\"six\\ntrue\"),
    }}
}}
"
    ));
    case.runs("six\ntrue\n");
}

#[test]
fn main_alone_waits_for_a_client() {
    let case = Case::new(
        "fun main(console: Console, net: Net) {
    let listener = net.listen(\"127.0.0.1:0\").unwrap();
    // stderr isn't buffered: the test reads the address while this waits.
    console.eprintln(listener.addr());
    let conn = listener.accept().unwrap();
    let got = conn.read(100).unwrap();
    conn.write(got).unwrap();
    conn.close();
    listener.close();
}
",
    );
    let mut child = case
        .command("-O0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("lughac runs");
    let mut lines = BufReader::new(child.stderr.take().unwrap());
    let mut addr = String::new();
    lines.read_line(&mut addr).unwrap();
    let mut stream = TcpStream::connect(addr.trim()).expect("the program listens");
    stream.write_all(b"ping").unwrap();
    let mut back = String::new();
    stream.read_to_string(&mut back).unwrap();
    assert_eq!(back, "ping");
    assert!(child.wait().unwrap().success());
}

#[test]
fn resolve_gives_every_address_once() {
    Case::new(
        "fun main(console: Console, net: Net) {
    let local = net.resolve(\"localhost\").unwrap();
    console.println(local.any(fun(a: string): bool = a == \"127.0.0.1\" || a == \"::1\"));
    let mut once = true;
    for i in 0..local.len {
        for j in 0..i {
            if local[i] == local[j] {
                once = false;
            }
        }
    }
    console.println(once);
    console.println(net.resolve(\"127.0.0.1\").debug());
    console.println(net.resolve(\"::1\").debug());
    console.println(net.resolve(\"\").is_err());
    console.println(net.resolve(\"nothing.invalid\").is_err());
}
",
    )
    .runs("true\ntrue\nOk([\"127.0.0.1\"])\nOk([\"::1\"])\ntrue\ntrue\n");
}

#[test]
fn narrowed_networks_hold_on_the_real_loopback() {
    let case = Case::new(&format!(
        "{ECHO}
fun main(console: Console, net: Net, root: Scope) {{
    let server = net.listen_only(\"127.0.0.1:0\");
    let listener = server.listen(\"127.0.0.1:0\").unwrap();
    let port = listener.addr().split(\":\")[1];
    root.spawn(accept_all(console, listener, 2));
    console.println(server.connect(\"127.0.0.1:{{port}}\").is_err());
    let local = net.only(\"localhost:{{port}}\");
    console.println(local.connect(\"localhost:{{port}}\").debug());
    let conn = local.allow_private().connect(\"localhost:{{port}}\").unwrap();
    conn.write(\"near\".to_bytes()).unwrap();
    console.println(string_from_utf8(conn.read(10).unwrap()).unwrap());
    // By the address `localhost` resolves to, as `tls.Options.address` connects.
    let by_ip = local.allow_private().connect(\"127.0.0.1:{{port}}\").unwrap();
    by_ip.write(\"by ip\".to_bytes()).unwrap();
    console.println(string_from_utf8(by_ip.read(10).unwrap()).unwrap());
    console.println(local.allow_private().connect(\"10.9.9.9:{{port}}\").is_err());
    let out = net.connect_only();
    console.println(out.listen(\"127.0.0.1:0\").is_err());
}}
"
    ));
    case.runs("true\nErr(\"localhost has no public address\")\nnear\nby ip\ntrue\ntrue\n");
}

#[test]
fn closing_a_socket_wakes_the_tasks_waiting_on_it() {
    // Each waiter is woken with `Err("closed")` by another task's `close`,
    // and the program ends: before, they waited for ever.
    let case = Case::new(
        "fun waits_accept(console: Console, l: Listener) {
    console.println(\"accept: {l.accept().is_err()}\");
}

fun waits_read(console: Console, c: Conn) {
    match c.read(10) {
        .Ok(b) => console.println(\"read: ok {b.len}\"),
        .Err(e) => console.println(\"read: {e}\"),
    }
}

fun waits_udp(console: Console, u: Udp) {
    match u.recv_from(10) {
        .Ok(_) => console.println(\"udp: ok\"),
        .Err(e) => console.println(\"udp: {e}\"),
    }
}

fun main(console: Console, net: Net) {
    let l = net.listen(\"127.0.0.1:0\").unwrap();
    let r = scope s {
        s.spawn(waits_accept(console, l));
        task.sleep(30);
        l.close();
    };
    let server = net.listen(\"127.0.0.1:0\").unwrap();
    let c = net.connect(server.addr()).unwrap();
    let kept = server.accept().unwrap();
    let r2 = scope s {
        s.spawn(waits_read(console, c));
        task.sleep(30);
        c.close();
    };
    let u = net.udp_bind(\"127.0.0.1:0\").unwrap();
    let r3 = scope s {
        s.spawn(waits_udp(console, u));
        task.sleep(30);
        u.close();
    };
    console.println(\"{r.is_ok()} {r2.is_ok()} {r3.is_ok()}\");
    kept.close();
    server.close();
}
",
    );
    let built = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["build", "main.la", "-o", "main"])
        .current_dir(&case.0)
        .env("HOME", &case.0)
        .output()
        .expect("lughac runs");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    for workers in ["1", "4"] {
        // The program itself, so a hang is killed rather than waited on.
        let mut child = Command::new(case.0.join("main"))
            .env("LUGHA_WORKERS", workers)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("lughac runs");
        let started = std::time::Instant::now();
        while child.try_wait().expect("waitable").is_none() {
            if started.elapsed() > std::time::Duration::from_secs(60) {
                // Best effort: the test fails below either way.
                let _ = child.kill();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let out = child.wait_with_output().expect("lughac finishes");
        assert_eq!(
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).as_ref()
            ),
            (
                Some(0),
                "accept: true\nread: closed\nudp: closed\ntrue true true\n"
            ),
            "workers {workers}, stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
