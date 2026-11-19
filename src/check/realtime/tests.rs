use crate::check::test_util::{errors, ok};

/// The E0482 messages `src` gets, in order.
fn breaks(src: &str) -> Vec<String> {
    errors(src)
        .into_iter()
        .filter(|(code, _)| *code == "E0482")
        .map(|(_, at)| at.to_string())
        .collect()
}

const ON_TICK: &str = "extern fun on_tick(f: realtime fun(i64, data): i64, user: data);\n";

#[test]
fn computing_matching_calling_helpers_and_writing_a_ring_is_allowed() {
    ok(&format!(
        "{ON_TICK}extern fun labs(x: i64): i64;\n\
         struct Pair {{ a: i64, b: i64 }}\n\
         fun clamp(n: i64, top: i64): i64 = if n > top {{ top }} else {{ n }};\n\
         fun main() {{\n\
             let (tx, rx) = Ring<i64>.new(16);\n\
             let scale = 3;\n\
             on_tick(fun(n: i64): i64 {{\n\
                 let p = Pair {{ a: n, b: scale }};\n\
                 let mut total = 0;\n\
                 for i in 0..p.a {{ total += i * p.b; }}\n\
                 match tx.try_send(clamp(total, 1000)) {{ .Sent => 0, _ => labs(n) }}\n\
             }});\n\
         }}"
    ));
}

#[test]
fn a_named_function_is_checked_like_a_closure() {
    let found = breaks(&format!(
        "{ON_TICK}fun tick(n: i64): i64 {{ let s = \"{{n}}\"; n }}\n\
         fun main() {{ on_tick(tick); }}"
    ));
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn helpers_calling_each_other_are_walked_once() {
    ok(&format!(
        "{ON_TICK}fun even(n: i64): bool = if n == 0 {{ true }} else {{ odd(n - 1) }};\n\
         fun odd(n: i64): bool = if n == 0 {{ false }} else {{ even(n - 1) }};\n\
         fun main() {{ on_tick(fun(n: i64): i64 = if even(n) {{ 1 }} else {{ 0 }}); }}"
    ));
}

#[test]
fn a_panics_message_may_be_built() {
    ok(&format!(
        "{ON_TICK}fun main() {{ on_tick(fun(n: i64): i64 {{ if n < 0 {{ panic(\"negative: {{n}}\"); }} n }}); }}"
    ));
}

#[test]
fn a_direct_callback_isnt_checked() {
    ok(
        "extern fun on_tick(f: direct fun(i64, data), user: data);\n\
        fun main() { on_tick(fun(n: i64) { let s = \"{n}\"; }); }",
    );
}

#[test]
fn a_generic_helper_passing_values_is_allowed() {
    ok(&format!(
        "{ON_TICK}fun first<T>(a: T, b: T): T = a;\n\
         fun main() {{ on_tick(fun(n: i64): i64 = first(n, 0)); }}"
    ));
}

#[test]
fn a_generic_helpers_type_parameter_method_cant_be_checked() {
    let found = breaks(&format!(
        "{ON_TICK}fun said<T: Show>(x: T): i64 {{ let s = x.show(); 0 }}\n\
         fun main() {{ on_tick(fun(n: i64): i64 = said(n)); }}"
    ));
    assert_eq!(found.len(), 1, "{found:?}");
}
