//! Tests of reference counting in codegen (`rc.rs`).

use crate::codegen::test_util::{ir_rc, ir_rc_check};

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
fn values_crossing_to_another_task_are_marked_shared() {
    let ir = ir_rc(
        "fun work(xs: string[]): string[] = xs;\n\
         fun main() { let (c, r) = Chan<string[]>.new(1); c.send([\"a\"]); \
         let _ = scope s { let t = s.spawn(work([\"b\"])); let r = t.join(); }; }",
    );
    let main = body(&ir, "main");
    // The send and the spawned call's environment; the joined result is
    // marked by the task that made it, not by `main`.
    assert_eq!(
        main.matches("call void @lugha_rt_rc_mark(").count(),
        2,
        "{main}"
    );
    let entry = &ir[ir
        .find("define internal void @lugha_task_main.0(")
        .expect("the task's entry")..];
    let entry = &entry[..entry.find("\n}").expect("a body ends")];
    assert!(entry.contains("@lugha_rt_rc_mark("), "{entry}");
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

#[test]
fn a_fresh_value_through_an_if_or_match_moves_out() {
    let ir = ir_rc(
        "enum T { Leaf, Node(T, T) }\n\
         fun make(d: i64): T = if d == 0 { T.Node(T.Leaf, T.Leaf) } else { T.Node(make(d - 1), make(d - 1)) };\n\
         fun pick(d: i64): T = match d { 0 => T.Leaf, _ => { T.Node(T.Leaf, T.Leaf) } };\n\
         fun main() { let t = make(2); let u = pick(1); }",
    );
    for name in ["make", "pick"] {
        let f = body(&ir, name);
        assert!(!f.contains("@lugha_rc_retain_"), "{name}: {f}");
    }
}

#[test]
fn a_local_goes_after_its_last_use_not_at_its_block_end() {
    let ir = ir_rc(
        "fun main(console: Console) { let xs = [\"a\"]; let n = xs.len; console.println(n); }",
    );
    let main = body(&ir, "main");
    // The last read of `xs`'s slot releases it, before the printing call.
    let last = &main[main.rfind("= load ptr, ptr %xs,").expect("read")..];
    let released = last.find("call void @lugha_rc_dec(").expect("released");
    let printed = last.find("call void %slot(").expect("printed");
    assert!(released < printed, "{main}");
}

#[test]
fn counts_are_read_and_written_as_unordered_atomics() {
    let ir = ir_rc("fun main() { let xs = [\"a\"]; let ys = xs; }");
    let inc = &ir[ir
        .find("define internal void @lugha_rc_inc(")
        .expect("defined")..];
    let inc = &inc[..inc.find("\n}").expect("a body ends")];
    assert!(
        inc.contains("load atomic i64, ptr %count.at unordered, align 8"),
        "{inc}"
    );
    assert!(
        inc.contains("store atomic i64 %next, ptr %count.at unordered, align 8"),
        "{inc}"
    );
}

#[test]
fn constants_become_immortal_once_filled() {
    let ir = ir_rc(
        "const NAMES: string[] = [\"a\", \"b\"];\n\
         fun main(console: Console) { console.println(NAMES.len); }",
    );
    let init = &ir[ir.find("define void @lugha_const_init(").expect("defined")..];
    let init = &init[..init.find("\n}").expect("a body ends")];
    let start = init.find("@lugha_rt_rc_freeze_start(").expect("frozen");
    let mark = init.find("@lugha_rt_rc_mark(").expect("marked");
    let end = init.find("@lugha_rt_rc_freeze_end(").expect("ended");
    assert!(start < mark && mark < end, "{init}");
}

#[test]
fn only_a_build_for_the_owner_check_tests_a_checked_count_of_one() {
    let src = "fun main() { let mut xs = [1]; xs[0] = 2; let n = xs.len; }";
    let plain = ir_rc(src);
    assert!(!body(&plain, "main").contains("not.checked.one"), "{plain}");
    let checked = ir_rc_check(src);
    assert!(
        body(&checked, "main").contains("not.checked.one"),
        "{checked}"
    );
}
