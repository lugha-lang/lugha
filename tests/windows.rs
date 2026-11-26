//! Windows on x86-64: `lughac build --target=x86_64-pc-windows-gnu` makes
//! a PE executable on Linux, and the programs in `tests/windows/` (a `.la`
//! file, or a folder with `main.la`) build. Where `wine` is installed each
//! runs under it as CI runs it on Windows (`.github/workflows/ci.yml`):
//! with the arguments `a b` and `ñandú`, `LUGHA_TEST_WIN=héllo`, its
//! `.stdin` file as stdin, and its `.stdout` and `.exit` (0 when there is
//! none) to match. Without `zig` the building tests print a note and pass,
//! unless `LUGHA_CROSS_TESTS=required`; without `wine` they only build.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "common/cross.rs"]
mod cross;

const WINDOWS: &str = "x86_64-pc-windows-gnu";

/// A temp dir removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Temp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha_windows_{}_{n}", std::process::id()));
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

/// Whether `wine` runs here; never required, since CI runs the cases on
/// Windows itself.
fn wine() -> bool {
    let here = Command::new("wine")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !here {
        eprintln!("windows: cases built but not run, needs wine");
    }
    here
}

/// Cases Wine can't run, each with why; CI runs them on Windows itself.
const WINE_CANT: [(&str, &str); 1] = [(
    "confined",
    "Wine's CreateSymbolicLinkW makes no link, and junctions can't be set",
)];

/// Every case: its name and the source to build (relative to the root).
fn cases() -> Vec<(String, String)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root().join("tests/windows")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            found.push((name.clone(), format!("tests/windows/{name}/main.la")));
        } else if path.extension().is_some_and(|e| e == "la") {
            found.push((name.clone(), format!("tests/windows/{name}.la")));
        }
    }
    found.sort();
    found
}

/// `exe` under wine, as CI runs it on Windows.
fn run(exe: &Path, stdin: Option<PathBuf>) -> Output {
    let input = match stdin {
        Some(file) => Stdio::from(std::fs::File::open(file).unwrap()),
        None => Stdio::null(),
    };
    Command::new("wine")
        .arg(exe)
        .args(["a b", "ñandú"])
        .env("LUGHA_TEST_WIN", "héllo")
        .env("WINEDEBUG", "-all")
        .current_dir(exe.parent().unwrap())
        .stdin(input)
        .output()
        .expect("wine runs")
}

