//! Callbacks on threads C made, end to end: a C library starting pthreads
//! of its own and calling back from them. A `direct` callback runs on C's
//! thread and never makes it wait; a `spawned` one becomes a task on the
//! workers. Each case runs on one worker and on four.

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
        let dir = std::env::temp_dir().join(format!("lugha-foreign-{}-{n}", std::process::id()));
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
const SIDE: &str = "#include <pthread.h>\n\
    #include <stdlib.h>\n\
    #include <unistd.h>\n\
    typedef void (*cb)(long, void *);\n\
    typedef long (*cbr)(long, void *);\n\
    typedef struct { cb f; cbr g; void *u; long n; long pause_ms; } job;\n\
    static pthread_t threads[1024]; static int started;\n\
    static long sum;\n\
    static void *calls(void *p) {\n\
        job *j = p;\n\
        if (j->pause_ms) usleep(j->pause_ms * 1000);\n\
        for (long i = 0; i < j->n; i++) { if (j->f) j->f(i, j->u); else sum += j->g(i, j->u); }\n\
        free(j);\n\
        return 0;\n\
    }\n\
    static void start(cb f, cbr g, void *u, long n, long pause_ms) {\n\
        job *j = malloc(sizeof *j); j->f = f; j->g = g; j->u = u; j->n = n; j->pause_ms = pause_ms;\n\
        pthread_create(&threads[started++], 0, calls, j);\n\
    }\n\
    void on_thread(cb f, void *u, long n, long pause_ms) { start(f, 0, u, n, pause_ms); }\n\
    void spawn_on_thread(cb f, void *u, long n) { start(f, 0, u, n, 0); }\n\
    void ask_on_thread(cbr g, void *u, long n) { start(0, g, u, n, 0); }\n\
    void burst(cb f, void *u, long n) {\n\
        for (long i = 0; i < n; i++) { job *j = malloc(sizeof *j); j->f = f; j->g = 0; j->u = u; j->n = 1; j->pause_ms = 0;\n\
            pthread_t t; pthread_create(&t, 0, calls, j); pthread_join(t, 0); }\n\
    }\n\
    void join_threads(void) { for (int i = 0; i < started; i++) pthread_join(threads[i], 0); started = 0; }\n\
    long asked_sum(void) { return sum; }\n";

const EXTERNS: &str = "import std.task;\n\
    extern fun on_thread(f: direct fun(i64, data), u: data, n: i64, pause_ms: i64);\n\
    extern fun spawn_on_thread(f: spawned fun(i64, data), u: data, n: i64);\n\
    extern fun ask_on_thread(f: direct fun(i64, data): i64, u: data, n: i64);\n\
    extern fun burst(f: direct fun(i64, data), u: data, n: i64);\n\
    extern blocking fun join_threads();\n\
    extern fun asked_sum(): i64;\n";

#[test]
fn a_direct_callback_on_cs_thread_sends_every_value_in_order() {
    Case::new(
        "fun main(console: Console) {\n\
             let (tx, rx) = Chan<i64>.unbounded();\n\
             let r = scope s {\n\
                 on_thread(fun(i: i64) { tx.send(i); }, 10000, 0);\n\
                 let mut next = 0;\n\
                 let mut ordered = true;\n\
                 while next < 10000 {\n\
                     let v = rx.recv().unwrap();\n\
                     if v != next { ordered = false; }\n\
                     next += 1;\n\
                 }\n\
                 join_threads();\n\
                 console.println(\"{next} {ordered}\");\n\
             };\n\
             console.println(r.is_ok());\n\
         }\n",
    )
    .runs(0, "10000 true\ntrue\n", "");
}

#[test]
fn a_spawned_callback_from_cs_thread_runs_as_a_task_that_may_wait() {
    Case::new(
        "fun main(console: Console) {\n\
             let (tx, rx) = Chan<i64>.unbounded();\n\
             let r = scope s {\n\
                 spawn_on_thread(fun(i: i64) { task.sleep(1); tx.send(i); }, 100);\n\
                 let mut sum = 0;\n\
                 for _ in 0..100 { sum += rx.recv().unwrap(); }\n\
                 join_threads();\n\
                 console.println(sum);\n\
             };\n\
             console.println(r.is_ok());\n\
         }\n",
    )
    .runs(0, "4950\ntrue\n", "");
}

#[test]
fn waiting_or_panicking_on_cs_thread_gives_c_zero_and_fails_the_scope() {
    Case::new(
        "fun main(console: Console) {\n\
             let (tx, rx) = Chan<i64>.new(1);\n\
             let r = scope s {\n\
                 ask_on_thread(fun(i: i64): i64 {\n\
                     if i == 1 { tx.send(i); tx.send(i); }\n\
                     if i == 3 { panic(\"three\"); }\n\
                     i\n\
                 }, 5);\n\
                 join_threads();\n\
                 console.println(asked_sum());\n\
             };\n\
             console.println(r.is_ok());\n\
         }\n",
    )
    // 0 + 0 (waited) + 2 + 0 (panicked) + 4.
    .runs(0, "6\nfalse\n", "");
    let waits = Case::new(
        "fun main(console: Console) {\n\
             let (tx, rx) = Chan<i64>.new(1);\n\
             let r = scope s independent {\n\
                 ask_on_thread(fun(i: i64): i64 { tx.send(i); i }, 2);\n\
                 join_threads();\n\
             };\n\
             console.println(asked_sum());\n\
         }\n",
    );
    waits.runs(0, "0\n", "can't wait on a thread C made");
}

