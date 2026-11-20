//! Callbacks registered against a scope, end to end: each case is a
//! program with its own C file, linked through a `[link]` table. A `spawned`
//! callback runs later as a task of its scope; a `direct` one during C's
//! call; a panic fails the scope; once the scope's body ends, C's calls
//! are dropped and the closures' captures let go of.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder holding one program, removed on drop.
struct Case(PathBuf);

impl Case {
    /// `main.la` and `side.c` (after the shared C below), linked by
    /// `[link] c = ["side.c"]`.
    fn new(main: &str, side: &str) -> Case {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-scb-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir is writable");
        std::fs::write(dir.join("main.la"), main).expect("writable");
        std::fs::write(dir.join("side.c"), format!("{SIDE}{side}")).expect("writable");
        std::fs::write(dir.join("lugha.toml"), "[link]\nc = [\"side.c\"]\n").expect("writable");
        Case(dir)
    }

    /// Builds at -O2 and runs on `workers` workers, with one more variable.
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

    /// On one worker and on four: exit `code` and exactly `stdout`.
    fn runs(&self, code: i32, stdout: &str) {
        for workers in ["1", "4"] {
            let out = self.run(workers, ("LUGHA_UNUSED", ""));
            assert_eq!(
                (out.status.code(), text(&out.stdout).as_str()),
                (Some(code), stdout),
                "workers {workers}, stderr: {}",
                text(&out.stderr)
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

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A C library keeping one callback with its user data, calling it when
/// told, and counting the calls it made.
const SIDE: &str = "#include <stdlib.h>\n\
    typedef void (*cb)(long, void *);\n\
    static cb kept; static void *kept_user; static void (*kept_free)(void *);\n\
    static long calls;\n\
    void on_event(cb f, void *user, void (*free_fn)(void *)) { kept = f; kept_user = user; kept_free = free_fn; }\n\
    void on_event_direct(cb f, void *user, void (*free_fn)(void *)) { on_event(f, user, free_fn); }\n\
    void fire(long n) { if (kept) { calls++; kept(n, kept_user); } }\n\
    long fired(void) { return calls; }\n\
    void forget(void) { if (kept_free) kept_free(kept_user); kept = 0; kept_free = 0; }\n";

const EXTERNS: &str = "import std.task;\n\
    extern fun on_event(f: spawned fun(i64, data), user: data, free: release);\n\
    extern fun on_event_direct(f: direct fun(i64, data), user: data, free: release);\n\
    extern fun fire(n: i64);\n\
    extern fun fired(): i64;\n\
    extern fun forget();\n";

#[test]
fn a_spawned_callback_runs_after_c_returns_as_a_task_that_may_wait() {
    Case::new(
        &format!(
            "{EXTERNS}\
             fun main(console: Console) {{\n\
                 let (tx, rx) = Chan<i64>.unbounded();\n\
                 let r = scope s {{\n\
                     on_event(fun(n: i64) {{\n\
                         task.sleep(5);\n\
                         tx.send(n * 10);\n\
                     }});\n\
                     fire(1);\n\
                     console.println(\"after fire\");\n\
                     fire(2);\n\
                 }};\n\
                 tx.close();\n\
                 let mut sum = 0;\n\
                 for v of rx {{ sum += v; }}\n\
                 console.println(\"{{r.is_ok()}} {{sum}}\");\n\
             }}\n"
        ),
        "",
    )
    .runs(0, "after fire\ntrue 30\n");
}

#[test]
fn direct_callbacks_run_during_cs_call() {
    Case::new(
        &format!(
            "{EXTERNS}\
             extern fun apply(n: i64, f: direct fun(i64, data): i64, user: data): i64;\n\
             extern fun twice(f: direct fun(i64), n: i64);\n\
             fun shout(n: i64) {{ }}\n\
             fun main(console: Console) {{\n\
                 on_event_direct(fun(n: i64) {{ console.println(\"inline {{n}}\"); }});\n\
                 fire(1);\n\
                 console.println(\"after fire\");\n\
                 console.println(apply(4, fun(x: i64): i64 = x + 1));\n\
                 twice(shout, 3);\n\
                 console.println(\"done\");\n\
             }}\n"
        ),
        "long apply(long n, long (*f)(long, void *), void *u) { return f(n, u); }\n\
         void twice(void (*f)(long), long n) { f(n); f(n); }\n",
    )
    .runs(0, "inline 1\nafter fire\n5\ndone\n");
}

#[test]
fn a_spawned_callbacks_panic_fails_its_scope_by_its_policy() {
    let body = |scope: &str| {
        format!(
            "{EXTERNS}\
             fun main(console: Console) {{\n\
                 let r = scope {scope} {{\n\
                     on_event(fun(n: i64) {{ if n == 2 {{ panic(\"two\"); }} }});\n\
                     fire(1);\n\
                     fire(2);\n\
                 }};\n\
                 console.println(r.is_ok());\n\
             }}\n"
        )
    };
    Case::new(&body("s"), "").runs(0, "false\n");
    let out = Case::new(&body("s independent"), "").run("1", ("LUGHA_UNUSED", ""));
    assert_eq!(text(&out.stdout), "true\n");
    assert!(
        text(&out.stderr).contains("task `callback` panicked: two"),
        "{}",
        text(&out.stderr)
    );
    Case::new(
        &body("s independent(fun(f: Failure<Error>) { println_failure(f); })").replace(
            "fun main",
            "fun println_failure(f: Failure<Error>) { }\nfun main",
        ),
        "",
    )
    .runs(0, "true\n");
}

#[test]
fn an_independent_handler_gets_a_callback_tasks_panic() {
    Case::new(
        &format!(
            "{EXTERNS}\
             fun main(console: Console) {{\n\
                 let (tx, rx) = Chan<string>.unbounded();\n\
                 let r = scope s independent(fun(f: Failure<Error>) {{ tx.send(f.show()); }}) {{\n\
                     on_event(fun(n: i64) {{ if n == 2 {{ panic(\"two\"); }} }});\n\
                     fire(2);\n\
                 }};\n\
                 tx.close();\n\
                 for m of rx {{ console.println(m.split(\" at \")[0]); }}\n\
                 console.println(r.is_ok());\n\
             }}\n"
        ),
        "",
    )
    .runs(0, "task `callback` panicked: two\ntrue\n");
}

#[test]
fn cancellation_waits_for_a_direct_callback_to_return() {
    Case::new(
        &format!(
            "{EXTERNS}\
             extern fun spin_ms(ms: i64);\n\
             fun main(console: Console) {{\n\
                 let (tx, rx) = Chan<i64>.unbounded();\n\
                 let r = scope s within 20 {{\n\
                     on_event_direct(fun(n: i64) {{\n\
                         spin_ms(80);\n\
                         tx.send(n);\n\
                         console.println(\"callback done\");\n\
                     }});\n\
                     fire(1);\n\
                     task.sleep(1000);\n\
                     console.println(\"not reached\");\n\
                 }};\n\
                 console.println(r.is_ok());\n\
             }}\n"
        ),
        "#include <time.h>\n\
         void spin_ms(long ms) {\n\
             struct timespec a, b; clock_gettime(CLOCK_MONOTONIC, &a);\n\
             do { clock_gettime(CLOCK_MONOTONIC, &b); }\n\
             while ((b.tv_sec - a.tv_sec) * 1000 + (b.tv_nsec - a.tv_nsec) / 1000000 < ms);\n\
         }\n",
    )
    .runs(0, "callback done\nfalse\n");
}

#[test]
fn after_the_scopes_body_ends_calls_are_dropped_and_captures_freed() {
    let case = Case::new(
        &format!(
            "{EXTERNS}\
             fun main(console: Console) {{\n\
                 let name = \"kept {{1 + 1}}\";\n\
                 let r = scope s {{\n\
                     on_event(fun(n: i64) {{ console.println(\"{{name}} {{n}}\"); }});\n\
                     on_event_direct(fun(n: i64) {{ console.println(\"{{name}} inline {{n}}\"); }});\n\
                     fire(1);\n\
                 }};\n\
                 fire(2);\n\
                 console.println(\"C called {{fired()}} times\");\n\
             }}\n"
        ),
        "",
    );
    case.runs(0, "kept 2 inline 1\nC called 2 times\n");
    // C never calls `release`: the scope's end let go of the captures.
    let out = case.run("1", ("LUGHA_RC_REPORT", "1"));
    assert!(
        text(&out.stderr).contains(" 0 live"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_spawned_callback_taking_a_pinned_handle_runs_on_cs_thread() {
    Case::new(
        "import std.task;\n\
         extern type Ctx pinned;\n\
         extern fun ctx_new(): Ctx;\n\
         extern fun ctx_on(c: Ctx, f: spawned fun(Ctx, i64, data), user: data);\n\
         extern fun ctx_fire(c: Ctx, n: i64);\n\
         extern fun ctx_check(c: Ctx): i64;\n\
         fun busy(n: i64, out: Sender<i64>) {\n\
             let mut s = 0;\n\
             for i in 0..n { s += i % 7; if i % 1000 == 0 { task.yield_now(); } }\n\
             out.send(s - s);\n\
         }\n\
         fun main(console: Console) {\n\
             let (tx, rx) = Chan<i64>.unbounded();\n\
             let r = scope s {\n\
                 for _ in 0..2 { s.spawn(busy(2000000, tx)); }\n\
                 let c = ctx_new();\n\
                 ctx_on(c, fun(c: Ctx, n: i64) {\n\
                     for _ in 0..20 { task.yield_now(); tx.send(ctx_check(c)); }\n\
                 });\n\
                 for i in 0..20 { ctx_fire(c, i); task.yield_now(); }\n\
             };\n\
             r.unwrap();\n\
             tx.close();\n\
             let mut wrong = 0;\n\
             for v of rx { wrong += v; }\n\
             console.println(\"wrong {wrong}\");\n\
         }\n",
        "#include <pthread.h>\n\
         typedef struct ctx { pthread_t owner; void (*f)(struct ctx *, long, void *); void *u; } ctx;\n\
         ctx *ctx_new(void) { ctx *c = calloc(1, sizeof *c); c->owner = pthread_self(); return c; }\n\
         void ctx_on(ctx *c, void (*f)(ctx *, long, void *), void *u) { c->f = f; c->u = u; }\n\
         void ctx_fire(ctx *c, long n) { c->f(c, n, c->u); }\n\
         long ctx_check(ctx *c) { return !pthread_equal(c->owner, pthread_self()); }\n",
    )
    .runs(0, "wrong 0\n");
}

#[test]
fn a_callback_without_data_runs_after_its_scope_too() {
    // It captures nothing, so nothing it uses can have been let go of.
    let side = "typedef long (*plain)(long);\n\
        static plain kept_plain;\n\
        void on_tick(plain f) { kept_plain = f; }\n\
        long tick(long n) { return kept_plain ? kept_plain(n) : -1; }\n";
    Case::new(
        &format!(
            "{EXTERNS}\
             extern fun on_tick(f: direct fun(i64): i64);\n\
             extern fun tick(n: i64): i64;\n\
             fun twice(n: i64): i64 = n * 2;\n\
             fun main(console: Console) {{\n\
                 let r = scope s {{\n\
                     on_tick(twice);\n\
                     console.println(tick(1));\n\
                 }};\n\
                 console.println(tick(21));\n\
             }}\n"
        ),
        side,
    )
    .runs(0, "2\n42\n");
}
