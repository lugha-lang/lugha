//! Weak references: `Weak<T>` refers to a `Ref<T>`'s box without keeping
//! it alive. `Weak.new(r)` and `w.get()` are its whole API.

use super::expr::Expect;
use super::{Checker, Type, errors};
use crate::ast::{Expr, Ident};

/// What codegen finds `Weak.new(r)` by in `Checked::callees`.
pub const WEAK_NEW: &str = "$Weak.new";

impl Checker {
    /// `Ref.new(…)` or `Weak.new(…)`, `t` naming which, called through
    /// `callee` on `base`; a type written on the maker (`Ref<f32>.new(1.5)`)
    /// is the box's.
    pub(super) fn box_maker(
        &mut self,
        call: &Expr,
        (callee, base, t): (&Expr, &Expr, &str),
        name: &Ident,
        args: &[Expr],
        expect: Option<&Expect>,
    ) -> Type {
        self.record(base, Type::Unit);
        self.record(callee, Type::Unit);
        let written = self.written_one(base, t).map(|w| {
            Expect::of(match t {
                "Ref" => Type::Ref(Box::new(w)),
                _ => Type::Weak(Box::new(w)),
            })
        });
        let expect = written.as_ref().or(expect);
        match t {
            "Ref" => self.ref_new(call, name, args, expect),
            _ => self.weak_new(call, name, args, expect),
        }
    }

    /// Whether the name `t` in an expression means the predeclared `Weak`.
    pub(super) fn predeclared_weak(&self, t: &str) -> bool {
        t == "Weak" && self.local(t).is_none()
    }

    /// `Weak.new(r)`: a weak reference to the box `r`, a `Weak<T>` for
    /// `r`'s `Ref<T>`. Any other `Weak.f(…)` is E0415.
    pub(super) fn weak_new(
        &mut self,
        call: &Expr,
        name: &Ident,
        args: &[Expr],
        expect: Option<&Expect>,
    ) -> Type {
        if name.name != "new" {
            let help = Some("a weak reference is made with `Weak.new(box)`".to_string());
            self.report(errors::no_method(
                &name.name,
                &Type::Weak(Box::new(Type::Error)),
                name.span,
                help,
            ));
            return self.unchecked(args);
        }
        let [value] = args else {
            self.report(errors::arity("new", 1, args.len(), call.span));
            return self.unchecked(args);
        };
        // `let w: Weak<f32> = Weak.new(r);` expects `r` to be a `Ref<f32>`.
        let boxed = match expect.map(|e| &e.ty) {
            Some(Type::Weak(inner)) => {
                self.expect_type(value, &Expect::of(Type::Ref(inner.clone())))
            }
            _ => self.value(value, None),
        };
        self.callees.insert(call.id, WEAK_NEW.to_string());
        match boxed {
            Type::Ref(inner) => Type::Weak(inner),
            Type::Error => Type::Weak(Box::new(Type::Error)),
            other => {
                let wanted = Type::Ref(Box::new(Type::Param("T".to_string())));
                self.report(errors::mismatch(wanted, other, value.span, None));
                Type::Weak(Box::new(Type::Error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type, ok};

    fn weak_of(t: Type) -> Type {
        Type::Weak(Box::new(t))
    }

    #[test]
    fn weak_references_are_made_and_read() {
        assert_eq!(
            let_type(
                "fun main() { let r = Ref.new(1); let w = Weak.new(r); }",
                "w"
            ),
            weak_of(Type::I64)
        );
        assert_eq!(
            let_type(
                "fun main() { let r = Ref.new(1); let w = Weak.new(r); let g = w.get(); }",
                "g"
            ),
            Type::Option(Box::new(Type::Ref(Box::new(Type::I64))))
        );
        ok("struct Node { value: i64, parent: Option<Weak<Node>> }
            fun main() {
                let a = Ref.new(Node { value: 1, parent: None });
                let b = Ref.new(Node { value: 2, parent: Some(Weak.new(a)) });
                let ws: Weak<Node>[] = [Weak.new(a), Weak.new(b)];
            }");
    }

    #[test]
    fn misuse_reuses_the_box_codes() {
        let first = |src: &str| errors(src)[0].0;
        assert_eq!(first("fun main() { let w = Weak.new(1); }"), "E0403");
        assert_eq!(
            first("fun main() { let r = Ref.new(1); let w = Weak.make(r); }"),
            "E0415"
        );
        assert_eq!(
            first("fun main() { let r = Ref.new(1); let w = Weak.new(r); let x = w.x; }"),
            "E0410"
        );
        assert_eq!(
            first("fun main() { let r = Ref.new(1); let w = Weak.new(r); let s = w.set(2); }"),
            "E0415"
        );
        assert_eq!(
            first("fun main() { let r = Ref.new(1); let w = Weak.new(r); let e = w == w; }"),
            "E0404"
        );
        assert_eq!(first("struct Weak { x: i64 }\nfun main() {}"), "E0302");
    }

    #[test]
    fn a_weak_reference_stays_in_its_task_and_has_no_equality() {
        let codes = |src: &str| errors(src).into_iter().map(|e| e.0).collect::<Vec<_>>();
        assert!(
            codes(
                "fun f(w: Weak<i64>) {}
                 fun main(root: Scope) { let r = Ref.new(1); let t = root.spawn(f(Weak.new(r))); }"
            )
            .contains(&"E0475")
        );
        assert!(
            codes(
                "struct S { w: Weak<i64> } with Eq
                 fun main() {}"
            )
            .contains(&"E0451")
        );
    }
}
