//! Operators on user types: `a * b` calls the `mul` of `a`'s type's block
//! `impl Mul<B> for A` (`check::overloads` chooses it by `b`'s type), a
//! bounded type parameter's through its bounds, and likewise `+`, `-`,
//! `/`, `%` and unary `-`. The call is recorded under the operator
//! expression's id, so instances and codegen treat it as a qualified call.
//! The numbers (and `string` and arrays, for `Add`) satisfy the prelude's
//! operator bounds as built-ins.

use super::iterators::is_operator_bound;
use super::{Checker, Type, errors};
use crate::ast::{AssignOp, BinOp, Expr};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// The method `op` calls, for the overloadable operators.
pub(super) fn method_of(op: BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::Add => "add",
        BinOp::Sub => "sub",
        BinOp::Mul => "mul",
        BinOp::Div => "div",
        BinOp::Rem => "rem",
        _ => return None,
    })
}

/// The method a compound assignment calls: `a += b` is `a = a.add(b)`.
pub(super) fn method_of_assign(op: AssignOp) -> Option<&'static str> {
    Some(match op {
        AssignOp::Add => "add",
        AssignOp::Sub => "sub",
        AssignOp::Mul => "mul",
        AssignOp::Div => "div",
        AssignOp::Rem => "rem",
        _ => return None,
    })
}

/// True for the types whose operators are methods: structs, enums,
/// interface values and type parameters. Numbers and strings keep the
/// built-in operators.
pub(super) fn overloaded(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Struct(..) | Type::Enum(..) | Type::Interface(..) | Type::Param(_)
    )
}

impl Checker {
    /// What `arg` lacks to satisfy a prelude operator bound as a built-in:
    /// nothing for a number (or a `string` and `Add`), the reason for a
    /// non-number and `Num`; `None` when the ordinary rules decide.
    pub(super) fn builtin_operator(&self, arg: &Type, key: &str) -> Option<Vec<String>> {
        let name = key.strip_prefix("$prelude.")?;
        if !is_operator_bound(name) {
            return None;
        }
        // `Float`: `f32` and `f64` only.
        if name == "Float" {
            let parameter = matches!(arg, Type::Param(p) if self.has_float(p));
            return match arg.is_float() || parameter || matches!(arg, Type::Error) {
                true => Some(Vec::new()),
                false => Some(vec!["only `f32` and `f64` are `Float`".to_string()]),
            };
        }
        // Strings and arrays join with `+`.
        if arg.is_numeric() || (name == "Add" && matches!(arg, Type::String | Type::Array(_))) {
            return Some(Vec::new());
        }
        let parameter = matches!(arg, Type::Param(p) if self.has_num(p));
        if name == "Num" && !parameter && !matches!(arg, Type::Error) {
            return Some(vec!["only the number types are `Num`".to_string()]);
        }
        None
    }

    /// True if type parameter `p` is bounded by the prelude's `Num`, or
    /// by `Float`, which allows everything `Num` does.
    pub(super) fn has_num(&self, p: &str) -> bool {
        self.bounds(p)
            .interfaces
            .iter()
            .any(|a| a.key == "$prelude.Num" || a.key == "$prelude.Float")
    }

    /// True if type parameter `p` is bounded by the prelude's `Float`.
    pub(super) fn has_float(&self, p: &str) -> bool {
        self.bounds(p)
            .interfaces
            .iter()
            .any(|a| a.key == "$prelude.Float")
    }

