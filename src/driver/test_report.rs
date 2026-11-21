//! How `lughac test` reports a finished test: a line of text, with the
//! failing test's panic and output, or under `--json` one JSON object; a
//! shuffled run's failures carry its seed, which repeats them.

use std::time::Duration;

use super::test::{Job, Outcome};
use super::{test_cycles, test_json};

/// The text report of a finished test that started: `ok`, `skipped`, or
/// `FAILED` with what it printed.
pub(super) fn line(job: &Job, outcome: &Outcome, timeout: Duration, seed: Option<u64>) -> String {
    let name = &job.name;
    let mut text = match outcome {
        Outcome::Passed(_) => format!("test {name} ... ok\n"),
        Outcome::Skipped(reason) => format!("test {name} ... skipped ({reason})\n"),
        Outcome::TimedOut => {
            let why = format!("timed out after {} s", timeout.as_secs());
            format!("test {name} ... FAILED ({})\n", with_seed(Some(why), seed))
        }
        Outcome::Failed {
            code,
            stdout,
            stderr,
            ..
        } => failure(name, *code, (stdout, stderr), seed),
        Outcome::NotStarted(why) => format!("test {name} ... not started ({why})\n"),
    };
    if let Some(cycles) = cycles_of(outcome) {
        text.push_str(&test_cycles::note(cycles));
    }
    text
}

/// The cycles a finished test left for the collector, if any.
fn cycles_of(outcome: &Outcome) -> Option<&test_cycles::Cycles> {
    match outcome {
        Outcome::Passed(cycles) | Outcome::Failed { cycles, .. } => cycles.as_ref(),
        _ => None,
    }
}

/// `why`, then the shuffle's seed, as the parenthesis after `FAILED` says them.
fn with_seed(why: Option<String>, seed: Option<u64>) -> String {
    match (why, seed) {
        (Some(why), Some(seed)) => format!("{why}, seed {seed}"),
        (Some(why), None) => why,
        (None, Some(seed)) => format!("seed {seed}"),
        (None, None) => String::new(),
    }
}

/// The report of a failed test: its panic (or exit code), then its output.
fn failure(
    name: &str,
    code: Option<i32>,
    (stdout, stderr): (&str, &str),
    seed: Option<u64>,
) -> String {
    let why = match code {
        Some(101) | None => None,
        Some(code) => Some(format!("exit code {code}")),
    };
    let mut text = match with_seed(why, seed) {
        why if why.is_empty() => format!("test {name} ... FAILED\n"),
        why => format!("test {name} ... FAILED ({why})\n"),
    };
    for line in stderr.lines() {
        text.push_str(&format!("    {line}\n"));
    }
    if !stdout.is_empty() {
        text.push_str("    --- output ---\n");
        for line in stdout.lines() {
            text.push_str(&format!("    {line}\n"));
        }
    }
    text
}

/// The `--json` line for a finished test; `None` for one that never started.
pub(super) fn json_result(
    job: &Job,
    outcome: &Outcome,
    (took, timeout): (Duration, Duration),
    seed: Option<u64>,
) -> Option<String> {
    let (status, message, at, output) = match outcome {
        Outcome::Passed(_) => ("passed", None, None, ""),
        Outcome::Skipped(reason) => ("skipped", Some(reason.clone()), None, ""),
        Outcome::TimedOut => {
            let message = format!("timed out after {} s", timeout.as_secs());
            ("timed_out", Some(message), None, "")
        }
        Outcome::Failed {
            code,
            stdout,
            stderr,
            ..
        } => match test_json::panic_of(stderr) {
            Some(panic) => ("failed", Some(panic.message), panic.at, stdout.as_str()),
            None => {
                let message = code.map_or("killed".to_string(), |c| format!("exit code {c}"));
                ("failed", Some(message), None, stdout.as_str())
            }
        },
        Outcome::NotStarted(_) => return None,
    };
    Some(test_json::result_line(&test_json::Result {
        name: &job.name,
        file: &job.file,
        line: job.line,
        status,
        message,
        at,
        output,
        ms: took.as_millis(),
        seed,
        cycles: cycles_of(outcome).map(test_cycles::json),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_shows_the_panic_then_the_output() {
        assert_eq!(
            failure("m.t", Some(101), ("hi\n", "panic: no at m.la:1:1\n"), None),
            "test m.t ... FAILED\n    panic: no at m.la:1:1\n    --- output ---\n    hi\n"
        );
        assert_eq!(
            failure("m.t", Some(3), ("", ""), None),
            "test m.t ... FAILED (exit code 3)\n"
        );
    }

    #[test]
    fn a_shuffled_failure_names_its_seed() {
        assert_eq!(
            failure("m.t", Some(101), ("", "panic: no\n"), Some(7)),
            "test m.t ... FAILED (seed 7)\n    panic: no\n"
        );
        assert_eq!(
            failure("m.t", Some(3), ("", ""), Some(7)),
            "test m.t ... FAILED (exit code 3, seed 7)\n"
        );
    }
}
