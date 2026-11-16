//! Unit tests of blocks for built-in types (`check::builtin_impls`), kept
//! apart to keep each file under 500 lines.

use crate::check::Type;
use crate::check::test_util::{diagnostics, errors, let_type, ok, test_mode};

const SHOW: &str = "interface Show { fun label(self): string; fun twice(self): string = self.label() + self.label(); }\n";

/// The codes of `body` after `SHOW`, with an empty `main`.
fn codes(body: &str) -> Vec<&'static str> {
    test_mode(&format!("{SHOW}{body}\nfun main() {{}}"))
        .err()
        .unwrap_or_default()
}

#[test]
fn a_built_in_type_meets_an_interface_through_its_block() {
    let blocks = "impl Show for f64 { fun label(self): string = \"f\"; }\n\
                  impl<T: Show> Show for T[] { fun label(self): string = \"a\"; }\n\
                  fun all<T: Show>(x: T): string = x.label();\n";
    for body in [
        "let s = 1.5.label();",
        "let s = Show.label(1.5);",
        "let s = [[1.5]].label();",
        "let s = all([1.5]);",
        "let s = 1.5.twice();",
        "let v: Show = 2.5; let s = v.label();",
        "let vs: Show[] = [2.5, [1.0]];",
    ] {
        ok(&format!("{SHOW}{blocks}fun main() {{ {body} }}"));
    }
    let ty = let_type(
        &format!("{SHOW}{blocks}fun main() {{ let s = 1.5.label(); }}"),
        "s",
    );
    assert_eq!(ty, Type::String);
    // Without a block: E0415 as a method, E0422 as a bound or a value.
    let none = |body: &str| {
        errors(&format!(
            "{SHOW}fun all<T: Show>(x: T) {{}}\nfun main() {{ {body} }}"
        ))[0]
            .0
    };
    assert_eq!(none("let s = true.label();"), "E0415");
    assert_eq!(none("all(true);"), "E0422");
    assert_eq!(none("let v: Show = true;"), "E0422");
}

#[test]
fn the_preludes_blocks_count_and_a_types_own_method_wins() {
    ok("fun main() { let a = 3.cmp(4); let b = \"a\".eq(\"b\"); let c = [1].eq([2]); }");
    // `Map` has its own `keys`: `m.keys()` is it, `Count.keys(m)` the item.
    let src = |body: &str| {
        format!(
            "interface Count {{ fun keys(self): i64; }}\n\
             impl<V> Count for Map<string, V> {{ fun keys(self): i64 = self.len; }}\n\
             fun main() {{ let m: Map<string, i64> = Map {{}}; {body} }}"
        )
    };
    assert!(matches!(
        let_type(&src("let k = m.keys();"), "k"),
        Type::Array(_)
    ));
    assert_eq!(let_type(&src("let k = Count.keys(m);"), "k"), Type::I64);
}

#[test]
fn a_block_is_written_where_and_for_what_it_may_be() {
    assert_eq!(
        codes("impl Show for fun(i64): i64 { fun label(self): string = \"\"; }"),
        ["E0459"]
    );
    assert_eq!(
        codes("impl<T> Show for T { fun label(self): string = \"\"; }"),
        ["E0459"]
    );
    assert_eq!(
        codes("impl<T, U> Show for T[] { fun label(self): string = \"\"; }"),
        ["E0459"]
    );
    assert_eq!(
        codes("struct B<T> { v: T }\nimpl<T> Show for B<T> { fun label(self): string = \"\"; }"),
        ["E0459"]
    );
    assert_eq!(
        codes(
            "struct P { x: i64 } with Eq\nimpl Ord for P[] { fun cmp(self, o: P[]): Ordering = Ordering.Equal; }"
        ),
        ["E0459"]
    );
    assert_eq!(
        codes("impl Show for f64 { fun labels(self): string = \"\"; }"),
        ["E0422"]
    );
    // Defaults may be left out.
    assert!(codes("impl Show for f64 { fun label(self): string = \"\"; }").is_empty());
}

#[test]
fn overlapping_blocks_are_e0461() {
    let d = diagnostics(&format!(
        "{SHOW}struct P {{ x: i64 }}\nimpl Show for P {{ fun label(self): string = \"\"; }}\n\
         impl<T: Show> Show for T[] {{ fun label(self): string = \"\"; }}\n\
         impl Show for P[] {{ fun label(self): string = \"\"; }}\nfun main() {{}}"
    ));
    assert_eq!(d[0].code, "E0461");
    assert_eq!(d[0].message, "`Show` is implemented for `P[]` twice");
    assert!(
        codes("impl Show for i64[] { fun label(self): string = \"\"; }\nimpl Show for f64[] { fun label(self): string = \"\"; }")
            .is_empty()
    );
}

#[test]
fn two_interfaces_with_an_item_make_the_call_qualified() {
    let src = format!(
        "{SHOW}interface Labels {{ fun label(self): string; }}\n\
         impl Show for f64 {{ fun label(self): string = \"a\"; }}\n\
         impl Labels for f64 {{ fun label(self): string = \"b\"; }}\n\
         fun main() {{ let a = 1.0.label(); let b = Labels.label(1.0); }}"
    );
    assert_eq!(errors(&src), [("E0460", "label")]);
}
