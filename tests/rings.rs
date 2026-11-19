//! Rings filled from threads C made, end to end: a `direct` callback on
//! C's own pthread writes a `Ring<T>` without waiting, and tasks read it
//! as any `Receiver`. Each case runs on one worker and on four.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder holding one program, removed on drop.
struct Case(PathBuf);

impl Case {
    /// `main.la`, after the shared externs, and the shared C library.
    fn new(main: &str) -> Case {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-rings-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir is writable");
        std::fs::write(dir.join("main.la"), format!("{EXTERNS}{main}")).expect("writable");
        std::fs::write(dir.join("side.c"), SIDE).expect("writable");
        std::fs::write(dir.join("lugha.toml"), "[link]\nc = [\"side.c\"]\n").expect("writable");
        Case(dir)
    }

    /// Builds at -O2, then runs on `workers` workers with one more variable.
    fn run(&self, workers: &str, var: (&str, &str)) -> Output {
        let built = Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(["build", "main.la", "-O2", "-o", "main"])
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .output()
            .expect("lughac runs");
        assert!(built.status.success(), "{}", text(&built.stderr));
        Command::new(self.0.join("main"))
            .env("LUGHA_WORKERS", workers)
            .env(var.0, var.1)
            .output()
            .expect("it runs")
    }

    /// On one worker and on four: exit `code`, exactly `stdout`, and
    /// stderr containing `stderr`.
    fn runs(&self, code: i32, stdout: &str, stderr: &str) {
        for workers in ["1", "4"] {
            let out = self.run(workers, ("LUGHA_UNUSED", ""));
            assert_eq!(
                (out.status.code(), text(&out.stdout).as_str()),
                (Some(code), stdout),
                "workers {workers}, stderr: {}",
                text(&out.stderr)
            );
            assert!(text(&out.stderr).contains(stderr), "{}", text(&out.stderr));
        }
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Threads of C's own: one calling a callback `n` times (after a pause),
/// or `n` threads each calling once; the results a callback with a result
/// gave, summed.
/// C threads of its own, each calling a callback `n` times (after a
/// pause) and summing what it gave back.
const SIDE: &str = "#include <pthread.h>\n\
    #include <stdlib.h>\n\
    #include <unistd.h>\n\
    typedef long (*cbr)(long, void *);\n\
    typedef struct { cbr g; void *u; long n; long pause_ms; } job;\n\
    static pthread_t threads[64]; static int started;\n\
    static long sum;\n\
    static void *calls(void *p) {\n\
        job *j = p; long s = 0;\n\
        if (j->pause_ms) usleep(j->pause_ms * 1000);\n\
        for (long i = 0; i < j->n; i++) s += j->g(i, j->u);\n\
        __atomic_add_fetch(&sum, s, __ATOMIC_SEQ_CST);\n\
        free(j);\n\
        return 0;\n\
    }\n\
    void on_threads(cbr g, void *u, long threads_n, long n, long pause_ms) {\n\
        for (long t = 0; t < threads_n; t++) {\n\
            job *j = malloc(sizeof *j); j->g = g; j->u = u; j->n = n; j->pause_ms = pause_ms;\n\
            pthread_create(&threads[started++], 0, calls, j);\n\
        }\n\
    }\n\
    void join_threads(void) { for (int i = 0; i < started; i++) pthread_join(threads[i], 0); started = 0; }\n\
    long given_sum(void) { return __atomic_load_n(&sum, __ATOMIC_SEQ_CST); }\n";

const EXTERNS: &str = "import std.task;\n\
    extern fun on_threads(f: direct fun(i64, data): i64, u: data, threads: i64, n: i64, pause_ms: i64);\n\
    extern blocking fun join_threads();\n\
    extern fun given_sum(): i64;\n";

#[test]
fn a_callback_on_cs_thread_fills_a_ring_a_task_drains() {
    Case::new(
        "fun main(console: Console) {\n\
             let (tx, rx) = Ring<i64>.new(1024);\n\
             let r = scope s {\n\
                 on_threads(fun(i: i64): i64 = match tx.try_send(i) { .Sent => 1, _ => 0 }, 1, 1000000, 0);\n\
                 let reader = s.spawn(fun(): (i64, bool) {\n\
                     let mut got = 0;\n\
                     let mut last = -1;\n\
                     let mut ordered = true;\n\
                     for v of rx { if v <= last { ordered = false; } last = v; got += 1; }\n\
                     (got, ordered)\n\
                 }());\n\
                 join_threads();\n\
                 tx.close();\n\
                 let (got, ordered) = reader.join().unwrap();\n\
                 console.println(\"{got == given_sum()} {ordered} {got > 0}\");\n\
             };\n\
             console.println(r.is_ok());\n\
         }\n",
    )
    .runs(0, "true true true\ntrue\n", "");
}

#[test]
fn a_task_waiting_on_an_empty_ring_wakes_when_c_writes() {
    Case::new(
        "fun main(console: Console) {\n\
             let (tx, rx) = Ring<i64>.new(4);\n\
             let r = scope s {\n\
                 on_threads(fun(i: i64): i64 = match tx.try_send(i + 41) { .Sent => 1, _ => 0 }, 1, 1, 100);\n\
                 console.println(rx.recv().unwrap());\n\
                 join_threads();\n\
             };\n\
             console.println(r.is_ok());\n\
         }\n",
    )
    .runs(0, "41\ntrue\n", "");
}

#[test]
fn two_writers_at_once_panic() {
    Case::new(
        "fun main(console: Console) {\n\
             let (tx, rx) = Ring<i64>.new(1);\n\
             let r = scope s independent {\n\
                 on_threads(fun(i: i64): i64 = match tx.try_send(i) { .Sent => 1, _ => 0 }, 4, 200000, 0);\n\
                 join_threads();\n\
             };\n\
             console.println(\"done\");\n\
         }\n",
    )
    .runs(0, "done\n", "a ring has one writer at a time");
}
