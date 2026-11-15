//! Deriving on request: `struct S { … } with I, J` and
//! `impl I for T = derive;`. A struct or enum meets a derivable interface
//! only when it asks, through one of these or a hand-written
//! `impl I for T { … }`: the struct and enum derive blocks are templates,
//! not implementations. `Eq` is still met without asking; `with Eq` is
//! checked all the same.

use std::collections::HashSet;

use super::derive::{Covers, EQ};
use super::generics::has_param;
use super::{Checker, Type, errors};
use crate::ast::{Bound, Item, Program};
use crate::span::Span;

impl Checker {
    /// Pass 3, after the derive blocks: every `with` list and
    /// `impl I for T = derive;` of `program`. E0315 for an unknown
    /// interface, E0462 for one without a block for the type's kind, E0461
    /// for one asked twice.
    pub(super) fn collect_with(&mut self, program: &Program) {
        for item in &program.items {
            match item {
                Item::Struct(s) => {
                    let owner = self.key(&s.name.name);
                    self.ask(&owner, Covers::Struct, &s.derives);
                }
                Item::Enum(e) => {
                    let owner = self.key(&e.name.name);
                    self.ask(&owner, Covers::Enum, &e.derives);
                }
                Item::Impl(i) if i.derive => {
                    let Some(b) = &i.interface else { continue };
                    let ty = match self.impl_owners.get(&i.span) {
                        Some(ty) => ty.clone(),
                        None => match self.named_type(&self.key(&i.ty.name)) {
                            Some(ty) => ty,
                            None => continue,
                        },
                    };
                    let (owner, kind) = match &ty {
                        Type::Struct(k, _) => (k.clone(), Covers::Struct),
                        Type::Enum(k, _) => (k.clone(), Covers::Enum),
                        // E0459, reported with the block.
                        _ => continue,
                    };
                    self.ask(&owner, kind, std::slice::from_ref(b));
                }
                _ => {}
            }
        }
    }

    /// Records that struct or enum `owner` asks to derive each of `asked`.
    fn ask(&mut self, owner: &str, kind: Covers, asked: &[Bound]) {
        for b in asked {
            // `Eq` is written as the built-in bound.
            let eq = b.module.is_none() && b.name.name == "Eq" && b.args.is_empty();
            let Some(key) = eq.then(|| EQ.to_string()).or_else(|| self.interface_key(b)) else {
                let is_type = b.module.is_none() && self.is_type_name(&b.name.name);
                self.report(errors::unknown_bound(&b.name.name, b.span, is_type));
                continue;
            };
            self.item_visible(&key, b.span);
            self.define_bound(b, &key);
            let shown = shown_interface(&key);
            if key != EQ && !self.derives.covers(&key, &kind) {
                let covered = self.derives.covered(&key);
                let what = match kind {
                    Covers::Enum => "an enum",
                    _ => "a struct",
                };
                let ty = owner.rsplit('.').next().unwrap_or(owner);
                let by_name = !self.by_block(&key);
                let d = errors::not_derivable(&shown, (ty, what), &covered, by_name, b.span);
                self.report(d);
                continue;
            }
            let at = (owner.to_string(), key);
            if let Some(first) = self.asked.get(&at) {
                let ty = Type::Struct(owner.to_string(), Vec::new());
                self.report(errors::duplicate_impl(&shown, &ty, b.span, *first));
                continue;
            }
            self.asked.insert(at, b.span);
        }
    }

    /// True if struct or enum `owner` asked to derive `interface`. The
    /// prelude's types (`Result`, `Ordering`) are built in, like `Option`,
    /// and meet every derivable interface their parts meet.
    pub(super) fn asks(&self, owner: &str, interface: &str) -> bool {
        owner.starts_with("$prelude.")
            || self
                .asked
                .contains_key(&(owner.to_string(), interface.to_string()))
    }

    /// After every module's declarations: E0463 for a type that derives and implements one
    /// interface, and E0451 for a field or payload that doesn't meet an
    /// interface its type derives. Fields that mention a type parameter
    /// are checked at each use instead.
    pub(super) fn check_with(&mut self, program: &Program) {
        let mut asked: Vec<(String, Span)> = Vec::new();
        for item in &program.items {
            let (name, derives) = match item {
                Item::Struct(s) => (&s.name.name, &s.derives),
                Item::Enum(e) => (&e.name.name, &e.derives),
                _ => continue,
            };
            let owner = self.key(name);
            asked.extend(derives.iter().map(|b| (owner.clone(), b.span)));
        }
        for item in &program.items {
            if let Item::Impl(i) = item
                && i.derive
                && let Some(b) = &i.interface
            {
                let owner = match self.impl_owners.get(&i.span) {
                    Some(Type::Struct(k, _) | Type::Enum(k, _)) => k.clone(),
                    _ => self.key(&i.ty.name),
                };
                asked.push((owner, b.span));
            }
        }
        for (owner, span) in asked {
            let found = self
                .asked
                .iter()
                .find(|((o, _), s)| *o == owner && **s == span)
                .map(|((_, k), _)| k.clone());
            if let Some(key) = found {
                self.check_one(&owner, &key, span);
            }
        }
    }

