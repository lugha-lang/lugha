//! Type arguments written at a use: `max<i64>(a, b)`, `xs.map<string>(f)`,
//! `Chan<Job>.new(16)`, `Pair<u8> { … }`, `Option<i64>.None`.
//!
//! After a name, `<` begins type arguments only when a list of types
//! closed by `>` follows and the token after the `>` can end or continue a
//! use: `(`, `.`, `{`, or the end of an expression (`)`, `,`, `;`, `]`,
//! `}`). Anything else makes the `<` a comparison, as it always was.

use super::{PResult, Parser};
use crate::ast::{Expr, ExprKind, Ident, TypeArgs};
use crate::lexer::TokenKind;
use crate::span::Span;

impl Parser<'_> {
    /// Type arguments for `expr` at a `<`, or `None`, leaving the parser
    /// where it was, when the context rule reads the `<` as a comparison.
    pub(super) fn try_type_args(&mut self) -> Option<TypeArgs> {
        let (pos, reported, next_id) = (self.pos, self.diagnostics.len(), self.next_id);
        let read = self.type_arg_list();
        let follows = matches!(
            self.peek(),
            TokenKind::LParen
                | TokenKind::Dot
                | TokenKind::LBrace
                | TokenKind::RParen
                | TokenKind::Comma
                | TokenKind::Semi
                | TokenKind::RBracket
                | TokenKind::RBrace
                | TokenKind::Eof
        );
        match read {
            Ok(args) if follows && self.diagnostics.len() == reported => Some(args),
            // Not type arguments: undo everything the attempt did.
            _ => {
                self.pos = pos;
                self.diagnostics.truncate(reported);
                self.next_id = next_id;
                None
            }
        }
    }

    /// `<T, _, …>`: one or more types, `_` for one left to inference.
    fn type_arg_list(&mut self) -> PResult<TypeArgs> {
        let open = self.bump();
        let mut args = Vec::new();
        loop {
            if matches!(self.peek(), TokenKind::Ident(n) if n == "_") {
                self.bump();
                args.push(None);
            } else {
                args.push(Some(self.ty()?));
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::Gt)?;
        Ok(TypeArgs {
            args,
            span: Span::new(open.start, self.prev_end()),
        })
    }

    /// After `expr<…>` in `postfix`: the arguments attached to `expr`, and a
    /// struct literal when `{` follows a struct's name (`Pair<u8> { … }`,
    /// `m.Pair<u8> { … }`).
    // Out of `postfix`, whose frame must stay small for the E0206 limit.
    #[inline(never)]
    pub(super) fn with_type_args(
        &mut self,
        start: usize,
        mut expr: Expr,
        args: TypeArgs,
    ) -> PResult<Expr> {
        let named = match &expr.kind {
            ExprKind::Name(name) => Some((
                None,
                Ident {
                    name: name.clone(),
                    span: expr.span,
                },
            )),
            ExprKind::Field(base, member) => match &base.kind {
                ExprKind::Name(m) => Some((
                    Some(Ident {
                        name: m.clone(),
                        span: base.span,
                    }),
                    member.clone(),
                )),
                _ => None,
            },
            _ => None,
        };
        if let Some((module, name)) = named
            && let Some(mut literal) = self.struct_lit_after(start, module, name)?
        {
            literal.type_args = Some(Box::new(args));
            return Ok(literal);
        }
        expr.span = Span::new(expr.span.start, self.prev_end());
        expr.type_args = Some(Box::new(args));
        Ok(expr)
    }
}

/// True if type arguments may follow `expr`: a name or a member name
/// without any yet.
pub(super) fn takes_type_args(expr: &Expr) -> bool {
    expr.type_args.is_none() && matches!(expr.kind, ExprKind::Name(_) | ExprKind::Field(..))
}

#[cfg(test)]
mod tests {
    use crate::parser::test_util::{body, expr};

    #[test]
    fn type_arguments_follow_names_by_the_context_rule() {
        assert_eq!(expr("max<i64>(a, b)"), "(call (targs max i64) a b)");
        assert_eq!(expr("max<i64>"), "(targs max i64)");
        assert_eq!(
            expr("xs.map<string>(f)"),
            "(call (targs (. xs map) string) f)"
        );
        assert_eq!(
            expr("Chan<Job>.new(16)"),
            "(call (. (targs Chan Job) new) 16)"
        );
        assert_eq!(expr("Option<i64>.None"), "(. (targs Option i64) None)");
        assert_eq!(
            expr("f<_, Map<string, i64[]>>(x)"),
            "(call (targs f _ (applied Map string i64[])) x)"
        );
        assert!(expr("Pair<u8> { a: 1, b: 2 }").starts_with("(targs (struct-lit Pair"));
        assert!(expr("m.Pair<u8> { a: 1 }").starts_with("(targs (struct-lit m.Pair"));
        // The draft's own example: `a` takes type arguments.
        assert_eq!(expr("f(a < b, c > (d))"), "(call f (call (targs a b c) d))");
    }

    #[test]
    fn otherwise_a_less_than_is_a_comparison() {
        assert_eq!(expr("a < b"), "(< a b)");
        assert_eq!(expr("f(a < b, c > d)"), "(call f (< a b) (> c d))");
        assert_eq!(expr("i < n && j > 0"), "(&& (< i n) (> j 0))");
        assert_eq!(expr("(1 + x) < y"), "(< (+ 1 x) y)");
        assert_eq!(
            body("if a < b { f(); }"),
            "(block (if (< a b) (block (; (call f)))))"
        );
    }
}