#[test]
fn every_windows_case_builds_and_runs_under_wine() {
    if !cross::ready("windows_build", &["zig"]) {
        return;
    }
    let t = Temp::new();
    let running = wine();
    let mut failed = Vec::new();
    for (name, source) in cases() {
        let exe = t.0.join(format!("{name}.exe"));
        let out = lughac(&[
            "build",
            &source,
            "--target",
            WINDOWS,
            "-o",
            exe.to_str().unwrap(),
        ]);
        assert!(out.status.success(), "{name}: {}", text(&out.stderr));
        if !running {
            continue;
        }
        if let Some((_, why)) = WINE_CANT.iter().find(|(n, _)| *n == name) {
            eprintln!("{name}: built, not run under wine: {why}");
            continue;
        }
        let expect =
            |ext: &str| std::fs::read_to_string(root().join(format!("tests/windows/{name}.{ext}")));
        let stdin =
            Some(root().join(format!("tests/windows/{name}.stdin"))).filter(|p| p.is_file());
        let out = run(&exe, stdin);
        let code: i32 = expect("exit").map_or(0, |c| c.trim().parse().unwrap());
        let stdout = expect("stdout").unwrap();
        // `.stderr`: what stderr must hold (wine may add lines of its own).
        let stderr = expect("stderr").unwrap_or_default();
        if out.status.code() != Some(code)
            || text(&out.stdout) != stdout
            || !text(&out.stderr).contains(stderr.trim_end())
        {
            failed.push(format!(
                "{name}: exit {:?} (want {code})\nstdout:\n{}\nstderr:\n{}",
                out.status.code(),
                text(&out.stdout),
                text(&out.stderr)
            ));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n\n"));
}

#[test]
fn run_and_test_refuse_a_windows_target() {
    let hello = "tests/windows/hello.la";
    for command in ["run", "test"] {
        let out = lughac(&[command, hello, "--target", WINDOWS]);
        let stderr = text(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{command}: {stderr}");
        assert!(
            stderr.contains("runs only on Windows"),
            "{command}: {stderr}"
        );
    }
}

#[test]
fn a_struct_passed_to_c_by_value_is_e0337_for_windows_only() {
    if !cross::ready("windows_e0337", &["zig"]) {
        return;
    }
    let t = Temp::new();
    let main = t.0.join("main.la");
    std::fs::write(
        &main,
        "extern struct Point { x: f64, y: f64 }\n\
         extern fun length(p: Point): f64;\n\
         extern fun made(): Point;\n\
         extern fun by_address(ptr p: Point): f64;\n\
         fun main(console: Console) {\n\
             let p = Point { x: 1.0, y: 2.0 };\n\
             console.println(length(made()) + by_address(ptr p));\n\
         }\n",
    )
    .unwrap();
    let path = main.to_str().unwrap();
    let out = lughac(&[
        "build",
        path,
        "--target",
        WINDOWS,
        "-o",
        t.0.join("m.exe").to_str().unwrap(),
    ]);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert_eq!(stderr.matches("error[E0337]").count(), 2, "{stderr}");
    assert!(
        stderr.contains("`Point` can't be passed to C by value on Windows yet"),
        "{stderr}"
    );
    // Elsewhere the same program checks clean.
    let linux = lughac(&["check", path]);
    assert!(linux.status.success(), "{}", text(&linux.stderr));
}

#[test]
fn a_windows_build_reports_lockdown_not_applied() {
    if !cross::ready("windows_lockdown", &["zig"]) || !wine() {
        return;
    }
    let t = Temp::new();
    let exe = t.0.join("hello.exe");
    let out = lughac(&[
        "build",
        "tests/windows/hello.la",
        "--target",
        WINDOWS,
        "-o",
        exe.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let ran = Command::new("wine")
        .arg(&exe)
        .env("LUGHA_LOCKDOWN", "report")
        .env("WINEDEBUG", "-all")
        .output()
        .expect("wine runs");
    let stderr = text(&ran.stderr);
    assert!(
        stderr.contains("lockdown: not applied on Windows"),
        "{stderr}"
    );
}

#[test]
fn the_windows_runtime_compiles_with_zig_without_warnings() {
    if !cross::ready("windows_runtime", &["zig"]) {
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
            "x86_64-windows-gnu",
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
fn a_windows_executable_imports_only_what_windows_has() {
    if !cross::ready("windows_imports", &["zig"]) {
        return;
    }
    let Ok(dump) = Command::new("llvm-objdump-21").arg("--version").output() else {
        eprintln!("windows_imports: skipped, needs llvm-objdump-21");
        return;
    };
    assert!(dump.status.success());
    let t = Temp::new();
    let exe = t.0.join("hello.exe");
    let out = lughac(&[
        "build",
        "tests/windows/hello.la",
        "--target",
        WINDOWS,
        "-o",
        exe.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let listed = Command::new("llvm-objdump-21")
        .arg("-p")
        .arg(&exe)
        .output()
        .unwrap();
    let dlls: Vec<String> = text(&listed.stdout)
        .lines()
        .filter_map(|l| l.trim().strip_prefix("DLL Name: "))
        .map(str::to_ascii_lowercase)
        .collect();
    assert!(!dlls.is_empty(), "{}", text(&listed.stdout));
    for dll in &dlls {
        let system = [
            "kernel32.dll",
            "ntdll.dll",
            "ws2_32.dll",
            "bcryptprimitives.dll",
        ]
        .contains(&dll.as_str())
            || dll.starts_with("api-ms-win-");
        assert!(
            system,
            "imports {dll}, which Windows doesn't ship: {dlls:?}"
        );
    }
}
