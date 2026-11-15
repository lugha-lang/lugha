//! Maps and sets in the checker: `#{ … }`
//! literals as calls of the prelude's `$map_of`/`$set_of`, keys without
//! floats (E0441), and equal literal keys (W0105).

use super::expr::Expect;
use super::{Checker, Type};
use crate::ast::{AssignOp, Expr, ExprKind, HashKind, HashLiteral, Ident};
use crate::diagnostic::Diagnostic;

/// The prelude's map and set types.
pub(super) const MAP: &str = "$prelude.Map";
pub(super) const SET: &str = "$prelude.Set";

/// The pseudo-callee an `==` records with its operand type, so each instance
/// lists the `same` methods of the maps and sets it compares (`same_instances`).
pub(super) const EQUAL: &str = "$equal";

/// The key type of a map or set type: `K` of `Map<K, V>`, `T` of `Set<T>`.
pub(super) fn key_of(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Struct(k, args) if k == MAP || k == SET => args.first(),
        _ => None,
    }
}

/// True for a diagnostic saying a key type isn't `Key`.
fn is_key_miss(d: &Diagnostic) -> bool {
    matches!(d.code, "E0422" | "E0451")
        && (d.message.contains("can't be a key") || d.message.contains("`Key`"))
}

impl Checker {
    /// `#{ … }`: a call of `$map_of(keys, values)` or `$set_of(keys)` with
    /// this expression's id; `#{}` takes which from `expect` (E0412 without).
    pub(super) fn hashed(
        &mut self,
        expr: &Expr,
        lit: &HashLiteral,
        expect: Option<&Expect>,
    ) -> Type {
        let expected = expect.map(|e| &e.ty);
        let kind = match (lit.kind, expected) {
            (HashKind::Empty, Some(Type::Struct(k, _))) if k == MAP => HashKind::Map,
            (HashKind::Empty, Some(Type::Struct(k, _))) if k == SET => HashKind::Set,
            (HashKind::Empty, Some(Type::Error)) => return Type::Error,
            (HashKind::Empty, _) => {
                let d = Diagnostic::error("E0412", "cannot infer the type of `#{}`", expr.span)
                    .with_help(
                        "annotate it: `let m: Map<string, i64> = #{};` or `let s: Set<i64> = #{};`",
                    );
                self.report(d);
                return Type::Error;
            }
            (kind, _) => kind,
        };
        self.duplicate_keys(lit);
        let reported = self.diagnostics.len();
        let ty = match kind {
            HashKind::Set => {
                let key = super::qualify(&["$prelude".to_string()], "$set_of");
                self.call_key(expr, &key, std::slice::from_ref(&lit.keys), expect)
            }
            _ => {
                let key = super::qualify(&["$prelude".to_string()], "$map_of");
                let args = [lit.keys.clone(), lit.values.clone()];
                self.call_key(expr, &key, &args, expect)
            }
        };
        // A written type was checked where it was written, so the literal's
        // own `Key` requirement on the same key would only repeat it; an
        // inferred one is checked here.
        if expect.is_none() {
            self.key_types(&ty, expr.span);
        } else {
            let mut i = reported;
            while i < self.diagnostics.len() {
                if self.diagnostics[i].span == expr.span && is_key_miss(&self.diagnostics[i]) {
                    self.diagnostics.remove(i);
                } else {
                    i += 1;
                }
            }
        }
        ty
    }

    /// `m[k] = v` on a map: its `insert` method, with `m` as a `mut
    /// self` receiver; true when `place` was such an entry and is checked.
    /// `a[i] = v` on a type with `set_index` is checked here too, as the
    /// same kind of call. A compound operator (`m[k] += v`) is left to the
    /// general path: it reads and writes the entry in place.
    pub(super) fn map_assign(
        &mut self,
        op: AssignOp,
        place: &Expr,
        value: &Expr,
        mark: usize,
    ) -> bool {
        let ExprKind::Index(base, _, key) = &place.kind else {
            return false;
        };
        if op != AssignOp::Assign {
            return false;
        }
        // The base's type decides; a probe whose findings the normal path repeats.
        let reported = self.diagnostics.len();
        let ty = self.value(base, None);
        // `a[i] = v` on a struct, enum, interface value or `T`: `set_index`.
        if super::indexing::by_method(&ty) {
            self.index_assign((place, base, &ty), (key, value), mark);
            return true;
        }
        if !matches!(&ty, Type::Struct(k, _) if k == MAP) {
            self.diagnostics.truncate(reported);
            return false;
        }
        let insert = Ident {
            name: "insert".to_string(),
            span: place.span,
        };
        let args = [(**key).clone(), value.clone()];
        self.on_value(place, (base, &ty), &insert, &args, (mark, None));
        true
    }

