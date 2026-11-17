//! The unit type `()`: W0106 for a `let` that binds a
//! `()` it didn't write.

use super::{Checker, errors};
use crate::ast::{Expr, ExprKind, Ident};

impl Checker {
    /// W0106 for `let name = init` where `init` is `()` but not the literal
    /// `()`: usually a call kept for a result it doesn't have. `_` and a
    /// written type (`let x: () = …`) are quiet.
    pub(super) fn unit_binding(&mut self, name: &Ident, init: &Expr) {
        if name.name == "_" || matches!(&init.kind, ExprKind::Tuple(xs) if xs.is_empty()) {
            return;
        }
        let mut d = errors::unit_binding(&name.name, name.span, init.span);
        if let ExprKind::Block(block) = &init.kind
            && let Some(semicolon) = self.stray_semicolon(block)
        {
            d = errors::with_stray_semicolon(d, semicolon);
        }
        self.report(d);
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type, ok, warnings};

    #[test]
    fn unit_is_a_value_everywhere() {
        ok("fun log(s: string) {}\n\
            fun save(p: string): Result<(), string> {\n\
                if p == \"\" { return Err(\"no path\"); }\n\
                Ok(())\n\
            }\n\
            fun twice(p: string): Result<(), string> { save(p)?; save(p) }\n\
            fun id<T>(x: T): T = x;\n\
            fun g(): () = log(\"g\");\n\
            fun h(): () { return; }\n\
            fun k(): () { return (); }\n\
            struct Box { u: (), n: i64 }\n\
            fun main(console: Console) {\n\
                let u: () = log(\"a\");\n\
                let us: ()[] = [(), u, id(())];\n\
                let o: Option<()> = Some(());\n\
                let b = Box { u: (), n: 1 };\n\
                console.println(u == ());\n\
                console.println(us.debug());\n\
                console.println(\"{u.debug()} {o.debug()}\");\n\
                let c: Chan<()> = Chan.new();\n\
                let done = scope s { let t: Task<()> = s.spawn(log(\"t\")); let r: Result<(), Stopped> = t.join(); };\n\
                let m: Map<string, ()> = Map { \"a\": () };\n\
            }");
        assert_eq!(let_type("fun main() { let x = (); }", "x"), Type::Unit);
        assert_eq!(
            let_type("fun log() {}\nfun main() { let x: () = log(); }", "x"),
            Type::Unit
        );
    }

    #[test]
    fn binding_a_unit_result_warns_w0106() {
        let src = "fun log() {}\nfun main() { let x = log(); }";
        assert_eq!(warnings(src), [("W0106", "x")]);
        for quiet in [
            "fun log() {}\nfun main() { let x: () = log(); }",
            "fun log() {}\nfun main() { let _ = log(); }",
            "fun main() { let x = (); }",
            "fun log() {}\nfun main() { log(); }",
        ] {
            assert_eq!(warnings(quiet), [], "{quiet}");
        }
        assert_eq!(
            warnings("fun main() { let x = { 1 + 1; }; }"),
            [("W0106", "x")]
        );
    }

    #[test]
    fn unit_is_a_pattern() {
        ok("fun log() {}\n\
            fun main(console: Console) {\n\
                let () = log();\n\
                match log() { () => console.println(1) }\n\
                let o: Option<()> = Some(());\n\
                match o { .Some(()) => console.println(2), .None => console.println(3) }\n\
                if let .Some(()) = o { console.println(4); }\n\
                let t = ((), 5);\n\
                let ((), n) = t;\n\
            }");
        assert_eq!(
            errors(
                "fun main(console: Console) { match 1 { () => console.println(1), _ => console.println(2) } }"
            ),
            [("E0403", "()")]
        );
        assert_eq!(
            errors("fun main(console: Console) { let o: Option<()> = None; match o { .Some(()) => console.println(1) } }")
                [0]
            .0,
            "E0417"
        );
    }

    #[test]
    fn unit_doesnt_cross_to_c() {
        assert_eq!(
            errors("extern fun f(u: ());\nfun main() {}"),
            [("E0409", "()")]
        );
        assert_eq!(
            errors("extern struct S { n: i64, u: () }\nfun main() {}"),
            [("E0437", "()")]
        );
    }
}
