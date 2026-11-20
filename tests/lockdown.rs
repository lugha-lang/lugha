//! Process lockdown end to end: C code reached through `extern`, and
//! programs started through `Proc`, can do only what `main` took. Each
//! case reads `LUGHA_LOCKDOWN=report` first and skips what this kernel
//! can't apply (seccomp, or Landlock before Linux 5.13).

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder, removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Temp {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-lock-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }

    /// `lughac <args>` on `main.la` holding `src`, reporting the lockdown.
    fn run(&self, src: &str, args: &[&str]) -> Output {
        std::fs::write(self.0.join("main.la"), src).unwrap();
        Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(args)
            .arg("main.la")
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .env("LUGHA_LOCKDOWN", "report")
            .output()
            .expect("lughac runs")
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// What this kernel applies: (seccomp, Landlock), from the report line.
fn applied(out: &Output) -> (bool, bool) {
    let stderr = text(&out.stderr);
    (
        stderr.contains("seccomp applied"),
        stderr.contains("landlock applied"),
    )
}

/// The C functions the cases call: the socket, file and process calls
/// lockdown limits, and `syscall` for `io_uring_setup` (number 425 on both
/// x86-64 and ARM64).
const C: &str = "extern fun socket(domain: i32, kind: i32, protocol: i32): i32;
extern fun open(path: string, flags: i32): i32;
extern fun fork(): i32;
extern fun syscall(number: i64, a: i64, b: i64): i64;
";

#[test]
fn without_capabilities_c_code_makes_no_socket_process_or_file() {
    let t = Temp::new();
    let out = t.run(
        &format!(
            "{C}
fun main(console: Console) {{
    console.println(socket(2, 1, 0));
    console.println(fork());
    console.println(syscall(425, 1, 0));
    console.println(open(\"/etc/passwd\", 0));
}}
"
        ),
        &["run"],
    );
    let (seccomp, landlock) = applied(&out);
    if !seccomp {
        eprintln!("skipped: this kernel can't apply seccomp");
        return;
    }
    let lines: Vec<String> = text(&out.stdout).lines().map(String::from).collect();
    assert_eq!(lines[..3], ["-1", "-1", "-1"], "{}", text(&out.stderr));
    if landlock {
        assert_eq!(lines[3], "-1");
    }
}

#[test]
fn with_the_capabilities_c_code_can() {
    let t = Temp::new();
    let out = t.run(
        &format!(
            "{C}
fun main(console: Console, net: Net, fs: Fs) {{
    console.println(socket(2, 1, 0) >= 0);
    console.println(open(\"/etc/passwd\", 0) >= 0);
    console.println(syscall(425, 1, 0));
}}
"
        ),
        &["run"],
    );
    let stdout = text(&out.stdout);
    assert!(stdout.starts_with("true\ntrue\n"), "{stdout}");
    // `io_uring` stays refused even with every capability.
    if applied(&out).0 {
        assert!(stdout.ends_with("-1\n"), "{stdout}");
    }
    assert!(
        text(&out.stderr).contains("net held, proc refused, fs held"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn net_and_clock_still_read_what_they_need_without_fs() {
    let t = Temp::new();
    let out = t.run(
        &format!(
            "{C}
fun main(console: Console, net: Net, clock: Clock) {{
    console.println(net.resolve(\"localhost\").is_ok());
    console.println(clock.local_offset(0).is_ok());
    console.println(open(\"/etc/hosts\", 0) >= 0);
    console.println(open(\"/etc/passwd\", 0));
}}
"
        ),
        &["run"],
    );
    let stdout = text(&out.stdout);
    assert!(
        stdout.starts_with("true\ntrue\ntrue\n"),
        "{stdout}{}",
        text(&out.stderr)
    );
    if applied(&out).1 {
        assert!(stdout.ends_with("-1\n"), "{stdout}");
    }
}

#[test]
fn started_programs_share_the_limits() {
    let t = Temp::new();
    let probe = t.0.join("probe");
    let out = t.run(
        &format!(
            "fun main(console: Console, proc: Proc) {{
    let read = proc.run(\"cat\", [\"/etc/hostname\"]).unwrap();
    console.println(read.code);
    let write = proc.run(\"sh\", [\"-c\", \"echo x > {} && echo wrote || echo refused\"]).unwrap();
    console.println(write.stdout.trim());
    match proc.run(\"python3\", [\"-c\", \"import socket; socket.socket()\"]) {{
        .Ok(py) => console.println(py.code != 0),
        .Err(_) => console.println(true),
    }}
}}
",
            probe.display()
        ),
        &["run"],
    );
    let (seccomp, landlock) = applied(&out);
    let lines: Vec<String> = text(&out.stdout).lines().map(String::from).collect();
    assert_eq!(lines.len(), 3, "{}{}", text(&out.stdout), text(&out.stderr));
    if landlock {
        assert_eq!(lines[1], "refused");
        assert!(!probe.exists());
    }
    if seccomp {
        assert_eq!(lines[2], "true", "a started program made a socket");
    }
}

#[test]
fn a_unix_socket_pair_needs_no_net() {
    let t = Temp::new();
    let out = t.run(
        "fun main(console: Console, proc: Proc) {
    match proc.run(\"python3\", [\"-c\", \"import socket; socket.socketpair()\"]) {
        .Ok(py) => console.println(py.code),
        .Err(_) => console.println(\"no python3\"),
    }
    match proc.run(\"python3\", [\"-c\", \"import socket; socket.socket()\"]) {
        .Ok(py) => console.println(py.code != 0),
        .Err(_) => console.println(\"no python3\"),
    }
    // Tools that start their children through a socket pair: uutils'
    // `timeout`, and anything on Rust's `std::process`.
    match proc.run(\"timeout\", [\"5\", \"true\"]) {
        .Ok(t) => console.println(t.code),
        .Err(e) => console.println(e),
    }
}
",
        &["run"],
    );
    if !applied(&out).0 {
        eprintln!("skipped: this kernel can't apply seccomp");
        return;
    }
    let lines: Vec<String> = text(&out.stdout).lines().map(String::from).collect();
    assert_eq!(lines.len(), 3, "{}{}", text(&out.stdout), text(&out.stderr));
    if lines[0] != "no python3" {
        assert_eq!(lines[0], "0", "an AF_UNIX socket pair was refused");
        assert_eq!(lines[1], "true", "a socket was made without Net");
    }
    assert_eq!(lines[2], "0", "{}", text(&out.stderr));
}

#[test]
fn a_plain_test_writes_only_in_its_folder() {
    let t = Temp::new();
    let probe = t.run("fun main(console: Console) {}\n", &["run"]);
    if !applied(&probe).1 {
        eprintln!("skipped: this kernel has no Landlock");
        return;
    }
    let outside = t.0.join("outside.txt");
    std::fs::write(
        t.0.join("main.la"),
        format!(
            "{C}
@test
fun stays_in(dir: Dir) {{
    dir.write_file(\"in.txt\", \"x\").unwrap();
    // O_WRONLY | O_CREAT, outside the test's folder.
    assert(open(\"{}\", 65) < 0);
}}
",
            outside.display()
        ),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["test", "main.la"])
        .current_dir(&t.0)
        .env("HOME", &t.0)
        .output()
        .expect("lughac runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
    assert!(!outside.exists());
}
