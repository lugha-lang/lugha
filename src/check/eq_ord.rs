//! W0108: a struct or enum whose `==` and `<` can disagree, because it
//! derives `Eq` (`with Eq`, or an interface extending it such as `Key`)
//! but writes `Ord` by hand. `Ord: Eq` promises `a == b` exactly when
//! `a.cmp(b)` is `Ordering.Equal`, and a derived `Eq` compares every field
//! while a hand-written `cmp` may compare fewer. A type with only one thing
//! to compare (a struct of one field, an enum whose variants carry nothing)
//! is left be: there a `cmp` nearly always agrees with `==`.

use super::derive::{EQ, ORD};
use super::{Checker, Type};
use crate::ast::{Item, Program};
use crate::diagnostic::Diagnostic;

impl Checker {
    /// W0108 for each struct or enum of `program` that derives `Eq` and
    /// implements `Ord` by hand. The prelude's and std's types are left be.
    pub(super) fn eq_ord_agree(&mut self, program: &Program) {
        if self.in_prelude() {
            return;
        }
        for item in &program.items {
            let Item::Impl(block) = item else {
                continue;
            };
            let Some(bound) = &block.interface else {
                continue;
            };
            if block.derive || self.interface_key(bound).as_deref() != Some(ORD) {
                continue;
            }
            // A block for a built-in type (`impl Ord for f64[]`) has no
            // derived `Eq` to disagree with.
            if block.target.is_some() {
                continue;
            }
            let owner = match self.impl_owners.get(&block.span) {
                Some(Type::Struct(k, _) | Type::Enum(k, _)) => k.clone(),
                _ => self.key(&block.ty.name),
            };
            if owner.starts_with("std.") || !self.derives_eq(&owner) {
                continue;
            }
            let Some((declared, several)) = declared_at(program, &owner, |n| self.key(n)) else {
                continue;
            };
            if !several {
                continue;
            }
            let name = owner.rsplit('.').next().unwrap_or(&owner);
            let d = Diagnostic::warning(
                "W0108",
                format!("`{name}`'s `==` and `<` can disagree"),
                declared,
            )
            .with_primary_label("`==` compares every field (derived `Eq`)")
            .with_label(block.ty.span, "`<` compares what this `cmp` compares")
            .with_help(format!(
                "write `impl Eq for {name}` comparing what `cmp` compares, so `a == b` \
                 exactly when `a.cmp(b)` is `Ordering.Equal`; or derive both, `with Ord`"
            ));
            self.report(d);
        }
    }

    /// True if `owner` derives `Eq`, asking for it or for an interface
    /// that extends it (`with Key`).
    fn derives_eq(&self, owner: &str) -> bool {
        self.asking
            .asked
            .keys()
            .filter(|(o, _)| o == owner)
            .any(|(_, k)| k == EQ || self.extended_keys(k).iter().any(|e| e == EQ))
    }
}

/// The name span of the struct or enum of `program` whose key is `owner`,
/// and whether `==` compares more than one thing there: a struct of two or
/// more fields, or an enum with a payload.
fn declared_at(
    program: &Program,
    owner: &str,
    key: impl Fn(&str) -> String,
) -> Option<(crate::span::Span, bool)> {
    program.items.iter().find_map(|item| match item {
        Item::Struct(s) if key(&s.name.name) == owner => Some((s.name.span, s.fields.len() > 1)),
        Item::Enum(e) if key(&e.name.name) == owner => {
            let payloads = e.variants.iter().any(|v| !v.payloads.is_empty());
            Some((e.name.span, payloads))
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::diagnostics_or_warnings;

    /// The warning codes `src` reports.
    fn warnings(src: &str) -> Vec<&'static str> {
        diagnostics_or_warnings(src)
            .into_iter()
            .filter(|d| d.code.starts_with('W'))
            .map(|d| d.code)
            .collect()
    }

    const CMP: &str = "fun cmp(self, o: Run): Ordering = if self.loss < o.loss { Ordering.Less } \
                       else if self.loss > o.loss { Ordering.Greater } else { Ordering.Equal };";

    #[test]
    fn a_derived_eq_beside_a_written_ord_is_w0108() {
        let eq = format!(
            "struct Run {{ loss: i64, name: string }} with Eq\nimpl Ord for Run {{ {CMP} }}\nfun main() {{}}"
        );
        assert_eq!(warnings(&eq), ["W0108"]);
        let key = format!(
            "struct Run {{ loss: i64, name: string }} with Key\nimpl Ord for Run {{ {CMP} }}\nfun main() {{}}"
        );
        assert_eq!(warnings(&key), ["W0108"]);
    }

    #[test]
    fn agreeing_or_both_written_is_no_warning() {
        let none: [&str; 0] = [];
        let derived = "struct Run { loss: i64, name: string } with Ord\nfun main() {}";
        assert_eq!(warnings(derived), none);
        let both = format!(
            "struct Run {{ loss: i64, name: string }}\n\
             impl Eq for Run {{ fun eq(self, o: Run): bool = self.loss == o.loss; }}\n\
             impl Ord for Run {{ {CMP} }}\nfun main() {{}}"
        );
        assert_eq!(warnings(&both), none);
        let eq_only = "struct Run { loss: i64 } with Eq\nfun main() {}";
        assert_eq!(warnings(eq_only), none);
    }

    #[test]
    fn one_thing_to_compare_is_no_warning() {
        let none: [&str; 0] = [];
        let one_field = "struct Box { v: i64 } with Eq\n\
            impl Ord for Box { fun cmp(self, o: Box): Ordering = if self.v < o.v { Ordering.Less } \
            else if o.v < self.v { Ordering.Greater } else { Ordering.Equal }; }\nfun main() {}";
        assert_eq!(warnings(one_field), none);
        let fieldless = "enum Level { Low, High } with Eq\n\
            impl Ord for Level { fun cmp(self, o: Level): Ordering = Ordering.Equal; }\nfun main() {}";
        assert_eq!(warnings(fieldless), none);
        // A payload is more to compare: `Circle(1.0)` and `Square(1.0)`.
        let payload = "enum Shape { Circle(i64), Square(i64) } with Eq\n\
            impl Ord for Shape { fun cmp(self, o: Shape): Ordering = Ordering.Equal; }\nfun main() {}";
        assert_eq!(warnings(payload), ["W0108"]);
    }
}
