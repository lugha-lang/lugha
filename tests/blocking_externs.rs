//! `extern blocking fun`: a C function run on a helper thread while its
//! task waits. Each case is a program with its own C file, linked through a
//! `[link]` table.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A temporary folder holding one program, removed on drop.
struct Case(PathBuf);

impl Case {
    /// `main.la` and `side.c`, linked by `[link] c = ["side.c"]`.
    fn new(main: &str, side: &str) -> Case {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-blk-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.la"), main).unwrap();
        std::fs::write(dir.join("side.c"), side).unwrap();
        std::fs::write(dir.join("lugha.toml"), "[link]\nc = [\"side.c\"]\n").unwrap();
        Case(dir)
    }

    /// `lughac run main.la` with `opt`, and `LUGHA_SCHED` set to `mode`.
    fn run(&self, opt: &str, mode: Option<&str>) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lughac"));
        command
            .args(["run", "main.la", opt])
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .env_remove("LUGHA_SCHED");
        if let Some(mode) = mode {
            command.env("LUGHA_SCHED", mode);
        }
        command.output().expect("lughac runs")
    }

    /// Runs the program at -O0 and -O2 and checks both print `stdout` and exit 0.
    fn runs(&self, stdout: &str) {
        for opt in ["-O0", "-O2"] {
            let out = self.run(opt, None);
            assert_eq!(
                (out.status.code(), text(&out.stdout).as_str()),
                (Some(0), stdout),
                "{opt}, stderr: {}",
                text(&out.stderr)
            );
        }
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A C function that sleeps `ms` milliseconds, then gives `x` back.
const SLOW: &str = "#include <time.h>\n\
    long slow(long x, long ms) {\n\
        struct timespec t = {ms / 1000, (ms % 1000) * 1000000};\n\
        nanosleep(&t, 0);\n\
        return x;\n\
    }\n";

/// Ticks three times, 100 ms apart, while a task calls `slow(7, 600)`.
const TICKING: &str = "import std.task;\n\
    extern blocking fun slow(x: i64, ms: i64): i64;\n\
    fun main(console: Console) {\n\
        let r = scope s {\n\
            s.spawn(fun() { console.println(\"row {slow(7, 600)}\"); }());\n\
            for i in 0..3 {\n\
                task.sleep(100);\n\
                console.println(\"tick {i}\");\n\
            }\n\
        };\n\
        r.unwrap();\n\
    }\n";

#[test]
fn other_tasks_run_while_a_blocking_c_function_waits() {
    Case::new(TICKING, SLOW).runs("tick 0\ntick 1\ntick 2\nrow 7\n");
}

#[test]
fn a_deterministic_run_calls_it_in_place() {
    let case = Case::new(TICKING, SLOW);
    let out = case.run("-O0", Some("deterministic"));
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "row 7\ntick 0\ntick 1\ntick 2\n");
}

#[test]
fn strings_and_out_parameters_cross_as_in_a_plain_call() {
    Case::new(
        "extern blocking fun measure(s: string, out n: i64): i64;\n\
         fun main(console: Console) {\n\
             let mut n = 0;\n\
             let doubled = measure(\"hello\", out n);\n\
             console.println(\"{n} {doubled}\");\n\
         }\n",
        "#include <string.h>\n\
         long measure(const char *s, long *n) { *n = (long)strlen(s); return 2 * *n; }\n",
    )
    .runs("5 10\n");
}

#[test]
fn a_deadline_waits_for_the_call_to_return() {
    let case = Case::new(
        "extern blocking fun slow(x: i64, ms: i64): i64;\n\
         fun main(console: Console) {\n\
             let r = scope s within 50 {\n\
                 slow(1, 400)\n\
             };\n\
             console.println(r.is_ok());\n\
         }\n",
        SLOW,
    );
    let started = Instant::now();
    let out = case.run("-O0", None);
    let took = started.elapsed();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    // Cancelled at the deadline, noticed once the call has returned.
    assert_eq!(text(&out.stdout), "false\n");
    assert!(took >= Duration::from_millis(400), "{took:?}");
}

#[test]
fn a_callback_run_during_a_blocking_call_ends_the_program() {
    let case = Case::new(
        "extern fun keep(f: fun(i64): i64);\n\
         extern blocking fun call_kept(x: i64): i64;\n\
         fun main(console: Console) {\n\
             keep(fun(x) = x + 1);\n\
             console.println(call_kept(1));\n\
         }\n",
        "static long (*kept)(long);\n\
         void keep(long (*f)(long)) { kept = f; }\n\
         long call_kept(long x) { return kept(x); }\n",
    );
    let out = case.run("-O0", None);
    assert_eq!(out.status.code(), Some(101), "{}", text(&out.stdout));
    assert!(
        text(&out.stderr).contains(
            "panic: a C callback ran on a helper thread, during a `blocking` extern call"
        ),
        "{}",
        text(&out.stderr)
    );
}
