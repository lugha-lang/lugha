//! Helper threads: the runtime's blocking work (files, reading input,
//! starting programs) runs on a helper thread while its task waits, so the
//! program's other tasks keep running; a deterministic run keeps it inline.

#[path = "common/cli.rs"]
mod cli;

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use cli::{TempDir, lughac, text};

/// `source` built as `prog` in a fresh folder holding `files` too.
fn built(source: &str, files: &[(&str, &[u8])]) -> TempDir {
    let mut all = vec![("prog.la", source.as_bytes())];
    all.extend_from_slice(files);
    let dir = TempDir::with(&all);
    let out = lughac(&dir.0, &["build", "prog.la", "-o", "prog"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    dir
}

fn program(dir: &TempDir) -> Command {
    let mut command = Command::new(dir.0.join("prog"));
    command.current_dir(&dir.0).env_remove("LUGHA_SCHED");
    command
}

fn ran(dir: &TempDir) -> Output {
    program(dir).output().expect("the program runs")
}

/// Three ticks, 100 ms apart, while `work` (a task) waits on something slow.
fn ticking(params: &str, work: &str) -> String {
    format!(
        "import std.task;

fun main({params}) {{
    let r = scope s {{
        s.spawn(fun() {{
            {work}
        }}());
        for i in 0..3 {{
            task.sleep(100);
            console.println(\"tick {{i}}\");
        }}
    }};
    r.unwrap();
}}
"
    )
}

#[test]
fn other_tasks_run_while_a_started_program_is_waited_for() {
    let source = ticking(
        "console: Console, proc: Proc",
        "proc.run(\"sleep\", [\"0.6\"]).unwrap();\n            console.println(\"slept\");",
    );
    let dir = built(&source, &[]);
    let out = ran(&dir);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "tick 0\ntick 1\ntick 2\nslept\n");
}

#[test]
fn a_deterministic_run_keeps_the_work_inline_and_repeats() {
    let source = ticking(
        "console: Console, proc: Proc",
        "proc.run(\"sleep\", [\"0.2\"]).unwrap();\n            console.println(\"slept\");",
    );
    let dir = built(&source, &[]);
    let runs: Vec<String> = (0..2)
        .map(|_| {
            let out = program(&dir)
                .env("LUGHA_SCHED", "deterministic")
                .output()
                .expect("the program runs");
            assert!(out.status.success(), "{}", text(&out.stderr));
            text(&out.stdout)
        })
        .collect();
    // Nothing else runs while the program is waited for, and no virtual
    // time passes, so `slept` comes first, every time.
    assert_eq!(runs[0], "slept\ntick 0\ntick 1\ntick 2\n");
    assert_eq!(runs[0], runs[1]);
}

#[test]
fn other_tasks_run_while_input_is_awaited() {
    let source = ticking(
        "console: Console",
        "console.println(\"got {console.read_line().unwrap()}\");",
    );
    let dir = built(&source, &[]);
    let mut child = program(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the program starts");
    let mut stdin = child.stdin.take().expect("piped");
    thread::sleep(Duration::from_millis(700));
    stdin.write_all(b"hello\n").expect("the program reads");
    drop(stdin);
    let out = child.wait_with_output().expect("the program ends");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "tick 0\ntick 1\ntick 2\ngot hello\n");
}

#[test]
fn other_tasks_run_while_a_fifo_is_opened_and_read() {
    let source = ticking(
        "console: Console, fs: Fs",
        "let dir = fs.cwd();
            let file = dir.open(\"pipe\").unwrap();
            let bytes = file.read(100).unwrap();
            console.println(\"read {bytes.len}\");",
    );
    let dir = built(&source, &[]);
    let made = Command::new("mkfifo")
        .arg(dir.0.join("pipe"))
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    let fifo = dir.0.join("pipe");
    let writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(700));
        // Opening blocks until the program opens its end.
        let mut end = std::fs::OpenOptions::new()
            .write(true)
            .open(fifo)
            .expect("opens");
        end.write_all(b"data").expect("writes");
    });
    let out = ran(&dir);
    writer.join().expect("the writer ends");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "tick 0\ntick 1\ntick 2\nread 4\n");
}

#[test]
fn a_task_waiting_on_a_helper_is_cancelled_at_its_deadline() {
    let source = "fun main(console: Console, proc: Proc) {
    let r = scope s within 100 {
        proc.run(\"sleep\", [\"3\"])
    };
    console.println(r.is_ok());
}
";
    let dir = built(source, &[]);
    let started = Instant::now();
    let out = ran(&dir);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "false\n");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "waited for the program: {:?}",
        started.elapsed()
    );
}

#[test]
fn many_tasks_reading_files_all_succeed() {
    let source = "fun main(console: Console, fs: Fs) {
    let dir = fs.cwd();
    let (tx, rx) = Chan<i64>.new(300);
    let r = scope s {
        for i in 0..300 {
            s.spawn(fun() {
                tx.send(dir.read_file(\"data.txt\").unwrap().len);
            }());
        }
    };
    tx.close();
    let mut total = 0;
    for n of rx {
        total += n;
    }
    console.println(\"{r.is_ok()} {total}\");
}
";
    let dir = built(source, &[("data.txt", b"abc")]);
    let out = ran(&dir);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "true 900\n");
}

#[test]
fn the_number_of_helpers_is_checked_at_start() {
    let dir = built("fun main() {}\n", &[]);
    for bad in ["0", "257", "x", ""] {
        let out = program(&dir)
            .env("LUGHA_HELPERS", bad)
            .output()
            .expect("the program runs");
        assert_eq!(out.status.code(), Some(101), "LUGHA_HELPERS={bad}");
        assert_eq!(
            text(&out.stderr),
            "panic: LUGHA_HELPERS must be from 1 to 256\n"
        );
    }
    let out = program(&dir)
        .env("LUGHA_HELPERS", "1")
        .output()
        .expect("the program runs");
    assert!(out.status.success(), "{}", text(&out.stderr));
}

#[test]
fn one_helper_still_serves_every_task() {
    let source = "fun main(console: Console, fs: Fs) {
    let dir = fs.cwd();
    let r = scope s {
        for i in 0..20 {
            s.spawn(dir.read_file(\"data.txt\").unwrap());
        }
    };
    console.println(\"done {r.is_ok()}\");
}
";
    let dir = built(source, &[("data.txt", b"abc")]);
    let out = program(&dir)
        .env("LUGHA_HELPERS", "1")
        .output()
        .expect("the program runs");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "done true\n");
}
