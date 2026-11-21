//! The cycle collector under the runtime's other switches: the cycle
//! programs (`tests/programs/runtime/cycles/`) list where their collected
//! boxes were made, pass the owner check, find no use after free when
//! poisoned, and collect at the same points in every deterministic run.

mod support;

use std::path::Path;

use support::runner::{
    Case, DEFAULT_TIMEOUT, Mode, Outcome, compare, discover, format_malformed, format_report,
    run_case,
};

/// The cycle programs, each with `env` added to its environment.
fn cases(env: &[(&str, &str)]) -> Vec<Case> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/runtime/cycles");
    let cases = match discover(&root) {
        Ok(cases) => cases,
        Err(malformed) => panic!("\n{}", format_malformed(&malformed)),
    };
    assert!(!cases.is_empty(), "the cycle programs are there");
    cases
        .into_iter()
        .map(|mut case| {
            for (key, value) in env {
                case.extras
                    .envvars
                    .push((key.to_string(), value.to_string()));
            }
            case
        })
        .collect()
}

/// What `case` printed to stderr and stdout, run once.
fn run(case: &Case) -> (String, String) {
    let workdir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lughac = Path::new(env!("CARGO_BIN_EXE_lughac"));
    match run_case(lughac, workdir, case, DEFAULT_TIMEOUT) {
        Outcome::Exited { stdout, stderr, .. } => (
            String::from_utf8_lossy(&stderr).into_owned(),
            String::from_utf8_lossy(&stdout).into_owned(),
        ),
        other => panic!("{}: {other:?}", case.name),
    }
}

/// Every case, run with `env`, prints and exits as it always does.
fn all_pass(env: &[(&str, &str)]) {
    let workdir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lughac = Path::new(env!("CARGO_BIN_EXE_lughac"));
    let cases = cases(env);
    let failures: Vec<_> = cases
        .iter()
        .map(|case| {
            let outcome = run_case(lughac, workdir, case, DEFAULT_TIMEOUT);
            (case.name.clone(), compare(case, &outcome))
        })
        .filter(|(_, mismatches)| !mismatches.is_empty())
        .collect();
    if !failures.is_empty() {
        panic!("\n{}", format_report(&failures, cases.len()));
    }
}

#[test]
fn the_detailed_report_lists_where_collected_boxes_were_made() {
    let case = cases(&[("LUGHA_RC_REPORT", "2")])
        .into_iter()
        .find(|c| c.name.ends_with("two_boxes.la"))
        .expect("two_boxes.la is a cycle program");
    let (stderr, _) = run(&case);
    assert!(
        stderr.contains("two_boxes.la:6:13: 1000 freed\n"),
        "{stderr}"
    );
    assert!(
        stderr.contains("two_boxes.la:7:13: 1000 freed\n"),
        "{stderr}"
    );
}

#[test]
fn the_plain_report_lists_no_sites() {
    let case = cases(&[])
        .into_iter()
        .find(|c| c.name.ends_with("two_boxes.la"))
        .expect("two_boxes.la is a cycle program");
    let (stderr, _) = run(&case);
    assert!(!stderr.contains("made at"), "{stderr}");
}

#[test]
fn cycle_programs_pass_the_owner_check() {
    all_pass(&[("LUGHA_RC_CHECK", "1")]);
}

#[test]
fn poisoned_cycle_programs_touch_nothing_freed() {
    all_pass(&[("LUGHA_RC_POISON", "1")]);
}

#[test]
fn deterministic_runs_collect_at_the_same_points() {
    for case in cases(&[("LUGHA_SCHED", "deterministic")]) {
        if !matches!(case.mode, Mode::Run { .. }) {
            continue;
        }
        let first = run(&case);
        let second = run(&case);
        assert_eq!(first, second, "{}", case.name);
    }
}
