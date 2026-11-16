//! Braces after a type name: `Point { x: 1, y: 2 }`, `Map { "a": 1 }`,
//! `Set { 1, 2 }`, `Map {}`. Which one the braces build depends on the
//! type, so the parser keeps both readings: the struct's fields, and the
//! call of `from_entries` or `from_items` the collection reading stands for.
//! A trailing comma is allowed. The old `#{ … }` is E0213.

use super::{PResult, Parser};
use crate::ast::{Collection, Expr, ExprKind, Ident, StructLiteral};
use crate::diagnostic::Diagnostic;
use crate::lexer::TokenKind;
use crate::span::Span;

/// One element between the braces.
enum Element {
    /// `key: value`, or an item when the value is `None`.
    Entry(Expr, Option<Expr>),
    /// `..base` at `Span`.
    Base(Span, Expr),
}

impl Parser<'_> {
    /// `Name { … }` (or `module.Name { … }`) at its `{`.
    pub(super) fn struct_lit(
        &mut self,
        start: usize,
        module: Option<Ident>,
        name: Ident,
    ) -> PResult<Expr> {
        let open = self.bump();
        let elements = self.comma_list(open, TokenKind::RBrace, |p| {
            // `..base`: the fields not written come from `base`.
            if p.at(&TokenKind::DotDot) {
                let at = p.bump();
                return Ok(Element::Base(at, p.expr()?));
            }
            // `Map { a: loop { … } }` is the entry `a`, not a labelled loop.
            let no_label = std::mem::replace(&mut p.no_label, true);
            let key = p.expr();
            p.no_label = no_label;
            let key = key?;
            let value = match p.eat(&TokenKind::Colon) {
                true => Some(p.expr()?),
                false => None,
            };
            Ok(Element::Entry(key, value))
        })?;
        let (mut written, mut base) = (Vec::new(), None);
        let last = elements.len().saturating_sub(1);
        for (i, element) in elements.into_iter().enumerate() {
            match element {
                Element::Entry(key, value) => written.push((key, value)),
                Element::Base(_, value) if i == last && base.is_none() => {
                    base = Some(Box::new(value))
                }
                Element::Base(at, _) => {
                    let message = "`..base` must come last in a struct literal, once";
                    let help = "write the fields first, then `..base`: `P { x: 1, ..p }`";
                    self.error(Diagnostic::error("E0201", message, at).with_help(help));
                }
            }
        }
        let pairs = written.iter().filter(|(_, v)| v.is_some()).count();
        if pairs > 0 && pairs < written.len() {
            // The first element decides; the first that differs is the error.
            let first_pair = written[0].1.is_some();
            let odd = written.iter().find(|(_, v)| v.is_some() != first_pair);
            let at = odd.map_or(name.span, |(k, _)| k.span);
            let message = match first_pair {
                true => "these braces hold `key: value` entries, not items",
                false => "these braces hold items, not `key: value` entries",
            };
            self.error(Diagnostic::error("E0201", message, at));
        }
        let fields: Vec<(Ident, Expr)> = written
            .iter()
            .filter_map(|(key, value)| match (&key.kind, value) {
                (ExprKind::Name(n), Some(v)) if key.type_args.is_none() => Some((
                    Ident {
                        name: n.clone(),
                        span: key.span,
                    },
                    v.clone(),
                )),
                _ => None,
            })
            .collect();
        let collection = match base {
            Some(_) => None,
            None => Some(Box::new(self.collection(
                start,
                (&module, &name),
                written,
                fields.len(),
            ))),
        };
        let literal = StructLiteral {
            module,
            name,
            fields,
            base,
            collection,
        };
        Ok(self.mk(start, ExprKind::StructLit(Box::new(literal))))
    }

    /// The collection reading of `elements`, written after `module.name`.
    fn collection(
        &mut self,
        start: usize,
        (module, name): (&Option<Ident>, &Ident),
        elements: Vec<(Expr, Option<Expr>)>,
        named: usize,
    ) -> Collection {
        let pairs = elements.iter().all(|(_, v)| v.is_some());
        let items = elements.iter().all(|(_, v)| v.is_none());
        let entries_call = pairs.then(|| {
            let keys = elements.iter().map(|(k, _)| k.clone()).collect();
            let values = elements.iter().filter_map(|(_, v)| v.clone()).collect();
            let keys = self.mk(start, ExprKind::Array(keys));
            let values = self.mk(start, ExprKind::Array(values));
            self.associated_call(start, (module, name), "from_entries", vec![keys, values])
        });
        let items_call = items.then(|| {
            let all = elements.iter().map(|(k, _)| k.clone()).collect();
            let all = self.mk(start, ExprKind::Array(all));
            self.associated_call(start, (module, name), "from_items", vec![all])
        });
        Collection {
            as_fields: named == elements.len(),
            elements,
            entries: entries_call,
            items: items_call,
        }
    }

    /// `module.name.function(args)`, spanning the literal.
    fn associated_call(
        &mut self,
        start: usize,
        (module, name): (&Option<Ident>, &Ident),
        function: &str,
        args: Vec<Expr>,
    ) -> Expr {
        let ty = match module {
            // `m.S { … }`, or `m.v.S { … }` through a re-export.
            Some(m) => {
                let mut parts = m.name.split('.');
                let first = parts.next().unwrap_or_default().to_string();
                let mut base = self.mk(start, ExprKind::Name(first));
                for part in parts {
                    let member = Ident {
                        name: part.to_string(),
                        span: m.span,
                    };
                    base = self.mk(start, ExprKind::Field(Box::new(base), member));
                }
                self.mk(start, ExprKind::Field(Box::new(base), name.clone()))
            }
            None => self.mk(start, ExprKind::Name(name.name.clone())),
        };
        let function = Ident {
            name: function.to_string(),
            span: name.span,
        };
        let callee = self.mk(start, ExprKind::Field(Box::new(ty), function));
        self.mk(start, ExprKind::Call(Box::new(callee), args))
    }

    /// `#{ … }`, gone: E0213, and the literal is skipped.
    #[inline(never)]
    pub(super) fn hash_literal(&mut self, start: usize) -> PResult<Expr> {
        let open = self.bump();
        let message = "`#{…}` literals are gone";
        let help = "write `Map { \"a\": 1 }` for a map or `Set { 1, 2 }` for a set";
        self.error(Diagnostic::error("E0213", message, open).with_help(help));
        let mut depth = 1usize;
        while depth > 0 && !self.at(&TokenKind::Eof) {
            match self.peek() {
                TokenKind::LBrace | TokenKind::HashBrace => depth += 1,
                TokenKind::RBrace => depth -= 1,
                _ => {}
            }
            self.bump();
        }
        Ok(self.mk(start, ExprKind::Tuple(Vec::new())))
    }
}

