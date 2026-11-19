//! A generic block has one spelling, its parameters after `impl`:
//! `impl<T: Num> Grid<T> { … }`. Once a file is parsed, a block written so
//! for a program's type becomes a struct's block (`ty`, `params`), the form
//! the checker reads; a program type written with anything else in its
//! `<…>`, as the old `impl Grid<T>` was, is E0214.

use super::Parser;
use crate::ast::{Bound, Ident, ImplDecl, Import, Item, Type, TypeKind, TypeParam};
use crate::diagnostic::Diagnostic;
use crate::lexer::TokenKind;
use crate::span::Span;

/// The built-in generic types a block may name; any other name in a block's
/// type is a program's, unless the file declares or imports one so named.
const BUILT_IN: [&str; 13] = [
    "Option",
    "Result",
    "Map",
    "Set",
    "Ref",
    "Task",
    "Chan",
    "Ring",
    "Sender",
    "Receiver",
    "RingSender",
    "View",
    "MutView",
];

/// Rewrites every `impl<…> N<…>` of a program type `N` into a struct's
/// block, and reports E0214 for one whose `<…>` isn't its parameters.
pub(super) fn normalise(items: &mut [Item], imports: &[Import], diagnostics: &mut Vec<Diagnostic>) {
    let own = own_types(items, imports);
    for item in items {
        let Item::Impl(i) = item else { continue };
        let Some(target) = &i.target else { continue };
        let TypeKind::Applied(applied) = &target.kind else {
            continue;
        };
        let name = &applied.name.name;
        let program = applied.module.is_some()
            || !BUILT_IN.contains(&name.as_str())
            || own.iter().any(|o| o == name);
        if !program {
            continue;
        }
        if names_params(&applied.args, &i.params) {
            let (module, ty) = (applied.module.clone(), applied.name.clone());
            i.module = module;
            i.ty = ty;
            i.target = None;
        } else {
            diagnostics.push(short_form(i, target, &applied.args));
        }
    }
}

/// The struct, enum, actor and extern type names the file declares or
/// imports by name.
fn own_types(items: &[Item], imports: &[Import]) -> Vec<String> {
    let declared = items.iter().filter_map(|item| match item {
        Item::Struct(s) => Some(s.name.name.clone()),
        Item::Enum(e) => Some(e.name.name.clone()),
        Item::ExternType(t) => Some(t.name.name.clone()),
        _ => None,
    });
    let imported = imports
        .iter()
        .flat_map(|i| &i.items)
        .map(|item| item.bound().name.clone());
    declared.chain(imported).collect()
}

/// True when `args` are exactly the block's parameters, in order, each a
/// bare name.
fn names_params(args: &[Type], params: &[TypeParam]) -> bool {
    args.len() == params.len()
        && args
            .iter()
            .zip(params)
            .all(|(a, p)| matches!(&a.kind, TypeKind::Named(n) if *n == p.name.name))
}

/// E0214 for a program type's block whose `<…>` isn't its `impl<…>`
/// parameters, with the long form as help.
fn short_form(i: &ImplDecl, target: &Type, args: &[Type]) -> Diagnostic {
    let names: Vec<String> = if args.iter().all(|a| matches!(a.kind, TypeKind::Named(_))) {
        args.iter().map(Type::written).collect()
    } else {
        i.params.iter().map(|p| p.name.name.clone()).collect()
    };
    let base = target.written();
    let base = base.split('<').next().unwrap_or(&base);
    long_form_error(base, &names.join(", "), &names.join(", "), target.span)
}

/// E0214 at `span` for a block of type `base`, the help writing `params`
/// after `impl` and `args` after the type's name.
pub(super) fn long_form_error(base: &str, params: &str, args: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0214",
        "a generic `impl` names its type parameters after `impl`",
        span,
    )
    .with_help(format!("write `impl<{params}> {base}<{args}>`"))
}

