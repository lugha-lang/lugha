//! Tasks on several worker threads (`LUGHA_WORKERS`): they run in
//! parallel, every concurrency program behaves with four workers, and the
//! runtime's scheduling is clean under ThreadSanitizer.

// Only the runner's discovery is used here: it runs each program itself,
// several times over one build, so the rest of the shared runner goes unused.
#[allow(dead_code, unused_imports)]
mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use support::runner::{Mode, discover, format_malformed};

/// A program in a temporary folder of its own (with C beside it when
/// given), removed on drop.
struct Program(PathBuf);

impl Program {
    fn new(source: &str, c: Option<&str>) -> Program {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-wk-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.la"), source).unwrap();
        if let Some(c) = c {
            std::fs::write(dir.join("side.c"), c).unwrap();
            std::fs::write(dir.join("lugha.toml"), "[link]\nc = [\"side.c\"]\n").unwrap();
        }
        Program(dir)
    }

    /// Builds it at -O2 into `main` beside it.
    fn build(&self) -> PathBuf {
        let exe = self.0.join("main");
        let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(["build", "main.la", "-O2", "-o", "main"])
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .output()
            .expect("lughac runs");
        assert!(out.status.success(), "{}", text(&out.stderr));
        exe
    }

    /// Runs it with `workers` workers.
    fn run(&self, workers: &str) -> Output {
        Command::new(self.build())
            .env("LUGHA_WORKERS", workers)
            .output()
            .expect("the program runs")
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Eight tasks, each busy for a while, then telling `main` through a
/// channel which thread it is on (C's answer); `main` prints how many
/// different threads answered.
const THREADS: &str = "extern fun thread_id(): i64;\n\
    fun busy(n: i64, out: Sender<i64>) {\n\
        let mut s = 0;\n\
        for i in 0..n { s += i % 7; }\n\
        out.send(thread_id() + s - s);\n\
    }\n\
    fun main(console: Console) {\n\
        let (tx, rx) = Chan<i64>.new(8);\n\
        let r = scope s {\n\
            for _ in 0..8 { s.spawn(busy(3_000_000, tx)); }\n\
        };\n\
        r.unwrap();\n\
        tx.close();\n\
        let mut ids: i64[] = [];\n\
        for id of rx {\n\
            let mut seen = false;\n\
            for known of ids { if known == id { seen = true; } }\n\
            if !seen { ids.push(id); }\n\
        }\n\
        console.println(ids.len);\n\
    }\n";

#[test]
fn tasks_run_on_several_threads() {
    let program = Program::new(
        THREADS,
        Some("#include <pthread.h>\nlong thread_id(void) { return (long)pthread_self(); }\n"),
    );
    let four = program.run("4");
    assert_eq!(four.status.code(), Some(0), "{}", text(&four.stderr));
    let threads: i64 = text(&four.stdout).trim().parse().unwrap();
    assert!(threads >= 2, "{threads} threads");
    let one = program.run("1");
    assert_eq!(text(&one.stdout), "1\n", "{}", text(&one.stderr));
}

#[test]
fn busy_tasks_add_up_with_one_or_four_workers() {
    let program = Program::new(
        "fun busy(n: i64, out: Sender<i64>) {\n\
             let mut s = 0;\n\
             for i in 0..n { s += i % 7; }\n\
             out.send(s);\n\
         }\n\
         fun main(console: Console) {\n\
             let (tx, rx) = Chan<i64>.new(4);\n\
             let r = scope s {\n\
                 for _ in 0..4 { s.spawn(busy(2_000_000, tx)); }\n\
             };\n\
             r.unwrap();\n\
             tx.close();\n\
             let mut sum = 0;\n\
             for v of rx { sum += v; }\n\
             console.println(sum);\n\
         }\n",
        None,
    );
    for workers in ["1", "4"] {
        let out = program.run(workers);
        assert_eq!(text(&out.stdout), "23999980\n", "{}", text(&out.stderr));
    }
}

#[test]
fn a_bad_worker_count_ends_the_program_at_start() {
    let program = Program::new("fun main(console: Console) { console.println(1); }\n", None);
    for bad in ["0", "x", "257"] {
        let out = program.run(bad);
        assert_eq!(out.status.code(), Some(101), "{bad}");
        assert_eq!(
            text(&out.stderr),
            "panic: LUGHA_WORKERS must be from 1 to 256\n",
            "{bad}"
        );
    }
}

#[test]
fn a_deadlock_across_workers_is_reported() {
    let out = Program::new(
        "fun wait(r: Receiver<i64>): i64 = r.recv().unwrap();\n\
         fun main(console: Console) {\n\
             let (tx, rx) = Chan<i64>.new(0);\n\
             let r = scope s {\n\
                 let a = s.spawn(wait(rx));\n\
                 a.join().unwrap()\n\
             };\n\
             tx.close();\n\
             console.println(r.is_ok());\n\
         }\n",
        None,
    )
    .run("4");
    assert_eq!(out.status.code(), Some(101));
    assert!(
        text(&out.stderr).starts_with("panic: deadlock: every green thread is waiting\n"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn ctrl_c_cancels_tasks_on_every_worker() {
    let program = Program::new(
        "import std.task;\n\
         fun spin(n: i64): i64 { let mut s = 0; loop { s += n; task.sleep(1); } }\n\
         fun main(console: Console) {\n\
             let r = scope s {\n\
                 for i in 0..6 { s.spawn(spin(i)); }\n\
             };\n\
             console.println(r.is_ok());\n\
         }\n",
        None,
    );
    let exe = program.build();
    let mut child = Command::new(exe)
        .env("LUGHA_WORKERS", "4")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the program starts");
    std::thread::sleep(Duration::from_millis(300));
    let pid = child.id().to_string();
    let sent = Command::new("kill").args(["-INT", &pid]).status().unwrap();
    assert!(sent.success());
    let status = child.wait().expect("it ends");
    assert_eq!(status.code(), Some(130));
}

#[test]
fn c_overflowing_its_stack_on_a_worker_is_reported() {
    let out = Program::new(
        "extern fun deep(n: i64): i64;\n\
         fun dig(n: i64): i64 = deep(n);\n\
         fun main(console: Console) {\n\
             let r = scope s {\n\
                 for i in 0..4 { s.spawn(dig(i)); }\n\
             };\n\
             console.println(r.is_ok());\n\
         }\n",
        Some(
            "long deep(long n) {\n\
                 volatile char pad[512];\n\
                 pad[0] = (char)n;\n\
                 return deep(n + 1) + pad[0];\n\
             }\n",
        ),
    )
    .run("4");
    assert_eq!(out.status.code(), Some(101), "{}", text(&out.stderr));
    assert_eq!(text(&out.stderr), "panic: stack overflow\n");
}

/// Concurrency programs whose own output lines depend on how tasks
/// interleave, or that end with objects still live, and why.
const LEAKING: &[&str] = &[
    // The waiting tasks aren't unwound.
    "actors/deadlock.la",
    "green_threads/deadlock.la",
    "channels/deadlock.la",
    // Ends the program unwound halfway.
    "deadlines/shield_value_lost.la",
    // `main` panics while the root scope holds the actor's task.
    "actors/after_stop.la",
];

/// Programs whose lines depend on the order tasks run in, and why.
const ORDERED: &[&str] = &[
    // Names a task after starting it: a task failing first reports its
    // own name, `bad`, not `first`.
    "scope_policies/handler.la",
];

/// `command`'s output, or None if it ran past `limit` (then killed): a hang
/// fails the sweep with the program's name instead of stopping it.
fn output_within(mut command: Command, limit: Duration) -> Option<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the program runs");
    let started = std::time::Instant::now();
    while child.try_wait().expect("waits").is_none() {
        if started.elapsed() > limit {
            // Killing a child that just ended is harmless: it's reaped next.
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Some(child.wait_with_output().expect("it ended"))
}

/// The lines of `bytes`, sorted: what four workers must print, in any order.
fn lines(bytes: &[u8]) -> Vec<String> {
    let mut lines: Vec<String> = text(bytes).lines().map(str::to_string).collect();
    lines.sort();
    lines
}

#[test]
fn concurrency_programs_behave_on_four_workers() {
    let workdir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = workdir.join("tests/programs/concurrency");
    let cases = match discover(&root) {
        Ok(cases) => cases,
        Err(malformed) => panic!("\n{}", format_malformed(&malformed)),
    };
    let scratch = std::env::temp_dir().join(format!("lugha-wk-all-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let mut failures = Vec::new();
    for case in cases.iter().filter(|c| matches!(c.mode, Mode::Run { .. })) {
        let Mode::Run { stdout, exit, .. } = &case.mode else {
            unreachable!("filtered to runs")
        };
        let exe = scratch.join("program");
        // Built for the owner check, which the last run turns on.
        // From the repository, by a relative path, as the runner builds:
        // panics print where they happened by it.
        let built = Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(["build", "-O2", "-o"])
            .arg(&exe)
            .arg(case.path.strip_prefix(workdir).unwrap_or(&case.path))
            .current_dir(workdir)
            .env("LUGHA_RC_CHECK", "1")
            .output()
            .expect("lughac runs");
        assert!(built.status.success(), "{}", case.name);
        for run in 0..5 {
            let mut command = Command::new(&exe);
            command
                .args(&case.extras.args)
                .envs(case.extras.envvars.iter().map(|(k, v)| (k, v)))
                .env("LUGHA_WORKERS", "4")
                .current_dir(&scratch);
            let checked = run == 4;
            if checked {
                command
                    .env("LUGHA_RC_CHECK", "1")
                    .env("LUGHA_RC_REPORT", "1");
            }
            let Some(out) = output_within(command, Duration::from_secs(30)) else {
                failures.push(format!("{} (run {run}): hung", case.name));
                continue;
            };
            let mut wrong = Vec::new();
            if out.status.code() != Some(*exit) {
                wrong.push(format!("exit {:?}", out.status.code()));
            }
            let ordered = ORDERED.iter().any(|o| case.name == *o);
            if !ordered && lines(&out.stdout) != lines(stdout) {
                wrong.push(format!("stdout {:?}", text(&out.stdout)));
            }
            let stderr = text(&out.stderr);
            if checked && stderr.contains("unshared object") {
                wrong.push(stderr.clone());
            }
            let leaked = !stderr.contains(" 0 live") && stderr.contains(" live");
            if checked && leaked && !LEAKING.iter().any(|l| case.name == *l) {
                wrong.push(format!("leaked: {stderr}"));
            }
            if !wrong.is_empty() {
                failures.push(format!("{} (run {run}): {}", case.name, wrong.join("; ")));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A C harness that includes the runtime, as `tests/waiting_runtime.rs`'s
/// does: 64 tasks on four workers pass tokens round a ring of channels and
/// are joined.
const HARNESS: &str = r#"#include "lugha_rt.c"

enum { TASKS = 64, ROUNDS = 200 };
static void *ring[TASKS];

static void relay(void *code, void *env, void *result) {
    int64_t me = (int64_t)(intptr_t)code;
    int64_t v;
    for (int r = 0; r < ROUNDS; r++) {
        lugha_rt_chan_recv(ring[me], &v);
        v += 1;
        lugha_rt_chan_send(ring[(me + 1) % TASKS], &v, "h", 1, 1);
    }
    *(int64_t *)result = v;
    (void)env;
}

int main(int argc, char **argv) {
    lugha_rt_init(argc, argv);
    // As lockdown does: from here on, workers may start.
    helpers_allowed = true;
    for (int i = 0; i < TASKS; i++) {
        ring[i] = lugha_rt_chan_new(1, 8, "h", 1, 1);
    }
    void *tasks[TASKS];
    for (int i = 0; i < TASKS; i++) {
        tasks[i] = lugha_rt_task_spawn(relay, (void *)(intptr_t)i, lugha_rt_alloc(8), 8);
    }
    int64_t start = 0;
    lugha_rt_chan_send(ring[0], &start, "h", 1, 1);
    int64_t last = 0;
    for (int i = 0; i < TASKS; i++) {
        int64_t v = *(int64_t *)lugha_rt_task_join(tasks[i]);
        last = v > last ? v : last;
    }
    printf("%ld %d\n", (long)last, workers_running);
    return 0;
}
"#;

/// Builds the harness once per flavour: plain, or with ThreadSanitizer.
fn harness(tsan: bool) -> &'static PathBuf {
    static PLAIN: OnceLock<PathBuf> = OnceLock::new();
    static TSAN: OnceLock<PathBuf> = OnceLock::new();
    let built = if tsan { &TSAN } else { &PLAIN };
    built.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!(
            "lugha-wk-h-{}-{}",
            std::process::id(),
            u8::from(tsan)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir is writable");
        std::fs::write(dir.join("lugha_rt.c"), lugha::link::RUNTIME_SOURCE).expect("writable");
        std::fs::write(dir.join("harness.c"), HARNESS).expect("writable");
        let exe = dir.join("harness");
        let mut cc = Command::new("cc");
        cc.args(["-std=gnu11", "-O1", "-pthread", "-funwind-tables", "-I"])
            .arg(&dir);
        if tsan {
            cc.args(["-fsanitize=thread", "-g"]);
        }
        let out = cc
            .arg(dir.join("harness.c"))
            .args(["-lm", "-o"])
            .arg(&exe)
            .output()
            .expect("cc runs");
        assert!(out.status.success(), "{}", text(&out.stderr));
        exe
    })
}

fn run_harness(tsan: bool) {
    let out = Command::new(harness(tsan))
        .env("LUGHA_WORKERS", "4")
        .output()
        .expect("harness runs");
    let stderr = text(&out.stderr);
    assert!(out.status.success() && stderr.is_empty(), "{stderr}");
    assert_eq!(text(&out.stdout), format!("{} 4\n", 64 * 200));
}

#[test]
fn a_ring_of_tasks_passes_its_token_on_four_workers() {
    run_harness(false);
}

#[test]
fn threadsanitizer_finds_no_race_in_the_scheduler() {
    run_harness(true);
}
