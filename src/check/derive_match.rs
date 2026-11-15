//! How derive blocks and blocks for built-in types match a type: a
//! block's type as a pattern over its parameters, whether two patterns
//! could cover one type, and the labels its hidden owner is named by.

use std::collections::HashMap;

use super::Type;
use super::derive::{Block, Covers};

/// True if `ty` is `pattern` with each parameter in `names` replaced, the
/// same way every time; `bound` gets what each stands for.
pub(super) fn matches(
    pattern: &Type,
    ty: &Type,
    names: &[&str],
    bound: &mut HashMap<String, Type>,
) -> bool {
    match (pattern, ty) {
        (Type::Param(n), _) if names.contains(&n.as_str()) => match bound.get(n) {
            Some(seen) => seen == ty,
            None => {
                bound.insert(n.clone(), ty.clone());
                true
            }
        },
        (Type::Array(p), Type::Array(t)) | (Type::Option(p), Type::Option(t)) => {
            matches(p, t, names, bound)
        }
        (Type::Struct(pk, pa), Type::Struct(tk, ta)) | (Type::Enum(pk, pa), Type::Enum(tk, ta)) => {
            pk == tk
                && pa.len() == ta.len()
                && pa.iter().zip(ta).all(|(p, t)| matches(p, t, names, bound))
        }
        _ => pattern == ty,
    }
}

/// What a block covers, as written: `T[]`, `struct`.
pub(super) fn block_type(covers: &Covers) -> String {
    match covers {
        Covers::Type(t) => super::shown(&t.to_string()),
        Covers::Struct => "struct".into(),
        Covers::Enum => "enum".into(),
        Covers::Tuple => "tuple".into(),
        Covers::Handle => "handle".into(),
    }
}

/// A short, symbol-safe name for what a block covers: `i64`, `array`, `Map`.
pub(super) fn label(covers: &Covers) -> String {
    match covers {
        Covers::Struct => "struct".into(),
        Covers::Enum => "enum".into(),
        Covers::Tuple => "tuple".into(),
        Covers::Handle => "handle".into(),
        Covers::Type(Type::Array(_)) => "array".into(),
        Covers::Type(Type::Option(_)) => "option".into(),
        Covers::Type(Type::Unit) => "unit".into(),
        Covers::Type(Type::Struct(key, _) | Type::Enum(key, _)) => {
            key.rsplit('.').next().unwrap_or(key).to_string()
        }
        Covers::Type(other) => other.to_string(),
    }
}

/// True if one type could be covered by both patterns: each pattern's
/// parameters (in `a` and `b`) stand for any type.
pub(super) fn overlap(a: &Type, b: &Type, a_names: &[&str], b_names: &[&str]) -> bool {
    match (a, b) {
        (Type::Param(n), _) if a_names.contains(&n.as_str()) => true,
        (_, Type::Param(n)) if b_names.contains(&n.as_str()) => true,
        (Type::Array(x), Type::Array(y)) | (Type::Option(x), Type::Option(y)) => {
            overlap(x, y, a_names, b_names)
        }
        (Type::Struct(ka, xa), Type::Struct(kb, xb)) | (Type::Enum(ka, xa), Type::Enum(kb, xb)) => {
            ka == kb
                && xa.len() == xb.len()
                && xa
                    .iter()
                    .zip(xb)
                    .all(|(x, y)| overlap(x, y, a_names, b_names))
        }
        _ => a == b,
    }
}

/// The names of a block's parameters.
pub(super) fn names(block: &Block) -> Vec<&str> {
    block.generics.iter().map(|g| g.name.as_str()).collect()
}
