//! `std.net`'s UDP end to end: Lugha programs on loopback sending and
//! receiving datagrams, timeouts that park one green thread, oversized
//! datagrams refused, shared ports, multicast groups joined and left, and
//! the machine's network interfaces.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Runs `main` (importing `std.net`) and returns its stdout, expecting exit 0.
fn run(main: &str) -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir: PathBuf = std::env::temp_dir().join(format!("lugha-udp-{}-{n}", std::process::id()));
    // A leftover from an earlier run would change what the test sees.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.la"),
        format!("import std.net;\nimport std.task;\n\n{main}"),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["run", "main.la"])
        .current_dir(&dir)
        .env("HOME", &dir)
        .output()
        .expect("lughac runs");
    // Best effort: a leftover temp dir must not mask the real failure.
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn datagrams_go_both_ways_and_name_their_sender() {
    let out = run("fun main() {
    let a = net.udp_bind(\"127.0.0.1:0\").unwrap();
    let b = net.udp_bind(\"127.0.0.1:0\").unwrap();
    a.set_timeout(2000);
    b.set_timeout(2000);
    a.send_to(\"ping\".to_bytes(), b.addr()).unwrap();
    let (got, from) = b.recv_from(100).unwrap();
    println(string_from_utf8(got).unwrap());
    println(from == a.addr());
    b.send_to(\"pong\".to_bytes(), from).unwrap();
    let (back, _) = a.recv_from(100).unwrap();
    println(string_from_utf8(back).unwrap());
    // An empty datagram is a datagram.
    a.send_to([], b.addr()).unwrap();
    println(b.recv_from(100).unwrap().0.len);
    a.close();
    b.close();
}");
    assert_eq!(out, "ping\ntrue\npong\n0\n");
}

#[test]
fn ipv6_loopback_works_when_the_machine_has_it() {
    let out = run("fun main() {
    match net.udp_bind(\"[::1]:0\") {
        .Ok(a) => {
            let b = net.udp_bind(\"[::1]:0\").unwrap();
            b.set_timeout(2000);
            a.send_to(\"six\".to_bytes(), b.addr()).unwrap();
            let (got, from) = b.recv_from(100).unwrap();
            println(string_from_utf8(got).unwrap());
            println(from.starts_with(\"[::1]:\"));
        }
        .Err(_) => println(\"no ipv6\"),
    }
}");
    assert!(out == "six\ntrue\n" || out == "no ipv6\n", "stdout: {out}");
}

#[test]
fn a_timeout_parks_only_its_green_thread() {
    let out = run("fun tick(c: Chan<i64>) {
    c.send(1);
}

fun main() {
    let u = net.udp_bind(\"127.0.0.1:0\").unwrap();
    u.set_timeout(200);
    let c: Chan<i64> = Chan.new();
    spawn tick(c);
    println(u.recv_from(100).debug());
    println(c.recv().debug());
}");
    assert_eq!(out, "Err(\"timed out\")\nSome(1)\n");
}

#[test]
fn bad_input_is_refused() {
    let out = run("fun main() {
    let a = net.udp_bind(\"127.0.0.1:0\").unwrap();
    let b = net.udp_bind(\"127.0.0.1:0\").unwrap();
    b.set_timeout(2000);
    a.send_to([7 as u8; 100], b.addr()).unwrap();
    println(b.recv_from(10).debug());
    println(a.send_to([], \"localhost:9\").debug());
    println(a.send_to([], \"127.0.0.1\").debug());
    println(b.recv_from(0).debug());
    println(b.recv_from(65537).debug());
    // A port taken without sharing can't be bound again.
    println(net.udp_bind(a.addr()).is_err());
    a.close();
    println((a.send_to([], b.addr())).debug());
    println(a.recv_from(10).debug());
    println(a.addr());
}");
    assert_eq!(
        out,
        "Err(\"datagram of 100 bytes is longer than max\")\n\
         Err(\"address must be an IP address with a port: localhost:9\")\n\
         Err(\"address must be an IP address with a port: 127.0.0.1\")\n\
         Err(\"max must be from 1 to 65536\")\n\
         Err(\"max must be from 1 to 65536\")\n\
         true\nErr(\"closed\")\nErr(\"closed\")\n\n"
    );
}

#[test]
fn shared_sockets_hear_a_multicast_group_until_they_leave() {
    let out = run("fun main() {
    let a = net.udp_bind_shared(\"0.0.0.0:0\").unwrap();
    let port = a.addr().split(\":\")[1];
    let b = net.udp_bind_shared(\"0.0.0.0:{port}\").unwrap();
    println(b.addr() == a.addr());
    let group = \"239.255.0.1\";
    if a.join(group, \"\").is_err() || b.join(group, \"\").is_err() {
        println(\"no multicast\");
        return;
    }
    let s = net.udp_bind(\"0.0.0.0:0\").unwrap();
    s.set_multicast_loop(true).unwrap();
    s.set_multicast_ttl(1).unwrap();
    a.set_timeout(2000);
    b.set_timeout(300);
    s.send_to(\"hello\".to_bytes(), \"{group}:{port}\").unwrap();
    println(string_from_utf8(a.recv_from(100).unwrap().0).unwrap());
    println(string_from_utf8(b.recv_from(100).unwrap().0).unwrap());
    b.leave(group, \"\").unwrap();
    s.send_to(\"again\".to_bytes(), \"{group}:{port}\").unwrap();
    println(string_from_utf8(a.recv_from(100).unwrap().0).unwrap());
    println(b.recv_from(100).debug());
    // Groups of the wrong family or that aren't multicast.
    println(a.join(\"ff02::fb\", \"\").is_err());
    println(a.join(\"10.0.0.1\", \"\").is_err());
    println(a.join(group, \"no-such-interface\").is_err());
    println(s.set_multicast_ttl(0).is_err());
}");
    if out == "true\nno multicast\n" {
        eprintln!("skipped: no multicast-capable interface");
        return;
    }
    assert_eq!(
        out,
        "true\nhello\nhello\nagain\nErr(\"timed out\")\ntrue\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn interfaces_include_loopback() {
    let out = run("fun main() {
    let all = net.interfaces().unwrap();
    for i of all {
        if i.name == \"lo\" {
            println(i.up);
            println(i.index >= 1);
            let mut has = false;
            for a of i.addresses {
                has = has || a == \"127.0.0.1\";
            }
            println(has);
        }
    }
    let mut names: Set<string> = #{};
    let mut once = true;
    for i of all {
        once = once && names.insert(i.name);
    }
    println(once);
}");
    assert_eq!(out, "true\ntrue\ntrue\ntrue\n");
}
