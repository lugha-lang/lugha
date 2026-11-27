//! Functions C calls by name, `extern fun name(…) { … }`, in a shared
//! library `lughac build --shared` makes: a C program built with `cc`
//! against the library and its header calls each, as the spec's section 8
//! says.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "common/cross.rs"]
mod cross;

/// A temp dir removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Temp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha_exports_{}_{n}", std::process::id()));
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

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `lughac build --shared lib.la` in `dir`, with more arguments.
fn lughac(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["build", "--shared"])
        .args(args)
        .current_dir(dir)
        .env_remove("LUGHA_CC")
        .output()
        .expect("lughac runs")
}

/// Builds `lib` as `libwords.so` (its header `lib.h`) and `host` (C,
/// including the header) against it, and runs the host.
fn run(lib: &str, host: &str) -> Output {
    let t = Temp::new();
    std::fs::write(t.0.join("lib.la"), lib).unwrap();
    std::fs::write(t.0.join("host.c"), host).unwrap();
    let built = lughac(&t.0, &["lib.la", "-o", "libwords.so"]);
    assert!(built.status.success(), "{}", text(&built.stderr));
    let cc = Command::new("cc")
        .args([
            "-Wall", "-Wextra", "-Werror", "host.c", "-L.", "-lwords", "-pthread",
        ])
        .arg("-Wl,-rpath,$ORIGIN")
        .args(["-o", "host"])
        .current_dir(&t.0)
        .output()
        .expect("cc runs");
    assert!(cc.status.success(), "{}", text(&cc.stderr));
    Command::new(t.0.join("host"))
        .current_dir(&t.0)
        .output()
        .expect("the host runs")
}

/// Runs `lib` and `host`, which must exit 0 printing exactly `want`.
fn prints(lib: &str, host: &str, want: &str) {
    let out = run(lib, host);
    assert!(
        out.status.success(),
        "{:?}: {}",
        out.status,
        text(&out.stderr)
    );
    assert_eq!(text(&out.stdout), want);
}

const VALUES: &str = r#"
extern struct Point { x: f64, y: f64 }
extern type Db;

extern fun add(a: i64, b: i32): i64 = a + b as i64;
extern fun is_big(n: u8): bool = n > 100;
extern fun upper(c: char): char = if c == 'a' { 'A' } else { c };
extern fun scale(p: Point, k: f64): Point = Point { x: p.x * k, y: p.y * k };
extern fun same(d: Db): Option<Db> = Some(d);
extern fun greet(name: Option<string>): string = "hello, {name.unwrap_or("you")}";
extern "words_longest" fun longest(text: string): string {
    let mut best = "";
    for w of text.split(" ") {
        if w.len > best.len {
            best = w;
        }
    }
    best
}
extern fun nothing() {}
"#;

const VALUES_HOST: &str = r#"#include <stdio.h>
#include <stdlib.h>
#include "lib.h"

int main(void) {
    printf("%lld\n", (long long)add(40, 2));
    printf("%d %d\n", is_big(7), is_big(200));
    printf("%u\n", upper('a'));
    Point p = scale((Point){1.5, -2.0}, 2.0);
    printf("%g %g\n", p.x, p.y);
    int marker;
    printf("%d\n", same((Db *)&marker) == (Db *)&marker);
    char *a = greet("Ada");
    char *b = greet(NULL);
    char *c = words_longest("the quick brown fox");
    printf("%s|%s|%s\n", a, b, c);
    free(a);
    free(b);
    free(c);
    nothing();
    return 0;
}
"#;

#[test]
fn c_calls_each_kind_of_export_through_the_header() {
    prints(
        VALUES,
        VALUES_HOST,
        "42\n0 1\n65\n3 -4\n1\nhello, Ada|hello, you|quick\n",
    );
}

#[test]
fn an_export_runs_as_a_task_and_may_wait() {
    let lib = r#"
import std.task;

const BASE: i64 = 1000;

fun square(n: i64): i64 {
    task.sleep(5);
    n * n
}

extern fun squares(a: i64, b: i64): i64 {
    let total = scope s {
        let x = s.spawn(square(a));
        let y = s.spawn(square(b));
        x.join().unwrap() + y.join().unwrap()
    };
    BASE + total.unwrap()
}
"#;
    let host = r#"#include <pthread.h>
#include <stdio.h>
#include "lib.h"

/* Four threads C made call at once, many times each. */
static void *calls(void *arg) {
    long long n = (long long)(long)arg;
    for (int i = 0; i < 50; i++) {
        if (squares(n, 2) != 1000 + n * n + 4) {
            fprintf(stderr, "wrong answer\n");
            return (void *)1;
        }
    }
    return NULL;
}

int main(void) {
    printf("%lld\n", (long long)squares(3, 4));
    pthread_t threads[4];
    for (long i = 0; i < 4; i++) {
        pthread_create(&threads[i], NULL, calls, (void *)(i + 1));
    }
    int failed = 0;
    for (int i = 0; i < 4; i++) {
        void *result;
        pthread_join(threads[i], &result);
        failed |= result != NULL;
    }
    printf("%s\n", failed ? "failed" : "every call answered");
    return failed;
}
"#;
    prints(lib, host, "1025\nevery call answered\n");
}

