//! C calls on Windows x64: what lughac can't pass yet. Windows passes a
//! struct by its own rules (one of 1, 2, 4 or 8 bytes in a register,
//! anything else through a pointer to a copy), not System V's, and the
//! lowering for them isn't written: an `extern` taking or giving an
//! `extern struct` by value is refused for a Windows build (E0337), so it
//! never builds with the wrong rules. Scalars and pointers need nothing:
//! LLVM passes them by Windows' rules itself.

use crate::ast::{Item, Type as Written, TypeKind};
use crate::check::{Checked, Type};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// E0337 at each struct passed by value through an `extern` in `items`
/// (its parameters not passed by address, its result, and those of a
/// callback it takes), by the types `checked` resolved.
pub fn struct_refusals<'a>(
    items: impl IntoIterator<Item = &'a Item>,
    checked: &Checked,
) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    for item in items {
        let Item::Extern(e) = item else { continue };
        // `ptr`, `mut` and `out` pass by address, which Windows passes as C does.
        let by_copy = e.params.iter().filter(|p| p.mode.is_none());
        let written = by_copy.map(|p| &p.ty).chain(e.ret.as_ref());
        for ty in written {
            by_value(ty, checked, &mut found);
        }
    }
    found
}

/// The spans in `ty`, written at an extern, where a struct goes by value:
/// `ty` itself, or a callback's parameter or result.
fn by_value(ty: &Written, checked: &Checked, found: &mut Vec<Diagnostic>) {
    if let TypeKind::Fun(params, ret) = &ty.kind {
        for inner in params.iter().chain(ret.as_deref()) {
            by_value(inner, checked, found);
        }
        return;
    }
    if let Some(Type::Struct(name, _)) = checked.annotations.get(&ty.span) {
        found.push(refused(name, ty.span));
    }
}

/// E0337: the `extern struct` `name`, at `span`, passed by value.
fn refused(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0337",
        format!("`{name}` can't be passed to C by value on Windows yet"),
        span,
    )
    .with_help(
        "pass it by address (`ptr` or `mut`), or build for another target; \
         Windows passes structs by its own rules, which lughac doesn't lower yet",
    )
}
