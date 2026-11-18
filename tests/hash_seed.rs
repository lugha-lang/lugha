//! Hash seeding: maps and sets hash with SipHash-1-3 under a key the
//! runtime draws for each process, fixed in a deterministic run and taken
//! from the seed in a shuffled one.

#[path = "common/cli.rs"]
mod cli;

use std::process::Command;

use cli::{TempDir, lughac, text};

/// Prints the hash of an `i64`, a short string and a longer one.
const HASHES: &str = "fun hashed<T: Key>(x: T): u64 {
    let mut h = Hasher.new();
    h.add(x);
    h.finish()
}

fun main(console: Console) {
    console.println(hashed(7));
    console.println(hashed(\"abc\"));
    console.println(hashed(\"hello, world!\"));
}
";

/// What the program printed, run with `LUGHA_SCHED` set to `mode` (or unset).
fn run(dir: &TempDir, mode: Option<&str>) -> String {
    let mut command = Command::new(dir.0.join("hashes"));
    if let Some(mode) = mode {
        command.env("LUGHA_SCHED", mode);
    }
    let out = command.output().expect("the program runs");
    assert!(out.status.success(), "{}", text(&out.stderr));
    text(&out.stdout)
}

fn built() -> TempDir {
    let dir = TempDir::with(&[("hashes.la", HASHES.as_bytes())]);
    let out = lughac(&dir.0, &["build", "hashes.la", "-o", "hashes"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    dir
}

#[test]
fn a_deterministic_run_gives_siphash_1_3_s_values() {
    // SipHash-1-3 under the deterministic key (splitmix64 of 0 and of 1),
    // from an independent implementation checked against SipHash-2-4's
    // published vector. A value feeds one block per `u64` word; a string
    // its bytes eight at a time, zero-padded, then its length.
    let dir = built();
    assert_eq!(
        run(&dir, Some("deterministic")),
        "776243587299223169\n13147443531794176667\n2673017165298350117\n"
    );
}

#[test]
fn each_run_draws_its_own_key_and_a_seed_repeats() {
    let dir = built();
    assert_ne!(run(&dir, None), run(&dir, None));
    let seeded = run(&dir, Some("shuffle:42"));
    assert_eq!(seeded, run(&dir, Some("shuffle:42")));
    assert_ne!(seeded, run(&dir, Some("shuffle:43")));
}
