//! Declarations: `enum`, `impl`, and parameter lists that may start with a
//! `self` receiver.

use super::{PResult, Parser};
use crate::ast::{
    Block, Bound, EnumDecl, FunDecl, Ident, ImplDecl, Param, Receiver, Type, TypeParam, Variant,
};
use crate::diagnostic::Diagnostic;
use crate::lexer::TokenKind;
use crate::span::Span;

impl Parser<'_> {
    /// `enum Name { Variant, Variant(T, …), … }`.
    pub(super) fn enum_decl(&mut self) -> PResult<EnumDecl> {
        let start = self.bump().start;
        let name = self.ident("enum name")?;
        let type_params = self.type_params()?;
        let open = self.expect(&TokenKind::LBrace)?;
        // `Self` in a payload is this enum, as in its `impl`.
        let params: Vec<Ident> = type_params.iter().map(|p| p.name.clone()).collect();
        let variants = self.in_impl(&name, &params, |this| {
            this.comma_list(open, TokenKind::RBrace, |p| {
                let name = p.ident("variant name")?;
                let payloads = if p.at(&TokenKind::LParen) {
                    let open = p.bump();
                    p.comma_list(open, TokenKind::RParen, |p| p.ty())?
                } else {
                    Vec::new()
                };
                let span = Span::new(name.span.start, p.prev_end());
                Ok(Variant {
                    name,
                    payloads,
                    span,
                })
            })
        })?;
        let span = Span::new(start, self.prev_end());
        let derives = self.with_list()?;
        Ok(EnumDecl {
            visibility: None,
            name,
            type_params,
            variants,
            span,
            derives,
        })
    }

    /// `impl [m.]Type[<T, …>] { [pub] fun … }`, or `impl [m.]I[<A, …>] for
    /// [m.]Type[<T, …>] { … }` for an interface's items.
    pub(super) fn impl_decl(&mut self) -> PResult<ImplDecl> {
        let start = self.bump().start;
        // `impl<T: B> I for T[]` and `impl<T> T[]`: the block's parameters,
        // for a built-in type.
        let generic = self.at(&TokenKind::Lt);
        let outer = self.impl_params()?;
        let interface = if self.for_ahead() {
            let bound = self.bound()?;
            self.expect(&TokenKind::For)?;
            Some(Box::new(bound))
        } else {
            None
        };
        // The old `impl Grid<T: Num>`: E0214, read as the block it meant.
        if !generic && self.bounded_header_ahead() {
            let first = self.ident("type name")?;
            let (module, ty) = self.qualified_name(first, "type name")?;
            let params = self.type_params()?;
            let shown: Vec<String> = params.iter().map(|p| self.param_text(p)).collect();
            let names: Vec<&str> = params.iter().map(|p| p.name.name.as_str()).collect();
            let at = Span::new(ty.span.start, self.prev_end());
            self.error(super::impl_forms::long_form_error(
                &ty.name,
                &shown.join(", "),
                &names.join(", "),
                at,
            ));
            return self.impl_rest(start, interface, (module, ty, params, None));
        }
        let (module, ty, params, target) = if generic || !self.bare_name_ahead() {
            // Any other type: `Grid<T>`, `Point[]`, `(A, B)`, `Map<string, V>`;
            // a program type's `impl<T> Grid<T>` becomes a struct's block once
            // the file is parsed (`parser::impl_forms`).
            let t = self.ty()?;
            let name = Ident {
                name: t.written(),
                span: t.span,
            };
            (None, name, outer, Some(Box::new(t)))
        } else {
            // `impl f64 { … }`: arithmetic with a number on the left.
            let number = number_type(self.peek());
            let first = match number {
                Some(name) => Ident {
                    name: name.to_string(),
                    span: self.bump(),
                },
                None => self.ident("type name")?,
            };
            let (module, ty) = if self.eat(&TokenKind::Dot) {
                (Some(first), self.ident("type name")?)
            } else {
                (None, first)
            };
            (module, ty, Vec::new(), None)
        };
        self.impl_rest(start, interface, (module, ty, params, target))
    }

    /// An `impl` after its header: `= derive;` or its methods.
    fn impl_rest(
        &mut self,
        start: usize,
        interface: Option<Box<Bound>>,
        (module, ty, params, target): ImplHead,
    ) -> PResult<ImplDecl> {
        // `impl I for T = derive;`: the items come from `I`'s derive blocks.
        let derive = interface.is_some() && self.eat(&TokenKind::Eq);
        let mut methods = Vec::new();
        if derive {
            if !matches!(self.peek(), TokenKind::Ident(w) if w == "derive") {
                return Err(self.expected("`derive`"));
            }
            self.bump();
            self.expect(&TokenKind::Semi)?;
        } else {
            let open = self.expect(&TokenKind::LBrace)?;
            // `Self` means this type throughout the block.
            let names: Vec<Ident> = params.iter().map(|p| p.name.clone()).collect();
            methods = match &target {
                // `impl<T> Grid<T>`, a program type's block: `Self` is also
                // the type in expressions (`Self { … }`, `Self.Variant`).
                Some(t) if super::impl_forms::own_header(t, &params).is_some() => {
                    let ty =
                        super::impl_forms::own_header(t, &params).expect("checked by the guard");
                    self.in_impl(&ty, &names, |p| p.impl_methods(open))?
                }
                Some(t) => {
                    let outer = self.impl_target.replace((**t).clone());
                    let methods = self.impl_methods(open);
                    self.impl_target = outer;
                    methods?
                }
                None => self.in_impl(&ty, &names, |p| p.impl_methods(open))?,
            };
        }
        // An interface's methods are named apart from the type's own:
        // `area@Shape`, the interface as written, its `.`s as `/`s (a
        // method's key is split at its last `.`). No program's name has `@`.
        let tag: Box<str> = match &interface {
            Some(b) => b.written().replace('.', "/").into(),
            None => Box::default(),
        };
        if !tag.is_empty() {
            for m in &mut methods {
                m.name.name = format!("{}@{tag}", m.name.name);
            }
        }
        Ok(ImplDecl {
            module,
            ty,
            params,
            methods,
            span: Span::new(start, self.prev_end()),
            actor: false,
            interface,
            tag,
            derive,
            target,
        })
    }

    /// `with I, J` after a struct's or enum's closing brace; empty without
    /// one. `with` is a word only here.
    pub(super) fn with_list(&mut self) -> PResult<Vec<Bound>> {
        if !matches!(self.peek(), TokenKind::Ident(w) if w == "with") {
            return Ok(Vec::new());
        }
        self.bump();
        let mut bounds = vec![self.bound()?];
        while self.eat(&TokenKind::Comma) {
            bounds.push(self.bound()?);
        }
        Ok(bounds)
    }

    /// True if the `impl` header ahead names an interface: a `for` comes
    /// before the block's `{`.
    fn for_ahead(&self) -> bool {
        (0..)
            .map(|n| self.peek_at(n))
            .take_while(|t| !matches!(t, TokenKind::LBrace | TokenKind::Eof | TokenKind::Semi))
            .any(|t| *t == TokenKind::For)
    }

    /// True if a bare name comes next, `Name` or `m.Name`, then the block's
    /// `{` or `= derive`: a struct's, enum's, actor's or extern type's
    /// block. Anything else is read as a type.
    fn bare_name_ahead(&self) -> bool {
        let mut n = 0;
        // A number's block (`impl i64 { … }`, the prelude's) is read as a type.
        if !matches!(self.peek_at(n), TokenKind::Ident(_)) {
            return false;
        }
        n += 1;
        while *self.peek_at(n) == TokenKind::Dot
            && matches!(self.peek_at(n + 1), TokenKind::Ident(_))
        {
            n += 2;
        }
        matches!(self.peek_at(n), TokenKind::LBrace | TokenKind::Eq)
    }

    /// The methods of an `impl` up to and including its `}`.
    pub(super) fn impl_methods(&mut self, open: Span) -> PResult<Vec<FunDecl>> {
        let mut methods = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            // `@caller_location` before a method (`check` rejects it where
            // no call site is at hand).
            let located = self.method_attributes()?;
            let visibility = self.visibility();
            if !self.at(&TokenKind::Fun) {
                if self.at(&TokenKind::Eof) {
                    self.close(open, &TokenKind::RBrace)?;
                }
                return Err(self.expected("`fun` or `}`"));
            }
            let mut method = self.fun_decl_with(true)?;
            method.visibility = visibility;
            method.caller_location = located;
            methods.push(method);
        }
        self.bump();
        Ok(methods)
    }

    /// `fun name(params): ret body`; in an `impl`, the parameters may start
    /// with `self` or `mut self`.
    pub(super) fn fun_decl_with(&mut self, in_impl: bool) -> PResult<FunDecl> {
        let start = self.bump().start;
        let name = self.ident("function name")?;
        let type_params = self.type_params()?;
        let (receiver, params) = self.receiver_params(in_impl)?;
        let ret = self.return_type()?;
        let body = self.fun_body()?;
        Ok(FunDecl {
            visibility: None,
            name,
            type_params,
            receiver,
            params,
            ret,
            body,
            span: Span::new(start, self.prev_end()),
            test: None,
            integration: false,
            caller_location: None,
            export: None,
        })
    }

    /// A function's body: a block, or `= e;`, which means exactly `{ e }`.
    pub(super) fn fun_body(&mut self) -> PResult<Block> {
        if !self.eat(&TokenKind::Eq) {
            return self.block();
        }
        let tail = self.expr()?;
        self.expect(&TokenKind::Semi)?;
        Ok(Block {
            stmts: Vec::new(),
            span: tail.span,
            tail: Some(Box::new(tail)),
        })
    }

    /// `( [receiver ,] params )`. Outside an `impl` a receiver is E0201, and
    /// parsing continues without it.
    pub(super) fn receiver_params(
        &mut self,
        in_impl: bool,
    ) -> PResult<(Option<Receiver>, Vec<Param>)> {
        let open = self.expect(&TokenKind::LParen)?;
        let starts_receiver = self.at(&TokenKind::SelfKw)
            || (self.at(&TokenKind::Mut) && self.peek_at(1) == &TokenKind::SelfKw);
        if !starts_receiver {
            let params = self.comma_list(open, TokenKind::RParen, |p| p.fun_param())?;
            return Ok((None, params));
        }
        let start = self.span().start;
        let mutable = self.eat(&TokenKind::Mut);
        self.bump();
        let receiver = Receiver {
            mutable,
            span: Span::new(start, self.prev_end()),
        };
        if !in_impl {
            let diagnostic =
                Diagnostic::error("E0201", "`self` is only allowed in methods", receiver.span)
                    .with_help("put this function in an `impl` block, or remove `self`");
            self.error(diagnostic);
        }
        let params = if self.eat(&TokenKind::Comma) {
            self.comma_list(open, TokenKind::RParen, |p| p.fun_param())?
        } else {
            self.close(open, &TokenKind::RParen)?;
            Vec::new()
        };
        Ok((in_impl.then_some(receiver), params))
    }
}

