//! The whole program suite for a Mac: every run case in `tests/programs/`
//! built for `aarch64-apple-darwin` here, on Linux, into a folder of plain
//! files that `tests/macos/suite.sh` runs on a Mac (CI's `macos` job).
//!
//! The building test is ignored: CI runs it with `--ignored` and
//! `LUGHA_MACOS_SUITE=<folder>`. `LUGHA_MACOS_SUITE_TARGET` builds for
//! another target instead, so the script can be tried on Linux.
//!
//! A case folder holds `name`, `exe`, `stdout`, `exit` and, when the case
//! has them, `stderr`, `args`, `stdin`, `envvars` and `scratch`. Cases a Mac
//! build refuses (E0334, E0335) are listed in `skipped.txt` with why.

// Only `discover` and `Case` are used; the rest of the runner runs `lughac`.
#[allow(dead_code, unused_imports)]
mod support;

#[path = "common/cross.rs"]
mod cross;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use support::runner::{Case, Mode, discover};

const MAC: &str = "aarch64-apple-darwin";

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn cases() -> Vec<Case> {
    match discover(&root().join("tests/programs")) {
        Ok(cases) => cases,
        Err(bad) => panic!("malformed cases: {bad:?}"),
    }
}

/// The files of `case`'s folder but `exe`, given what building it printed
/// on stderr: warnings `lughac run` prints before the program's own
/// output, so they are taken off the front of an expected stderr.
fn case_files(case: &Case, built: &[u8]) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let Mode::Run {
        stdout,
        stderr,
        exit,
    } = &case.mode
    else {
        return Err("a rejected program isn't run".into());
    };
    let mut files = vec![
        ("name", case.name.clone().into_bytes()),
        ("stdout", stdout.clone()),
        ("exit", format!("{exit}\n").into_bytes()),
    ];
    // Without a .stderr, stderr isn't compared, as on Linux.
    if let Some(want) = stderr {
        match want.strip_prefix(built) {
            Some(rest) => files.push(("stderr", rest.to_vec())),
            None => return Err("its build printed what its .stderr doesn't start with".into()),
        }
    }
    let extras = &case.extras;
    if !extras.args.is_empty() {
        files.push(("args", lines(extras.args.iter().cloned())));
    }
    if let Some(stdin) = &extras.stdin {
        files.push(("stdin", stdin.clone()));
    }
    if !extras.envvars.is_empty() {
        let vars = extras.envvars.iter().map(|(k, v)| format!("{k}={v}"));
        files.push(("envvars", lines(vars)));
    }
    if extras.scratch {
        files.push(("scratch", Vec::new()));
    }
    Ok(files)
}

fn lines(items: impl Iterator<Item = String>) -> Vec<u8> {
    items
        .map(|line| line + "\n")
        .collect::<String>()
        .into_bytes()
}

/// Why a Mac build of a case is refused rather than failed: a library
/// built for this machine that a foreign build can't link.
fn refused(stderr: &str) -> Option<&'static str> {
    if stderr.contains("E0334") {
        Some("E0334: std.tls, std.http and std.crypto need this machine's OpenSSL")
    } else if stderr.contains("E0335") {
        Some("E0335: a [link] pkg-config line names this machine's libraries")
    } else {
        None
    }
}

/// What became of one case.
enum Built {
    Ready,
    Skipped(String),
    Failed(String),
}

/// Builds `case` into `dir`, from where `lughac run` would build it.
fn build(case: &Case, dir: &Path, target: &str) -> Built {
    let arg = match case.extras.scratch {
        true => case.path.clone(),
        false => case
            .path
            .strip_prefix(root())
            .map_or(case.path.clone(), Path::to_path_buf),
    };
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .arg("build")
        .arg(&arg)
        .args(["--target", target, "-o"])
        .arg(dir.join("exe"))
        .current_dir(root())
        .env_remove("LUGHA_CC")
        .output()
        .expect("lughac runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        return match refused(&stderr) {
            Some(why) => Built::Skipped(why.into()),
            None => Built::Failed(stderr.into_owned()),
        };
    }
    let files = match case_files(case, &out.stderr) {
        Ok(files) => files,
        Err(why) => return Built::Failed(why),
    };
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).expect("the suite folder is writable");
    }
    Built::Ready
}

