//! Choosing an operator's block. `a * b` calls the `mul` of the block
//! `impl Mul<B> for A` whose argument is `b`'s exact type; a type may have
//! one block per right-hand type. Only an exact type fits, and a literal
//! must fit exactly one of them (E0457), so nothing is ranked and adding a
//! block can't change an existing choice silently. A number literal on
//! the left fits the number types with a block for the right operand.

use super::applied::Applied;
use super::expr::Expect;
use super::generics::subst;
use super::methods::Method;
use super::{Checker, Type};
use crate::ast::{Expr, ExprKind, Ident, UnOp};
use crate::diagnostic::Diagnostic;

/// The operator interface whose item is `name`: `$prelude.Mul` for `mul`.
pub fn interface_of(name: &str) -> Option<&'static str> {
    Some(match name {
        "add" => "$prelude.Add",
        "sub" => "$prelude.Sub",
        "mul" => "$prelude.Mul",
        "div" => "$prelude.Div",
        "rem" => "$prelude.Rem",
        "neg" => "$prelude.Neg",
        _ => return None,
    })
}

/// The number types, which a literal on the left may be.
const NUMBERS: [Type; 10] = [
    Type::I8,
    Type::I16,
    Type::I32,
    Type::I64,
    Type::U8,
    Type::U16,
    Type::U32,
    Type::U64,
    Type::F32,
    Type::F64,
];

impl Checker {
    /// The right-hand types of `l`'s blocks of operator interface `key`,
    /// in the order written: one per block.
    pub(super) fn operator_rights(&self, l: &Type, key: &str) -> Vec<Type> {
        let owned = match l {
            Type::Struct(o, targs) | Type::Enum(o, targs) => Some((o, targs.as_slice())),
            Type::Handle(o) => Some((o, &[][..])),
            _ => None,
        };
        if let Some((owner, targs)) = owned
            && !owner.starts_with('$')
        {
            return self
                .impls
                .get(owner)
                .into_iter()
                .flatten()
                .filter(|e| e.key == key)
                .map(|e| e.args_at(targs).first().cloned().unwrap_or(Type::Unit))
                .collect();
        }
        self.derives
            .blocks_for(key, l)
            .into_iter()
            .map(|(b, args)| {
                b.args
                    .first()
                    .map_or(Type::Unit, |a| subst(a, &b.generics, &args))
            })
            .collect()
    }

    /// True if `l` has any block of the operator whose item is `name`.
    pub(super) fn has_operator(&self, l: &Type, name: &str) -> bool {
        interface_of(name).is_some_and(|key| !self.operator_rights(l, key).is_empty())
    }

