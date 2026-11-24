//! macOS on Apple Silicon: `lughac build
//! --target=aarch64-apple-darwin` makes a Mach-O executable on Linux, and
//! the programs in `tests/macos/` build; CI runs them on a Mac and compares
//! their output (`.github/workflows/ci.yml`). Without `zig` the building
//! tests print a note and pass, unless `LUGHA_CROSS_TESTS=required`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "common/cross.rs"]
mod cross;

const MAC: &str = "aarch64-apple-darwin";

/// A temp dir removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Temp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha_macos_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn lughac(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(args)
        .current_dir(root())
        .env_remove("LUGHA_CC")
        .output()
        .expect("lughac runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn every_macos_case_builds_into_a_mach_o_arm64_executable() {
    if !cross::ready("macos_build", &["zig"]) {
        return;
    }
    let t = Temp::new();
    let mut cases = 0;
    for entry in std::fs::read_dir(root().join("tests/macos"))
        .unwrap()
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "la") {
            continue;
        }
        cases += 1;
        let exe = t.0.join(path.file_stem().unwrap());
        let out = lughac(&[
            "build",
            path.to_str().unwrap(),
            "--target",
            MAC,
            "-o",
            exe.to_str().unwrap(),
        ]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}: {}",
            path.display(),
            text(&out.stderr)
        );
        let bytes = std::fs::read(&exe).expect("built");
        // Mach-O 64-bit (0xfeedfacf, little-endian), CPU type ARM64 (0x0100000c).
        assert_eq!(bytes[..4], [0xcf, 0xfa, 0xed, 0xfe], "{}", path.display());
        assert_eq!(bytes[4..8], [0x0c, 0x00, 0x00, 0x01], "{}", path.display());
        assert!(
            path.with_extension("stdout").is_file(),
            "{} has its .stdout",
            path.display()
        );
    }
    assert!(cases >= 2, "{cases} cases");
}

#[test]
fn a_macos_program_is_never_run_here() {
    let hello = "tests/macos/hello.la";
    for command in ["run", "test"] {
        let out = lughac(&[command, hello, "--target", MAC]);
        let stderr = text(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{command}: {stderr}");
        assert!(stderr.contains("runs only on a Mac"), "{command}: {stderr}");
    }
}

#[test]
fn the_macos_runtime_compiles_with_zig() {
    if !cross::ready("macos_runtime", &["zig"]) {
        return;
    }
    let t = Temp::new();
    let source = t.0.join("lugha_rt.c");
    let runtime = lugha::link::runtime_source(lugha::link::System::MacOs);
    std::fs::write(&source, runtime.as_bytes()).unwrap();
    let out = Command::new("zig")
        .args([
            "cc",
            "-target",
            "aarch64-macos.14.4-none",
            "-O2",
            "-funwind-tables",
            "-c",
        ])
        .arg(&source)
        .arg("-o")
        .arg(t.0.join("lugha_rt.o"))
        .env("ZIG_GLOBAL_CACHE_DIR", t.0.join("zig"))
        .output()
        .expect("zig runs");
    assert!(out.status.success(), "{}", text(&out.stderr));
    // macOS's own parts are in it, Linux's aren't.
    assert!(runtime.contains("kqueue()") && !runtime.contains("epoll_create1"));
}

#[test]
fn a_build_leaves_nothing_where_it_runs() {
    // `zig cc --version` for a macOS target writes an empty `a.o` into the
    // folder it runs in; asking the compiler's version must not.
    if !cross::ready("macos_build", &["zig"]) {
        return;
    }
    let t = Temp::new();
    std::fs::copy(root().join("tests/macos/hello.la"), t.0.join("hello.la")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["build", "hello.la", "--target", MAC, "-o", "hello"])
        .current_dir(&t.0)
        .env_remove("LUGHA_CC")
        .output()
        .expect("lughac runs");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let mut left: Vec<String> = std::fs::read_dir(&t.0)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["hello", "hello.la"]);
}