#[test]
#[ignore = "CI builds the suite for its Mac: LUGHA_MACOS_SUITE=<folder>"]
fn build_the_suite_for_a_mac() {
    let out = PathBuf::from(std::env::var_os("LUGHA_MACOS_SUITE").expect("LUGHA_MACOS_SUITE"));
    let target = std::env::var("LUGHA_MACOS_SUITE_TARGET").unwrap_or_else(|_| MAC.into());
    if target == MAC && !cross::ready("macos_suite", &["zig"]) {
        return;
    }
    let cases: Vec<Case> = cases()
        .into_iter()
        .filter(|c| matches!(c.mode, Mode::Run { .. }))
        .collect();
    let next = AtomicUsize::new(0);
    let skipped = Mutex::new(Vec::new());
    let failed = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(case) = cases.get(i) else { break };
                    // Numbered from 1, padded, so the folders sort by name.
                    let dir = out.join(format!("{:04}", i + 1));
                    std::fs::create_dir_all(&dir).expect("the suite folder is writable");
                    match build(case, &dir, &target) {
                        Built::Ready => {}
                        Built::Skipped(why) => {
                            // Nothing to run: the folder goes.
                            std::fs::remove_dir_all(&dir).expect("the folder was made");
                            skipped
                                .lock()
                                .unwrap()
                                .push(format!("{} — {why}", case.name));
                        }
                        Built::Failed(why) => {
                            failed.lock().unwrap().push(format!("{}: {why}", case.name));
                        }
                    }
                }
            });
        }
    });
    let mut skipped = skipped.into_inner().unwrap();
    skipped.sort();
    std::fs::write(out.join("skipped.txt"), lines(skipped.into_iter()))
        .expect("the suite folder is writable");
    let failed = failed.into_inner().unwrap();
    assert!(
        failed.is_empty(),
        "{} of {} cases didn't build:\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );
}

/// `tests/macos/known.txt`: `<case> — <reason>` per line, `#` comments.
fn known() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(root().join("tests/macos/known.txt")).expect("known.txt");
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| match l.split_once(" — ") {
            Some((name, why)) => (name.to_string(), why.trim().to_string()),
            None => (l.to_string(), String::new()),
        })
        .collect()
}

#[test]
fn every_known_difference_names_a_run_case_and_why() {
    let cases = cases();
    for (name, why) in known() {
        let case = cases.iter().find(|c| c.name == name);
        assert!(
            case.is_some_and(|c| matches!(c.mode, Mode::Run { .. })),
            "known.txt: {name} isn't a run case"
        );
        assert!(!why.is_empty(), "known.txt: {name} has no reason");
    }
}

#[test]
fn a_case_folder_mirrors_its_case() {
    // A real case's shape, with every extra set.
    let mut case = cases().swap_remove(0);
    case.name = "x/y.la".into();
    case.mode = Mode::Run {
        stdout: b"out\n".to_vec(),
        stderr: Some(b"warning\npanic\n".to_vec()),
        exit: 101,
    };
    case.extras.args = vec!["a b".into(), "c".into()];
    case.extras.stdin = Some(b"in".to_vec());
    case.extras.scratch = true;
    case.extras.envvars = vec![("K".into(), "v=w".into())];
    let files = case_files(&case, b"warning\n").unwrap();
    let get = |n: &str| files.iter().find(|(k, _)| *k == n).map(|(_, v)| v.clone());
    assert_eq!(get("name").unwrap(), b"x/y.la");
    assert_eq!(get("exit").unwrap(), b"101\n");
    assert_eq!(get("stderr").unwrap(), b"panic\n");
    assert_eq!(get("args").unwrap(), b"a b\nc\n");
    assert_eq!(get("stdin").unwrap(), b"in");
    assert_eq!(get("envvars").unwrap(), b"K=v=w\n");
    assert_eq!(get("scratch").unwrap(), b"");
    // What the build printed must be what the case expects first.
    assert!(case_files(&case, b"other\n").is_err());
    // Without a .stderr, stderr isn't compared: warnings are fine.
    let plain = Case {
        mode: Mode::Run {
            stdout: Vec::new(),
            stderr: None,
            exit: 0,
        },
        ..case
    };
    let files = case_files(&plain, b"warning\n").unwrap();
    assert!(files.iter().all(|(k, _)| *k != "stderr"));
}

#[test]
fn a_mac_build_refused_is_skipped_with_why() {
    assert!(refused("error[E0334]: …").unwrap().starts_with("E0334"));
    assert!(refused("error[E0335]: …").unwrap().starts_with("E0335"));
    assert!(refused("error[E0101]: …").is_none());
}
