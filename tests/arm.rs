//! Linux on ARM64: programs from
//! `tests/programs/` built with `--target aarch64-unknown-linux-gnu` and run
//! under `qemu-aarch64` match their expectation files, green threads,
//! actors, panics and counting included; and `extern struct`s cross to
//! C by value as AAPCS64 passes them, against C compiled by zig for ARM.
//! Without `zig`, `qemu-aarch64` and an ARM64 glibc for qemu to load
//! (`/usr/aarch64-linux-gnu`, Ubuntu's `libc6-arm64-cross`) each test
//! prints a note and passes, as on a machine that can't build for ARM,
//! unless `LUGHA_CROSS_TESTS=required`. `lughac run` and `lughac test`
//! with `--target` start the programs under qemu themselves.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "common/c_shapes.rs"]
mod c_shapes;
#[path = "common/cross.rs"]
mod cross;
use c_shapes::{DECLS, PROGRAMS, SIDE};

const TARGET: &str = "aarch64-unknown-linux-gnu";

/// A cross-section of the language: each a `tests/programs/` case stem.
const CASES: [&str; 24] = [
    "basics/expressions/arith",
    "basics/expressions/div_zero",
    "types/methods/methods",
    "collections/growable_arrays/stack_overflow",
    "patterns/equality/patterns",
    "text/input_and_interpolation/interp",
    "numbers/integer_methods/hashing",
    "text/format_specifiers/format",
    "collections/maps_and_sets/maps",
    "collections/maps_and_sets/missing_key",
    "numbers/float_bits/float_bits",
    "collections/joining/concat",
    "concurrency/green_threads/fan_out",
    "concurrency/green_threads/gc",
    "concurrency/green_threads/deadlock",
    "concurrency/actors/bank",
    "concurrency/actors/many_actors",
    "runtime/collector/spawned_alloc",
    "runtime/small_frames/deep_match",
    "runtime/small_frames/arm_results",
    "runtime/small_frames/big_frame_overflow",
    "runtime/small_frames/green_big_frame_overflow",
    "runtime/slot_sharing/sequential_values",
    "runtime/slot_sharing/popover_depth",
];

/// An empty temporary folder, removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Temp {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-arm-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir is writable");
        Temp(dir)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Whether this machine has zig, qemu and an ARM64 glibc for qemu to load.
fn can_build_for_arm(test: &str) -> bool {
    cross::ready(test, &["zig", "qemu-aarch64", "/usr/aarch64-linux-gnu/lib"])
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn lughac(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(args)
        .current_dir(cwd)
        .env_remove("LUGHA_CC")
        .output()
        .expect("lughac runs")
}

fn qemu(exe: &Path) -> Output {
    Command::new("qemu-aarch64")
        .args(["-L", "/usr/aarch64-linux-gnu"])
        .arg(exe)
        .output()
        .expect("qemu-aarch64 runs")
}

/// The case's expectation file with `extension`, if it has one.
fn expected(stem: &str, extension: &str) -> Option<String> {
    std::fs::read_to_string(root().join(format!("tests/programs/{stem}.{extension}"))).ok()
}

#[test]
fn programs_run_on_arm_as_they_do_on_x86_64() {
    if !can_build_for_arm("programs_run_on_arm") {
        return;
    }
    let t = Temp::new();
    for stem in CASES {
        let exe = t.0.join(stem.replace('/', "_"));
        let source = format!("tests/programs/{stem}.la");
        let built = lughac(
            root(),
            &[
                "build",
                &source,
                "--target",
                TARGET,
                "-o",
                exe.to_str().unwrap(),
            ],
        );
        assert_eq!(
            built.status.code(),
            Some(0),
            "{stem}: {}",
            text(&built.stderr)
        );
        let elf = std::fs::read(&exe).expect("built");
        // ELF `e_machine`, little-endian at offset 18: 183 is AArch64.
        assert_eq!(u16::from_le_bytes([elf[18], elf[19]]), 183, "{stem}");
        let out = qemu(&exe);
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

/// Builds `main` (after `DECLS`) with the shapes' C side in `side.c` for
/// ARM — the C compiled by zig — and runs it at -O0 and
/// -O2 under qemu, checking it prints `prints`.
fn runs_on_arm(name: &str, main: &str, prints: &str) {
    let t = Temp::new();
    std::fs::write(t.0.join("main.la"), [DECLS, main].concat()).unwrap();
    std::fs::write(t.0.join("side.c"), SIDE).unwrap();
    std::fs::write(t.0.join("lugha.toml"), "[link]\nc = [\"side.c\"]\n").unwrap();
    for opt in ["-O0", "-O2"] {
        let built = Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(["build", "main.la", "--target", TARGET, opt, "-o", "prog"])
            .current_dir(&t.0)
            .env("HOME", &t.0)
            .env_remove("LUGHA_CC")
            .output()
            .expect("lughac runs");
        assert_eq!(
            built.status.code(),
            Some(0),
            "{name} {opt}: {}",
            text(&built.stderr)
        );
        let out = qemu(&t.0.join("prog"));
        assert_eq!(
            (out.status.code(), text(&out.stdout)),
            (Some(0), prints.to_string()),
            "{name} {opt}: {}",
            text(&out.stderr)
        );
    }
}

#[test]
fn extern_structs_cross_to_c_by_value_on_arm() {
    if !can_build_for_arm("extern_structs_by_value") {
        return;
    }
    for (name, main, prints) in PROGRAMS {
        runs_on_arm(name, main, prints);
    }
}

/// A fresh folder holding `main.la` with `program`.
fn project(program: &str) -> Temp {
    let t = Temp::new();
    std::fs::write(t.0.join("main.la"), program).unwrap();
    t
}

#[test]
fn run_starts_an_arm_program_under_qemu_with_its_exit_code() {
    if !can_build_for_arm("run_on_arm") {
        return;
    }
    let t = project("fun main(console: Console) {\n    console.println(\"hi\");\n}\n");
    let out = lughac(&t.0, &["run", "main.la", "--target", TARGET]);
    assert_eq!(
        (out.status.code(), text(&out.stdout)),
        (Some(0), "hi\n".to_string()),
        "{}",
        text(&out.stderr)
    );
    let source = "tests/programs/basics/expressions/div_zero.la";
    let out = lughac(root(), &["run", source, "--target", TARGET]);
    assert_eq!(out.status.code(), Some(101), "{}", text(&out.stderr));
    assert_eq!(
        Some(text(&out.stderr)),
        expected("basics/expressions/div_zero", "stderr")
    );
}

#[test]
fn test_runs_arm_tests_under_qemu() {
    if !can_build_for_arm("test_on_arm") {
        return;
    }
    let tests = "@test\nfun adds() {\n    assert_eq(1 + 1, 2);\n}\n\n@test\nfun wrong() {\n    assert_eq(1 + 1, 3);\n}\n";
    let t = project(tests);
    let out = lughac(&t.0, &["test", "main.la", "--target", TARGET]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}{}", text(&out.stderr));
    assert!(stdout.contains("test main.adds ... ok"), "{stdout}");
    assert!(stdout.contains("test main.wrong ... FAILED"), "{stdout}");
    assert!(stdout.contains("1 passed, 1 failed"), "{stdout}");
}

#[test]
fn run_without_qemu_says_what_to_install() {
    let t = project("fun main(console: Console) {\n    console.println(\"hi\");\n}\n");
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["run", "main.la", "--target", TARGET])
        .current_dir(&t.0)
        .env("PATH", "")
        .output()
        .expect("lughac runs");
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "running a program built for {TARGET} needs qemu-aarch64"
        )),
        "{stderr}"
    );
}