    /// `lhs op rhs` (or `-lhs`, with no `rhs`) where `lhs`, already
    /// checked, has the overloaded type `l`: the call of method `name`.
    /// E0404 when `l` has no method of that name and shape.
    pub(super) fn operator_call(
        &mut self,
        expr: &Expr,
        (symbol, name, at): (&str, &str, Span),
        l: &Type,
        rhs: Option<&Expr>,
    ) -> Type {
        let args: &[Expr] = rhs.map_or(&[], std::slice::from_ref);
        match l {
            // Through `T`'s operator bound: `T: Mul<f64>` gives `x * 2.0`.
            Type::Param(p) => {
                let Some(applied) = self.operator_bound(p, name, args.len()) else {
                    let help = format!(
                        "add a bound that allows it, such as `{p}: Num` or `{p}: {}`",
                        bound_name(name)
                    );
                    let d = errors::bad_operands(symbol, std::slice::from_ref(l), expr.span);
                    self.report(d.with_help(help));
                    return self.unchecked(args);
                };
                let p = p.clone();
                let m = self.bound_item_call(expr, &p, &applied, name);
                return self.arguments(expr, name, &m, args, (&[], None));
            }
            // An interface value has no operator blocks.
            Type::Interface(..) => {
                let help = format!("call its method instead: `x.{name}(…)`");
                let d = errors::bad_operands(symbol, std::slice::from_ref(l), expr.span);
                self.report(d.with_help(help));
                return self.unchecked(args);
            }
            _ => {}
        }
        if !self.has_operator(l, name) {
            let interface = super::overloads::interface_of(name)
                .unwrap_or_default()
                .trim_start_matches("$prelude.");
            let help = format!("write `impl {interface}<…> for {l} {{ fun {name}(…) }}`");
            let d = errors::bad_operands(symbol, std::slice::from_ref(l), expr.span);
            self.report(d.with_help(help));
            return self.unchecked(args);
        }
        let right = match rhs {
            Some(arg) => match self.chosen_right(l, name, arg) {
                Ok(right) => right,
                Err(()) => return self.unchecked(args),
            },
            None => None,
        };
        let m = self.operator_item(expr, l, name, right);
        self.hover.function(at, m.span, Some(expr));
        if m.generics.is_empty() && !m.key.is_empty() {
            self.used(expr, &m.key, &[]);
        }
        // A generic type's block takes its parameters from `l`.
        let bound = match l {
            Type::Struct(_, targs) | Type::Enum(_, targs) => targs.clone(),
            _ => Vec::new(),
        };
        self.arguments(expr, name, &m, args, (&bound, None))
    }

    /// `place op= value` on an overloaded `target`: `place = place.op(value)`,
    /// whose result must be `target` (E0403).
    pub(super) fn operator_assign(
        &mut self,
        (symbol, name): (&str, &str),
        (call, place, target): (&Expr, &Expr, &Type),
        value: &Expr,
    ) {
        let span = Span::new(place.span.start, value.span.end);
        let ty = self.operator_call(call, (symbol, name, span), target, Some(value));
        if ty != *target && !matches!(ty, Type::Error | Type::Never) {
            let message = format!("`{symbol}` gives {ty} here, but the place is {target}");
            self.report(Diagnostic::error("E0403", message, span));
        }
    }

    /// The bound of type parameter `p` that gives operator item `name`
    /// taking `arity` operands: an operator interface, `Num` or `Float`
    /// (an interface's own `mul` is a method, called by name).
    fn operator_bound(&self, p: &str, name: &str, arity: usize) -> Option<super::applied::Applied> {
        let me = Type::Param(p.to_string());
        let operator = |key: &str| {
            super::impls::is_operator(key) || key == "$prelude.Num" || key == "$prelude.Float"
        };
        self.bounds(p).interfaces.into_iter().find(|a| {
            operator(&a.key)
                && self
                    .items_for(a, &me)
                    .iter()
                    .any(|r| r.name == name && r.receiver == Some(false) && r.params.len() == arity)
        })
    }
}

/// The prelude bound for the operator method `name`.
fn bound_name(name: &str) -> &'static str {
    match name {
        "add" => "Add",
        "sub" => "Sub",
        "mul" => "Mul",
        "div" => "Div",
        "rem" => "Rem",
        _ => "Neg",
    }
}

#[cfg(test)]
#[path = "operators_tests.rs"]
mod tests;