    /// E0463 and E0451 for `owner` deriving `key`, asked at `span`.
    fn check_one(&mut self, owner: &str, key: &str, span: Span) {
        let shown = shown_interface(key);
        let ty = owner.rsplit('.').next().unwrap_or(owner).to_string();
        let written = self
            .impls
            .get(owner)
            .and_then(|es| es.iter().find(|e| e.key == key))
            .map(|e| e.span);
        if let Some(written) = written {
            let d = errors::derived_and_written(&ty, &shown, span, written);
            self.report(d);
            return;
        }
        for (path, part) in self.parts_of(owner) {
            if has_param(&part) {
                continue;
            }
            if let Some((inner, why)) = self.unmet_derived(key, &part, &path, &mut HashSet::new()) {
                let me = Type::Struct(ty.clone(), Vec::new());
                let d = match key == super::json::BOUND {
                    true => errors::not_json(&me, &inner, &super::json::describe(&why), span),
                    false => {
                        let name = key.rsplit('.').next().unwrap_or(key);
                        let what = super::derive_meets::describe(name, &why);
                        let covered = self.derives.covered(key);
                        errors::not_derived(&me, name, (&inner, &what), &covered, span)
                    }
                };
                self.report(d);
                // Uses of the type aren't reported again.
                self.broken.insert((owner.to_string(), key.to_string()));
                return;
            }
        }
    }

    /// A struct's fields or an enum's payloads, each with its path
    /// (`.name`, `.Variant.0`), in their declared types.
    fn parts_of(&self, owner: &str) -> Vec<(String, Type)> {
        if let Some(info) = self.structs.get(owner) {
            return info
                .fields
                .iter()
                .map(|(n, t)| (format!(".{n}"), t.clone()))
                .collect();
        }
        let Some(info) = self.enums.get(owner) else {
            return Vec::new();
        };
        info.variants
            .iter()
            .flat_map(|(v, payloads)| {
                payloads
                    .iter()
                    .enumerate()
                    .map(move |(i, t)| (format!(".{v}.{i}"), t.clone()))
            })
            .collect()
    }
}

/// An interface's name as programs write it: `json.Json` for std's,
/// else its own name.
pub(super) fn shown_interface(key: &str) -> String {
    if key == super::json::BOUND {
        return "json.Json".to_string();
    }
    key.rsplit('.').next().unwrap_or(key).to_string()
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{diagnostics, errors, ok};

    /// A derivable `Show` with blocks for `i64` and every struct.
    const SHOW: &str = "interface Show { fun show(self): string; }\n\
        derive Show for i64 { fun show(self): string = \"{self}\"; }\n\
        derive Show for struct { fun show(self): string = \"s\"; }\n\
        fun show<T: Show>(x: T): string = x.show();\n";

    #[test]
    fn with_and_derive_ask_for_the_blocks() {
        ok(&format!(
            "{SHOW}struct P {{ a: i64 }} with Show\n\
             struct Q {{ a: i64 }}\nimpl Show for Q = derive;\n\
             fun main() {{ let s = show(P {{ a: 1 }}) + show(Q {{ a: 2 }}); }}"
        ));
    }

    #[test]
    fn eq_is_accepted_and_checked() {
        ok("struct P { a: i64 } with Eq\nfun main() { let b = P { a: 1 } == P { a: 1 }; }");
        assert_eq!(
            errors("struct P { f: fun(): i64 } with Eq\nfun main() {}"),
            [("E0451", "Eq")]
        );
    }

    #[test]
    fn an_interface_without_a_block_for_the_kind_is_e0462() {
        let src = format!("{SHOW}enum E {{ A }} with Show\nfun main() {{}}");
        assert_eq!(errors(&src), [("E0462", "Show")]);
        let help = diagnostics(&src)[0].help.clone().expect("a help");
        assert!(help.contains("`i64` and structs"), "{help}");
        assert_eq!(
            errors("struct P { a: i64 } with Display\nfun main() {}"),
            [("E0462", "Display")]
        );
    }

    #[test]
    fn asking_twice_is_e0461() {
        let src = format!("{SHOW}struct P {{ a: i64 }} with Show, Show\nfun main() {{}}");
        assert_eq!(errors(&src), [("E0461", "Show")]);
    }

    #[test]
    fn an_unknown_interface_is_e0315() {
        assert_eq!(
            errors("struct P { a: i64 } with Nope\nfun main() {}"),
            [("E0315", "Nope")]
        );
    }
}
