//! Kotlin and Java calling a Lugha library: `lughac build --shared --jni
//! com.example.Words` writes the class and compiles JNI glue into the
//! library, and a JVM program calls `tests/jni/words.la` through it. The
//! JVM tests need `javac`, `java` and `kotlinc` on `PATH` (`tests/kotlin.sh`
//! fetches Kotlin's); without them they print a note and pass, unless
//! `LUGHA_CROSS_TESTS=required`.

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
        let dir = std::env::temp_dir().join(format!("lugha_jni_{}_{n}", std::process::id()));
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

/// `lughac build --shared` of `file` with `args`, in `dir`.
fn lughac(dir: &Path, file: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["build", "--shared"])
        .arg(file)
        .args(args)
        .current_dir(dir)
        .env_remove("LUGHA_CC")
        .output()
        .expect("lughac runs")
}

/// Runs `program` and asserts it succeeded.
fn ok(mut program: Command, what: &str) -> String {
    let out = program.output().unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(
        out.status.success(),
        "{what}: {:?}\n{}{}",
        out.status,
        text(&out.stdout),
        text(&out.stderr)
    );
    text(&out.stdout)
}

/// `tests/jni/words.la` built with `--jni` (and `extra`) into `t`.
fn library(t: &Temp, extra: &[&str]) {
    let mut args = vec!["--jni", "com.example.Words", "-o", "libwords.so"];
    args.extend(extra);
    let built = lughac(&t.0, &root().join("tests/jni/words.la"), &args);
    assert!(built.status.success(), "{}", text(&built.stderr));
}

#[test]
fn kotlin_calls_the_library_through_the_object_lughac_writes() {
    if !cross::ready("jni_kotlin", &["javac", "kotlinc"]) {
        return;
    }
    let t = Temp::new();
    library(&t, &[]);
    let class = t.0.join("com/example/Words.kt");
    assert!(
        class.is_file(),
        "the Kotlin object is written beside the library"
    );
    let mut kotlinc = Command::new("kotlinc");
    kotlinc
        .arg(&class)
        .arg(root().join("tests/jni/Main.kt"))
        .args(["-include-runtime", "-d", "main.jar"])
        .current_dir(&t.0);
    ok(kotlinc, "kotlinc");
    let mut java = Command::new("java");
    java.arg(format!("-Djava.library.path={}", t.0.display()))
        .args(["-jar", "main.jar"])
        .current_dir(&t.0);
    let out = ok(java, "java");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines[..9],
        [
            "4",
            "hello, Ada",
            "hello, you",
            "true",
            "4",
            "true",
            "false true",
            "65",
            "1234"
        ],
        "{out}"
    );
    assert_eq!(lines[9], "1.5");
    assert!(
        lines[10].starts_with("Panic: index out of bounds: the length is 3 but the index is 7"),
        "{out}"
    );
    assert_eq!(
        lines[11],
        "refused: a string holding a NUL character can't reach Lugha"
    );
    assert_eq!(lines[12..], ["2", "four threads answered"], "{out}");
}

#[test]
fn java_calls_the_library_through_the_class_lughac_writes() {
    if !cross::ready("jni_java", &["javac"]) {
        return;
    }
    let t = Temp::new();
    library(&t, &["--jni-lang", "java"]);
    let class = t.0.join("com/example/Words.java");
    assert!(
        class.is_file(),
        "the Java class is written beside the library"
    );
    let mut javac = Command::new("javac");
    javac
        .args(["-encoding", "UTF-8", "-d", "classes"])
        .arg(&class)
        .arg(root().join("tests/jni/Main.java"))
        .current_dir(&t.0);
    ok(javac, "javac");
    let mut java = Command::new("java");
    java.arg(format!("-Djava.library.path={}", t.0.display()))
        .args(["-cp", "classes", "Main"])
        .current_dir(&t.0);
    let out = ok(java, "java");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines[..4],
        ["4", "hello, you", "4", "null: text is null"],
        "{out}"
    );
    assert!(lines[4].starts_with("Panic: index out of bounds"), "{out}");
    assert_eq!(lines[5], "3");
}

#[test]
fn what_java_cant_call_is_e0487() {
    let t = Temp::new();
    for (src, says) in [
        ("extern fun f(n: u8): i64 = 0;", "`u8`"),
        (
            "extern struct P { x: f64 }\nextern fun f(p: P): f64 = p.x;",
            "`P`",
        ),
        ("extern fun f(): u64 = 0;", "`u64` as a result"),
        (
            "extern fun word_count(): i64 = 0;\nextern fun wordCount(): i64 = 1;",
            "two methods `wordCount`",
        ),
    ] {
        let file = t.0.join("lib.la");
        std::fs::write(&file, src).unwrap();
        let out = lughac(&t.0, &file, &["--jni", "com.example.W"]);
        assert_eq!(out.status.code(), Some(1), "{src}");
        let said = text(&out.stderr);
        assert!(
            said.contains("E0487") && said.contains(says),
            "{src}: {said}"
        );
    }
    let file = t.0.join("lib.la");
    std::fs::write(&file, "extern fun new(): i64 = 0;").unwrap();
    let java = lughac(
        &t.0,
        &file,
        &["--jni", "com.example.W", "--jni-lang", "java"],
    );
    assert!(
        text(&java.stderr).contains("a word Java reserves"),
        "{}",
        text(&java.stderr)
    );
    // Kotlin escapes its own keywords instead.
    std::fs::write(&file, "extern fun object(): i64 = 0;").unwrap();
    let kotlin = lughac(&t.0, &file, &["--jni", "com.example.W", "-o", "libw.so"]);
    assert!(kotlin.status.success(), "{}", text(&kotlin.stderr));
    let source = std::fs::read_to_string(t.0.join("com/example/W.kt")).unwrap();
    assert!(source.contains("external fun `object`(): Long"), "{source}");
}

#[test]
fn jni_needs_shared_and_a_class_java_can_hold() {
    let t = Temp::new();
    let file = root().join("tests/jni/words.la");
    let without = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .arg("build")
        .arg(&file)
        .args(["--jni", "com.example.Words"])
        .current_dir(&t.0)
        .output()
        .expect("lughac runs");
    assert_eq!(without.status.code(), Some(2));
    let bad = lughac(&t.0, &file, &["--jni", "com..Words"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(text(&bad.stderr).contains("needs a class name Java can hold"));
}