/// The number type a keyword names, for an `impl` of one.
/// An `impl` block's header: the module and name of the type (for a
/// struct's block), its parameters, and the type as written otherwise.
type ImplHead = (Option<Ident>, Ident, Vec<TypeParam>, Option<Box<Type>>);

fn number_type(kind: &TokenKind) -> Option<&'static str> {
    Some(match kind {
        TokenKind::TyI8 => "i8",
        TokenKind::TyI16 => "i16",
        TokenKind::TyI32 => "i32",
        TokenKind::TyI64 => "i64",
        TokenKind::TyU8 => "u8",
        TokenKind::TyU16 => "u16",
        TokenKind::TyU32 => "u32",
        TokenKind::TyU64 => "u64",
        TokenKind::TyF32 => "f32",
        TokenKind::TyF64 => "f64",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ast::{ImplDecl, Item};
    use crate::parser::test_util::{errors, parse_src, program};

    /// The one `impl` block of `src`.
    fn block(src: &str) -> ImplDecl {
        let found = parse_src(src).unwrap_or_else(|d| panic!("{d:?}")).items;
        match found.as_slice() {
            [Item::Impl(i)] => i.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_interface_block_is_for_a_name_or_for_any_type() {
        // A struct's block: its name, parameters after it.
        for src in [
            "impl Shape for Circle { }",
            "impl Shape for geo.Circle { }",
            "impl<T: Ord, U> Source<T> for Stack<T, U> { }",
            "impl Json for Point = derive;",
        ] {
            let i = block(src);
            assert!(i.target.is_none(), "{src}");
        }
        assert_eq!(
            block("impl<T: Ord, U> Source<T> for Stack<T, U> { }")
                .params
                .len(),
            2
        );
        // Any other type is read whole; `impl<…>` gives the parameters.
        for (src, written, params) in [
            ("impl Show for f64 { }", "f64", 0),
            ("impl Show for string { }", "string", 0),
            ("impl Show for Point[] { }", "Point[]", 0),
            ("impl Show for (Point, i64) { }", "(Point,i64)", 0),
            (
                "impl Show for Map<string, Point> { }",
                "Map<string,Point>",
                0,
            ),
            ("impl<T: Show> Show for T[] { }", "T[]", 1),
            ("impl<A, B> Show for (A, B) { }", "(A,B)", 2),
            ("impl<T: Show> Show for Option<T> { }", "Option<T>", 1),
        ] {
            let i = block(src);
            let target = i.target.unwrap_or_else(|| panic!("{src}"));
            assert_eq!(target.written(), written, "{src}");
            assert_eq!(i.params.len(), params, "{src}");
            assert_eq!(i.ty.name, written, "{src}");
        }
        // Without an interface: a built-in type's own methods.
        for (src, written) in [
            ("impl<T> T[] { fun first(self): T = self[0]; }", "T[]"),
            ("impl string { fun shout(self): string = self; }", "string"),
            ("impl<T: Ord> T[] { }", "T[]"),
        ] {
            let i = block(src);
            assert!(i.interface.is_none(), "{src}");
            assert_eq!(
                i.target.map(|t| t.written()).as_deref(),
                Some(written),
                "{src}"
            );
        }
        let i = block("impl<T: Show> Show for T[] { fun show(self): string = \"\"; }");
        assert_eq!(i.methods[0].name.name, "show@Show");
    }

    #[test]
    fn enums_have_positional_payloads() {
        assert_eq!(
            program("enum Shape { Circle(f64), Rect(f64, f64,), Empty, }"),
            "(enum Shape (Circle f64) (Rect f64 f64) Empty)"
        );
        assert_eq!(program("enum E {}"), "(enum E)");
        assert_eq!(
            program("enum List { Nil, Cons(i64, List) }"),
            "(enum List Nil (Cons i64 List))"
        );
    }

    #[test]
    fn impl_blocks_hold_methods_with_receivers() {
        let src = "impl Point {\n\
                   fun origin(): Point = Point { x: 0.0 };\n\
                   fun dist(self, q: Point): f64 = q.x;\n\
                   fun scale(mut self, k: f64) { self.x *= k; }\n\
                   }";
        assert_eq!(
            program(src),
            "(impl Point \
             (fun origin () Point (block (struct-lit Point (x 0.0)))) \
             (fun dist (self (q Point)) f64 (block (. q x))) \
             (fun scale ((mut self) (k f64)) void (block (*= (. self x) k))))"
        );
    }

    #[test]
    fn self_outside_impl_is_e0201() {
        assert_eq!(errors("fun f(self) {}"), [("E0201", "self")]);
        assert_eq!(
            errors("fun f(mut self, x: i64) {}"),
            [("E0201", "mut self")]
        );
        assert_eq!(errors("impl P { let x = 1; }"), [("E0201", "let")]);
    }

    #[test]
    fn option_types_and_self_expressions() {
        assert_eq!(
            program("fun f(a: Option<Option<i64>>, b: Option<i64>[]): Option<P> = None;"),
            "(fun f ((a (Option (Option i64))) (b (Option i64)[])) (Option P) (block None))"
        );
        assert_eq!(
            program("impl P { fun me(self): P = self; }"),
            "(impl P (fun me (self) P (block self)))"
        );
    }

    #[test]
    fn rust_paths_are_e0207_with_help_and_read_as_dots() {
        use crate::parser::test_util::{expr, parse_src};
        let diags = parse_src("fun t() { Shape::Circle(1.0) }").expect_err("E0207");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, "E0207");
        assert_eq!(diags[0].help.as_deref(), Some("write `Shape.Circle`"));
        assert_eq!(expr("Shape.Circle(1.0)"), "(call (. Shape Circle) 1.0)");
    }

    #[test]
    fn top_level_recovery_stops_at_enum_and_impl() {
        assert_eq!(
            errors("let x = 1;\nenum E { A B }\nimpl P { fun f(self) {} }"),
            [("E0201", "let"), ("E0201", "B")]
        );
    }
}