#[test]
fn deep_recursion_on_cs_thread_uses_its_own_stack() {
    Case::new(
        "fun depth(n: i64): i64 = if n == 0 { 0 } else { 1 + depth(n - 1) };\n\
         fun main(console: Console) {\n\
             let r = scope s independent {\n\
                 ask_on_thread(fun(i: i64): i64 = if i == 0 { depth(10000) } else { depth(100000000) }, 2);\n\
                 join_threads();\n\
             };\n\
             console.println(asked_sum());\n\
         }\n",
    )
    .runs(0, "10000\n", "stack overflow");
}

#[test]
fn short_lived_threads_and_their_strings_leave_nothing_behind() {
    let case = Case::new(
        "fun main(console: Console) {\n\
             let (tx, rx) = Chan<string>.unbounded();\n\
             let r = scope s {\n\
                 burst(fun(i: i64) { tx.send(\"value {i}\"); }, 200);\n\
                 let mut total = 0;\n\
                 for _ in 0..200 { total += rx.recv().unwrap().char_count(); }\n\
                 console.println(total);\n\
             };\n\
             console.println(r.is_ok());\n\
         }\n",
    );
    case.runs(0, "1400\ntrue\n", "");
    let out = case.run("4", ("LUGHA_RC_REPORT", "1"));
    assert!(
        text(&out.stderr).contains(" 0 live"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn waiting_on_what_only_cs_thread_sends_isnt_a_deadlock() {
    Case::new(
        "fun main(console: Console) {\n\
             let (tx, rx) = Chan<i64>.unbounded();\n\
             let r = scope s {\n\
                 on_thread(fun(i: i64) { tx.send(i + 41); }, 1, 100);\n\
                 console.println(rx.recv().unwrap());\n\
                 join_threads();\n\
             };\n\
             console.println(r.is_ok());\n\
         }\n",
    )
    .runs(0, "41\ntrue\n", "");
}

/// C's thread counts the allocations made while it runs a callback: the
/// system allocator and `mmap`, interposed for this program only.
const COUNTING: &str = "#define _GNU_SOURCE\n\
#include <pthread.h>\n#include <stddef.h>\n#include <stdint.h>\n\
#include <unistd.h>\n#include <sys/mman.h>\n#include <sys/syscall.h>\n\
extern void *__libc_malloc(size_t);\nextern void *__libc_calloc(size_t, size_t);\n\
static __thread int watching;\nstatic int counted;\n\
void *malloc(size_t n) { if (watching) counted++; return __libc_malloc(n); }\n\
void *calloc(size_t a, size_t b) { if (watching) counted++; return __libc_calloc(a, b); }\n\
void *mmap(void *a, size_t l, int p, int f, int fd, off_t o) {\n\
    if (watching) counted++;\n    return (void *)syscall(SYS_mmap, a, l, p, f, fd, o); }\n\
typedef void (*cb_t)(float *, size_t, void *);\n\
struct job { cb_t cb; void *d; float buf[4]; };\n\
static void *play(void *p) { struct job *j = p; watching = 1; j->cb(j->buf, 4, j->d); watching = 0; return 0; }\n\
int64_t first_call(cb_t cb, void *d) {\n\
    struct job j = { cb, d, { 1, 2, 3, 4 } };\n\
    pthread_t t; pthread_create(&t, 0, play, &j); pthread_join(t, 0);\n\
    return (int64_t)counted * 1000 + (int64_t)j.buf[3]; }\n";

#[test]
fn a_realtime_callbacks_first_call_on_cs_thread_allocates_nothing() {
    let dir = std::env::temp_dir().join(format!("lugha-foreign-rt-{}", std::process::id()));
    // A leftover from an earlier run would change what the test sees.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir is writable");
    let main = "import std.task;\n\
        extern fun first_call(cb: realtime fun(MutView<f32>, data), d: data): i64;\n\
        fun main(console: Console) {\n\
            task.sleep(1);\n\
            console.println(first_call(fun(out: MutView<f32>) { out[3] = out[3] * 2.0; }));\n\
        }\n";
    std::fs::write(dir.join("main.la"), main).expect("writable");
    std::fs::write(dir.join("side.c"), COUNTING).expect("writable");
    std::fs::write(dir.join("lugha.toml"), "[link]\nc = [\"side.c\"]\n").expect("writable");
    let case = Case(dir);
    for workers in ["1", "4"] {
        let out = case.run(workers, ("LUGHA_UNUSED", ""));
        // No allocation on C's thread, and the callback wrote its buffer.
        assert_eq!(
            (out.status.code(), text(&out.stdout).as_str()),
            (Some(0), "8\n"),
            "{workers}: {}",
            text(&out.stderr)
        );
    }
}