    /// Records that `at` compares values of `ty` with `==`: anything but a
    /// number, `bool`, `char` or `string` compares through `Eq`:
    /// its type's own `eq` or its derive block's.
    pub(super) fn equality_used(&mut self, at: &Expr, ty: &Type) {
        if !ty.is_numeric() && !matches!(ty, Type::Bool | Type::String | Type::Char | Type::Error) {
            self.used(at, EQUAL, std::slice::from_ref(ty));
        }
    }

    /// W0105 for a literal key equal to an earlier one in the same literal.
    fn duplicate_keys(&mut self, lit: &HashLiteral) {
        let (message, help) = match lit.kind {
            HashKind::Set => (
                "this value is already in the set",
                "a set holds it once; remove one of them",
            ),
            _ => (
                "this key is already in the map",
                "the later entry wins; remove one of them",
            ),
        };
        let ExprKind::Array(keys) = &lit.keys.kind else {
            return;
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
        for k in keys {
            let Some(l) = literal(k) else { continue };
            if seen.contains(&l) {
                let d = Diagnostic::warning("W0105", message, k.span).with_help(help);
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
    use crate::check::test_util::{diagnostics_or_warnings, errors, let_type, ok};

    #[test]
    fn literals_take_their_types_from_entries_or_context() {
        let ty = let_type("fun main() { let m = #{\"a\": 1}; }", "m");
        assert_eq!(ty.to_string(), "$prelude.Map<string, i64>");
        let ty = let_type("fun main() { let s = #{'a', 'b'}; }", "s");
        assert_eq!(ty.to_string(), "$prelude.Set<char>");
        let ty = let_type("fun main() { let m: Map<u8, bool> = #{}; }", "m");
        assert!(matches!(ty, Type::Struct(k, _) if k == "$prelude.Map"));
        assert_eq!(errors("fun main() { let e = #{}; }"), [("E0412", "#{}")]);
        assert_eq!(
            errors("fun main() { let m = #{\"a\": 1, \"b\": 2.5}; }")[0].0,
            "E0401"
        );
    }

    #[test]
    fn keys_are_key_and_entries_change_by_assignment() {
        // Floats are `Eq` but not `Key`.
        assert_eq!(errors("fun f(s: Set<f64>) {}\nfun main() {}")[0].0, "E0451");
        assert_eq!(errors("fun main() { let m = #{1.5: 1}; }")[0].0, "E0451");
        assert_eq!(
            errors("fun main() { let m = #{\"a\": 1}; m[\"a\"] = 2; }"),
            [("E0501", "m")]
        );
        ok("fun main() { let mut m = #{\"a\": 1}; m[\"a\"] += 2; }");
        ok("fun main() { let mut m = #{\"a\": 1}; m[\"a\"] = m[\"a\"] + 2; }");
    }

    #[test]
    fn equal_literal_keys_are_w0105() {
        let found = diagnostics_or_warnings(
            "fun main() { let m = #{\"a\": 1, \"a\": 2}; let s = #{1, 1}; }",
        );
        let codes: Vec<&str> = found.iter().map(|d| d.code).collect();
        assert_eq!(codes, ["W0105", "W0105"], "{found:?}");
        assert!(found[0].message.contains("map") && found[1].message.contains("set"));
    }

    #[test]
    fn diagnostics_name_prelude_types_as_written() {
        let found = crate::check::test_util::diagnostics(
            "fun main() { let m: Map<string, i64> = #{\"a\": 1}; let n: i64 = m; }",
        );
        assert!(
            found[0].message.contains("found Map<string, i64>"),
            "{found:?}"
        );
        assert!(!format!("{found:?}").contains("$prelude."), "{found:?}");
    }

    #[test]
    fn a_modules_own_map_hides_the_preludes() {
        ok("struct Map { n: i64 }\nfun main() { let m: Map = Map { n: 1 }; let p = #{1: 2}; }");
    }
}
