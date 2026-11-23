//! Shared by the tests that build for other platforms
//! (`tests/arm.rs`, `tests/musl.rs`): whether their tools are here. On a
//! developer's machine a missing tool skips the test with a note; with
//! `LUGHA_CROSS_TESTS=required` (set in CI) it fails it, so those tests
//! never pass by not running.

use std::path::Path;
use std::process::Command;

/// Whether every tool in `tools` is here (a program that runs, or a
/// folder, written as an absolute path); when one isn't, prints that
/// `test` is skipped and returns false, or panics when required.
pub fn ready(test: &str, tools: &[&str]) -> bool {
    let missing: Vec<&str> = tools.iter().copied().filter(|t| !present(t)).collect();
    let required = std::env::var("LUGHA_CROSS_TESTS").as_deref() == Ok("required");
    decide(test, &missing, required)
}

/// What [`ready`] does once it knows what's `missing`.
pub fn decide(test: &str, missing: &[&str], required: bool) -> bool {
    if missing.is_empty() {
        return true;
    }
    let needs = missing.join(" and ");
    assert!(
        !required,
        "{test}: needs {needs} (LUGHA_CROSS_TESTS=required)"
    );
    eprintln!("{test}: skipped, needs {needs}");
    false
}

/// Whether `tool` is here: a folder for an absolute path, else a program
/// that answers `version` (zig) or `--version`.
fn present(tool: &str) -> bool {
    if tool.starts_with('/') {
        return Path::new(tool).is_dir();
    }
    let arg = if tool == "zig" {
        "version"
    } else {
        "--version"
    };
    Command::new(tool)
        .arg(arg)
        .output()
        .is_ok_and(|o| o.status.success())
}