    /// The right-hand type of `l`'s block of `name`'s interface that `arg`
    /// fits: `Ok(None)` when `l` has no such block, `Err(())` after
    /// reporting that none or several fit (E0404, E0457).
    pub(super) fn chosen_right(
        &mut self,
        l: &Type,
        name: &str,
        arg: &Expr,
    ) -> Result<Option<Type>, ()> {
        let Some(key) = interface_of(name) else {
            return Ok(None);
        };
        let rights = self.operator_rights(l, key);
        if rights.is_empty() {
            return Ok(None);
        }
        let fitting: Vec<&Type> = match literal_kind(arg) {
            Some(kind) => rights.iter().filter(|t| kind.fits(t)).collect(),
            None => {
                // A probe: the call checks the argument again against its choice.
                let reported = self.diagnostics.len();
                let ty = self.value(arg, None);
                self.diagnostics.truncate(reported);
                if matches!(ty, Type::Error | Type::Never) {
                    return Err(());
                }
                rights.iter().filter(|t| **t == ty).collect()
            }
        };
        let shown = |c: &[&Type]| {
            c.iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match fitting.as_slice() {
            [one] => Ok(Some((*one).clone())),
            [] => {
                let all: Vec<&Type> = rights.iter().collect();
                let interface = key.trim_start_matches("$prelude.");
                let message = format!("no `impl {interface}` of `{l}` takes this operand");
                let help = format!("`{l}` has `{interface}` for {}", shown(&all));
                self.report(Diagnostic::error("E0404", message, arg.span).with_help(help));
                Err(())
            }
            several => {
                let message = format!(
                    "this operand fits more than one `{name}` of `{l}`: {}",
                    shown(several)
                );
                let help = match literal_kind(arg) {
                    Some(_) => format!("say which: {}", spelled(arg, several)),
                    // A generic type whose parameters became the same type.
                    None => format!("for `{l}` these blocks take the same type"),
                };
                self.report(Diagnostic::error("E0457", message, arg.span).with_help(help));
                Err(())
            }
        }
    }

    /// The call of `l`'s block of `name`'s interface at right-hand type
    /// `right` (none for `neg`), recorded for codegen, as the method to
    /// check the call against.
    pub(super) fn operator_item(
        &mut self,
        call: &Expr,
        l: &Type,
        name: &str,
        right: Option<Type>,
    ) -> Method {
        let key = interface_of(name).expect("an operator's item");
        let applied = Applied {
            key: key.to_string(),
            args: right.into_iter().collect(),
        };
        if let Type::Struct(o, _) | Type::Enum(o, _) | Type::Handle(o) = l
            && !o.starts_with('$')
        {
            return self.item_call(call, l, &applied, name);
        }
        let at = (!applied.args.is_empty()).then_some(applied.args.as_slice());
        let (block, args) = self
            .builtin_block_at(key, at, l)
            .expect("checked: the block was found");
        self.block_call(call, l, (&block, &args), name)
    }

    /// `lhs op rhs` with a built-in type or a number literal on the left and
    /// a type with blocks for it on the right: `2.0 * v` through `impl
    /// Mul<V> for f64`. `None` leaves the built-in operator.
    pub(super) fn builtin_on_left(
        &mut self,
        expr: &Expr,
        (symbol, name): (&str, &str),
        (lhs, rhs): (&Expr, &Expr),
    ) -> Option<Type> {
        let key = interface_of(name)?;
        // Only blocks written for built-in types can apply. Probing the left
        // operand is the costly part (a chain `a + b + c + …` would probe
        // every prefix), so it comes last, when a block takes the right one.
        let rights: Vec<Type> = self
            .derives
            .blocks
            .iter()
            .filter(|b| b.written && b.interface == key)
            .filter_map(|b| b.args.first().cloned())
            .collect();
        if rights.is_empty() {
            return None;
        }
        let reported = self.diagnostics.len();
        let right = self.value(rhs, None);
        self.diagnostics.truncate(reported);
        let taken = rights.iter().any(|r| *r == right || has_params(r));
        if !taken {
            return None;
        }
        let literal = literal_kind(lhs);
        let left = literal.is_none().then(|| {
            let reported = self.diagnostics.len();
            let ty = self.value(lhs, None);
            self.diagnostics.truncate(reported);
            ty
        });
        // Numbers, strings and the like on the right keep the built-in operator.
        let plain = right.is_numeric()
            || matches!(
                right,
                Type::String | Type::Bool | Type::Char | Type::Error | Type::Never | Type::Param(_)
            );
        if plain {
            return None;
        }
        if left.as_ref().is_some_and(super::operators::overloaded) {
            return None;
        }
        let has = |c: &Self, l: &Type| c.operator_rights(l, key).contains(&right);
        let fitting: Vec<Type> = match (&left, &literal) {
            (Some(l), _) => has(self, l).then(|| l.clone()).into_iter().collect(),
            (None, Some(kind)) => NUMBERS
                .iter()
                .filter(|t| kind.fits(t) && has(self, t))
                .cloned()
                .collect(),
            (None, None) => Vec::new(),
        };
        match fitting.as_slice() {
            [] => None,
            [l] => {
                let l = l.clone();
                self.expect_type(lhs, &Expect::of(l.clone()));
                let at = crate::span::Span::new(lhs.span.end, rhs.span.start);
                let parts = (symbol, name, at);
                Some(self.operator_call(expr, parts, &l, Some(rhs)))
            }
            several => {
                let types: Vec<&Type> = several.iter().collect();
                let message = format!(
                    "this operand fits more than one `{name}` with `{right}`: {}",
                    types
                        .iter()
                        .map(|t| format!("`{t}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                let help = format!("say which: {}", spelled(lhs, &types));
                self.report(Diagnostic::error("E0457", message, lhs.span).with_help(help));
                Some(Type::Error)
            }
        }
    }

    /// True if built-in `target` has a block of `name`'s interface for
    /// `value`'s type: `k *= v` with `impl Mul<V> for f64`.
    pub(super) fn number_block(&mut self, target: &Type, name: &str, value: &Expr) -> bool {
        let Some(key) = interface_of(name) else {
            return false;
        };
        if super::operators::overloaded(target) {
            return false;
        }
        let rights = self.operator_rights(target, key);
        if rights.is_empty() {
            return false;
        }
        // A probe: the assignment checks the value again.
        let reported = self.diagnostics.len();
        let right = self.value(value, None);
        self.diagnostics.truncate(reported);
        rights.contains(&right)
    }

    /// `v.mul(x)` on a type with blocks of `mul`'s interface: the block `x`
    /// fits, as for `v * x`. `None` when it has none.
    pub(super) fn operator_method_call(
        &mut self,
        call: &Expr,
        receiver: &Type,
        name: &Ident,
        args: &[Expr],
    ) -> Option<Result<Method, ()>> {
        if interface_of(&name.name).is_none() || !self.has_operator(receiver, &name.name) {
            return None;
        }
        let right = match (name.name.as_str(), args) {
            ("neg", []) => None,
            (_, [arg]) => match self.chosen_right(receiver, &name.name, arg) {
                Ok(right) => right,
                Err(()) => return Some(Err(())),
            },
            _ => return None,
        };
        Some(Ok(self.operator_item(call, receiver, &name.name, right)))
    }
}

/// True if a block's argument names a parameter, so it may take many types.
fn has_params(ty: &Type) -> bool {
    match ty {
        Type::Param(_) => true,
        Type::Array(t) | Type::Option(t) => has_params(t),
        Type::Struct(_, args) | Type::Enum(_, args) => args.iter().any(has_params),
        _ => false,
    }
}

/// Ways to write `arg` as each of `types`: `2.0`, `2 as u8`.
fn spelled(arg: &Expr, types: &[&Type]) -> String {
    let ways: Vec<String> = types
        .iter()
        .map(|t| match &arg.kind {
            ExprKind::Int(n) if t.is_float() => format!("`{n}.0`"),
            ExprKind::Int(n) => format!("`{n} as {t}`"),
            _ => format!("a `{t}`"),
        })
        .collect();
    ways.join(" or ")
}

/// What kind of literal an operand is, if it takes its type from where it's used.
enum Literal {
    Integer,
    Float,
}

impl Literal {
    fn fits(&self, ty: &Type) -> bool {
        match self {
            // `2` is never a float: `let x: f64 = 2;` is E0401.
            Literal::Integer => ty.is_integer(),
            Literal::Float => ty.is_float(),
        }
    }
}

fn literal_kind(e: &Expr) -> Option<Literal> {
    match &e.kind {
        ExprKind::Int(_) => Some(Literal::Integer),
        ExprKind::Float(_) => Some(Literal::Float),
        ExprKind::Unary(UnOp::Neg, inner) => literal_kind(inner),
        _ => None,
    }
}

#[cfg(test)]
#[path = "overloads_tests.rs"]
mod tests;
