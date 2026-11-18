//! Tests of landing pads (`unwind.rs`).

use crate::codegen::test_util::ir_raw;

/// The definition of `lugha_fn_<name>`, up to its closing brace.
fn body<'a>(ir: &'a str, name: &str) -> &'a str {
    let call = format!("@lugha_fn_{name}(");
    let at = ir
        .match_indices("define ")
        .map(|(at, _)| at)
        .find(|&at| ir[at..].lines().next().is_some_and(|l| l.contains(&call)))
        .unwrap_or_else(|| panic!("{name}: {ir}"));
    let start = &ir[at..];
    &start[..start.find("\n}").expect("a body ends")]
}

#[test]
fn a_frame_holding_nothing_keeps_plain_calls() {
    let ir = ir_raw(
        "fun fib(n: i64): i64 = if n < 2 { n } else { fib(n - 1) + fib(n - 2) };\n\
         fun main() { let x = fib(3); }",
    );
    let fib = body(&ir, "fib");
    assert!(!fib.contains("invoke"), "{fib}");
    assert!(!fib.contains("landingpad"), "{fib}");
}

#[test]
fn a_call_with_a_pending_defer_unwinds_to_a_pad_running_it() {
    let ir = ir_raw(
        "fun work(console: Console) { defer console.println(\"done\"); let xs: i64[] = []; console.println(xs[1]); }\n\
         fun main(console: Console) { work(console); }",
    );
    let work = body(&ir, "work");
    assert!(
        work.contains("invoke void @lugha_rt_panic_bounds("),
        "{work}"
    );
    assert!(work.contains("landingpad { ptr, i32 }"), "{work}");
    assert!(work.contains("cleanup"), "{work}");
    assert!(work.contains("resume { ptr, i32 }"), "{work}");
    assert!(
        work.contains("personality ptr @__gcc_personality_v0"),
        "{work}"
    );
}

#[test]
fn a_call_with_a_counted_local_unwinds_to_a_pad_releasing_it() {
    let ir = ir_raw(
        "fun f(n: i64): i64 { let xs = [\"a\" + \"b\"]; g(n) + xs.len }\n\
         fun g(n: i64): i64 = n * 2;\n\
         fun main() { let x = f(1); }",
    );
    let f = body(&ir, "f");
    assert!(f.contains("invoke i64 @lugha_fn_g("), "{f}");
    let pad = &f[f.find("landingpad").expect("a pad")..];
    assert!(pad.contains("@lugha_rc_dec("), "{f}");
}

#[test]
fn every_function_has_unwind_tables() {
    let ir = ir_raw("fun main() { }");
    assert!(ir.contains("uwtable"), "{ir}");
}

#[test]
fn main_lets_go_of_its_arguments_when_it_unwinds() {
    let ir = ir_raw("fun main(args: string[]) { let x = args[0]; }");
    let at = ir
        .find("define internal i32 @lugha_entry(")
        .expect("the C entry");
    let main = &ir[at..at + ir[at..].find("\n}").expect("a body ends")];
    assert!(main.contains("invoke void @lugha_fn_main("), "{main}");
    assert!(main.contains("landingpad"), "{main}");
}

#[test]
fn a_struct_result_through_a_pointer_keeps_its_pad() {
    let ir = ir_raw(
        "struct P { name: string, n: i64 }\n\
         fun make(n: i64): P = P { name: \"p\" + \"q\", n: n };\n\
         fun main(console: Console) { defer console.println(\"bye\"); let p = make(1); console.println(p.n); }",
    );
    let main = body(&ir, "main");
    assert!(
        main.contains("invoke void @lugha_fn_make(ptr sret(%P)"),
        "{main}"
    );
}
