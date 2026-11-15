//! Written types: the syntax of a type, resolved later by the checker.

use super::Ident;
use crate::span::Span;

/// A written type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type {
    /// Which type.
    pub kind: TypeKind,
    /// Where it was written.
    pub span: Span,
}

/// The types a program can write, as written; struct names are unresolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeKind {
    I32,
    I64,
    U8,
    F64,
    Bool,
    String,
    /// `u32`, `u64`, `f32`.
    U32,
    U64,
    F32,
    /// `i8`, `i16`, `u16`.
    I8,
    I16,
    U16,
    /// `char`.
    Char,
    /// A struct name, resolved by the checker.
    Named(String),
    /// `T[]`.
    Array(Box<Type>),
    /// `Option<T>`.
    Option(Box<Type>),
    /// `m.T`, `T<args>` or `m.T<args>`. Boxed to keep `Type`,
    /// and so `Expr` (through casts), small for the parser's recursion.
    Applied(Box<AppliedType>),
    /// `(A, B, …)`, two or more elements.
    Tuple(Vec<Type>),
    /// `fun(A, B): R`; no return type means `void`.
    Fun(Vec<Type>, Option<Box<Type>>),
}

/// A module-qualified type name, type arguments, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedType {
    /// `m` in `m.T`.
    pub module: Option<Ident>,
    /// The type's name.
    pub name: Ident,
    /// `<args>` in order; empty for a plain `m.T`.
    pub args: Vec<Type>,
}

impl Type {
    /// The type as written, without spaces: `i64`, `geo.Point[]`,
    /// `Pair<string,(i64,i64)>`, `fun(i64):bool`.
    pub fn written(&self) -> String {
        let list = |ts: &[Type]| ts.iter().map(Type::written).collect::<Vec<_>>().join(",");
        match &self.kind {
            TypeKind::I32 => "i32".into(),
            TypeKind::I64 => "i64".into(),
            TypeKind::U8 => "u8".into(),
            TypeKind::F64 => "f64".into(),
            TypeKind::Bool => "bool".into(),
            TypeKind::String => "string".into(),
            TypeKind::U32 => "u32".into(),
            TypeKind::U64 => "u64".into(),
            TypeKind::F32 => "f32".into(),
            TypeKind::I8 => "i8".into(),
            TypeKind::I16 => "i16".into(),
            TypeKind::U16 => "u16".into(),
            TypeKind::Char => "char".into(),
            TypeKind::Named(n) => n.clone(),
            TypeKind::Array(inner) => format!("{}[]", inner.written()),
            TypeKind::Option(inner) => format!("Option<{}>", inner.written()),
            TypeKind::Applied(a) => {
                let mut out = match &a.module {
                    Some(m) => format!("{}.{}", m.name, a.name.name),
                    None => a.name.name.clone(),
                };
                if !a.args.is_empty() {
                    out.push_str(&format!("<{}>", list(&a.args)));
                }
                out
            }
            TypeKind::Tuple(ts) => format!("({})", list(ts)),
            TypeKind::Fun(params, ret) => match ret {
                Some(r) => format!("fun({}):{}", list(params), r.written()),
                None => format!("fun({})", list(params)),
            },
        }
    }
}
