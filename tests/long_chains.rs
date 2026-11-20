//! Long chains of values are freed without recursing, on any stack: each
//! case under `tests/programs/runtime/long_chains/`, and the unwinding one
//! that walks a long list recursively, run again under a 256 MiB address
//! space (`prlimit --as`), where every task's stack shrinks to 1 MiB, and
//! must print and exit as it does without the limit.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The address space each program gets: small enough that task stacks
/// shrink to their smallest.
const ADDRESS_SPACE: &str = "--as=268435456";

/// Every case this file runs, as paths from the repository's root.
fn cases(root: &Path) -> Vec<PathBuf> {
    let dir = root.join("tests/programs/runtime/long_chains");
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("the long_chains cases exist")
        .map(|entry| entry.expect("a readable entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "la"))
        .collect();
    cases.push(root.join("tests/programs/control/unwinding/list_overflow.la"));
    cases.sort();
    cases
}

/// A file beside the case with the given extension, or `""` when absent.
fn expected(case: &Path, extension: &str) -> String {
    std::fs::read_to_string(case.with_extension(extension)).unwrap_or_default()
}

#[test]
fn long_chains_free_on_small_stacks() {
    if Command::new("prlimit").arg("--version").output().is_err() {
        eprintln!("skipped: no prlimit");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = std::env::temp_dir().join(format!("lugha-chains-{}", std::process::id()));
    // A leftover from an earlier run would change nothing, but keeps it tidy.
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("a temporary folder");
    let mut failures = Vec::new();
    for case in cases(root) {
        let name = case
            .file_stem()
            .expect("a file name")
            .to_string_lossy()
            .into_owned();
        let program = out_dir.join(&name);
        let built = Command::new(env!("CARGO_BIN_EXE_lughac"))
            .arg("build")
            .arg(&case)
            .arg("-o")
            .arg(&program)
            .output()
            .expect("lughac runs");
        assert!(
            built.status.success(),
            "{name} builds: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        let ran = Command::new("prlimit")
            .arg(ADDRESS_SPACE)
            .arg("--")
            .arg(&program)
            .env("LUGHA_WORKERS", "1")
            .output()
            .expect("prlimit runs");
        let code = ran
            .status
            .code()
            .map_or("signal".to_string(), |c| c.to_string());
        let got = (
            code,
            String::from_utf8_lossy(&ran.stdout).into_owned(),
            String::from_utf8_lossy(&ran.stderr).into_owned(),
        );
        let want = (
            expected(&case, "exit").trim().to_string(),
            expected(&case, "stdout"),
            expected(&case, "stderr"),
        );
        if got != want {
            failures.push(format!("{name}: got {got:?}, want {want:?}"));
        }
    }
    let _ = std::fs::remove_dir_all(&out_dir);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
