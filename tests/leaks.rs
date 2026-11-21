//! Every end-to-end program under `tests/programs/` that runs, with freed
//! objects kept as tombstones (`LUGHA_RC_POISON=1`, so touching one panics)
//! and the count report at exit: it must print what it always prints, exit
//! the same way, and have freed everything it made, panicking or not: a
//! panic unwinds its task, letting go of what each frame held.

mod support;

use std::path::Path;

use support::runner::{
    Case, DEFAULT_TIMEOUT, Mismatch, Mode, Outcome, compare, discover, format_malformed,
    format_report, run_case,
};

/// Programs that end with objects still live, and why: each a known gap,
/// not a regression.
const KNOWN: &[(&str, &str)] = &[
    // A panic while unwinding ends the program at once.
    (
        "control/unwinding/in_defer.la",
        "a panic in a defer ends the program unwound halfway",
    ),
    (
        "concurrency/deadlines/shield_value_lost.la",
        "a shield's value lost while unwinding ends the program unwound halfway",
    ),
    // A panic in `main` ends the program with its root scope's tasks
    // (here an actor's) still held.
    (
        "concurrency/actors/after_stop.la",
        "`main` panics while the root scope holds the actor's task",
    ),
    // A deadlock ends the program with every task still waiting.
    (
        "concurrency/actors/deadlock.la",
        "the waiting tasks aren't unwound",
    ),
    (
        "concurrency/green_threads/deadlock.la",
        "the waiting tasks aren't unwound",
    ),
    (
        "concurrency/channels/deadlock.la",
        "the waiting tasks aren't unwound",
    ),
    // A fault in C ends the program from a signal handler: nothing
    // unwinds, and no report is printed.
    (
        "collections/growable_arrays/segv_from_c.la",
        "a fault in C ends the program from its signal handler",
    ),
];

#[test]
fn programs_free_everything_they_make() {
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
            case.extras
                .envvars
                .push(("LUGHA_RC_REPORT".to_string(), "1".to_string()));
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

/// `case` run poisoned, with the report at exit; a case that expects the
/// report itself expects the rest of its stderr here.
fn counted(mut case: Case) -> Case {
    for (key, value) in [("LUGHA_RC_POISON", "1"), ("LUGHA_RC_REPORT", "1")] {
        case.extras
            .envvars
            .push((key.to_string(), value.to_string()));
    }
    if let Mode::Run {
        stderr: Some(expected),
        ..
    } = &mut case.mode
    {
        *expected = String::from_utf8_lossy(expected)
            .split_inclusive('\n')
            .filter(|l| !l.starts_with("rc: "))
            .collect::<String>()
            .into_bytes();
    }
    case
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

/// A program must have freed everything: its report says `0 live`.
fn leaks(case: &Case, report: Option<&str>) -> Vec<Mismatch> {
    let Mode::Run { .. } = case.mode else {
        return Vec::new();
    };
    match report {
        // The collector's count, if any, follows a `;`.
        Some(line)
            if line
                .split(';')
                .next()
                .is_some_and(|l| l.ends_with(" 0 live")) =>
        {
            Vec::new()
        }
        Some(line) => vec![problem(format!("leaked: {line}"))],
        None => vec![problem("no report at exit".to_string())],
    }
}

/// A one-line problem.
fn problem(what: String) -> Mismatch {
    Mismatch { what, values: None }
}
