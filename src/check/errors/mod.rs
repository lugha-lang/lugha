//! One constructor per checker error code, so the wording lives in one place
//! (codes are stable; messages may improve).
//! Split by code range: `names` (E03xx), `types` (E04xx), `places` (E05xx, W01xx),
//! with `generics` holding the codes for generics from both ranges, and one module per
//! feature for the rest (`interfaces`, `control`, `externs`, …).

mod concurrency;
mod consts;
mod control;
mod derive;
mod externs;
mod generics;
mod impls;
mod interfaces;
mod names;
mod patterns;
mod places;
mod templates;
mod test_fns;
mod types;
mod values;
mod visibility;

pub(super) use concurrency::*;
pub(super) use consts::*;
pub(super) use control::*;
pub(super) use derive::*;
pub(super) use externs::*;
pub(super) use generics::*;
pub(super) use impls::*;
pub(super) use interfaces::*;
pub(super) use names::*;
pub(super) use patterns::*;
pub(super) use places::*;
pub(super) use templates::*;
pub(super) use test_fns::*;
pub(super) use types::*;
pub(super) use values::*;
pub(super) use visibility::*;

use crate::diagnostic::{Diagnostic, Label};

fn with_reason(d: Diagnostic, reason: Option<&Label>) -> Diagnostic {
    match reason {
        Some(label) => d.with_label(label.span, label.message.clone()),
        None => d,
    }
}

/// `e` shortened for a help: names, fields, literals and calls as
/// written, anything longer as `…`.
pub(in crate::check) fn brief(e: &crate::ast::Expr) -> String {
    match &e.kind {
        crate::ast::ExprKind::Name(n) => n.clone(),
        crate::ast::ExprKind::Field(base, field) => format!("{}.{}", brief(base), field.name),
        crate::ast::ExprKind::Int(n) => n.to_string(),
        crate::ast::ExprKind::Bool(b) => b.to_string(),
        crate::ast::ExprKind::Call(callee, args) => {
            let inside = if args.is_empty() { "" } else { "…" };
            format!("{}({inside})", brief(callee))
        }
        crate::ast::ExprKind::StructLit(lit) => match &lit.module {
            Some(m) => format!("{}.{} {{ … }}", m.name, lit.name.name),
            None => format!("{} {{ … }}", lit.name.name),
        },
        crate::ast::ExprKind::Index(base, _, _) => format!("{}[…]", brief(base)),
        _ => "…".to_string(),
    }
}
