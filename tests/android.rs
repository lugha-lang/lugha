//! Android: `lughac build --target=aarch64-linux-android` (and x86-64)
//! makes a position-independent executable for Android's own loader and C
//! library, through the NDK. Nothing here runs one; CI's Android job runs
//! the cases in `tests/android/` on an x86-64 emulator
//! (`.github/workflows/ci.yml`). Without an NDK (`$ANDROID_NDK_HOME` or
//! `$ANDROID_NDK_ROOT`) the tests print a note and pass, unless
//! `LUGHA_CROSS_TESTS=required`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "common/cross.rs"]
mod cross;

const TARGETS: [(&str, u16); 2] = [
    ("aarch64-linux-android", 183), // EM_AARCH64
    ("x86_64-linux-android", 62),   // EM_X86_64
];

/// A temp dir removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Temp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha_android_{}_{n}", std::process::id()));
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

/// The NDK's folder, when one is named; else the test is skipped (or
/// fails, when required).
fn ndk(test: &str) -> Option<PathBuf> {
    let named = lugha::link::ndk::root(&|name| std::env::var_os(name));
    let Some(dir) = named.filter(|d| lugha::link::ndk::clang(d).is_file()) else {
        let required = std::env::var("LUGHA_CROSS_TESTS").as_deref() == Ok("required");
        cross::decide(test, &["Android's NDK ($ANDROID_NDK_HOME)"], required);
        return None;
    };
    // An absolute path is checked as a folder.
    cross::ready(test, &[dir.to_str()?]).then_some(dir)
}

/// `lughac build <args> --target <target>` in `dir`.
fn build(dir: &Path, target: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lughac"))
        .arg("build")
        .args(args)
        .args(["--target", target])
        .current_dir(dir)
        .env_remove("LUGHA_CC")
        .output()
        .expect("lughac runs")
}

/// What an Android executable's ELF headers say: its machine, whether it
/// is position-independent (`ET_DYN`), the loader it names, and its LOAD
/// segments' smallest alignment.
struct Elf {
    machine: u16,
    pie: bool,
    interpreter: String,
    load_align: u64,
}

fn elf(exe: &Path) -> Elf {
    let b = std::fs::read(exe).unwrap();
    assert_eq!(&b[..4], b"\x7fELF", "{} isn't an ELF file", exe.display());
    assert_eq!(b[4], 2, "64-bit");
    let u16_at = |at: usize| u16::from_le_bytes(b[at..at + 2].try_into().unwrap());
    let u64_at = |at: usize| u64::from_le_bytes(b[at..at + 8].try_into().unwrap());
    let (phoff, phentsize, phnum) = (u64_at(0x20) as usize, u16_at(0x36), u16_at(0x38));
    let mut interpreter = String::new();
    let mut load_align = u64::MAX;
    for i in 0..phnum as usize {
        let at = phoff + i * phentsize as usize;
        let kind = u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let (offset, size, align) = (u64_at(at + 8) as usize, u64_at(at + 32), u64_at(at + 48));
        match kind {
            1 => load_align = load_align.min(align),
            3 => {
                let bytes = &b[offset..offset + size as usize];
                interpreter = text(bytes).trim_end_matches('\0').to_string();
            }
            _ => {}
        }
    }
    Elf {
        machine: u16_at(0x12),
        pie: u16_at(0x10) == 3,
        interpreter,
        load_align,
    }
}

#[test]
fn android_cases_build_as_pies_for_androids_loader() {
    if ndk("android").is_none() {
        return;
    }
    let t = Temp::new();
    let cases = [
        "hello",
        "tasks",
        "main_panic",
        "unwinding",
        "folders",
        "tcp",
        "lookup",
    ];
    for (target, machine) in TARGETS {
        for case in cases {
            let exe = t.0.join(format!("{case}-{target}"));
            let source = format!("tests/android/{case}.la");
            let out = build(root(), target, &[&source, "-o", exe.to_str().unwrap()]);
            assert!(out.status.success(), "{case}: {}", text(&out.stderr));
            let elf = elf(&exe);
            assert_eq!(elf.machine, machine, "{case} for {target}");
            assert!(elf.pie, "{case} for {target} is position-independent");
            assert_eq!(elf.interpreter, "/system/bin/linker64", "{case}");
            assert!(
                elf.load_align >= 16384,
                "{case}: aligned to {:#x}",
                elf.load_align
            );
        }
    }
}

#[test]
fn without_an_ndk_a_build_says_how_to_get_one() {
    let t = Temp::new();
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args([
            "build",
            "tests/android/hello.la",
            "--target",
            "aarch64-linux-android",
        ])
        .arg("-o")
        .arg(t.0.join("hello"))
        .current_dir(root())
        .env_remove("LUGHA_CC")
        .env_remove("ANDROID_NDK_HOME")
        .env_remove("ANDROID_NDK_ROOT")
        .output()
        .expect("lughac runs");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    let said = text(&out.stderr);
    assert!(said.contains("needs Android's NDK"), "{said}");
    assert!(said.contains("$ANDROID_NDK_HOME"), "{said}");
}

#[test]
fn android_refuses_to_run_here() {
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args([
            "run",
            "tests/android/hello.la",
            "--target",
            "x86_64-linux-android",
        ])
        .current_dir(root())
        .output()
        .expect("lughac runs");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("runs only on Android"));
}

#[test]
fn the_android_runtime_compiles_with_the_ndk_without_warnings() {
    let Some(ndk) = ndk("android_runtime") else {
        return;
    };
    let t = Temp::new();
    let source = t.0.join("lugha_rt.c");
    let runtime = lugha::link::runtime_source(lugha::link::System::Android);
    std::fs::write(&source, runtime.as_bytes()).unwrap();
    for (target, _) in TARGETS {
        let out = Command::new(lugha::link::ndk::clang(&ndk))
            .arg(format!("--target={target}{}", lugha::link::ndk::API))
            .args(["-O2", "-Wall", "-Wextra", "-c"])
            .arg(&source)
            .arg("-o")
            .arg(t.0.join("lugha_rt.o"))
            .output()
            .expect("the NDK's clang runs");
        assert!(out.status.success(), "{target}: {}", text(&out.stderr));
        assert!(
            !text(&out.stderr).contains("warning:"),
            "{target}: {}",
            text(&out.stderr)
        );
    }
}
