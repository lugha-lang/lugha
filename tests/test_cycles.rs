//! `lughac test`'s note for cycles: a test whose run left `Ref` cycles
//! for the collector gets a note with where their boxes were made, in text
//! and in `--json`; a test without cycles gets none, and the note is never
//! a failure.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder holding `t.la`, removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn with(source: &str) -> Temp {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-tcyc-{}-{n}", std::process::id()));
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

/// `lughac test t.la` in `dir` with `args`: its stdout and exit code.
fn test(dir: &Path, args: &[&str]) -> (String, Option<i32>) {
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .arg("test")
        .arg("t.la")
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .output()
        .expect("lughac runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code(),
    )
}

const SOURCE: &str = "struct Node { name: string, next: Option<Ref<Node>> }

struct Tree { name: string, parent: Option<Weak<Tree>> }

fun pair() {
    let a = Ref.new(Node { name: \"a\", next: None });
    let b = Ref.new(Node { name: \"b\", next: Some(a) });
    a.update(fun(n) = Node { name: n.name, next: Some(b) });
}

@test
fun leaves_cycles() {
    for i in 0..200 {
        pair();
    }
}

@test
fun links_weakly() {
    let root = Ref.new(Tree { name: \"root\", parent: None });
    let kid = Ref.new(Tree { name: \"kid\", parent: Some(Weak.new(root)) });
    assert(kid.get().parent.is_some());
    assert(root.get().name == \"root\");
}

@test
fun fails_after_cycles() {
    for i in 0..5000 {
        pair();
    }
    assert(false);
}
";

#[test]
fn a_test_that_left_cycles_gets_a_note_with_their_sites() {
    let dir = Temp::with(SOURCE);
    let (out, code) = test(&dir.0, &["--filter", "leaves_cycles"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains(
            "test t.leaves_cycles ... ok\n  note: 200 cycles freed (400 boxes), made at t.la:6:13, t.la:7:13; a `Weak` back-link avoids them\n"
        ),
        "{out}"
    );
}

#[test]
fn a_test_without_cycles_gets_no_note() {
    let dir = Temp::with(SOURCE);
    let (out, code) = test(&dir.0, &["--filter", "links_weakly"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("test t.links_weakly ... ok\n"), "{out}");
    assert!(!out.contains("note:"), "{out}");
}

#[test]
fn a_failing_test_shows_its_failure_and_the_note() {
    let dir = Temp::with(SOURCE);
    let (out, code) = test(&dir.0, &["--filter", "fails_after_cycles"]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("test t.fails_after_cycles ... FAILED\n"),
        "{out}"
    );
    assert!(
        out.contains("panic: assertion failed at t.la:31:5"),
        "{out}"
    );
    assert!(out.contains("  note: "), "{out}");
}

#[test]
fn the_json_report_carries_the_cycles() {
    let dir = Temp::with(SOURCE);
    let (out, _) = test(&dir.0, &["--json"]);
    let line = |name: &str| {
        out.lines()
            .find(|l| l.contains(&format!("\"test\":\"t.{name}\"")))
            .unwrap_or_else(|| panic!("no {name} line in {out}"))
            .to_string()
    };
    assert!(
        line("leaves_cycles").contains(
            "\"cycles\":{\"count\":200,\"boxes\":400,\"sites\":[\"t.la:6:13\",\"t.la:7:13\"]}"
        ),
        "{out}"
    );
    assert!(!line("links_weakly").contains("cycles"), "{out}");
}
