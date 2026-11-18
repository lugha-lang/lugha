//! `lughac test`'s scheduling modes: plain tests on virtual time, which
//! jumps to the next timer once every task waits, integration tests on real
//! time, and `--shuffle=SEED` picking ready tasks from a seed, so a race
//! shows up and repeats.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A temporary folder holding `t.la`, removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn with(source: &str) -> Temp {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-modes-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("writable");
        std::fs::write(dir.join("t.la"), source).expect("writable");
        Temp(dir)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `lughac test` in `dir` with `args`: what it printed, and how long the
/// tests took to run (building excluded, measured from a second run).
fn test(dir: &Path, args: &[&str]) -> (String, Duration) {
    let run = || {
        let started = Instant::now();
        let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
            .arg("test")
            .args(args)
            .current_dir(dir)
            .env("HOME", dir)
            .output()
            .expect("lughac runs");
        let text = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        (text, started.elapsed())
    };
    // The first run builds and caches; the second times the tests.
    run();
    run()
}

const TIMED: &str = "import std.task;

@test
fun sleeps_three_minutes() {
    for i in 0..3 {
        task.sleep(60_000);
    }
}

@test
fun deadlines_and_after_fire() {
    let r = scope s within 5_000 {
        task.sleep(60_000);
    };
    assert(r.is_err());
    let (tx, rx) = Chan<i64>.new(0);
    let got = select {
        m = rx.recv() => 1,
        after(30_000) => 2,
    };
    assert_eq(got, 2);
}
";

#[test]
fn plain_tests_run_on_virtual_time() {
    let t = Temp::with(TIMED);
    let (out, took) = test(&t.0, &[]);
    assert!(out.contains("2 passed, 0 failed"), "{out}");
    assert!(
        took < Duration::from_secs(30),
        "minutes of timers took {took:?}"
    );
}

#[test]
fn integration_tests_keep_real_time() {
    let t = Temp::with(
        "import std.task;\n\n@test(integration)\nfun waits() {\n    task.sleep(1_500);\n}\n",
    );
    let (out, took) = test(&t.0, &["--integration"]);
    assert!(out.contains("1 passed, 0 failed"), "{out}");
    assert!(
        took >= Duration::from_millis(1_500),
        "a real sleep took {took:?}"
    );
}

const RACE: &str = "@test
fun first_writer_wins() {
    let (tx, rx) = Chan<string>.new(2);
    let r = scope s {
        s.spawn(tx.send(\"a\"));
        s.spawn(tx.send(\"b\"));
    };
    assert_eq(rx.recv(), Some(\"a\"));
}
";

#[test]
fn a_shuffle_finds_an_order_dependence_and_repeats_it() {
    let t = Temp::with(RACE);
    let (plain, _) = test(&t.0, &[]);
    assert!(plain.contains("1 passed"), "{plain}");
    // Some seed in the first sixteen breaks the FIFO assumption.
    let failing = (1..=16)
        .find(|seed| {
            let (out, _) = test(&t.0, &[&format!("--shuffle={seed}")]);
            out.contains("FAILED")
        })
        .expect("a seed that reorders the writers");
    let seed = format!("--shuffle={failing}");
    let (once, _) = test(&t.0, &[&seed]);
    let (again, _) = test(&t.0, &[&seed]);
    assert!(once.contains(&format!("FAILED (seed {failing})")), "{once}");
    assert_eq!(once, again);
}

#[test]
fn a_shuffle_without_a_seed_draws_and_prints_one() {
    let t = Temp::with(RACE);
    let (out, _) = test(&t.0, &["--shuffle"]);
    assert!(out.starts_with("shuffle seed "), "{out}");
    let (json, _) = test(&t.0, &["--shuffle=3", "--json"]);
    assert!(json.contains("\"seed\":3"), "{json}");
}
