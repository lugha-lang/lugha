//! Callbacks and handles end to end: each case
//! is a program with its own C file, linked through a `[link]` table,
//! run by `lughac`.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder holding one program, removed on drop.
struct Case(PathBuf);

impl Case {
    /// `main.la` and `side.c`, linked by `[link] c = ["side.c"]`.
    fn new(main: &str, side: &str) -> Case {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-cb-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.la"), main).unwrap();
        std::fs::write(dir.join("side.c"), side).unwrap();
        std::fs::write(dir.join("lugha.toml"), "[link]\nc = [\"side.c\"]\n").unwrap();
        Case(dir)
    }

    fn lughac(&self, args: &[&str]) -> Output {
        self.lughac_with(args, ("HOME", self.0.to_str().expect("a UTF-8 temp path")))
    }

    /// `lughac` with one more variable in its environment.
    fn lughac_with(&self, args: &[&str], (key, value): (&str, &str)) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(args)
            .current_dir(&self.0)
            .env("HOME", &self.0)
            // One worker, so one C stack: these cases are about callbacks
            // and C's stack, not threads.
            .env("LUGHA_WORKERS", "1")
            .env(key, value)
            .output()
            .expect("lughac runs")
    }

    /// Runs the program at -O0 and -O2 and checks both print `stdout` and exit 0.
    fn runs(&self, stdout: &str) {
        for opt in ["-O0", "-O2"] {
            let out = self.lughac(&["run", "main.la", opt]);
            assert_eq!(
                (out.status.code(), text(&out.stdout).as_str()),
                (Some(0), stdout),
                "{opt}, stderr: {}",
                text(&out.stderr)
            );
        }
    }

    /// Checks `lughac check` rejects the program with exactly these codes, in order.
    fn rejects(&self, codes: &[&str]) -> String {
        let out = self.lughac(&["check", "main.la", "--diagnostics=json"]);
        let stderr = text(&out.stderr) + &text(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{stderr}");
        let found: Vec<&str> = stderr
            .lines()
            .filter_map(|l| l.split("\"code\":\"").nth(1))
            .map(|rest| &rest[..5])
            .collect();
        assert_eq!(found, codes, "{stderr}");
        stderr
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

const THINGS: &str = "#include <stddef.h>\n\
    typedef struct { long id; } thing;\n\
    static thing things[3] = {{1}, {2}, {3}};\n\
    thing *thing_make(long i) { return &things[i]; }\n\
    long thing_id(thing *t) { return t->id; }\n\
    thing *thing_find(long i) { return i < 3 ? &things[i] : NULL; }\n\
    long thing_is_null(thing *t) { return t == NULL; }\n";

#[test]
fn handles_come_from_c_and_go_back() {
    Case::new(
        "extern type Thing;\n\
         extern fun thing_make(i: i64): Thing;\n\
         extern fun thing_id(t: Thing): i64;\n\
         extern fun thing_find(i: i64): Option<Thing>;\n\
         extern fun thing_is_null(t: Option<Thing>): i64;\n\
         struct Holder { t: Thing, name: string }\n\
         impl Thing { fun id(self): i64 = thing_id(self); }\n\
         fun main(console: Console) {\n\
             let a = thing_make(0);\n\
             let b = thing_make(1);\n\
             let h = Holder { t: b, name: \"b\" };\n\
             console.println(thing_id(a));\n\
             console.println(h.t.id());\n\
             console.println(a == thing_make(0));\n\
             console.println(a == b);\n\
             console.println(a != b);\n\
             console.println(a.debug());\n\
             console.println(\"{h.t.debug()}\");\n\
             match thing_find(2) { .Some(t) => console.println(thing_id(t)), .None => console.println(\"none\") }\n\
             match thing_find(5) { .Some(t) => console.println(thing_id(t)), .None => console.println(\"none\") }\n\
             console.println(thing_is_null(None));\n\
             console.println(thing_is_null(Some(a)));\n\
             let ts = [a, b];\n\
             console.println(ts[1].id());\n\
             console.println(ts == [a, b]);\n\
             console.println(h.debug());\n\
         }\n",
        THINGS,
    )
    .runs(
        "1\n2\ntrue\nfalse\ntrue\n<Thing>\n<Thing>\n3\nnone\n1\n0\n2\ntrue\nHolder { t: <Thing>, name: \"b\" }\n",
    );
}

#[test]
fn named_functions_and_capture_free_closures_need_no_data() {
    Case::new(
        "extern fun apply_sum(n: i64, f: direct fun(i64): i64): i64;\n\
         fun square(x: i64): i64 = x * x;\n\
         fun main(console: Console) {\n\
             console.println(apply_sum(4, square));\n\
             console.println(apply_sum(3, fun(x) = x + 10));\n\
             console.println(apply_sum(2, square));\n\
         }\n",
        "long apply_sum(long n, long (*f)(long)) {\n\
             long s = 0;\n\
             for (long i = 0; i < n; i++) s += f(i);\n\
             return s;\n\
         }\n",
    )
    .runs("14\n33\n1\n");
}

/// Keeps callbacks with their user data and calls them later.
const REGISTRY: &str = "typedef void (*cb_t)(long, void *);\n\
    static cb_t cbs[8];\n\
    static void *datas[8];\n\
    static void (*rels[8])(void *);\n\
    static int count, released;\n\
    long reg(cb_t f, void *d, void (*rel)(void *)) { cbs[count] = f; datas[count] = d; rels[count] = rel; return count++; }\n\
    void fire(long i, long x) { cbs[i](x, datas[i]); }\n\
    void fire_all(long x) { for (int i = 0; i < count; i++) if (cbs[i]) cbs[i](x, datas[i]); }\n\
    void drop(long i) { rels[i](datas[i]); cbs[i] = 0; released++; }\n\
    long released_count(void) { return released; }\n";

const REGISTRY_EXTERNS: &str = "extern fun reg(f: direct fun(i64, data), user: data, rel: release): i64;\n\
    extern fun fire(i: i64, x: i64);\n\
    extern fun fire_all(x: i64);\n\
    extern fun drop(i: i64);\n\
    extern fun released_count(): i64;\n";

#[test]
fn closures_keep_their_captures_while_c_holds_them() {
    Case::new(
        &[REGISTRY_EXTERNS, "\
             fun churn() { let mut i = 0; while i < 20000 { let s = \"garbage {i}\"; i += 1; } }\n\
             fun main(console: Console) {\n\
                 let (tx, rx) = Chan<i64>.unbounded();\n\
                 let label = \"b {1 + 1}\";\n\
                 let ia = reg(fun(x: i64) { console.println(\"a: {x}\"); tx.send(x); });\n\
                 let ib = reg(fun(x: i64) { console.println(\"{label}: {100 - x}\"); tx.send(x); });\n\
                 churn();\n\
                 fire(ia, 5);\n\
                 fire(ib, 1);\n\
                 fire_all(2);\n\
                 churn();\n\
                 fire(ia, 1);\n\
                 tx.close();\n\
                 let mut sum = 0;\n\
                 for x of rx { sum += x; }\n\
                 console.println(sum);\n\
             }\n"].concat(),
        REGISTRY,
    )
    .runs("a: 5\nb 2: 99\na: 2\nb 2: 98\na: 1\n11\n");
}

#[test]
fn release_frees_a_callback_c_drops() {
    let case = Case::new(
        &[
            REGISTRY_EXTERNS,
            "\
             fun main(console: Console) {\n\
                 let (seen, saw) = Chan<i64>.unbounded();\n\
                 let first = reg(fun(x: i64) { seen.send(x); });\n\
                 let second = reg(fun(x: i64) { console.println(\"second {x}\"); });\n\
                 fire(first, 3);\n\
                 drop(first);\n\
                 fire_all(4);\n\
                 drop(second);\n\
                 console.println(released_count());\n\
                 seen.close();\n\
                 let mut total = 0;\n\
                 for x of saw { total += x; }\n\
                 console.println(total);\n\
             }\n",
        ]
        .concat(),
        REGISTRY,
    );
    case.runs("second 4\n2\n3\n");
    // Released by C, each closure's environment is freed.
    let out = case.lughac_with(&["run", "main.la"], ("LUGHA_RC_REPORT", "1"));
    assert!(
        text(&out.stderr).trim_end().ends_with(" 0 live"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn callbacks_take_and_give_handles_numbers_and_bools() {
    Case::new(
        "extern type Thing;\n\
         extern fun thing_id(t: Thing): i64;\n\
         extern fun mix(f: direct fun(Thing, i32, f64, bool, data): f64, user: data): f64;\n\
         extern fun maybe(f: direct fun(Option<Thing>, data): i64, user: data): i64;\n\
         extern fun pick(f: direct fun(i64, data): Option<Thing>, user: data): i64;\n\
         fun main(console: Console) {\n\
             let scale = 2.0;\n\
             console.println(mix(fun(t: Thing, n: i32, x: f64, b: bool): f64 {\n\
                 if b { (thing_id(t) + n as i64).round<f64>() * x * scale } else { 0.0 }\n\
             }));\n\
             console.println(maybe(fun(t: Option<Thing>): i64 {\n\
                 match t { .Some(x) => thing_id(x), .None => 7 }\n\
             }));\n\
             let things: Thing[] = [];\n\
             console.println(pick(fun(i: i64): Option<Thing> = if i == 0 { None } else { None }));\n\
         }\n",
        &format!(
            "{THINGS}\
             double mix(double (*f)(thing *, int, double, _Bool, void *), void *d) {{ return f(&things[1], 3, 0.5, 1, d); }}\n\
             long maybe(long (*f)(thing *, void *), void *d) {{ return f(NULL, d) * 10 + f(&things[2], d); }}\n\
             long pick(thing *(*f)(long, void *), void *d) {{ return f(0, d) == NULL ? 1 : 0; }}\n"
        ),
    )
    .runs("5.0\n73\n1\n");
}

#[test]
fn renamed_externs_call_one_c_function_with_different_callbacks() {
    Case::new(
        "extern type Thing;\n\
         extern fun thing_make(i: i64): Thing;\n\
         extern fun thing_id(t: Thing): i64;\n\
         extern \"run_cb\" fun run_int(f: direct fun(i64, data): i64, user: data, x: i64): i64;\n\
         extern \"run_cb\" fun run_thing(f: direct fun(Thing, data): i64, user: data, t: Thing): i64;\n\
         fun main(console: Console) {\n\
             let k = 100;\n\
             console.println(run_int(fun(x: i64): i64 = x + k, 5));\n\
             console.println(run_thing(fun(t: Thing): i64 = thing_id(t) * k, thing_make(2)));\n\
         }\n",
        &format!("{THINGS}long run_cb(long (*f)(void *, void *), void *d, void *x) {{ return f(x, d); }}\n"),
    )
    .runs("105\n300\n");
}

#[test]
fn a_panic_in_a_direct_callback_gives_c_zero_and_fails_the_scope() {
    let case = Case::new(
        "extern fun apply_sum(n: i64, f: direct fun(i64): i64): i64;\n\
         fun boom(x: i64): i64 { if x == 2 { panic(\"no twos\"); } x }\n\
         fun main(console: Console) { console.println(apply_sum(4, boom)); }\n",
        "long apply_sum(long n, long (*f)(long)) { long s = 0; for (long i = 0; i < n; i++) s += f(i); return s; }\n",
    );
    let out = case.lughac(&["run", "main.la"]);
    // 0 + 1 + 0 + 3: C got 0 for the call that panicked, and went on; the
    // root scope failed, so the program ends with 101 once `main` returns.
    assert_eq!(out.status.code(), Some(101), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "4\n");
    assert!(
        text(&out.stderr).contains("task `callback` panicked: no twos"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn bad_callback_programs_are_rejected_with_their_codes() {
    let c = "long f(void) { return 0; }\n";
    Case::new(
        "extern fun f(cb: direct fun(i64): string);\nfun main() {}\n",
        c,
    )
    .rejects(&["E0409"]);
    Case::new(
        "extern fun f(cb: direct fun(i64), u: data);\nfun main() {}\n",
        c,
    )
    .rejects(&["E0435"]);
    let stderr = Case::new(
        "extern fun f(cb: direct fun(i64));\nfun main(console: Console) { let k = 1; f(fun(x: i64) { console.println(x + k); }); }\n",
        c,
    )
    .rejects(&["E0436"]);
    assert!(stderr.contains("has no `data`"), "{stderr}");
}

#[test]
fn a_panic_in_a_direct_callback_unwinds_to_its_entry_only() {
    let case = Case::new(
        &[
            REGISTRY_EXTERNS,
            "\
             fun main(console: Console) {\n\
                 defer console.println(\"main isn't unwound\");\n\
                 let i = reg(fun(x: i64) {\n\
                     defer console.println(\"callback cleaned up\");\n\
                     panic(\"callback failed\");\n\
                 });\n\
                 fire(i, 1);\n\
             }\n",
        ]
        .concat(),
        REGISTRY,
    );
    let out = case.lughac(&["run", "main.la"]);
    // The callback's `defer` runs; `main` goes on and ends normally.
    assert_eq!(out.status.code(), Some(101), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "callback cleaned up\nmain isn't unwound\n"
    );
    assert!(
        text(&out.stderr).contains("task `callback` panicked: callback failed"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn c_runs_on_its_own_stack_whichever_task_calls_it() {
    // A local's address in C, from `main` and from a task: both on the one
    // C stack, near each other; task stacks are 8 MiB apart.
    Case::new(
        "extern fun c_where(): i64;\n\
         fun from_task(): i64 = c_where();\n\
         fun main(console: Console) {\n\
             let here = c_where();\n\
             let r = scope s { s.spawn(from_task()).join().unwrap() };\n\
             let there = r.unwrap();\n\
             let apart = if here > there { here - there } else { there - here };\n\
             console.println(apart < 65_536);\n\
         }\n",
        "long c_where(void) { volatile char local = 0; return (long)&local; }\n",
    )
    .runs("true\n");
}

#[test]
fn a_callback_may_call_c_again() {
    Case::new(
        "extern fun apply_sum(n: i64, f: direct fun(i64): i64): i64;\n\
         extern fun twice(x: i64): i64;\n\
         fun main(console: Console) {\n\
             console.println(apply_sum(4, fun(x) = twice(x)));\n\
         }\n",
        "long apply_sum(long n, long (*f)(long)) {\n\
             long s = 0;\n\
             for (long i = 0; i < n; i++) s += f(i);\n\
             return s;\n\
         }\n\
         long twice(long x) { return 2 * x; }\n",
    )
    .runs("12\n");
}

#[test]
fn a_direct_callback_that_would_wait_panics_into_its_scope() {
    let case = Case::new(
        "import std.task;\n\
         extern fun apply_sum(n: i64, f: direct fun(i64): i64): i64;\n\
         fun main(console: Console) {\n\
             console.println(apply_sum(2, fun(x) { task.sleep(1); x }));\n\
         }\n",
        "long apply_sum(long n, long (*f)(long)) {\n\
             long s = 0;\n\
             for (long i = 0; i < n; i++) s += f(i);\n\
             return s;\n\
         }\n",
    );
    let out = case.lughac(&["run", "main.la"]);
    assert_eq!(out.status.code(), Some(101));
    assert!(
        text(&out.stderr).contains("task `callback` panicked: a direct C callback can't wait"),
        "{}",
        text(&out.stderr)
    );
}
