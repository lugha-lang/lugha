//! `lughac test [paths…] [--filter TEXT] [--timeout SECS]`: runs a
//! program's `@test` functions.
//!
//! Each file holding `@test` functions is built, with its imports, into one
//! test executable; each test then runs as its own process of it, named by
//! `LUGHA_TEST`, so a panic fails only that test. Tests run in parallel, one
//! per CPU, each killed after the time limit. The executables are started
//! directly (no shell) from a fresh temporary folder, removed afterwards.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::codegen::{Cpu, Target};

use super::Output;
use super::emulate::{self, Runner};
use super::fmt::{display, la_files};
use super::pipeline::{Failure, TempDir};
use super::test_cycles::{self, Cycles};
use super::{test_build, test_json, test_report};
use crate::lexer::{self, TokenKind};

/// What `lughac test` was asked to run.
pub(super) struct Request<'a> {
    pub paths: &'a [PathBuf],
    /// `--filter`s: a test runs when any matches (all, when none is given).
    pub filters: &'a [String],
    /// `--exact`: a filter must equal the whole name.
    pub exact: bool,
    /// `--integration`: run the `@test(integration)` tests too.
    pub integration: bool,
    pub timeout: Duration,
    /// `--list`: name the tests, build nothing.
    pub list: bool,
    /// `--json`: one JSON object per line, for tools.
    pub json: bool,
    /// `--cpu`: the level the test executables are built for.
    pub cpu: Cpu,
    /// `--target`: the platform they are built for, run under qemu for another CPU.
    pub target: Target,
    /// `--shuffle`: plain tests pick ready tasks at random from this seed.
    pub shuffle: Option<u64>,
}

impl Request<'_> {
    /// Whether the test `name` is one to run or list.
    fn wants(&self, name: &str) -> bool {
        self.filters.is_empty()
            || self.filters.iter().any(|f| {
                if self.exact {
                    name == f
                } else {
                    name.contains(f.as_str())
                }
            })
    }
}

/// One test to run: its name as reported, its file's executable, where
/// it is declared, whether it is an integration test, and the fresh folder
/// it is given (made just before it runs, removed after).
pub(super) struct Job {
    pub name: String,
    test: String,
    exe: PathBuf,
    pub file: String,
    pub line: usize,
    integration: bool,
    folder: PathBuf,
    /// How its executable starts: itself, or under qemu.
    runner: Runner,
}

/// How one test ended.
pub(super) enum Outcome {
    /// Exited with 0, with the cycles it left for the collector, if any.
    Passed(Option<Cycles>),
    /// Exited with this code (101 for a panic), with its output and the
    /// cycles it left.
    Failed {
        code: Option<i32>,
        stdout: String,
        stderr: String,
        cycles: Option<Cycles>,
    },
    TimedOut,
    /// Ended through `skip(reason)`.
    Skipped(String),
    /// Couldn't be started at all: lughac's own failure.
    NotStarted(String),
}

