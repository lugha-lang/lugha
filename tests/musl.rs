//! Building for another platform through
//! zig: programs from `tests/programs/` built for Linux with musl, static
//! executables, run natively (x86-64) and under `qemu-aarch64` (ARM64) and
//! match their expectation files, panics and their unwinding included; what
//! a foreign build refuses (E0334, E0335); and the messages for a missing or
//! old zig. Without `zig` (and `qemu-aarch64` for ARM) the building tests
//! print a note and pass, as on a machine that can't build for musl.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A cross-section of the language: each a `tests/programs/` case stem.
const CASES: [&str; 8] = [
    "basics/expressions/div_zero",
    "collections/maps_and_sets/maps",
    "collections/growable_arrays/stack_overflow",
    "text/format_specifiers/format",
    "concurrency/green_threads/fan_out",
    "concurrency/green_threads/deadlock",
    "concurrency/actors/bank",
    "runtime/collector/spawned_alloc",
];

const X86: &str = "x86_64-unknown-linux-musl";
const ARM: &str = "aarch64-unknown-linux-musl";

/// A temp dir removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Temp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha_musl_{}_{n}", std::process::id()));
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

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Whether every program in `tools` runs; prints why not.
fn has(test: &str, tools: &[&str]) -> bool {
    let runs = |program: &&str| {
        let arg = if *program == "zig" {
            "version"
        } else {
            "--version"
        };
        Command::new(program)
            .arg(arg)
            .output()
            .is_ok_and(|o| o.status.success())
    };
    let ok = tools.iter().all(runs);
    if !ok {
        eprintln!("{test}: skipped, needs {}", tools.join(" and "));
    }
    ok
}

/// `lughac` with `args` in `cwd`, `$LUGHA_CC` unset, `PATH` as given.
fn lughac(cwd: &Path, args: &[&str], path: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lughac"));
    command.args(args).current_dir(cwd).env_remove("LUGHA_CC");
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.output().expect("lughac runs")
}

/// The case's expectation file with `extension`, if it has one.
fn expected(stem: &str, extension: &str) -> Option<String> {
    std::fs::read_to_string(root().join(format!("tests/programs/{stem}.{extension}"))).ok()
}

/// Whether the ELF file at `path` has no `PT_INTERP` header, so no dynamic
/// loader: a static executable.
fn is_static(path: &Path) -> bool {
    let elf = std::fs::read(path).expect("built");
    let u64_at = |at: usize| u64::from_le_bytes(elf[at..at + 8].try_into().unwrap());
    let u16_at = |at: usize| u16::from_le_bytes([elf[at], elf[at + 1]]);
    let (phoff, size, count) = (u64_at(0x20) as usize, u16_at(0x36) as usize, u16_at(0x38));
    // PT_INTERP is program header type 3.
    (0..count as usize)
        .all(|i| u32::from_le_bytes(elf[phoff + i * size..][..4].try_into().unwrap()) != 3)
}

/// Builds every case for `target` and runs it with `run`, checking its
/// exit code and output against the case's files.
fn cases_match(target: &str, machine: u16, run: impl Fn(&Path) -> Output) {
    let t = Temp::new();
    for stem in CASES {
        let exe = t.0.join(stem.replace('/', "_"));
        let source = format!("tests/programs/{stem}.la");
        let exe_text = exe.to_str().unwrap();
        let built = lughac(
            root(),
            &["build", &source, "--target", target, "-o", exe_text],
            None,
        );
        assert_eq!(
            built.status.code(),
            Some(0),
            "{stem}: {}",
            text(&built.stderr)
        );
        let elf = std::fs::read(&exe).expect("built");
        // ELF `e_machine`, little-endian at offset 18.
        assert_eq!(u16::from_le_bytes([elf[18], elf[19]]), machine, "{stem}");
        assert!(is_static(&exe), "{stem}: a musl build is static");
        let out = run(&exe);
        let code: i32 = expected(stem, "exit").map_or(0, |e| e.trim().parse().expect("a code"));
        assert_eq!(
            out.status.code(),
            Some(code),
            "{stem}: {}",
            text(&out.stderr)
        );
        if let Some(stdout) = expected(stem, "stdout") {
            assert_eq!(text(&out.stdout), stdout, "{stem}");
        }
        if let Some(stderr) = expected(stem, "stderr") {
            assert_eq!(text(&out.stderr), stderr, "{stem}");
        }
    }
}

