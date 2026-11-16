//! Braces after a type name read as a collection: `Map { "a": 1 }`,
//! `Set { 1, 2 }`, `Bag { "apple" }`. A type that implements the prelude's
//! `FromEntries` takes `key: value` entries and one that implements
//! `FromItems` takes items; the literal is the call of its `from_entries`
//! or `from_items`. In the module that declares the type, and for every
//! other struct, the braces are the struct's fields.

use super::expr::Expect;
use super::{Checker, Type};
use crate::ast::{Collection, Expr, ExprKind, StructLiteral};
use crate::diagnostic::Diagnostic;

/// The prelude interfaces behind the literal forms.
pub(super) const FROM_ENTRIES: &str = "$prelude.FromEntries";
pub(super) const FROM_ITEMS: &str = "$prelude.FromItems";

impl Checker {
    /// The collection reading of `lit` at `expr` when it applies: its type,
    /// with the call recorded for codegen (`Checked::collections`). `None` when the braces are the
    /// struct's fields (or name no struct), for `struct_literal`.
    pub(super) fn collection_literal(
        &mut self,
        (expr, lit, key): (&Expr, &StructLiteral, &str),
        expect: Option<&Expect>,
    ) -> Option<Type> {
        let collection = lit.collection.as_deref()?;
        let owns = |interface: &str| {
            self.impls
                .get(key)
                .is_some_and(|blocks| blocks.iter().any(|b| b.key == interface))
        };
        let (entries, items) = (owns(FROM_ENTRIES), owns(FROM_ITEMS));
        let module = key.rsplit_once('.').map_or("", |(m, _)| m);
        let own_module = module == self.module.join(".");
        if (!entries && !items) || own_module {
            if collection.as_fields {
                return None;
            }
            let why = match own_module && (entries || items) {
                true => "in its own module a type's braces are its fields",
                false => "a struct's braces hold `field: value`",
            };
            // The fields first, the likelier fix; then the interfaces.
            let name = &lit.name.name;
            let fields: Vec<String> = self.structs[key]
                .fields
                .iter()
                .filter(|(f, _)| !f.starts_with('$'))
                .map(|(f, _)| format!("{f}: …"))
                .collect();
            let written = match fields.is_empty() {
                true => format!("{name} {{}}"),
                false => format!("{name} {{ {} }}", fields.join(", ")),
            };
            let help = match own_module && (entries || items) {
                true => format!(
                    "write its fields, `{written}`, or call `{name}.from_entries` or `{name}.from_items`"
                ),
                false => format!(
                    "write its fields, `{written}`; to build it from entries or items, implement `FromEntries` or `FromItems` for `{name}`"
                ),
            };
            return Some(self.wrong_form(lit, collection, (why, &help)));
        }
        let call = match (&collection.entries, &collection.items) {
            (Some(call), _) if entries => call,
            (_, Some(call)) if items => call,
            _ => {
                let (why, help) = match entries {
                    true => (
                        "it is built from `key: value` entries",
                        "write `key: value`",
                    ),
                    false => ("it is built from items", "write the items alone"),
                };
                return Some(self.wrong_form(lit, collection, (why, help)));
            }
        };
        // Only the prelude's map and set promise each key once.
        if key == super::maps::MAP || key == super::maps::SET {
            self.duplicate_elements(collection, key == super::maps::SET);
        }
        // `Map {}` with nothing to give its type arguments.
        let typed = expr.type_args.is_some()
            || expect.is_some_and(|e| matches!(&e.ty, Type::Struct(k, _) if k == key))
            || expect.is_some_and(|e| e.ty == Type::Error);
        if collection.elements.is_empty() && !typed {
            let name = &lit.name.name;
            let message = format!("cannot infer the type arguments of `{name} {{}}`");
            let help = format!(
                "write them, `{name}<string, i64> {{}}`, or annotate the variable: `let m: {name}<string, i64> = {name} {{}};`"
            );
            self.report(Diagnostic::error("E0412", message, expr.span).with_help(help));
            return Some(Type::Error);
        }
        let reported = self.diagnostics.len();
        let ty = self.expr(call, expect);
        self.string_key_help(collection, reported);
        // A written type was checked where it was written, so the call's own
        // `Key` requirement on the same key would only repeat it.
        if expect.is_some() {
            let span = call.span;
            let mut i = reported;
            while i < self.diagnostics.len() {
                let d = &self.diagnostics[i];
                let key_miss = matches!(d.code, "E0422" | "E0451")
                    && (d.message.contains("can't be a key") || d.message.contains("`Key`"));
                if d.span == span && key_miss {
                    self.diagnostics.remove(i);
                } else {
                    i += 1;
                }
            }
        }
        self.collections.insert(call.id);
        Some(ty)
    }