/// Runs `lughac test`; returns the exit code.
pub(super) fn test(request: &Request, out: &mut Output) -> u8 {
    let here = [PathBuf::from(".")];
    let paths = if request.paths.is_empty() {
        &here[..]
    } else {
        request.paths
    };
    let mut code = 0;
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            if let Err(error) = la_files(path, &mut files) {
                out.error(&format!("cannot read `{}`: {error}", path.display()));
                code = 2;
            }
        } else {
            files.push(path.clone());
        }
    }
    if request.list {
        return code.max(list(request, &files, out));
    }
    // Before building: a missing emulator is found in a moment.
    let runner = match emulate::runner(request.target) {
        Ok(runner) => runner,
        Err(message) => {
            out.error(&message);
            return 2;
        }
    };
    let temp = match TempDir::new() {
        Ok(temp) => temp,
        Err(error) => {
            out.error(&format!("cannot create a temporary directory: {error}"));
            return 2;
        }
    };
    // The files to build: those holding tests, and with filters only those
    // holding a test they select, found by parsing.
    let mut selected: Vec<PathBuf> = Vec::new();
    for file in &files {
        // `./geo.la` from walking `.` reads, and reports, as `geo.la`.
        let file = file.strip_prefix(".").unwrap_or(file);
        match std::fs::read_to_string(file) {
            Ok(text) if !has_tests(&text) || !selects(request, file, &text) => continue,
            Ok(_) => selected.push(file.to_path_buf()),
            Err(error) => {
                out.error(&format!("cannot read `{}`: {error}", display(file)));
                code = 2;
            }
        }
    }
    let mut jobs = Vec::new();
    // Files with errors: their tests never run, so the summary counts them.
    let mut uncompiled = 0;
    for (g, members) in test_build::groups(&selected).iter().enumerate() {
        let exe = temp.path.join(format!("test{g}"));
        let result =
            test_build::build_group(&selected, members, &exe, (request.cpu, request.target));
        for (failure, files) in result.failures {
            uncompiled += files;
            match failure {
                Failure::Program(source, diagnostics) => {
                    out.diagnostics(&source, &diagnostics);
                    code = code.max(1);
                }
                Failure::Internal(source, report) => {
                    out.report(&source, &report);
                    code = 2;
                }
            }
        }
        for (i, tests) in result.built {
            let file = &selected[i];
            let module = test_json::module_name(file);
            jobs.extend(tests.into_iter().map(|(test, line, _, integration)| {
                let job = Job {
                    name: format!("{module}.{}", test_json::bare(&test)),
                    test,
                    exe: exe.clone(),
                    file: display(file),
                    line,
                    integration,
                    folder: PathBuf::new(),
                    runner: runner.clone(),
                };
                (i, job)
            }));
        }
    }
    // In file order, whatever the groups: a stable sort keeps declaration order.
    jobs.sort_by_key(|(i, _)| *i);
    let mut jobs: Vec<Job> = jobs.into_iter().map(|(_, job)| job).collect();
    jobs.retain(|j| request.wants(&j.name));
    for (n, job) in jobs.iter_mut().enumerate() {
        job.folder = temp.path.join(format!("case{n}"));
    }
    let (mut passed, mut failed, mut skipped) = (0, 0, 0);
    // `--json` reports each test as it finishes, for an editor's live view.
    // A drawn seed is said first, so the run can be repeated.
    if let Some(seed) = request.shuffle
        && !request.json
    {
        out.print(&format!("shuffle seed {seed}\n"));
    }
    let outcomes = run_all(
        &jobs,
        (request.timeout, request.integration, request.shuffle),
        |job, outcome, took| {
            if request.json
                && let Some(line) =
                    test_report::json_result(job, outcome, (took, request.timeout), request.shuffle)
            {
                out.print(&line);
            }
        },
    );
    for (job, outcome) in jobs.iter().zip(&outcomes) {
        match outcome {
            Outcome::Passed(_) => passed += 1,
            Outcome::Skipped(_) => skipped += 1,
            Outcome::NotStarted(why) => {
                out.error(&format!("cannot run test `{}`: {why}", job.name));
                code = 2;
                continue;
            }
            _ => failed += 1,
        }
        if !request.json {
            out.print(&test_report::line(
                job,
                outcome,
                request.timeout,
                request.shuffle,
            ));
        }
    }
    let summary = match request.json {
        true => test_json::summary_line(passed, failed, skipped, uncompiled),
        false => test_json::summary_text(passed, failed, skipped, uncompiled),
    };
    out.print(&summary);
    if failed > 0 {
        code = code.max(1);
    }
    code
}

/// A seed for `--shuffle` given none: from the clock, then printed.
pub(super) fn draw_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64 % 1_000_000)
}

/// Whether `file` holds a test `request` wants: with no filters every
/// file does; a file that doesn't parse is kept, so its errors show.
fn selects(request: &Request, file: &Path, text: &str) -> bool {
    if request.filters.is_empty() {
        return true;
    }
    let module = test_json::module_name(file);
    test_json::listed(text).is_none_or(|tests| {
        tests
            .iter()
            .any(|(test, _, _)| request.wants(&format!("{module}.{test}")))
    })
}

/// Whether `text` holds `@test`, as tokens; a file that doesn't lex but
/// mentions it is built anyway, so its errors are reported.
fn has_tests(text: &str) -> bool {
    match lexer::lex(text) {
        Ok((tokens, _)) => tokens
            .windows(2)
            .any(|w| w[0].kind == TokenKind::At && w[1].kind == TokenKind::Ident("test".into())),
        Err(_) => text.contains("@test"),
    }
}

/// `--list`: every test of `files` that `request` wants, by parsing alone;
/// files that don't parse are skipped (their errors show elsewhere).
fn list(request: &Request, files: &[PathBuf], out: &mut Output) -> u8 {
    let mut code = 0;
    for file in files {
        let file = file.strip_prefix(".").unwrap_or(file);
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) => {
                out.error(&format!("cannot read `{}`: {error}", display(file)));
                code = 2;
                continue;
            }
        };
        let module = test_json::module_name(file);
        for (test, line, col) in test_json::listed(&text).unwrap_or_default() {
            let name = format!("{module}.{test}");
            if request.wants(&name) {
                out.print(&test_json::list_line(
                    request.json,
                    &name,
                    &display(file),
                    line,
                    col,
                ));
            }
        }
    }
    code
}

