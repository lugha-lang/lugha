//! Order: `<`, `<=`, `>` and `>=` on numbers, strings and chars compare
//! inline; on a struct, enum or tuple they go through the prelude's `Ord`,
//! derived (`with Ord`, the tuple block) or written (`impl Ord for T`),
//! and call its `cmp`.

use super::instances::Instance;
use super::{Checker, Type};
use crate::ast::Expr;

/// The prelude's `Ordering` enum.
pub const ORDERING: &str = "$prelude.Ordering";

/// The key of an ordering comparison's use: `cmp` at its type.
pub(super) const ORDER: &str = "$order";

/// True for the types whose `<` goes through `Ord`: structs, enums and
/// tuples.
pub(super) fn ordered_through_ord(ty: &Type) -> bool {
    matches!(ty, Type::Struct(..) | Type::Enum(..))
}

impl Checker {
    /// Records the `cmp` an ordering comparison at `ty` calls: through a
    /// type parameter, whatever each instance's type needs.
    pub(super) fn order_uses(&mut self, expr: &Expr, ty: &Type) {
        // Through a type parameter, each instance decides: a number,
        // string or char compares inline and needs no `cmp`.
        if matches!(ty, Type::Param(_)) || ordered_through_ord(ty) {
            self.used(expr, ORDER, std::slice::from_ref(ty));
        }
    }

    /// The `cmp` an ordering comparison at concrete type `ty` calls: its
    /// hand-written `impl Ord`'s (listed when generic, lazy, or for
    /// `every`), or the covering block's.
    pub(super) fn ord_instances(&self, ty: &Type, every: bool) -> Vec<Instance> {
        if !ordered_through_ord(ty) {
            return Vec::new();
        }
        if let Type::Struct(owner, targs) | Type::Enum(owner, targs) = ty
            && self.derives.has_own(owner, "cmp")
            && let Some(m) = self.written_cmp(owner)
        {
            let listed = every || !m.generics.is_empty() || super::instances::lazy_key(&m.key);
            return if listed {
                vec![Instance {
                    key: m.key.clone(),
                    args: targs.clone(),
                }]
            } else {
                Vec::new()
            };
        }
        let union = |key: &str| self.holds_union(key);
        let found = self.derives.target(super::derive::ORD, "cmp", ty, union);
        found.into_iter().collect()
    }

    /// The method of `owner`'s hand-written `impl Ord`, if it has one.
    fn written_cmp(&self, owner: &str) -> Option<&super::methods::Method> {
        let entry = self
            .impls
            .get(owner)?
            .iter()
            .find(|e| e.key == super::derive::ORD)?;
        self.methods.get(owner)?.get(&entry.method("cmp"))
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{errors, ok};

    const LARGEST: &str = "fun largest<T: Ord>(a: T, b: T): T = if a < b { b } else { a };\n";

    #[test]
    fn a_type_that_asks_or_implements_is_ord() {
        ok(&format!(
            "{LARGEST}struct Score {{ points: i64 }} with Ord
            fun main() {{ let a = Score {{ points: 1 }}; let b = Score {{ points: 2 }};
                let x = a < b; let y = a >= b; let z = largest(a, b); let t = (a, 1) < (b, 0);
                let s = [b, a].sort(); }}"
        ));
        ok("enum Level { Low, High } with Ord
            fun main() { let up = Level.Low < Level.High; }");
        ok("struct P { x: i64 } with Eq
            impl Ord for P { fun cmp(self, o: P): Ordering = Ordering.Equal; }
            fun main() { let b = P { x: 1 } < P { x: 2 }; }");
        ok("struct Box<T> { v: T } with Ord
            fun main() { let b = Box { v: 1 } < Box { v: 2 }; }");
    }

    #[test]
    fn a_type_that_doesnt_isnt() {
        // A type's own `cmp` doesn't count.
        let own = "struct P { x: i64 }\nimpl P { fun cmp(self, o: P): Ordering = Ordering.Equal; }\n\
            fun main() { let a = P { x: 1 }; let b = a < a; }";
        assert_eq!(errors(own), [("E0422", "a < a")]);
        assert_eq!(
            errors(&format!(
                "{LARGEST}struct P {{ x: i64 }}\nfun main() {{ let a = P {{ x: 1 }}; let c = largest(a, a); }}"
            )),
            [("E0422", "largest(a, a)")]
        );
        // Arrays and floats aren't `Ord`; floats keep `<`.
        assert_eq!(errors("fun main() { let b = [1] < [2]; }")[0].0, "E0404");
        assert_eq!(errors("fun main() { let s = [1.5].sort(); }")[0].0, "E0451");
        ok("fun main() { let b = 1.5 < 2.5; }");
        assert_eq!(
            errors("struct P { x: f64 } with Ord\nfun main() {}")[0].0,
            "E0451"
        );
    }

    #[test]
    fn total_order_is_ieee_754s() {
        ok(
            "fun main() { let a = [2.5, -0.0].sort_by(f64.total_order); \
            let b: fun(f32, f32): bool = f32.total_order; let c = f64.total_order(1.0, 2.0); }",
        );
    }

    #[test]
    fn a_module_s_own_ordering_means_its_own() {
        let src = "enum Ordering { Less, More }
            struct P { x: i64 }
            impl P { fun cmp(self, o: P): Ordering = Ordering.More; }
            fun main() { let a = P { x: 1 }; let m = a.cmp(a);";
        ok(&format!("{src} }}"));
        assert_eq!(
            errors(&format!("{src} let b = a < a; }}")),
            [("E0422", "a < a")]
        );
    }
}