impl Parser<'_> {
    /// True if `Name<T: B, …>` comes next, a name and a `:` inside its
    /// `<…>`: the old header of a generic struct's block.
    pub(super) fn bounded_header_ahead(&self) -> bool {
        let mut n = 0;
        if !matches!(self.peek_at(n), TokenKind::Ident(_)) {
            return false;
        }
        n += 1;
        while *self.peek_at(n) == TokenKind::Dot
            && matches!(self.peek_at(n + 1), TokenKind::Ident(_))
        {
            n += 2;
        }
        if *self.peek_at(n) != TokenKind::Lt {
            return false;
        }
        let mut depth = 0usize;
        loop {
            match self.peek_at(n) {
                TokenKind::Lt => depth += 1,
                TokenKind::Gt => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                TokenKind::Colon if depth == 1 => return true,
                TokenKind::Eof | TokenKind::LBrace | TokenKind::Semi => return false,
                _ => {}
            }
            n += 1;
        }
    }

    /// `T: A + B` as written, for E0214's help.
    pub(super) fn param_text(&self, p: &TypeParam) -> String {
        if p.bounds.is_empty() {
            return p.name.name.clone();
        }
        let bounds: Vec<String> = p.bounds.iter().map(Bound::written).collect();
        format!("{}: {}", p.name.name, bounds.join(" + "))
    }
}

/// The type's name when `t` is `Name<A, …>` with exactly the block's
/// parameters as its arguments, in order: a struct's, enum's or actor's
/// block written `impl<T> Grid<T>`.
pub(super) fn own_header(t: &Type, params: &[TypeParam]) -> Option<Ident> {
    let TypeKind::Applied(a) = &t.kind else {
        return None;
    };
    let names = a.args.len() == params.len()
        && a.args
            .iter()
            .zip(params)
            .all(|(arg, p)| matches!(&arg.kind, TypeKind::Named(n) if *n == p.name.name));
    (a.module.is_none() && names).then(|| a.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::test_util::parse_src;

    fn impls(src: &str) -> Vec<ImplDecl> {
        parse_src(src)
            .unwrap()
            .items
            .into_iter()
            .filter_map(|i| match i {
                Item::Impl(i) => Some(i),
                _ => None,
            })
            .collect()
    }

    fn codes(src: &str) -> Vec<String> {
        match parse_src(src) {
            Ok(_) => Vec::new(),
            Err(d) => d.into_iter().map(|d| d.code.to_string()).collect(),
        }
    }

    #[test]
    fn a_program_types_long_form_becomes_a_struct_block() {
        let src = "struct Grid<T> { c: T[] }\nimpl<T: Num> Grid<T> { fun n(self): i64 = 1; }\n\
                   interface Show { fun show(self): string; }\n\
                   impl<T> Show for geo.Grid<T> { fun show(self): string = \"\"; }";
        let found = impls(src);
        assert!(found[0].target.is_none() && found[0].ty.name == "Grid");
        assert_eq!(found[0].params[0].name.name, "T");
        assert!(found[1].target.is_none() && found[1].module.as_ref().unwrap().name == "geo");
    }

    #[test]
    fn built_in_types_stay_targets_unless_the_file_declares_the_name() {
        let found = impls("impl<T> Option<T> { }\nimpl<K, V> Map<K, V> { }\nimpl<T> T[] { }");
        assert!(found.iter().all(|i| i.target.is_some()));
        let found = impls("struct Map<K, V> { k: K, v: V }\nimpl<K, V> Map<K, V> { }");
        assert!(found[0].target.is_none());
    }

    #[test]
    fn the_short_form_is_e0214() {
        for src in [
            "struct Grid<T> { c: T[] }\nimpl Grid<T> { }",
            "struct Grid<T> { c: T[] }\nimpl Grid<T: Num> { }",
            "interface S { }\nstruct Grid<T> { c: T[] }\nimpl S for Grid<T> { }",
            "struct Grid<T> { c: T[] }\nimpl<T> Grid<i64> { }",
        ] {
            assert_eq!(codes(src), ["E0214"], "{src}");
        }
    }

    #[test]
    fn interface_parents_join_with_plus_and_a_comma_is_e0215() {
        let src = "interface A { }\ninterface B { }\ninterface C: A + B { }";
        assert!(codes(src).is_empty());
        assert_eq!(
            codes("interface A { }\ninterface B { }\ninterface C: A, B { }"),
            ["E0215"]
        );
    }
}
