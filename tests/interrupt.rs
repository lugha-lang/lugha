//! Ctrl-C: a running program sent SIGINT. The first cancels the root
//! scope's tasks and `main`, whose `defer`s run, and the program exits with
//! 130; a second exits at once; a program that never started a task exits
//! at once.

#[path = "common/cli.rs"]
mod cli;

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use cli::{TempDir, lughac, text};

/// Tasks waiting when Ctrl-C comes: each unwinds, running its `defer`s.
const WAITING: &str = r#"import std.task;

fun worker(console: Console) {
    defer console.println("worker: stopped");
    task.sleep(60_000);
}

fun main(console: Console, root: Scope) {
    defer console.println("main: stopped");
    root.spawn(worker(console));
    console.println("ready");
    console.flush();
    task.sleep(60_000);
}
"#;

/// A cleanup holding cancellation off for a minute: only a second Ctrl-C
/// ends it.
const STUBBORN: &str = r#"import std.task;

fun main(console: Console) {
    defer {
        shield within 60_000 {
            console.println("cleaning");
            console.flush();
            task.sleep(60_000);
        }
    };
    console.println("ready");
    console.flush();
    task.sleep(60_000);
}
"#;

/// A busy `main` that never started a task: no scheduler to stop it.
const NO_TASKS: &str = r#"fun main(console: Console) {
    console.println("ready");
    console.flush();
    let mut i = 0;
    while i >= 0 {
        i = (i + 1) % 1_000;
    }
}
"#;

/// Builds `source` and starts it, its stdout piped, once it printed `ready`.
fn started(dir: &TempDir, source: &str) -> (Child, BufReader<std::process::ChildStdout>) {
    std::fs::write(dir.0.join("main.la"), source).expect("temp dir is writable");
    let built = lughac(&dir.0, &["build", "main.la", "-o", "main"]);
    assert!(built.status.success(), "{}", text(&built.stderr));
    let mut child = Command::new(dir.0.join("main"))
        .stdout(Stdio::piped())
        .spawn()
        .expect("the program starts");
    let mut out = BufReader::new(child.stdout.take().expect("piped"));
    let mut line = String::new();
    out.read_line(&mut line).expect("stdout is readable");
    assert_eq!(line, "ready\n");
    (child, out)
}

/// Sends SIGINT to `child`.
fn interrupt(child: &Child) {
    let sent = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(sent.success());
}

/// Waits up to ten seconds for `child` to end: its exit code and the rest of its stdout.
fn ended(mut child: Child, mut out: BufReader<std::process::ChildStdout>) -> (Option<i32>, String) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("the program can be waited for") {
            break status;
        }
        if Instant::now() > deadline {
            // Best effort: the assertion below reports the hang.
            let _ = child.kill();
            panic!("the program didn't end after Ctrl-C");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut rest = String::new();
    out.read_to_string(&mut rest).expect("stdout is readable");
    (status.code(), rest)
}

#[test]
fn ctrl_c_unwinds_every_task_and_exits_with_130() {
    let dir = TempDir::with(&[]);
    let (child, out) = started(&dir, WAITING);
    interrupt(&child);
    let (code, rest) = ended(child, out);
    assert_eq!(code, Some(130));
    let mut lines: Vec<_> = rest.lines().collect();
    lines.sort();
    assert_eq!(lines, ["main: stopped", "worker: stopped"]);
}

#[test]
fn a_second_ctrl_c_exits_at_once() {
    let dir = TempDir::with(&[]);
    let (child, mut out) = started(&dir, STUBBORN);
    interrupt(&child);
    let mut line = String::new();
    out.read_line(&mut line).expect("stdout is readable");
    assert_eq!(line, "cleaning\n");
    interrupt(&child);
    let (code, _) = ended(child, out);
    assert_eq!(code, Some(130));
}

#[test]
fn a_program_without_tasks_exits_at_once() {
    let dir = TempDir::with(&[]);
    let (child, out) = started(&dir, NO_TASKS);
    interrupt(&child);
    let (code, rest) = ended(child, out);
    assert_eq!((code, rest.as_str()), (Some(130), ""));
}