    /// E0470: the braces hold the wrong kind of element for `lit`'s type.
    fn wrong_form(
        &mut self,
        lit: &StructLiteral,
        collection: &Collection,
        (why, help): (&str, &str),
    ) -> Type {
        let message = format!("these braces can't build `{}`: {why}", lit.name.name);
        let at = collection
            .elements
            .iter()
            .find(|(key, value)| value.is_none() || !matches!(key.kind, ExprKind::Name(_)))
            .map_or(lit.name.span, |(key, _)| key.span);
        self.report(Diagnostic::error("E0470", message, at).with_help(help));
        for (key, value) in &collection.elements {
            self.value(key, None);
            value.iter().for_each(|v| _ = self.value(v, None));
        }
        Type::Error
    }

    /// The help for a bare key name nothing defines: the string it meant.
    fn string_key_help(&mut self, collection: &Collection, from: usize) {
        for (key, value) in &collection.elements {
            let ExprKind::Name(name) = &key.kind else {
                continue;
            };
            if value.is_none() {
                continue;
            }
            for d in &mut self.diagnostics[from..] {
                if d.code == "E0301" && d.span == key.span && d.help.is_none() {
                    d.help = Some(format!("for the string, write `\"{name}\"`"));
                }
            }
        }
    }

    /// W0105 for a literal key (or item) equal to an earlier one.
    fn duplicate_elements(&mut self, collection: &Collection, items: bool) {
        let (message, help) = match items {
            true => (
                "this value is already in the set",
                "a set holds it once; remove one of them",
            ),
            false => (
                "this key is already in the map",
                "the later entry wins; remove one of them",
            ),
        };
        // Literal keys, compared as written: numbers, strings, chars, bools.
        let literal = |e: &Expr| match &e.kind {
            ExprKind::Int(n) => Some(format!("i{n}")),
            ExprKind::Str(t) => Some(format!("s{t}")),
            ExprKind::Char(c) => Some(format!("c{c}")),
            ExprKind::Bool(b) => Some(format!("b{b}")),
            _ => None,
        };
        let mut seen: Vec<String> = Vec::new();
        for (key, _) in &collection.elements {
            let Some(l) = literal(key) else { continue };
            if seen.contains(&l) {
                let d = Diagnostic::warning("W0105", message, key.span).with_help(help);
                self.report(d);
            } else {
                seen.push(l);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type, ok, warnings};

    const BAG: &str = "struct Bag { n: i64 }\n\
        impl FromItems<string> for Bag { fun from_items(items: string[]): Bag = Bag { n: items.len }; }\n";

    #[test]
    fn the_type_decides_the_reading() {
        let map = |k: Type, v: Type| Type::Struct("$prelude.Map".into(), vec![k, v]);
        let src = "fun main() { let m = Map { \"a\": 1 }; }";
        assert_eq!(let_type(src, "m"), map(Type::String, Type::I64));
        let src = "fun main() { let m = Map<u8, bool> {}; }";
        assert_eq!(let_type(src, "m"), map(Type::U8, Type::Bool));
        let set = Type::Struct("$prelude.Set".into(), vec![Type::Char]);
        assert_eq!(
            let_type("fun main() { let s: Set<char> = Set {}; }", "s"),
            set
        );
        // In its own module a collection type's braces are its fields.
        ok(&format!("{BAG}fun main() {{ let b = Bag {{ n: 2 }}; }}"));
        assert_eq!(
            errors(&format!("{BAG}fun main() {{ let b = Bag {{ \"a\" }}; }}")),
            [("E0470", "\"a\"")]
        );
    }

    #[test]
    fn wrong_forms_and_unknown_keys() {
        assert_eq!(
            errors("fun main() { let s = Set { \"a\": 1 }; }")[0].0,
            "E0470"
        );
        assert_eq!(errors("fun main() { let m = Map { 1, 2 }; }")[0].0, "E0470");
        assert_eq!(
            errors("fun main() { let m = Map {}; }"),
            [("E0412", "Map {}")]
        );
        let d = crate::check::test_util::diagnostics("fun main() { let m = Map { ann: 1 }; }");
        assert_eq!(
            d[0].help.as_deref(),
            Some("for the string, write `\"ann\"`")
        );
    }

    #[test]
    fn only_maps_and_sets_warn_of_repeats() {
        assert_eq!(
            warnings("fun main() { let s = Set { 1, 1 }; }"),
            [("W0105", "1")]
        );
        let bag = format!("{BAG}fun main() {{ let b = Bag.from_items([\"a\", \"a\"]); }}");
        assert_eq!(warnings(&bag), Vec::<(&str, &str)>::new());
    }
}
