//! Unit tests of `impl I for T` blocks (`check::impls`), kept apart to
//! keep each file under 500 lines.

use crate::check::test_util::{diagnostics, errors, ok};

const SHAPE: &str = "interface Shape { fun area(self): f64; fun name(self): string = \"shape\"; }\n\
    interface Plot { fun area(self): f64; }\n\
    struct C { r: f64 }\n";

fn codes(body: &str) -> Vec<&'static str> {
    errors(&format!("{SHAPE}{body}\nfun main() {{}}"))
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

#[test]
fn a_block_implements_and_its_items_are_reached_every_way() {
    ok(&format!(
        "{SHAPE}impl C {{ fun area(self): i64 = 0; }}\n\
         impl Shape for C {{ fun area(self): f64 = self.r; }}\n\
         fun t<T: Shape>(x: T): f64 = x.area();\n\
         fun main() {{\n\
             let c = C {{ r: 1.0 }};\n\
             let own: i64 = c.area();\n\
             let a: f64 = Shape.area(c) + t(c);\n\
             let s: Shape = c;\n\
             let n: string = c.name();\n\
         }}"
    ));
}

#[test]
fn a_block_must_match_its_interface() {
    assert_eq!(
        codes("impl Shape for C { fun area(self): i64 = 1; }"),
        ["E0422"]
    );
    assert_eq!(codes("impl Shape for C {}"), ["E0422"]);
    let src = format!(
        "{SHAPE}impl Shape for C {{ fun area(self): f64 = 1.0; fun more(self) {{}} }}\nfun main() {{}}"
    );
    let help = diagnostics(&src)[0].help.clone().expect("a help");
    assert!(help.contains("`more` isn't an item of `Shape`"), "{help}");
}

#[test]
fn a_misplaced_block_is_e0459() {
    assert_eq!(
        codes("impl Shape for fun(): i64 { fun area(self): f64 = 1.0; }"),
        ["E0459"]
    );
    assert_eq!(
        codes("impl Display for C { fun show(self): string = \"c\"; }"),
        ["E0459"]
    );
    use crate::check::modules::test_util::errors;
    let found = errors(&[
        (
            "main.la",
            "import geo;\nimport shapes;\n\
             impl shapes.Shape for geo.Sq { fun area(self): f64 = 1.0; }\nfun main() {}\n",
        ),
        ("geo.la", "public struct Sq { s: f64 }\n"),
        (
            "shapes.la",
            "public interface Shape { fun area(self): f64; }\n",
        ),
    ]);
    assert_eq!(found.iter().map(|(c, _)| *c).collect::<Vec<_>>(), ["E0459"]);
}

#[test]
fn two_blocks_of_one_interface_are_e0461() {
    let block = "impl Shape for C { fun area(self): f64 = 1.0; }\n";
    assert_eq!(codes(&format!("{block}{block}")), ["E0461"]);
    ok(
        "interface From<T> { fun from(x: T): Self; }\nstruct P { n: i64 }\n\
        impl From<i64> for P { fun from(x: i64): P = P { n: x }; }\n\
        impl From<bool> for P { fun from(x: bool): P = P { n: 0 }; }\nfun main() {}",
    );
}

#[test]
fn a_name_two_implemented_interfaces_share_is_e0460() {
    let blocks = "impl Shape for C { fun area(self): f64 = 1.0; }\n\
                  impl Plot for C { fun area(self): f64 = 2.0; }\n";
    assert_eq!(
        codes(&format!(
            "{blocks}fun f() {{ let a = C {{ r: 1.0 }}.area(); }}"
        )),
        ["E0460"]
    );
    let src = format!("{SHAPE}{blocks}fun main() {{ let c = C {{ r: 1.0 }}; let a = c.area(); }}");
    let help = diagnostics(&src)[0].help.clone().expect("a help");
    assert!(help.contains("`Plot.area(c)` or `Shape.area(c)`"), "{help}");
    ok(&format!(
        "{SHAPE}{blocks}fun main() {{ let a = Plot.area(C {{ r: 1.0 }}); }}"
    ));
}

#[test]
fn a_qualified_call_needs_the_block() {
    assert_eq!(
        codes("fun f() { let a = Shape.area(C { r: 1.0 }); }"),
        ["E0422"]
    );
    assert_eq!(
        codes("fun f() { let a = Shape.size(C { r: 1.0 }); }"),
        ["E0415"]
    );
}
