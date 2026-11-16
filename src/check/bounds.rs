//! Bounds: what `Eq` and `Ord` allow, E0419 for a
//! type argument that misses its bound, and the parameter a comparison needs one on.

use super::generics::{Bounds, Generic};
use super::{Checker, Type, errors};
use crate::span::Span;

impl Checker {
    /// E0419 for the first argument that doesn't meet its parameter's bounds.
    pub(super) fn meets_bounds(&mut self, generics: &[Generic], args: &[Type], span: Span) -> bool {
        for (g, arg) in generics.iter().zip(args) {
            // The interfaces come first: `Ord` extends `Eq`, and a type
            // missing `Ord` is reported as missing it, not as missing `Eq`.
            // The `ord` flag stands for the prelude's interface there.
            for interface in &g.bounds.interfaces {
                // A derivable interface (`json.Json`) is met through its
                // derive blocks: E0450 names the part
                // that isn't data.
                // `Ord` on a type written before the derive blocks are in
                // (a struct's own bound): checked once they are.
                if !self.asking.settled && interface.key == super::derive::ORD {
                    self.asking
                        .bounds
                        .push((arg.clone(), interface.key.clone(), span));
                    continue;
                }
                if self.derives.derivable(&interface.key) {
                    if !self.meets_derived(&interface.key, arg, span) {
                        return false;
                    }
                    continue;
                }
                // Only the numbers are `Num`, and only `f32`/`f64` are
                // `Float`: E0419, like `Ord`.
                let builtin = interface.key.strip_prefix("$prelude.");
                if let Some(bound @ ("Num" | "Float")) = builtin
                    && self
                        .builtin_operator(arg, &interface.key)
                        .is_some_and(|p| !p.is_empty())
                {
                    self.report(errors::unmet_bound(arg, bound, span));
                    return false;
                }
                // A bound's arguments may name the other parameters.
                let interface = interface.subst(generics, args);
                if !self.satisfies(arg, &interface, span) {
                    return false;
                }
            }
            // `Eq` on a struct, enum, tuple, array or `Option` is the
            // prelude's derivable `Eq`: E0451 with the path.
            let ord = g
                .bounds
                .interfaces
                .iter()
                .any(|i| i.key == super::derive::ORD);
            if ord && super::derive::compared_through_eq(arg) {
                // `Ord` was checked, and a type meeting it meets `Eq`.
            } else if g.bounds.eq && super::derive::compared_through_eq(arg) && !self.asking.settled
            {
                // Before the derive blocks are in: checked once they are.
                let eq = super::derive::EQ.to_string();
                self.asking.bounds.push((arg.clone(), eq, span));
            } else if g.bounds.eq && super::derive::compared_through_eq(arg) {
                if !self.meets_derived(super::derive::EQ, arg, span) {
                    return false;
                }
            } else if g.bounds.eq && !self.eq_ok(arg) {
                self.report(errors::unmet_bound(arg, "Eq", span));
                return false;
            }
        }
        true
    }

    /// The bounds of the parameter `name` in scope.
    pub(super) fn bounds(&self, name: &str) -> Bounds {
        self.type_params
            .iter()
            .find(|g| g.name == name)
            .map_or_else(Bounds::default, |g| g.bounds.clone())
    }

    /// True if `==` works on `ty`: always, except through a parameter
    /// without `Eq`.
    pub(super) fn eq_ok(&self, ty: &Type) -> bool {
        match ty {
            // `Num` gives `==` and `<` too.
            Type::Param(n) => self.bounds(n).eq || self.has_num(n),
            Type::Array(inner) | Type::Option(inner) => self.eq_ok(inner),
            // Only C knows which field of a union is live.
            Type::Struct(key, _) if self.holds_union(key) => false,
            // A function anywhere inside has no `==`.
            Type::Struct(_, args) | Type::Enum(_, args) => {
                !self.holds_fun(ty) && args.iter().all(|a| self.eq_ok(a))
            }
            // Neither do interface values, nor boxes.
            // Nor tasks and channels.
            Type::Fun(..)
            | Type::Interface(..)
            | Type::Ref(_)
            | Type::Task(_)
            | Type::Chan(_)
            | Type::Actor(_) => false,
            _ => true,
        }
    }

    /// True if `<` works on `ty`: numbers, strings, and parameters with `Ord`.
    pub(super) fn ord_ok(&self, ty: &Type) -> bool {
        match ty {
            Type::Param(n) => self.bounds(n).ord || self.has_num(n),
            // Structs, enums and tuples are judged by `Ord` itself.
            Type::Struct(..) | Type::Enum(..) => true,
            // `()`: always equal.
            _ => ty.is_numeric() || matches!(ty, Type::String | Type::Char | Type::Unit),
        }
    }