/// Puts written type arguments on the type names of `literal`'s collection
/// calls, `Map<string, i64> {}`'s `Map<string, i64>.from_entries([], [])`.
pub(super) fn type_args_to_calls(literal: &mut StructLiteral, args: &crate::ast::TypeArgs) {
    let Some(collection) = literal.collection.as_mut() else {
        return;
    };
    for call in [&mut collection.entries, &mut collection.items]
        .into_iter()
        .flatten()
    {
        if let ExprKind::Call(callee, _) = &mut call.kind
            && let ExprKind::Field(ty, _) = &mut callee.kind
        {
            ty.type_args = Some(Box::new(args.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::test_util::{errors, expr};

    #[test]
    fn braces_hold_fields_entries_or_items() {
        assert_eq!(expr("P { x: 1, y: 2 }"), "(struct-lit P (x 1) (y 2))");
        assert_eq!(
            expr("Map { \"a\": 1, k: 2 }"),
            "(collection Map (: \"a\" 1) (: k 2))"
        );
        assert_eq!(expr("Set { 1, 2, }"), "(collection Set 1 2)");
        assert_eq!(expr("Map {}"), "(struct-lit Map)");
        assert_eq!(
            expr("Map<string, i64> {}"),
            "(targs (struct-lit Map) string i64)"
        );
    }

    #[test]
    fn mixing_and_the_old_form_are_errors() {
        assert_eq!(
            errors("fun f() { let m = Map { \"a\": 1, 2 }; }"),
            [("E0201", "2")]
        );
        assert_eq!(errors("fun f() { let m = #{\"a\": 1}; }")[0].0, "E0213");
    }
}
