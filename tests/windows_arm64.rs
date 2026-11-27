//! Windows on ARM64: `lughac build --target=aarch64-pc-windows-gnu` makes
//! an ARM64 PE executable on Linux. Nothing here runs one, since Wine on
//! an x86-64 machine can't; CI's ARM64 Windows runner runs the cases in
//! `tests/windows/` and the program suite (`.github/workflows/ci.yml`).
//! Without `zig` the tests print a note and pass, unless
//! `LUGHA_CROSS_TESTS=required`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "common/cross.rs"]
mod cross;

#[path = "common/c_shapes.rs"]
mod c_shapes;
use c_shapes::{DECLS, PROGRAMS, SIDE};

const TARGET: &str = "aarch64-pc-windows-gnu";

/// The machine an ARM64 PE executable names in its file header.
const ARM64: u16 = 0xAA64;

/// A temp dir removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Temp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha_winarm_{}_{n}", std::process::id()));
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

/// `lughac build <args> --target aarch64-pc-windows-gnu` in `dir`.
fn build(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lughac"))
        .arg("build")
        .args(args)
        .args(["--target", TARGET])
        .current_dir(dir)
        .env_remove("LUGHA_CC")
        .output()
        .expect("lughac runs")
}

/// The machine a PE file's header names: the `PE\0\0` signature at the
/// offset the DOS header gives at 0x3c, then the machine.
fn pe_machine(exe: &Path) -> u16 {
    let bytes = std::fs::read(exe).unwrap();
    assert_eq!(&bytes[..2], b"MZ", "{} isn't a PE file", exe.display());
    let at = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(&bytes[at..at + 4], b"PE\0\0");
    u16::from_le_bytes([bytes[at + 4], bytes[at + 5]])
}

#[test]
fn windows_cases_build_as_arm64_executables() {
    if !cross::ready("windows_arm64", &["zig"]) {
        return;
    }
    let t = Temp::new();
    for case in ["hello", "tasks", "main_panic", "unwinding", "cancel_stress"] {
        let exe = t.0.join(format!("{case}.exe"));
        let source = format!("tests/windows/{case}.la");
        let out = build(root(), &[&source, "-o", exe.to_str().unwrap()]);
        assert!(out.status.success(), "{case}: {}", text(&out.stderr));
        assert_eq!(pe_machine(&exe), ARM64, "{case}");
        assert!(
            !t.0.join(format!("{case}.pdb")).exists(),
            "a build writes only its .exe"
        );
    }
}

/// Builds the struct shapes against zig's C at -O0 and -O2. With
/// `LUGHA_CASES_OUT` set, each executable and what it must print are kept
/// there (`<shape>_O0.exe`, `.stdout`), as cases CI's ARM64 Windows runner
/// runs: the rules checked against Windows' own C.
#[test]
fn extern_structs_build_against_zigs_c_for_arm64_windows() {
    if !cross::ready("windows_arm64_structs", &["zig"]) {
        return;
    }
    let keep = std::env::var_os("LUGHA_CASES_OUT").map(PathBuf::from);
    for (name, main, prints) in PROGRAMS {
        let t = Temp::new();
        std::fs::write(t.0.join("main.la"), [DECLS, main].concat()).unwrap();
        std::fs::write(t.0.join("side.c"), SIDE).unwrap();
        std::fs::write(t.0.join("lugha.toml"), "[link]\nc = [\"side.c\"]\n").unwrap();
        for opt in ["-O0", "-O2"] {
            let out = build(&t.0, &["main.la", opt, "-o", "prog.exe"]);
            assert!(out.status.success(), "{name} {opt}: {}", text(&out.stderr));
            assert_eq!(pe_machine(&t.0.join("prog.exe")), ARM64);
            if let Some(dir) = &keep {
                let case = dir.join(format!("{name}_{}", &opt[1..]));
                std::fs::copy(t.0.join("prog.exe"), case.with_extension("exe")).unwrap();
                std::fs::write(case.with_extension("stdout"), prints).unwrap();
            }
        }
    }
}

#[test]
fn the_arm64_windows_runtime_compiles_with_zig_without_warnings() {
    if !cross::ready("windows_arm64_runtime", &["zig"]) {
        return;
    }
    let t = Temp::new();
    let source = t.0.join("lugha_rt.c");
    let runtime = lugha::link::runtime_source(lugha::link::System::Windows);
    std::fs::write(&source, runtime.as_bytes()).unwrap();
    let out = Command::new("zig")
        .args([
            "cc",
            "-target",
            "aarch64-windows-gnu",
            "-O2",
            "-Wall",
            "-Wextra",
            "-c",
        ])
        .arg(&source)
        .arg("-o")
        .arg(t.0.join("lugha_rt.o"))
        .env("ZIG_GLOBAL_CACHE_DIR", t.0.join("zig"))
        .output()
        .expect("zig runs");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        !text(&out.stderr).contains("warning:"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn tls_cases_build_with_an_openssl_for_arm64_windows() {
    if !cross::ready("windows_arm64_tls", &["zig"]) {
        return;
    }
    if std::env::var_os("LUGHA_OPENSSL_AARCH64_PC_WINDOWS_GNU").is_none() {
        eprintln!("windows_arm64: TLS cases not built, needs LUGHA_OPENSSL_AARCH64_PC_WINDOWS_GNU");
        return;
    }
    let t = Temp::new();
    for case in ["trust", "roots"] {
        let exe = t.0.join(format!("{case}.exe"));
        let source = format!("tests/windows/openssl/{case}.la");
        let out = build(root(), &[&source, "-o", exe.to_str().unwrap()]);
        assert!(out.status.success(), "{case}: {}", text(&out.stderr));
        assert_eq!(pe_machine(&exe), ARM64, "{case}");
    }
}