#[test]
fn programs_built_for_x86_64_musl_are_static_and_run_as_on_glibc() {
    if !cfg!(target_arch = "x86_64") || !has("x86_64_musl", &["zig"]) {
        return;
    }
    // 62 is x86-64.
    cases_match(X86, 62, |exe| {
        Command::new(exe)
            .env("LUGHA_WORKERS", "1")
            .output()
            .expect("runs")
    });
}

#[test]
fn programs_built_for_arm64_musl_run_under_qemu() {
    if !has("arm64_musl", &["zig", "qemu-aarch64"]) {
        return;
    }
    // 183 is AArch64; a static executable needs no ARM sysroot.
    cases_match(ARM, 183, |exe| {
        let mut qemu = Command::new("qemu-aarch64");
        qemu.arg(exe)
            .env("LUGHA_WORKERS", "1")
            .output()
            .expect("qemu-aarch64 runs")
    });
}

/// `main.la` holding `program` and a `lugha.toml` holding `manifest`, in a
/// fresh folder.
fn project(program: &str, manifest: &str) -> Temp {
    let t = Temp::new();
    std::fs::write(t.0.join("main.la"), program).unwrap();
    std::fs::write(t.0.join("lugha.toml"), manifest).unwrap();
    t
}

#[test]
fn openssl_is_refused_for_a_foreign_target_at_its_import() {
    let program = "import std.crypto;\n\nfun main(console: Console) {\n    console.println(crypto.sha256([]).len);\n}\n";
    let t = project(program, "");
    let out = lughac(
        &t.0,
        &["build", "main.la", "--target", ARM, "-o", "prog"],
        None,
    );
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("error[E0334]"), "{stderr}");
    assert!(
        stderr.contains(&format!("`std.crypto` can't be built for {ARM}")),
        "{stderr}"
    );
    assert!(stderr.contains("main.la:1:"), "{stderr}");
    assert!(!t.0.join("prog").exists());
}

#[test]
fn pkg_config_is_refused_for_a_foreign_target_at_its_line() {
    let program = "fun main(console: Console) {\n    console.println(\"hi\");\n}\n";
    let t = project(program, "[link]\npkg-config = [\"sqlite3\"]\n");
    let out = lughac(
        &t.0,
        &["build", "main.la", "--target", ARM, "-o", "prog"],
        None,
    );
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("error[E0335]"), "{stderr}");
    assert!(
        stderr.contains(&format!("`pkg-config` can't be used for {ARM}")),
        "{stderr}"
    );
    assert!(stderr.contains("lugha.toml:2:"), "{stderr}");
}

#[test]
fn a_missing_zig_stops_a_foreign_build_with_exit_2() {
    let t = project(
        "fun main(console: Console) {\n    console.println(\"hi\");\n}\n",
        "",
    );
    // An empty PATH: no zig, and no other program either.
    let out = lughac(
        &t.0,
        &["build", "main.la", "--target", ARM, "-o", "prog"],
        Some(""),
    );
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains(&format!("building for {ARM} needs zig 0.17.0 or newer")),
        "{stderr}"
    );
    assert!(stderr.contains("$LUGHA_CC"), "{stderr}");
}

#[test]
fn an_old_zig_is_named_and_refused() {
    use std::os::unix::fs::PermissionsExt;
    let t = project(
        "fun main(console: Console) {\n    console.println(\"hi\");\n}\n",
        "",
    );
    let bin = t.0.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let zig = bin.join("zig");
    std::fs::write(&zig, "#!/bin/sh\necho 0.16.0\n").unwrap();
    std::fs::set_permissions(&zig, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = bin.to_str().unwrap();
    let out = lughac(
        &t.0,
        &["build", "main.la", "--target", ARM, "-o", "prog"],
        Some(path),
    );
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("0.17.0 or newer") && stderr.contains("\"0.16.0\""),
        "{stderr}"
    );
}

#[test]
fn lugha_cc_is_used_for_a_foreign_target_without_asking_zig() {
    let t = project(
        "fun main(console: Console) {\n    console.println(\"hi\");\n}\n",
        "",
    );
    // A `$LUGHA_CC` that doesn't exist: the failure names it, not zig.
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["build", "main.la", "--target", ARM, "-o", "prog"])
        .current_dir(&t.0)
        .env("LUGHA_CC", "no-such-cc-for-lugha")
        .env("PATH", "")
        .output()
        .expect("lughac runs");
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("no-such-cc-for-lugha"), "{stderr}");
    assert!(!stderr.contains("zig"), "{stderr}");
}
