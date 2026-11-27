//! Lugha functions C calls by name, `extern fun
//! name(…) { … }`: what may cross (E0485), one function per C name (E0486,
//! E0306), and a shared library's shape (E0338). From Lugha such a function
//! is called as any other.

use super::{Checker, Type, errors};
use crate::ast::{FunDecl, Param, Program, TypeKind};

/// What C can pass an exported function, and take back from one.
const CAN_CROSS: &str = "numbers, `bool`, `char`, `string`, `Option<string>` (a parameter only), handles, `Option` of a handle and extern structs";

impl Checker {
    /// The parameters of exported `f` and their types; one C can't pass is
    /// E0485, and typed as an error so nothing else reports it.
    pub(super) fn export_params(&mut self, f: &FunDecl) -> (Vec<Param>, Vec<Type>) {
        let mut types = Vec::with_capacity(f.params.len());
        for p in &f.params {
            let span = crate::span::Span::new(p.name.span.start, p.ty.span.end);
            let refused = match &p.ty.kind {
                TypeKind::Named(name) if name == "data" || name == "release" => {
                    Some(format!("`{name}`"))
                }
                TypeKind::Fun(..) => Some("a callback".to_string()),
                _ if p.mode.is_some() => Some("a `mut` or `out` parameter".to_string()),
                _ => None,
            };
            if let Some(what) = refused {
                self.report(errors::export_type(&what, CAN_CROSS, span));
                types.push(Type::Error);
                continue;
            }
            let ty = self.resolve(&p.ty);
            if !self.crosses_from_c(&ty) {
                self.refuse_export(&ty, p.ty.span);
            }
            types.push(ty);
        }
        (f.params.clone(), types)
    }

    /// Exported `f`'s result `ret` (E0485), and its C name (E0306, E0486).
    pub(super) fn export_result(&mut self, f: &FunDecl, ret: &Type) {
        if let Some(written) = &f.ret
            && !(self.crosses_into_c(ret) || *ret == Type::Unit)
        {
            self.refuse_export(ret, written.span);
        }
        let Some(export) = &f.export else {
            return;
        };
        let (symbol, span) = match &export.symbol {
            Some(s) => (s.name.as_str(), s.span),
            None => (f.name.name.as_str(), f.name.span),
        };
        if !super::env::c_name(symbol) {
            self.report(errors::bad_symbol(symbol, span));
            return;
        }
        match self.exports.get(symbol) {
            Some(first) => self.report(errors::export_twice(symbol, span, *first)),
            None => {
                self.exports.insert(symbol.to_string(), span);
            }
        }
    }

    /// E0485 for `ty`, with a capability's own help.
    fn refuse_export(&mut self, ty: &Type, at: crate::span::Span) {
        if matches!(ty, Type::Error) {
            return;
        }
        let capability = matches!(ty, Type::Interface(key, _) if super::impls::is_capability(key));
        let help = if capability {
            "an exported function gets no capabilities: C has none to give"
        } else {
            CAN_CROSS
        };
        self.report(errors::export_type(&format!("`{ty}`"), help, at));
    }

    /// True for what C may pass in: a result's types, and C's text, copied.
    fn crosses_from_c(&self, ty: &Type) -> bool {
        self.crosses_into_c(ty) || matches!(ty, Type::Option(inner) if **inner == Type::String)
    }

    /// True for what an exported function may give C: numbers, `bool`,
    /// `char`, handles, `Option` of a handle, extern structs, and a
    /// `string`, which C gets as its own copy.
    fn crosses_into_c(&self, ty: &Type) -> bool {
        ty.is_numeric()
            || matches!(ty, Type::Bool | Type::Char | Type::String | Type::Handle(_))
            || matches!(ty, Type::Option(inner) if matches!(**inner, Type::Handle(_)))
            || matches!(ty, Type::Struct(key, _) if self.c_structs.contains(key))
            || matches!(ty, Type::Error)
    }

    /// The main file's entry: `main` for a program; for a shared library,
    /// no `main` and at least one export (E0338).
    pub(super) fn check_entry(&mut self, program: &Program) {
        if !self.library_mode {
            self.check_main(program);
            return;
        }
        for item in &program.items {
            if let crate::ast::Item::Fun(f) = item
                && f.name.name == "main"
                && f.export.is_none()
            {
                self.report(errors::shared_main(f.name.span));
            }
        }
        if self.exports.is_empty() {
            self.report(errors::shared_empty());
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{errors, ok};

    #[test]
    fn an_exported_function_is_called_from_lugha_as_any_other() {
        ok("extern fun add(a: i64, b: i64): i64 = a + b;\nfun main() { debug(add(1, 2)); }");
        ok(
            "extern \"words_count\" fun count(s: string): i64 { s.len }\nfun main() { debug(count(\"ab\")); }",
        );
        ok(
            "extern struct P { x: f64, y: f64 }\nextern fun len2(p: P): f64 = p.x * p.x + p.y * p.y;\nfun main() {}",
        );
        ok("extern type Db;\nextern fun same(d: Db): Option<Db> = Some(d);\nfun main() {}");
        ok("extern fun shout(s: Option<string>): string = s.unwrap_or(\"\");\nfun main() {}");
    }

    #[test]
    fn what_c_cant_pass_is_e0485() {
        for (src, at) in [
            ("extern fun f(xs: i64[]) {}", "i64[]"),
            ("extern fun f(): Option<string> = None;", "Option<string>"),
            ("extern fun f(out x: i64) {}", "x: i64"),
            (
                "extern fun f(cb: direct fun(i64)) {}",
                "cb: direct fun(i64)",
            ),
            ("extern fun f(d: data) {}", "d: data"),
            ("struct S { a: i64 }\nextern fun f(s: S) {}", "S"),
        ] {
            assert_eq!(
                errors(&format!("{src}\nfun main() {{}}")),
                [("E0485", at)],
                "{src}"
            );
        }
    }

    #[test]
    fn a_capability_is_e0485_with_its_own_help() {
        let found =
            crate::check::test_util::diagnostics("extern fun f(net: Net): i64 = 0;\nfun main() {}");
        let d = found.iter().find(|d| d.code == "E0485").expect("E0485");
        assert!(
            d.help
                .as_deref()
                .is_some_and(|h| h.contains("no capabilities"))
        );
    }

    #[test]
    fn one_function_per_c_name() {
        assert_eq!(
            errors("extern fun f(): i64 = 1;\nextern \"f\" fun g(): i64 = 2;\nfun main() {}"),
            [("E0486", "\"f\"")]
        );
        assert_eq!(
            errors("extern \"lugha_x\" fun f() {}\nfun main() {}"),
            [("E0306", "\"lugha_x\"")]
        );
    }
}
