//! What `lughac test` gives a test: a fresh folder, a fixed clock, an empty
//! environment, seeded randomness and a loopback-only network; the real world
//! only to `@test(integration)` tests, under `--integration`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder holding `files`, removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn with(files: &[(&str, &str)]) -> Temp {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-caps-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("writable");
        for (name, text) in files {
            std::fs::write(dir.join(name), text).expect("writable");
        }
        Temp(dir)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn lughac(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", cwd)
        .output()
        .expect("lughac runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Plain tests with each stand-in, and one integration test.
const TESTS: &str = r#"import std.time;

fun stamp(clock: Clock): string {
    let c = time.civil(clock.now_ms(), 0).unwrap();
    "{c.year}-{c.month:02}-{c.day:02}"
}

@test
fun files_land_in_a_fresh_folder(dir: Dir) {
    assert(dir.list(".").unwrap().len == 0);
    dir.write_file("note.txt", "hi").unwrap();
    assert_eq(dir.read_file("note.txt").unwrap(), "hi");
    assert(dir.read_file("/etc/hostname").is_err());
    assert(dir.read_file("../x").is_err());
}

@test
fun the_date_is_fixed(clock: Clock, env: Env) {
    assert_eq(stamp(clock), "2000-01-01");
    assert(env.get("HOME").is_none());
}

@test
fun randomness_repeats(entropy: Entropy) {
    assert_eq(entropy.next_u64(), 16294208416658607535);
}

@test
fun loopback_only(net: Net) {
    let l = net.listen("127.0.0.1:0").unwrap();
    let c = net.connect(l.addr()).unwrap();
    c.close();
    l.close();
    assert(net.connect("example.com:80").is_err());
}

@test(integration)
fun the_real_world(fs: Fs, proc: Proc) {
    assert(proc.run("true", []).unwrap().code == 0);
}

fun main() {}
"#;

// Unix only for now: on Windows a program holding `Dir` without `Fs` runs at
// Low integrity, which can't write even its own folder.
#[cfg(unix)]
#[test]
fn plain_tests_get_stand_ins_and_integration_tests_wait_for_the_flag() {
    let t = Temp::with(&[("t.la", TESTS)]);
    let out = lughac(&t.0, &["test", "t.la"]);
    assert_eq!(
        (out.status.code(), text(&out.stdout)),
        (
            Some(0),
            "test t.files_land_in_a_fresh_folder ... ok\n\
             test t.the_date_is_fixed ... ok\n\
             test t.randomness_repeats ... ok\n\
             test t.loopback_only ... ok\n\
             test t.the_real_world ... skipped (integration: run with --integration)\n\
             \n5 tests: 4 passed, 0 failed, 1 skipped\n"
                .to_string()
        ),
        "{}",
        text(&out.stderr)
    );
    let out = lughac(&t.0, &["test", "--integration", "t.la"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
    assert!(text(&out.stdout).ends_with("5 tests: 5 passed, 0 failed\n"));
    let out = lughac(&t.0, &["test", "--json", "--filter", "real_world", "t.la"]);
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("\"status\":\"skipped\"")
            && stdout.contains("integration: run with --integration"),
        "{stdout}"
    );
    // Nothing a plain test wrote is left beside the program.
    assert!(!t.0.join("note.txt").exists());
}

// Unix only for now: on Windows a program holding `Dir` without `Fs` runs at
// Low integrity, which can't write even its own folder.
#[cfg(unix)]
#[test]
fn each_test_gets_its_own_folder_removed_afterwards_even_on_failure() {
    let src = "@test\n\
               fun fails(console: Console, dir: Dir) {\n\
                   dir.write_file(\"left.txt\", \"x\").unwrap();\n\
                   console.println(dir.path().unwrap());\n\
                   assert(false);\n\
               }\n\
               @test\n\
               fun sees_an_empty_one(dir: Dir) {\n\
                   assert(!dir.exists(\"left.txt\"));\n\
                   assert(!dir.write_file(\"/tmp/x\", \"y\").is_ok());\n\
               }\n\
               fun main() {}\n";
    let t = Temp::with(&[("f.la", src)]);
    let out = lughac(&t.0, &["test", "f.la"]);
    assert_eq!(out.status.code(), Some(1));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("test f.sees_an_empty_one ... ok"),
        "{stdout}"
    );
    let folder = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("/").map(|p| format!("/{p}")))
        .unwrap_or_else(|| panic!("the failed test's output names its folder: {stdout}"));
    assert!(!Path::new(&folder).exists(), "{folder} was left behind");
}

// Unix only: it links with `ln -s`; Windows' confinement has its own cases
// under `tests/windows/confined/`.
#[cfg(unix)]
#[test]
fn a_test_folder_is_confined_by_the_kernel_even_against_its_own_links() {
    let src = "@test(integration)\n\
               fun links_lead_nowhere(dir: Dir, proc: Proc) {\n\
                   let base = dir.path().unwrap();\n\
                   let r = proc.run(\"ln\", [\"-s\", \"/\", base + \"/root\"]).unwrap();\n\
                   assert_eq(r.code, 0);\n\
                   assert(dir.list(\"root\").is_err());\n\
                   assert(dir.read_file(\"root/etc/passwd\").is_err());\n\
                   dir.write_file(\"kept.txt\", \"x\").unwrap();\n\
                   assert(dir.exists(\"kept.txt\"));\n\
               }\n\
               fun main() {}\n";
    let t = Temp::with(&[("l.la", src)]);
    let out = lughac(&t.0, &["test", "--integration", "l.la"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
}

#[test]
fn only_an_integration_test_takes_fs_or_proc() {
    for params in ["fs: Fs", "proc: Proc"] {
        let src = format!("@test\nfun t({params}) {{}}\nfun main() {{}}\n");
        let t = Temp::with(&[("e.la", &src)]);
        let out = lughac(&t.0, &["test", "e.la"]);
        let stderr = text(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{stderr}");
        assert!(
            stderr.contains("error[E0434]: only an integration test is given `Fs` and `Proc`")
                && stderr.contains("@test(integration)"),
            "{stderr}"
        );
    }
}

#[test]
fn the_formatter_keeps_the_integration_mark() {
    let src = "@test(integration)\nfun real() {}\n\nfun main() {}\n";
    let t = Temp::with(&[("m.la", src)]);
    let out = lughac(&t.0, &["fmt", "--check", "m.la"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
}
