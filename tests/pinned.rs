//! Pinned handles and pinned scopes on four workers: each case is a
//! program with its own C file, which checks that every call on a pinned
//! value, and every call in a pinned scope, runs on the right thread while
//! tasks yield, sleep, wait on channels and call actors in between.

// Unix only: the C side names threads with POSIX calls (`pthread_self`,
// `gettid`).
#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder holding one program, removed on drop.
struct Case(PathBuf);

impl Case {
    /// `main.la` and `side.c`, linked by `[link] c = ["side.c"]`.
    fn new(main: &str) -> Case {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-pinned-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir is writable");
        std::fs::write(dir.join("main.la"), main).expect("writable");
        std::fs::write(dir.join("side.c"), SIDE).expect("writable");
        std::fs::write(dir.join("lugha.toml"), "[link]\nc = [\"side.c\"]\n").expect("writable");
        Case(dir)
    }

    /// Builds at -O2, then runs ten times on four workers: each run exits
    /// 0 and prints `stdout`.
    fn runs(&self, stdout: &str) {
        let built = Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(["build", "main.la", "-O2", "-o", "main"])
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .output()
            .expect("lughac runs");
        assert!(built.status.success(), "{}", text(&built));
        for _ in 0..10 {
            let out = Command::new(self.0.join("main"))
                .env("LUGHA_WORKERS", "4")
                .output()
                .expect("it runs");
            assert_eq!(
                (
                    out.status.code(),
                    String::from_utf8_lossy(&out.stdout).as_ref()
                ),
                (Some(0), stdout),
                "{}",
                text(&out)
            );
        }
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A context that remembers the thread that made it and counts calls made
/// on any other; the calling thread's id; whether it is the main thread.
const SIDE: &str = "#define _GNU_SOURCE\n\
    #include <pthread.h>\n\
    #include <stdlib.h>\n\
    #include <unistd.h>\n\
    typedef struct { pthread_t owner; long wrong; } ctx;\n\
    ctx *ctx_new(void) { ctx *c = malloc(sizeof *c); c->owner = pthread_self(); c->wrong = 0; return c; }\n\
    void ctx_draw(ctx *c, long n) { (void)n; if (!pthread_equal(c->owner, pthread_self())) c->wrong++; }\n\
    long ctx_wrong(ctx *c) { return c->wrong; }\n\
    #ifdef __APPLE__\n\
    long thread_id(void) { unsigned long long id; pthread_threadid_np(NULL, &id); return (long)id; }\n\
    long on_main_thread(void) { return pthread_main_np(); }\n\
    #else\n\
    long thread_id(void) { return (long)gettid(); }\n\
    long on_main_thread(void) { return getpid() == gettid(); }\n\
    #endif\n";

/// Busy unpinned tasks beside the pinned ones, so idle workers steal.
const BUSY: &str = "fun busy(n: i64, out: Sender<i64>) {\n\
         let mut s = 0;\n\
         for i in 0..n { s += i % 7; if i % 1000 == 0 { task.yield_now(); } }\n\
         out.send(s - s);\n\
     }\n";

#[test]
fn a_task_touching_a_pinned_handle_stays_on_its_thread() {
    Case::new(&format!(
        "import std.task;\n\
         extern type Ctx pinned;\n\
         extern fun ctx_new(): Ctx;\n\
         extern fun ctx_draw(c: Ctx, n: i64);\n\
         extern fun ctx_wrong(c: Ctx): i64;\n\
         {BUSY}\
         fun draw(n: i64, out: Sender<i64>) {{\n\
             let c = ctx_new();\n\
             let (tx, rx) = Chan<i64>.new(1);\n\
             for i in 0..n {{\n\
                 ctx_draw(c, i);\n\
                 task.yield_now();\n\
                 tx.send(i);\n\
                 rx.recv().unwrap();\n\
                 if i % 50 == 0 {{ task.sleep(1); }}\n\
             }}\n\
             out.send(ctx_wrong(c));\n\
         }}\n\
         fun main(console: Console) {{\n\
             let (tx, rx) = Chan<i64>.unbounded();\n\
             let r = scope s {{\n\
                 for _ in 0..2 {{ s.spawn(busy(2000000, tx)); }}\n\
                 for _ in 0..8 {{ s.spawn(draw(300, tx)); }}\n\
             }};\n\
             r.unwrap();\n\
             tx.close();\n\
             let mut wrong = 0;\n\
             for v of rx {{ wrong += v; }}\n\
             console.println(\"wrong {{wrong}}\");\n\
         }}\n"
    ))
    .runs("wrong 0\n");
}

#[test]
fn a_pinned_scope_keeps_its_body_tasks_nested_scopes_and_actors_on_one_thread() {
    Case::new(&format!(
        "import std.task;\n\
         extern fun thread_id(): i64;\n\
         extern fun on_main_thread(): i64;\n\
         {BUSY}\
         actor Probe {{\n\
             home: i64,\n\
             fun check(self): i64 = if thread_id() == self.home {{ 0 }} else {{ 1 }};\n\
         }}\n\
         fun nested(home: i64, out: Sender<i64>) {{\n\
             for _ in 0..50 {{ task.yield_now(); if thread_id() != home {{ out.send(1); }} }}\n\
         }}\n\
         fun member(home: i64, out: Sender<i64>) {{\n\
             let mut wrong = 0;\n\
             for i in 0..200 {{\n\
                 task.yield_now();\n\
                 if i % 40 == 0 {{ task.sleep(1); }}\n\
                 if thread_id() != home {{ wrong += 1; }}\n\
             }}\n\
             let r = scope inner {{ inner.spawn(nested(home, out)); }};\n\
             r.unwrap();\n\
             out.send(wrong);\n\
         }}\n\
         fun group(out: Sender<i64>) {{\n\
             let r = scope g pinned {{\n\
                 let home = thread_id();\n\
                 for _ in 0..4 {{ g.spawn(member(home, out)); }}\n\
                 for _ in 0..100 {{ task.yield_now(); if thread_id() != home {{ out.send(1); }} }}\n\
             }};\n\
             r.unwrap();\n\
         }}\n\
         fun main(console: Console) {{\n\
             let (tx, rx) = Chan<i64>.unbounded();\n\
             let r = scope outer {{\n\
                 for _ in 0..2 {{ outer.spawn(busy(2000000, tx)); }}\n\
                 outer.spawn(group(tx));\n\
                 let ui = scope ui pinned {{\n\
                     let home = thread_id();\n\
                     console.println(\"main thread {{on_main_thread()}}\");\n\
                     let p = ui.spawn(Probe {{ home: home }});\n\
                     for _ in 0..4 {{ ui.spawn(member(home, tx)); }}\n\
                     for _ in 0..100 {{\n\
                         task.yield_now();\n\
                         tx.send(p.check());\n\
                         if thread_id() != home {{ tx.send(1); }}\n\
                     }}\n\
                 }};\n\
                 ui.unwrap();\n\
             }};\n\
             r.unwrap();\n\
             tx.close();\n\
             let mut wrong = 0;\n\
             for v of rx {{ wrong += v; }}\n\
             console.println(\"wrong {{wrong}}\");\n\
         }}\n"
    ))
    .runs("main thread 1\nwrong 0\n");
}
