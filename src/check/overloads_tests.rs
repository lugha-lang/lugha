//! Unit tests of operator blocks (`check::overloads`), kept apart to keep
//! each file under 500 lines.

use crate::check::Type;
use crate::check::test_util::{diagnostics, errors, let_type, ok};

const V: &str = "struct V { x: f64 }\n\
    impl Mul for V { fun mul(self, w: V): V = V { x: self.x * w.x }; }\n\
    impl Mul<f64> for V { fun mul(self, k: f64): V = V { x: self.x * k }; }\n\
    impl Mul<u8> for V { fun mul(self, n: u8): V = V { x: self.x * (n as f64) }; }\n\
    impl Mul<i64> for V { fun mul(self, n: i64): V = V { x: self.x * (n as f64) }; }\n";

#[test]
fn the_right_operand_chooses_the_block() {
    ok(&format!(
        "{V}fun main() {{ let v = V {{ x: 1.0 }}; let b: u8 = 2;\n\
         let a = v * v; let c = v * 2.0; let d = v * b; let e = v.mul(3.0);\n\
         let f = Mul.mul(v, b); let mut g = v; g *= 2.0; g *= v; }}"
    ));
}

#[test]
fn a_literal_fitting_two_is_e0457_and_none_is_e0404() {
    let src = format!("{V}fun main() {{ let v = V {{ x: 1.0 }}; let a = v * 2; }}");
    assert_eq!(errors(&src), [("E0457", "2")]);
    let src = format!("{V}fun main() {{ let v = V {{ x: 1.0 }}; let a = v * \"s\"; }}");
    assert_eq!(errors(&src), [("E0404", "\"s\"")]);
    let d = diagnostics(&format!(
        "{V}fun main() {{ let v = V {{ x: 1.0 }}; let a = v * true; }}"
    ));
    assert!(d[0].help.as_deref().unwrap().contains("`f64`"), "{d:?}");
}

#[test]
fn the_result_is_the_blocks_and_a_number_may_be_on_the_left() {
    let src = "struct M { m: f64 }\nstruct A { a: f64 }\n\
               impl Mul for M { fun mul(self, o: M): A = A { a: self.m * o.m }; }\n\
               impl Mul<M> for f64 { fun mul(self, o: M): M = M { m: self * o.m }; }\n\
               fun main() { let a = M { m: 2.0 } * M { m: 3.0 }; let b = 2.0 * M { m: 1.0 }; }";
    assert_eq!(let_type(src, "a"), Type::Struct("A".into(), vec![]));
    assert_eq!(let_type(src, "b"), Type::Struct("M".into(), vec![]));
    // A block giving another type doesn't keep `T: Mul`'s promise.
    let bound = src.replace(
        "fun main() {",
        "fun sq<T: Mul>(x: T): T = x * x;\nfun main() { let c = sq(M { m: 1.0 });",
    );
    assert_eq!(errors(&bound)[0].0, "E0422");
}

#[test]
fn only_an_operator_bound_gives_an_operator() {
    let scales = "interface Scales { fun mul(self, k: f64): Self; }\n";
    assert_eq!(
        errors(&format!(
            "{scales}fun f<T: Scales>(s: T): T = s * 2.0;\nfun main() {{}}"
        )),
        [("E0404", "s * 2.0")]
    );
    ok(&format!(
        "{scales}fun f<T: Scales + Mul<f64>>(s: T): T = s * 2.0;\nfun main() {{}}"
    ));
    // An own method named `add` is only a method.
    let own = "struct P { n: i64 }\nimpl P { fun add(self, o: P): P = o; }\n";
    assert_eq!(
        errors(&format!(
            "{own}fun main() {{ let p = P {{ n: 1 }} + P {{ n: 2 }}; }}"
        ))[0]
            .0,
        "E0404"
    );
}

#[test]
fn defaults_fill_trailing_arguments() {
    ok("interface Conv<To = Self> { fun conv(self): To; }\n\
        struct C { c: f64 }\nimpl Conv for C { fun conv(self): C = self; }\n\
        impl Conv<f64> for i64 { fun conv(self): f64 = self as f64; }\n\
        fun same<T: Conv>(x: T): T = x.conv();\nfun main() { let c = same(C { c: 1.0 }); }");
    assert_eq!(
        errors(
            "interface Two<A, B = A> { fun a(self): A; }\nfun f<T: Two>(x: T) {}\nfun main() {}"
        ),
        [("E0418", "Two")]
    );
}