/// Runs every job, up to one per CPU at a time, telling `done` about each
/// as it finishes (on this thread); the outcomes in job order.
fn run_all(
    jobs: &[Job],
    (timeout, integration, shuffle): (Duration, bool, Option<u64>),
    mut done: impl FnMut(&Job, &Outcome, Duration),
) -> Vec<Outcome> {
    let workers = thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(jobs.len().max(1));
    let next = AtomicUsize::new(0);
    let mut outcomes: Vec<Option<Outcome>> = (0..jobs.len()).map(|_| None).collect();
    let (send, receive) = mpsc::channel();
    thread::scope(|scope| {
        for _ in 0..workers {
            let send = send.clone();
            let next = &next;
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(i) else {
                        break;
                    };
                    let started = Instant::now();
                    let outcome = match job.integration && !integration {
                        true => Outcome::Skipped("integration: run with --integration".to_string()),
                        false => run_one(job, timeout, shuffle),
                    };
                    // The receiver lives until every result is in.
                    let _ = send.send((i, outcome, started.elapsed()));
                }
            });
        }
        drop(send);
        for (i, outcome, took) in receive {
            done(&jobs[i], &outcome, took);
            outcomes[i] = Some(outcome);
        }
    });
    outcomes
        .into_iter()
        .map(|o| o.unwrap_or(Outcome::NotStarted("never ran".to_string())))
        .collect()
}

/// Runs one test as its own process, capturing what it prints, with its
/// fresh folder (`LUGHA_TEST_DIR`), removed afterwards. A plain test runs in
/// that folder; an integration test where `lughac test` was started.
fn run_one(job: &Job, timeout: Duration, shuffle: Option<u64>) -> Outcome {
    if let Err(error) = std::fs::create_dir(&job.folder) {
        return Outcome::NotStarted(format!("cannot make the test's folder: {error}"));
    }
    let mut command = job.runner.command(&job.exe);
    command
        .env("LUGHA_TEST", &job.test)
        .env("LUGHA_TEST_DIR", &job.folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A plain test runs deterministic, on virtual time; an integration
    // test, given the real world, on its time.
    if !job.integration {
        command.current_dir(&job.folder);
        let mode = shuffle.map_or("deterministic".to_string(), |s| format!("shuffle:{s}"));
        command.env("LUGHA_SCHED", mode);
    }
    let outcome = run_child(command.spawn(), timeout);
    // Best effort: the run's temporary folder, removed at the end, holds it too.
    let _ = std::fs::remove_dir_all(&job.folder);
    outcome
}

/// Waits for a started test, reading its output, and says how it ended.
fn run_child(child: std::io::Result<Child>, timeout: Duration) -> Outcome {
    let mut child = match child {
        Ok(child) => child,
        Err(error) => return Outcome::NotStarted(error.to_string()),
    };
    // Readers on their own threads, so a test printing a lot never blocks.
    let stdout = reader(child.stdout.take());
    let stderr = reader(child.stderr.take());
    let status = wait(&mut child, timeout);
    let stdout = stdout.join().unwrap_or_default();
    let mut stderr = stderr.join().unwrap_or_default();
    let cycles = test_cycles::take(&mut stderr);
    match status {
        None => Outcome::TimedOut,
        Some(Ok(status)) if status.success() => Outcome::Passed(cycles),
        Some(Ok(status)) if let Some(reason) = test_json::skipped(status.code(), &stderr) => {
            Outcome::Skipped(reason)
        }
        Some(Ok(status)) => Outcome::Failed {
            code: status.code(),
            stdout,
            stderr,
            cycles,
        },
        Some(Err(error)) => Outcome::NotStarted(error.to_string()),
    }
}

fn reader(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut text = Vec::new();
        if let Some(mut pipe) = pipe {
            // A read error only loses output that is shown on failure anyway.
            let _ = pipe.read_to_end(&mut text);
        }
        String::from_utf8_lossy(&text).into_owned()
    })
}

/// Waits for `child` up to `timeout`; `None` when it was killed for it.
fn wait(child: &mut Child, timeout: Duration) -> Option<std::io::Result<std::process::ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(Ok(status)),
            Ok(None) if Instant::now() >= deadline => {
                // Already gone or not ours to kill: either way it's over.
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(error) => return Some(Err(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_are_found_by_tokens() {
        assert!(has_tests("@test fun t() {}"));
        assert!(!has_tests("// @test\nfun t() {}"));
        assert!(!has_tests(
            "fun main(console: Console) { console.println(\"@test\"); }"
        ));
        // Doesn't lex, mentions it: built, so the error is reported.
        assert!(has_tests("@test fun t() { ` }"));
    }
}
