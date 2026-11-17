//! The reference-counting prototype against the end-to-end programs: every
//! program under `tests/programs/` that runs, built with `--memory=rc`, must
//! print what it prints under the collector, exit the same way, free
//! everything it made when it ends normally, and never touch freed memory
//! (`LUGHA_RC_POISON=1` turns that into a panic). Programs lughac refuses
//! under it (C callbacks) are skipped.

mod support;

use std::path::Path;

use support::runner::{
    Case, DEFAULT_TIMEOUT, Mismatch, Mode, Outcome, compare, discover, format_malformed,
    format_report, run_case,
};

/// Programs that don't pass yet, and why: each is a known gap of the
/// prototype, not a regression.
const KNOWN: &[(&str, &str)] = &[
    // `Ref` cycles: the cycle collector comes with the runtime's phase.
    (
        "types/shared_boxes/linked.la",
        "a doubly linked list of boxes is a cycle",
    ),
    // A panicking actor's stack isn't unwound: what its frames held stays.
    (
        "concurrency/actors/supervisor.la",
        "a panic doesn't unwind yet",
    ),
    // A value returned from inside `for … of mut` through a type's `index`
    // doesn't see the element handed back after it was made.
    (
        "interfaces/read_modify_write/loops.la",
        "the hand-back after a return",
    ),
    // At -O0 a frame holds more slots than under the collector.
    (
        "runtime/slot_sharing/popover_depth.la",
        "an -O0 frame is larger",
    ),
    (
        "runtime/small_frames/deep_match.la",
        "an -O0 frame is larger",
    ),
];

#[test]
fn programs_run_the_same_under_reference_counting() {
    let workdir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lughac = Path::new(env!("CARGO_BIN_EXE_lughac"));
    let root = workdir.join("tests/programs");
    let cases = match discover(&root) {
        Ok(cases) => cases,
        Err(malformed) => panic!("\n{}", format_malformed(&malformed)),
    };
    let cases: Vec<Case> = cases
        .into_iter()
        .filter(|case| matches!(case.mode, Mode::Run { .. }))
        .filter(|case| !KNOWN.iter().any(|(name, _)| case.name == *name))
        .map(counted)
        .collect();
    let failures: Vec<_> = cases
        .iter()
        .map(|case| {
            let outcome = run_case(lughac, workdir, case, DEFAULT_TIMEOUT);
            let (outcome, report) = split_report(outcome);
            // Refused for what a module it imports uses: skipped.
            if refused(&outcome) {
                return (case.name.clone(), Vec::new());
            }
            let mut mismatches = compare(case, &outcome);
            mismatches.extend(leaks(case, report.as_deref()));
            (case.name.clone(), mismatches)
        })
        .filter(|(_, mismatches)| !mismatches.is_empty())
        .collect();
    if !failures.is_empty() {
        panic!("\n{}", format_report(&failures, cases.len()));
    }
}

#[test]
fn freed_memory_is_reused_without_poisoning() {
    let workdir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lughac = Path::new(env!("CARGO_BIN_EXE_lughac"));
    let cases = match discover(&workdir.join("tests/programs/runtime/rc_alloc")) {
        Ok(cases) => cases,
        Err(malformed) => panic!("\n{}", format_malformed(&malformed)),
    };
    assert!(!cases.is_empty(), "the allocator's programs are there");
    let failures: Vec<_> = cases
        .into_iter()
        .map(|mut case| {
            // Without poisoning freed blocks go back on their lists, so
            // later objects reuse them.
            for (key, value) in [("LUGHA_MEMORY", "rc"), ("LUGHA_RC_REPORT", "1")] {
                case.extras
                    .envvars
                    .push((key.to_string(), value.to_string()));
            }
            let outcome = run_case(lughac, workdir, &case, DEFAULT_TIMEOUT);
            let (outcome, report) = split_report(outcome);
            let mut mismatches = compare(&case, &outcome);
            mismatches.extend(leaks(&case, report.as_deref()));
            (case.name.clone(), mismatches)
        })
        .filter(|(_, mismatches)| !mismatches.is_empty())
        .collect();
    if !failures.is_empty() {
        panic!("\n{}", format_report(&failures, failures.len()));
    }
}

#[test]
fn every_known_gap_is_a_program_that_exists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    for (name, why) in KNOWN {
        assert!(
            root.join(name).exists(),
            "{name} ({why}) is gone: drop it from KNOWN"
        );
    }
}

/// `case` run under reference counting, poisoned, with the report at exit.
fn counted(mut case: Case) -> Case {
    for (key, value) in [
        ("LUGHA_MEMORY", "rc"),
        ("LUGHA_RC_POISON", "1"),
        ("LUGHA_RC_REPORT", "1"),
    ] {
        case.extras
            .envvars
            .push((key.to_string(), value.to_string()));
    }
    case
}

/// True if lughac refused the program for what the prototype doesn't
/// cover (a task in an imported module, say).
fn refused(outcome: &Outcome) -> bool {
    matches!(outcome, Outcome::Exited { stderr, code: Some(2), .. }
        if String::from_utf8_lossy(stderr).contains("--memory=rc doesn't support"))
}

/// The outcome without the report's line on stderr, and that line.
fn split_report(outcome: Outcome) -> (Outcome, Option<String>) {
    let Outcome::Exited {
        stdout,
        stderr,
        code,
    } = outcome
    else {
        return (outcome, None);
    };
    let text = String::from_utf8_lossy(&stderr).into_owned();
    let report = text
        .lines()
        .find(|l| l.starts_with("rc: "))
        .map(str::to_string);
    let rest: String = text
        .split_inclusive('\n')
        .filter(|l| !l.starts_with("rc: "))
        .collect();
    let outcome = Outcome::Exited {
        stdout,
        stderr: rest.into_bytes(),
        code,
    };
    (outcome, report)
}

/// A program ending normally must have freed everything: its report says
/// `0 live`. One that panics ends with what it held.
fn leaks(case: &Case, report: Option<&str>) -> Vec<Mismatch> {
    let Mode::Run { exit: 0, .. } = case.mode else {
        return Vec::new();
    };
    match report {
        Some(line) if line.ends_with(" 0 live") => Vec::new(),
        Some(line) => vec![problem(format!("leaked: {line}"))],
        None => vec![problem("no report at exit".to_string())],
    }
}

/// A one-line problem.
fn problem(what: String) -> Mismatch {
    Mismatch { what, values: None }
}
