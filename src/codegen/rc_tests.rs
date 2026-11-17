//! Tests of reference counting in codegen (`rc.rs`).

use crate::codegen::test_util::{ir, ir_rc};

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
fn a_copy_adds_a_holder_instead_of_marking_the_array() {
    let ir = ir_rc(
        "fun main(console: Console) { let xs = [\"a\"]; let ys = xs; console.println(xs.len + ys.len); }",
    );
    let main = body(&ir, "main");
    assert!(main.contains("call void @lugha_rc_inc("), "{main}");
    assert!(!main.contains("or i64 %cap"), "{main}");
}

#[test]
fn a_block_end_releases_its_locals() {
    let ir = ir_rc("fun f() { let xs = [1]; }\nfun main() { f(); }");
    assert!(body(&ir, "f").contains("call void @lugha_rc_dec("), "{ir}");
}

#[test]
fn a_returned_local_moves_out_without_a_new_holder() {
    let ir = ir_rc("fun f(): i64[] { let xs = [1]; xs }\nfun main() { let ys = f(); }");
    assert!(!body(&ir, "f").contains("@lugha_rc_inc("), "{ir}");
}

#[test]
fn a_write_clones_an_array_whose_count_is_not_one() {
    let ir = ir_rc("fun main() { let mut xs = [1]; let ys = xs; xs[0] = 2; let n = ys.len; }");
    let main = body(&ir, "main");
    assert!(main.contains("icmp ne i64 %count, 1"), "{main}");
    assert!(main.contains("cow.clone"), "{main}");
}

#[test]
fn objects_carry_their_drop_and_constants_never_die() {
    let ir = ir_rc("fun main() { let xs = [\"hey\"]; }");
    // One call makes the object and gives it its drop.
    assert!(
        ir.contains("@lugha_rt_rc_alloc(i64 24, ptr @lugha_rt_rc_drop_pointers)"),
        "{ir}"
    );
    assert!(!body(&ir, "main").contains("lugha_rt_rc_set_drop"), "{ir}");
    assert!(!body(&ir, "main").contains("@lugha_rt_alloc("), "{ir}");
    // The count of a literal: the immortal one.
    assert!(
        ir.contains("{ ptr null, i64 -9223372036854775808, i64 3,"),
        "{ir}"
    );
}

#[test]
fn a_recursive_enum_is_counted_through_its_box() {
    let ir = ir_rc("enum L { Nil, Cons(i64, L) }\nfun main() { let l = L.Cons(1, L.Nil); }");
    assert!(ir.contains("@lugha_rc_release_1L("), "{ir}");
    assert!(ir.contains("@lugha_rc_drop_box_1L"), "{ir}");
}

#[test]
fn without_the_flag_nothing_is_counted() {
    let ir = ir(
        "fun main(console: Console) { let xs = [\"a\"]; let ys = xs; console.println(ys.len); }",
    );
    assert!(!ir.contains("lugha_rc_"), "{ir}");
}

#[test]
fn values_crossing_to_another_task_are_marked_shared() {
    let ir = ir_rc(
        "fun work(xs: string[]): string[] = xs;\n\
         fun main() { let c: Chan<string[]> = Chan.buffered(1); c.send([\"a\"]); \
         let t = spawn work([\"b\"]); let r = t.join(); }",
    );
    let main = body(&ir, "main");
    // The send, the spawned call's environment and the joined result.
    assert!(
        main.matches("call void @lugha_rt_rc_mark(").count() >= 3,
        "{main}"
    );
    assert!(
        main.contains("call void @lugha_rt_rc_chan_setup("),
        "a channel knows how to release what it buffers: {main}"
    );
    assert!(
        main.contains("call void @lugha_rt_rc_task_setup("),
        "{main}"
    );
}

#[test]
fn a_plain_count_is_one_unsigned_compare() {
    let ir = ir_rc("fun main() { let xs = [\"a\"]; let ys = xs; }");
    let inc = &ir[ir
        .find("define internal void @lugha_rc_inc(")
        .expect("defined")..];
    let inc = &inc[..inc.find("\n}").expect("a body ends")];
    assert_eq!(inc.matches("icmp ult").count(), 1, "{inc}");
    assert!(inc.contains("call void @lugha_rt_rc_slow("), "{inc}");
}
