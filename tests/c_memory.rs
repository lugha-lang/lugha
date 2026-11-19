//! C memory end to end: views of it made from a `CPtr`, and callbacks
//! taking views, against C code compiled by the system compiler.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temporary folder holding one program and its C file, removed on drop.
struct Case(PathBuf);

impl Case {
    fn new(main: &str, side: &str) -> Case {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-cmem-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.la"), main).unwrap();
        std::fs::write(dir.join("side.c"), side).unwrap();
        std::fs::write(dir.join("lugha.toml"), "[link]\nc = [\"side.c\"]\n").unwrap();
        Case(dir)
    }

    fn run(&self, opt: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(["run", "main.la", opt])
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .output()
            .expect("lughac runs")
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

const SIDE: &str = r#"
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <stddef.h>
typedef struct { size_t size; uint8_t *data; } byte_vec;
typedef union { int64_t i64; double f64; } val_union;
typedef struct { uint8_t kind; val_union of; } val;
static uint8_t memory[16];
void message(byte_vec *out) { out->size = 5; out->data = malloc(5); memcpy(out->data, "hello", 5); }
void byte_vec_delete(byte_vec *v) { free(v->data); v->data = NULL; v->size = 0; }
uint8_t *memory_data(void) { return memory; }
uint64_t memory_sum(void) { uint64_t s = 0; for (int i = 0; i < 16; i++) s += memory[i]; return s; }
uint8_t *nothing(void) { return NULL; }
uint8_t *odd(void) { return memory + 1; }
static double samples[4] = { 1, 2, 3, 4 };
double fill(void (*cb)(void *, const int32_t *, size_t, double *, size_t), void *env) {
    int32_t in[3] = { 10, 20, 30 };
    cb(env, in, 3, samples, 4);
    return samples[0] + samples[1] + samples[2] + samples[3];
}
typedef void (*host)(void *env, const val *args, size_t nargs, val *results, size_t nresults);
int64_t call_host(host f, void *env) {
    val args[2] = { { 1, { .i64 = 40 } }, { 1, { .i64 = 2 } } };
    val results[1] = { { 0, { .i64 = 0 } } };
    f(env, args, 2, results, 1);
    return results[0].of.i64;
}
"#;

const MAIN: &str = r#"
extern union ValUnion { int64: i64, float64: f64 }
extern struct Val { kind: u8, value: ValUnion }
extern struct ByteVec { size: u64, data: CPtr }
extern fun message(out v: ByteVec);
extern fun byte_vec_delete(mut v: ByteVec);
extern fun memory_data(): CPtr;
extern fun memory_sum(): u64;
extern fun nothing(): CPtr;
extern fun call_host(f: direct fun(data, View<Val>, MutView<Val>), d: data): i64;
extern fun fill(f: direct fun(data, View<i32>, MutView<f64>), d: data): f64;

fun total(xs: View<i32>): f64 {
    let mut sum = 0.0;
    for x of xs { sum += x as f64; }
    sum
}

fun main(console: Console) {
    let mut msg = ByteVec { size: 0, data: CPtr.null };
    message(out msg);
    let bytes = msg.data.view(msg.size, fun(v: View<u8>) = v.to_array());
    console.println(string_from_utf8(bytes).unwrap());
    byte_vec_delete(mut msg);
    console.println(msg.data == CPtr.null);
    let mem = memory_data();
    mem.view(16, fun(v: MutView<u8>) { v.slice(4, 7).copy_from([1, 2, 3]); });
    console.println(memory_sum());
    console.println(mem.view(16, fun(v: View<u8>) = v.slice(3, 6).to_array()).debug());
    console.println(nothing().view(0, fun(v: View<u8>) = v.len));
    let base = 100;
    let sum = call_host(fun(args: View<Val>, results: MutView<Val>) {
        results[0] = Val { kind: 1, value: ValUnion { int64: args[0].value.int64 + args[1].value.int64 + base } };
    });
    console.println(sum);
    let filled = fill(fun(input: View<i32>, mut_out: MutView<f64>) {
        let t = total(input);
        for i in 0..mut_out.len { mut_out[i] = mut_out[i] * 10.0 + t; }
        mut_out[0] += 0.5;
    });
    console.println(filled);
    nothing().view(1, fun(v: View<u8>) = v.len);
}
"#;

/// Programs that panic on a view, each with its message.
const PANICS: [(&str, &str); 4] = [
    (
        "memory_data().view(4, fun(v: View<u8>) = v[4]);",
        "index out of bounds",
    ),
    ("odd().view(1, fun(v: View<i32>) = v.len);", "not aligned"),
    (
        "memory_data().view(4, fun(v: MutView<u8>) { v.copy_from([1]); });",
        "copy_from",
    ),
    (
        "memory_data().view(4, fun(v: View<u8>) = v.slice(2, 5).len);",
        "slice out of bounds",
    ),
];

#[test]
fn views_read_and_write_c_memory_in_place() {
    for opt in ["-O0", "-O2"] {
        let out = Case::new(MAIN, SIDE).run(opt);
        assert_eq!(
            text(&out.stdout),
            "hello\ntrue\n6\n[0, 1, 2]\n0\n142\n340.5\n",
            "{opt}: {}",
            text(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(101), "{opt}");
        assert!(
            text(&out.stderr).contains("NULL CPtr given to view"),
            "{}",
            text(&out.stderr)
        );
    }
}

#[test]
fn views_panic_out_of_bounds_misaligned_and_mismatched() {
    let head = "extern fun memory_data(): CPtr;\nextern fun odd(): CPtr;\nfun main() {\n";
    for (body, message) in PANICS {
        let out = Case::new(&format!("{head}    {body}\n}}\n"), SIDE).run("-O0");
        assert_eq!(
            out.status.code(),
            Some(101),
            "{body}: {}",
            text(&out.stderr)
        );
        assert!(
            text(&out.stderr).contains(message),
            "{body}: {}",
            text(&out.stderr)
        );
    }
}
