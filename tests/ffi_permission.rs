//! The `ffi` permission end to end: a package declaring C code (`extern`,
//! `[link]`) builds only when the program's `lugha.toml` grants it or a
//! package depending on it; `lugha.lock` records who may run C, and
//! `lughac update` says when a package newly may.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder, removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Temp {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-ffi-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }

    fn write(&self, path: &str, text: &str) {
        let file = self.0.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    /// Package `name` in `name/`, its `lib.la` being `lib`, listing `deps`
    /// (each a sibling folder), with `extra` lines in its manifest.
    fn package(&self, name: &str, lib: &str, deps: &[&str], extra: &str) {
        let mut manifest = format!(
            "[package]\nname = \"{name}\"\nversion = \"1.0.0\"\nsource = \"src\"\n{extra}\n"
        );
        if !deps.is_empty() {
            manifest.push_str("[packages]\n");
            for d in deps {
                manifest.push_str(&format!("{d} = {{ path = \"../{d}\" }}\n"));
            }
        }
        self.write(&format!("{name}/lugha.toml"), &manifest);
        self.write(&format!("{name}/src/lib.la"), lib);
    }

    /// The program in `app/`: its manifest's `[packages]` lines, and a main
    /// printing `body`.
    fn app(&self, packages: &str, imports: &str, body: &str) {
        self.write("app/lugha.toml", &format!("[packages]\n{packages}"));
        self.write(
            "app/main.la",
            &format!(
                "{imports}\nfun main(console: Console) {{\n    console.println({body});\n}}\n"
            ),
        );
    }

    fn lughac(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(args)
            .arg("main.la")
            .current_dir(self.0.join("app"))
            .env("HOME", &self.0)
            .env("LUGHA_CACHE", self.0.join("cache"))
            .output()
            .expect("lughac runs")
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

const SQLITE: &str = "import zlib;\n\nextern fun abs(n: i32): i32;\n\npublic fun size(): i32 = abs(-3) + zlib.level();\n";
const ZLIB: &str =
    "extern fun labs(n: i64): i64;\n\npublic fun level(): i32 = labs(-4).truncate<i32>();\n";

/// `sqlite` (C) depending on `zlib` (C), and a program using `sqlite`.
fn chain(t: &Temp, grant: &str) {
    t.package("sqlite", SQLITE, &["zlib"], "");
    t.package("zlib", ZLIB, &[], "");
    t.app(
        &format!("sqlite = {{ path = \"../sqlite\"{grant} }}\n"),
        "import sqlite;",
        "sqlite.size()",
    );
}

#[test]
fn ungranted_c_code_is_e0333_with_the_entry_to_change() {
    let t = Temp::new();
    chain(&t, "");
    let out = t.lughac(&["check"]);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains(
            "error[E0333]: package `sqlite` declares C code (`extern fun abs`) but has no `ffi` permission"
        ) && stderr.contains("add `allow = [\"ffi\"]` to `sqlite`'s entry in lugha.toml"),
        "{stderr}"
    );
    assert!(
        stderr.contains("package `zlib` declares C code (`extern fun labs`)")
            && stderr
                .contains("to `sqlite`'s entry in lugha.toml (`zlib` is one of its dependencies)"),
        "{stderr}"
    );
}

#[test]
fn a_grant_covers_the_package_and_its_dependencies() {
    let t = Temp::new();
    chain(&t, ", allow = [\"ffi\"]");
    let out = t.lughac(&["run"]);
    assert_eq!(
        (out.status.code(), text(&out.stdout)),
        (Some(0), "7\n".to_string()),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn extern_types_and_link_tables_need_it_and_the_program_never_does() {
    let t = Temp::new();
    t.package(
        "handles",
        "extern type Db;\n\npublic fun one(): i64 = 1;\n",
        &[],
        "",
    );
    // A C file that doesn't compile: refused before anything compiles it.
    t.package(
        "linked",
        "public fun two(): i64 = 2;\n",
        &[],
        "\n[link]\nc = [\"broken.c\"]\n",
    );
    t.write("linked/broken.c", "this is not C\n");
    t.package("pure", "public fun three(): i64 = 3;\n", &[], "");
    t.app(
        "handles = { path = \"../handles\" }\nlinked = { path = \"../linked\" }\npure = { path = \"../pure\", allow = [\"ffi\"] }\n",
        "import handles;\nimport linked;\nimport pure;\nextern fun abs(n: i32): i32;",
        "handles.one() + linked.two() + pure.three() + abs(-1) as i64",
    );
    let out = t.lughac(&["run"]);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("package `handles` declares C code (`extern type Db`)")
            && stderr.contains("package `linked` declares C code (a `[link]` table)")
            && !stderr.contains("broken.c")
            && !stderr.contains("`pure`")
            && !stderr.contains("`abs`"),
        "{stderr}"
    );
}

#[test]
fn only_the_program_grants() {
    let t = Temp::new();
    t.package("sqlite", SQLITE, &[], "");
    t.write(
        "sqlite/lugha.toml",
        "[package]\nname = \"sqlite\"\nversion = \"1.0.0\"\nsource = \"src\"\n\n[packages]\nzlib = { path = \"../zlib\", allow = [\"ffi\"] }\n",
    );
    t.package("zlib", ZLIB, &[], "");
    t.app(
        "sqlite = { path = \"../sqlite\", allow = [\"ffi\"] }\n",
        "import sqlite;",
        "sqlite.size()",
    );
    let stderr = text(&t.lughac(&["check"]).stderr);
    assert!(
        stderr.contains(
            "error[E0321]: bad `lugha.toml` line: only the program's lugha.toml grants permissions"
        ),
        "{stderr}"
    );
}

/// A package reading C's memory through a `CPtr` it is handed, and the
/// program's own C, which needs no grant, to make one.
const REPORT: &str = "public fun peek(p: CPtr): u8 = p.view(1, fun(v: View<u8>) = v[0]);\n";
const CALLOC: &str = "extern fun calloc(n: u64, size: u64): CPtr;";

#[test]
fn an_ungranted_package_viewing_c_memory_is_e0333() {
    let t = Temp::new();
    t.package("report", REPORT, &[], "");
    t.app(
        "report = { path = \"../report\" }\n",
        &format!("import report;\n{CALLOC}"),
        "report.peek(calloc(1, 8))",
    );
    let out = t.lughac(&["check"]);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains(
            "error[E0333]: package `report` reads C's memory (`p.view`) but has no `ffi` permission"
        ) && stderr.contains("add `allow = [\"ffi\"]` to `report`'s entry in lugha.toml"),
        "{stderr}"
    );
}

#[test]
fn a_grant_lets_a_package_view_c_memory_directly_or_through_a_dependent() {
    let t = Temp::new();
    t.package("report", REPORT, &[], "");
    t.app(
        "report = { path = \"../report\", allow = [\"ffi\"] }\n",
        &format!("import report;\n{CALLOC}"),
        "report.peek(calloc(1, 8))",
    );
    let out = t.lughac(&["run"]);
    assert_eq!(
        (out.status.code(), text(&out.stdout)),
        (Some(0), "0\n".to_string()),
        "{}",
        text(&out.stderr)
    );
    let t = Temp::new();
    t.package("report", REPORT, &[], "");
    t.package(
        "outer",
        "import report;\n\npublic fun peek(p: CPtr): u8 = report.peek(p);\n",
        &["report"],
        "",
    );
    t.app(
        "outer = { path = \"../outer\", allow = [\"ffi\"] }\n",
        &format!("import outer;\n{CALLOC}"),
        "outer.peek(calloc(1, 8))",
    );
    let out = t.lughac(&["run"]);
    assert_eq!(
        (out.status.code(), text(&out.stdout)),
        (Some(0), "0\n".to_string()),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn passing_a_cptr_or_taking_a_view_needs_no_grant() {
    let t = Temp::new();
    t.package(
        "pass",
        "public fun along(p: CPtr): CPtr = p;\n\npublic fun first(v: View<u8>): u8 = v[0];\n",
        &[],
        "",
    );
    t.app(
        "pass = { path = \"../pass\" }\n",
        &format!("import pass;\n{CALLOC}"),
        "pass.along(calloc(1, 8)).view(1, fun(v: View<u8>) = pass.first(v))",
    );
    let out = t.lughac(&["run"]);
    assert_eq!(
        (out.status.code(), text(&out.stdout)),
        (Some(0), "0\n".to_string()),
        "{}",
        text(&out.stderr)
    );
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_lock_records_who_runs_c_and_update_says_when_that_grows() {
    let t = Temp::new();
    let repo = t.0.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    let release = |lib: &str, tag: &str| {
        std::fs::write(repo.join("lib.la"), lib).unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", tag]);
        git(&repo, &["tag", tag]);
    };
    release("public fun size(): i64 = 1;\n", "v1.0.0");
    // Windows' `C:\x` is the URL `file:///C:/x`.
    let path = repo.display().to_string().replace('\\', "/");
    let url = format!(
        "file://{}{path}",
        if path.starts_with('/') { "" } else { "/" }
    );
    t.app(
        &format!("geo = {{ git = \"{url}\", version = \"1\", allow = [\"ffi\"] }}\n"),
        "import geo;",
        "geo.size()",
    );
    let first = t.lughac(&["run"]);
    assert_eq!(text(&first.stdout), "1\n", "{}", text(&first.stderr));
    let lock = std::fs::read_to_string(t.0.join("app/lugha.lock")).unwrap();
    assert!(!lock.contains("[ffi]"), "pure so far: {lock}");
    // The next release runs C code: still granted, now visibly so.
    release(
        "extern fun labs(n: i64): i64;\n\npublic fun size(): i64 = labs(-2);\n",
        "v1.1.0",
    );
    let update = t.lughac(&["update"]);
    assert_eq!(
        (update.status.code(), text(&update.stdout)),
        (
            Some(0),
            "geo now runs C code, through its own ffi grant\n".to_string()
        ),
        "{}",
        text(&update.stderr)
    );
    let lock = std::fs::read_to_string(t.0.join("app/lugha.lock")).unwrap();
    assert!(
        lock.contains("[ffi]\ngeo = { through = \"geo\" }\n"),
        "{lock}"
    );
    assert_eq!(text(&t.lughac(&["run"]).stdout), "2\n");
}
