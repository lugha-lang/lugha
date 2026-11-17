use super::{PResult, Parser};
use crate::ast::{Bound, FunDecl, InterfaceDecl, InterfaceItem, Type, TypeParam};
use crate::lexer::TokenKind;
use crate::span::Span;

impl Parser<'_> {
    /// `<T, R = Self>`: an interface's parameters, each with its bounds and
    /// an optional default. A parameter without a default after one with
    /// is E0201: a use leaves out trailing arguments only.
    fn interface_params(&mut self) -> PResult<(Vec<TypeParam>, Vec<Option<Type>>)> {
        if !self.at(&TokenKind::Lt) {
            return Ok((Vec::new(), Vec::new()));
        }
        let open = self.bump();
        if self.at(&TokenKind::Gt) {
            return Err(self.expected("type parameter name"));
        }
        let mut defaults: Vec<Option<Type>> = Vec::new();
        let params = self.comma_list(open, TokenKind::Gt, |p| {
            let name = p.ident("type parameter name")?;
            let mut bounds = Vec::new();
            if p.eat(&TokenKind::Colon) {
                loop {
                    bounds.push(p.bound()?);
                    if !p.eat(&TokenKind::Plus) {
                        break;
                    }
                }
            }
            let default = if p.eat(&TokenKind::Eq) {
                Some(p.ty()?)
            } else if defaults.iter().any(Option::is_some) {
                return Err(p.expected("`=` and a default, as the parameters before it have"));
            } else {
                None
            };
            defaults.push(default);
            Ok(TypeParam { name, bounds })
        })?;
        Ok((params, defaults))
    }

    /// `interface Name[: I, J] { fun area(self): f64; … }`.
    pub(super) fn interface_decl(&mut self) -> PResult<InterfaceDecl> {
        let start = self.bump().start;
        let name = self.ident("interface name")?;
        let (type_params, defaults) = self.interface_params()?;
        let mut extends = Vec::new();
        if self.eat(&TokenKind::Colon) {
            extends.push(self.bound()?);
            while self.eat(&TokenKind::Comma) {
                extends.push(self.bound()?);
            }
        }
        let open = self.expect(&TokenKind::LBrace)?;
        let mut items = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            if !self.at(&TokenKind::Fun) {
                if self.at(&TokenKind::Eof) {
                    self.close(open, &TokenKind::RBrace)?;
                }
                return Err(self.expected("`fun` or `}`"));
            }
            items.push(self.interface_item()?);
        }
        self.bump();
        Ok(InterfaceDecl {
            visibility: None,
            name,
            type_params,
            defaults,
            extends,
            auto: false,
            items,
            span: Span::new(start, self.prev_end()),
        })
    }

    /// `fun name(receiver, params): ret;`, or with a default body after the
    /// signature: `= expr;` or a block, for methods only (E0201).
    fn interface_item(&mut self) -> PResult<InterfaceItem> {
        let start = self.bump().start;
        let name = self.ident("method name")?;
        let (receiver, params) = self.receiver_params(true)?;
        let ret = self.return_type()?;
        let default = if self.eat(&TokenKind::Semi) {
            None
        } else {
            let body = self.fun_body()?;
            let span = Span::new(start, self.prev_end());
            Some(FunDecl {
                visibility: None,
                name: name.clone(),
                type_params: Vec::new(),
                receiver,
                params: params.clone(),
                ret: ret.clone(),
                body,
                span,
                test: None,
                integration: false,
                caller_location: None,
            })
        };
        Ok(InterfaceItem {
            name,
            receiver,
            params,
            ret,
            default,
            span: Span::new(start, self.prev_end()),
        })
    }

    /// `Eq`, `Ord`, `Shape`, `m.Shape`, or with arguments, `Source<T>`;
    /// or a function type, `fun(A): R`, the prelude's interface for its
    /// number of parameters.
    pub(super) fn bound(&mut self) -> PResult<Bound> {
        if self.at(&TokenKind::Fun) {
            return self.fun_bound();
        }
        let first = self.ident("bound name")?;
        let (module, name) = self.qualified_name(first, "bound name")?;
        let args = if self.at(&TokenKind::Lt) {
            let open = self.bump();
            if self.at(&TokenKind::Gt) {
                return Err(self.expected("type"));
            }
            self.nested(|p| p.comma_list(open, TokenKind::Gt, |p| p.ty()))?
        } else {
            Vec::new()
        };
        let start = module.as_ref().map_or(name.span.start, |m| m.span.start);
        let span = Span::new(start, self.prev_end());
        Ok(Bound {
            module,
            name,
            args,
            span,
        })
    }
}

impl Parser<'_> {
    /// `fun(A, B): R` as a bound: `$Fun2<A, B, R>`, a missing result `()`.
    fn fun_bound(&mut self) -> PResult<Bound> {
        let t = self.ty()?;
        let crate::ast::TypeKind::Fun(params, ret) = t.kind else {
            return Err(self.expected("`fun(…)`"));
        };
        let unit = crate::ast::Type {
            kind: crate::ast::TypeKind::Tuple(Vec::new()),
            span: t.span,
        };
        let mut args = params;
        args.push(ret.map_or(unit, |r| *r));
        Ok(Bound {
            module: None,
            name: crate::ast::Ident {
                name: format!("$Fun{}", args.len() - 1),
                span: t.span,
            },
            args,
            span: t.span,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::test_util::{parse_src, program};

    #[test]
    fn interfaces_list_signatures_without_bodies() {
        assert_eq!(
            program(
                "interface Shape { fun area(self): f64; fun grow(mut self, by: f64); fun make(r: f64): Circle; }"
            ),
            "(interface Shape (fun area (self) f64) (fun grow ((mut self) (by f64)) void) (fun make ((r f64)) Circle))"
        );
        assert_eq!(
            program("private interface Empty {}"),
            "(private interface Empty)"
        );
    }

    #[test]
    fn bounds_name_interfaces_here_or_in_a_module() {
        assert_eq!(
            program("fun f<T: Shape + Eq, U: shapes.Shape>(x: T) {}"),
            "(fun f (type-params (T Shape Eq) (U shapes.Shape)) ((x T)) void (block))"
        );
    }

    #[test]
    fn methods_may_have_default_bodies() {
        assert_eq!(
            program(
                "interface Shape { fun area(self): f64; fun name(self): string = \"s\"; fun big(self): bool { self.area() > 1.0 } }"
            ),
            "(interface Shape (fun area (self) f64) (fun name (self) string (block \"s\")) (fun big (self) bool (block (> (call (. self area)) 1.0))))"
        );
    }

    #[test]
    fn interfaces_take_type_parameters_and_bounds_arguments() {
        assert_eq!(
            program("interface Source<T: Eq> { fun next(mut self): Option<T>; }"),
            "(interface Source (type-params (T Eq)) (fun next ((mut self)) (Option T)))"
        );
        assert_eq!(
            program("fun f<T, S: Source<T> + geo.Sink<T[]>>(s: S) {}"),
            "(fun f (type-params T (S (Source T) (geo.Sink T[]))) ((s S)) void (block))"
        );
    }

    #[test]
    fn associated_functions_may_have_defaults() {
        let p = parse_src("interface Make { fun make(): i64 = 1; }").expect("parses");
        let crate::ast::Item::Interface(i) = &p.items[0] else {
            panic!("an interface")
        };
        assert!(i.items[0].default.is_some());
    }
}