#[test]
fn a_panic_in_an_export_ends_the_host_with_101() {
    let lib = r#"
extern fun pick(i: i64): i64 {
    let xs = [1, 2, 3];
    xs[i]
}
extern fun with_nul(): string = "a\0b";
"#;
    let out = run(
        lib,
        "#include <stdio.h>\n#include \"lib.h\"\nint main(void) {\n    printf(\"%lld\\n\", (long long)pick(1));\n    fflush(stdout);\n    pick(7);\n    printf(\"not reached\\n\");\n    return 0;\n}\n",
    );
    assert_eq!(out.status.code(), Some(101), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "2\n");
    assert!(
        text(&out.stderr).contains("panic: index out of bounds"),
        "{}",
        text(&out.stderr)
    );
    let nul = run(
        lib,
        "#include \"lib.h\"\nint main(void) {\n    with_nul();\n    return 0;\n}\n",
    );
    assert_eq!(nul.status.code(), Some(101));
    assert!(
        text(&nul.stderr).contains("can't contain a NUL byte"),
        "{}",
        text(&nul.stderr)
    );
}

#[test]
fn only_the_exports_are_visible() {
    let t = Temp::new();
    std::fs::write(
        t.0.join("lib.la"),
        "fun helper(): i64 = 1;\nextern fun one(): i64 = helper();\n",
    )
    .unwrap();
    let built = lughac(&t.0, &["lib.la"]);
    assert!(built.status.success(), "{}", text(&built.stderr));
    // Named as the system names a library, its header beside it.
    assert!(t.0.join("liblib.so").is_file());
    assert!(t.0.join("lib.h").is_file());
    let nm = Command::new("nm")
        .args(["-D", "--defined-only", "liblib.so"])
        .current_dir(&t.0)
        .output()
        .expect("nm runs");
    let shown: Vec<String> = text(&nm.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().nth(2).map(str::to_string))
        .collect();
    assert_eq!(shown, ["one"]);
}

#[test]
fn a_shared_library_has_exports_and_no_main() {
    let t = Temp::new();
    std::fs::write(
        t.0.join("main.la"),
        "fun main() {}\nextern fun one(): i64 = 1;\n",
    )
    .unwrap();
    std::fs::write(t.0.join("none.la"), "fun helper(): i64 = 1;\n").unwrap();
    for (file, says) in [
        ("main.la", "has no `main`"),
        ("none.la", "needs a function C calls"),
    ] {
        let out = lughac(&t.0, &[file]);
        assert_eq!(out.status.code(), Some(1), "{file}");
        let said = text(&out.stderr);
        assert!(
            said.contains("E0338") && said.contains(says),
            "{file}: {said}"
        );
    }
    let musl = lughac(&t.0, &["none.la", "--target", "x86_64-unknown-linux-musl"]);
    assert_eq!(musl.status.code(), Some(2));
    assert!(text(&musl.stderr).contains("musl builds are static"));
}

/// A Windows DLL, called by a host zig builds for Windows, under Wine
/// (without Wine, both only build).
#[test]
fn a_windows_dll_is_called_as_on_linux() {
    if !cross::ready("exports_windows", &["zig"]) {
        return;
    }
    let t = Temp::new();
    std::fs::write(t.0.join("lib.la"), VALUES).unwrap();
    std::fs::write(t.0.join("host.c"), VALUES_HOST).unwrap();
    let built = lughac(
        &t.0,
        &[
            "lib.la",
            "--target",
            "x86_64-pc-windows-gnu",
            "-o",
            "words.dll",
        ],
    );
    assert!(built.status.success(), "{}", text(&built.stderr));
    assert!(
        t.0.join("words.lib").is_file(),
        "the import library is named after the DLL"
    );
    let cc = Command::new("zig")
        .args([
            "cc",
            "-target",
            "x86_64-windows-gnu",
            "-Wall",
            "-Werror",
            "host.c",
            "words.dll",
        ])
        .args(["-o", "host.exe"])
        .current_dir(&t.0)
        .env("ZIG_GLOBAL_CACHE_DIR", t.0.join("zig"))
        .output()
        .expect("zig runs");
    assert!(cc.status.success(), "{}", text(&cc.stderr));
    // Without Wine the DLL and its host only build, as in tests/windows.rs.
    let wine = Command::new("wine").arg("--version").output();
    if !wine.is_ok_and(|o| o.status.success()) {
        eprintln!("exports_windows: built, not run: needs wine");
        return;
    }
    let out = Command::new("wine")
        .arg("host.exe")
        .current_dir(&t.0)
        .env("WINEDEBUG", "-all")
        .output()
        .expect("wine runs");
    assert!(out.status.success(), "{:?}", out.status);
    assert_eq!(
        text(&out.stdout).replace("\r\n", "\n"),
        "42\n0 1\n65\n3 -4\n1\nhello, Ada|hello, you|quick\n"
    );
}

/// Every other target links a shared library: Mach-O, PE and Android's ELF,
/// their headers beside them (only built here; their hosts run elsewhere).
#[test]
fn a_shared_library_links_for_every_target() {
    if !cross::ready("exports_targets", &["zig"]) {
        return;
    }
    let t = Temp::new();
    std::fs::write(t.0.join("lib.la"), VALUES).unwrap();
    let mut targets = vec![
        ("aarch64-apple-darwin", "libwords.dylib"),
        ("aarch64-pc-windows-gnu", "words.dll"),
        ("aarch64-unknown-linux-gnu", "libwords.so"),
    ];
    // Android's needs the NDK, named as tests/android.rs names it.
    if lugha::link::ndk::root(&|name| std::env::var_os(name)).is_some() {
        targets.push(("aarch64-linux-android", "libwords.so"));
    }
    for (target, name) in targets {
        let dir = t.0.join(target);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join(name);
        let built = lughac(
            &t.0,
            &["lib.la", "--target", target, "-o", out.to_str().unwrap()],
        );
        assert!(built.status.success(), "{target}: {}", text(&built.stderr));
        assert!(out.is_file() && dir.join("lib.h").is_file(), "{target}");
    }
}