    /// The parameter a comparison needs a bound on: `T` in `T`, `T[]` or
    /// `Pair<T, i64>` when `T` lacks `Eq` (or `Ord`, for `ord`).
    pub(super) fn missing_bound(&self, ty: &Type, ord: bool) -> Option<String> {
        match ty {
            Type::Param(n) if (ord && !self.bounds(n).ord) || (!ord && !self.bounds(n).eq) => {
                Some(n.clone())
            }
            Type::Array(inner) | Type::Option(inner) => self.missing_bound(inner, ord),
            Type::Struct(_, args) | Type::Enum(_, args) => {
                args.iter().find_map(|a| self.missing_bound(a, ord))
            }
            _ => None,
        }
    }

    /// True if a value of `ty` holds a function or an interface value
    /// somewhere: in a field, a payload or an element, through any depth of
    /// structs and enums. Neither has `==`.
    pub(super) fn holds_fun(&self, ty: &Type) -> bool {
        self.holds_fun_in(ty, &mut Vec::new())
    }

    fn holds_fun_in(&self, ty: &Type, seen: &mut Vec<Type>) -> bool {
        let inner: Vec<Type> = match ty {
            // A box makes its holder lose `==` too.
            Type::Fun(..)
            | Type::Interface(..)
            | Type::Ref(_)
            | Type::Task(_)
            | Type::Chan(_)
            | Type::Actor(_) => return true,
            Type::Array(inner) | Type::Option(inner) => return self.holds_fun_in(inner, seen),
            // Recursive types meet themselves again: one look is enough.
            Type::Struct(..) | Type::Enum(..) if seen.contains(ty) => return false,
            Type::Struct(key, args) => {
                let fields = self
                    .structs
                    .get(key)
                    .map(|s| s.fields.clone())
                    .unwrap_or_default();
                fields
                    .iter()
                    .filter_map(|(f, _)| self.field_type(key, args, f))
                    .collect()
            }
            Type::Enum(key, args) => {
                let variants = self.enum_variants(key, args).unwrap_or_default();
                variants.into_iter().flat_map(|(_, ps)| ps).collect()
            }
            _ => return false,
        };
        seen.push(ty.clone());
        inner.iter().any(|t| self.holds_fun_in(t, seen))
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{diagnostics, errors, let_type, ok};

    #[test]
    fn type_arguments_must_meet_their_bounds() {
        let set = "struct Set<T: Ord> { items: T[] }\n";
        assert_eq!(
            errors(&format!("{set}fun f(s: Set<i64[]>) {{}}\nfun main() {{}}")),
            // Arrays aren't `Ord`: the part is named (E0451).
            [("E0451", "Set<i64[]>")]
        );
        ok(&format!(
            "{set}fun f(s: Set<string>) {{}}\nfun g<U: Ord>(s: Set<U>) {{}}\nfun main() {{}}"
        ));
        assert_eq!(
            errors(&format!("{set}fun g<U>(s: Set<U>) {{}}\nfun main() {{}}")),
            [("E0419", "Set<U>")]
        );
        let d = diagnostics(&format!("{set}fun f(s: Set<i64[]>) {{}}\nfun main() {{}}"));
        assert_eq!(
            d[0].message,
            "`i64[]` doesn't meet `Ord`: it is `i64[]`, which no `derive Ord` block covers"
        );
    }

    #[test]
    fn a_bare_parameter_allows_only_what_its_bounds_give() {
        let d = diagnostics("fun same<T>(a: T, b: T): bool = a == b;\nfun main() {}");
        assert_eq!(d[0].code, "E0404");
        assert!(
            d[0].help.as_deref().unwrap().contains("T: Eq"),
            "{:?}",
            d[0].help
        );
        assert_eq!(
            errors("fun less<T: Eq>(a: T, b: T): bool = a < b;\nfun main() {}"),
            [("E0404", "a < b")]
        );
        // A bare parameter shows its debug form; it prints with `T: Display`.
        ok(
            "fun f<T>(console: Console, a: T) { console.println(a.debug()); }\nfun g<T: Display>(a: T) { debug(a); }\nfun main() {}",
        );
        ok(
            "fun same<T: Eq>(a: T, b: T): bool = a == b;\nfun less<T: Ord>(a: T, b: T): bool = a < b;\nfun main() {}",
        );
        for (body, code, at) in [
            ("let c = a + a;", "E0404", "a + a"),
            ("let c = a as i64;", "E0408", "a as i64"),
            ("let n = a.len;", "E0410", "len"),
            ("let c = a[0];", "E0411", "a"),
            ("let x: T = 0;", "E0401", "0"),
        ] {
            let src = format!("fun f<T>(a: T) {{ {body} }}\nfun main() {{}}");
            assert_eq!(errors(&src), [(code, at)], "{body}");
        }
        assert_eq!(
            let_type(
                "fun id<T>(a: T): T { let b = a; b }\nfun main() { let x = 1; }",
                "x"
            ),
            Type::I64
        );
    }
}
