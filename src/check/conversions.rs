//! Conversions between numbers: `as` where nothing can be lost, and the
//! methods that name a loss, `truncate`, `saturate`, `round`, `floor` and
//! `ceil`, or check for one, `try_into`. Each method takes its target as a
//! type argument: `big.truncate<u8>()`.

use super::expr::Expect;
use super::{Checker, Type, errors};
use crate::ast::{Expr, ExprKind, Ident};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// The conversion methods.
pub const CONVERSIONS: [&str; 6] = ["try_into", "truncate", "saturate", "round", "floor", "ceil"];

/// Every numeric type, what a `T: Num` may be.
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

/// An integer type's width in bits and whether it is signed.
pub fn int_bits(ty: &Type) -> Option<(u32, bool)> {
    Some(match ty {
        Type::I8 => (8, true),
        Type::I16 => (16, true),
        Type::I32 => (32, true),
        Type::I64 => (64, true),
        Type::U8 => (8, false),
        Type::U16 => (16, false),
        Type::U32 => (32, false),
        Type::U64 => (64, false),
        _ => return None,
    })
}

/// True if every value of `from` converts to `to` and back unchanged.
pub fn lossless(from: &Type, to: &Type) -> bool {
    if from == to {
        return true;
    }
    match (from, to) {
        (Type::F32, Type::F64) => true,
        // A code point is at most 0x10FFFF, and every byte is one.
        (Type::Char, Type::U32 | Type::I32 | Type::I64 | Type::U64) => true,
        (Type::U8, Type::Char) => true,
        // A float holds every integer up to its significand's width.
        (_, Type::F32) => int_bits(from).is_some_and(|(bits, _)| bits <= 16),
        (_, Type::F64) => int_bits(from).is_some_and(|(bits, _)| bits <= 32),
        _ => match (int_bits(from), int_bits(to)) {
            (Some((a, signed_a)), Some((b, signed_b))) => match (signed_a, signed_b) {
                (true, false) => false,
                (false, true) => b > a,
                _ => b >= a,
            },
            _ => false,
        },
    }
}

impl Checker {
    /// The types `ty` may be: itself, or every type a numeric type
    /// parameter's bound admits.
    fn instances_of_number(&self, ty: &Type) -> Vec<Type> {
        match ty {
            Type::Param(p) if self.has_float(p) => vec![Type::F32, Type::F64],
            Type::Param(p) if self.has_num(p) => NUMBERS.to_vec(),
            _ => vec![ty.clone()],
        }
    }

    /// E0469 unless `from as to` is lossless at every instance.
    pub(super) fn lossless_cast(&mut self, from: &Type, to: &Type, span: Span) -> bool {
        if from == to {
            return true;
        }
        let froms = self.instances_of_number(from);
        let tos = self.instances_of_number(to);
        if froms.iter().all(|f| tos.iter().all(|t| lossless(f, t))) {
            return true;
        }
        self.report(lossy_cast(from, to, span));
        false
    }

    /// `x.truncate<u8>()` and the other conversion methods on a number
    /// `receiver`. `None` if `name` isn't one or `receiver` isn't a number.
    pub(super) fn conversion_call(
        &mut self,
        call: &Expr,
        (receiver, name): (&Type, &Ident),
        args: &[Expr],
        expect: Option<&Expect>,
    ) -> Option<Type> {
        let method = name.name.as_str();
        let number = match receiver {
            Type::Param(p) => self.has_num(p),
            _ => receiver.is_numeric(),
        };
        if !CONVERSIONS.contains(&method) || !number {
            return None;
        }
        let written = match &call.kind {
            ExprKind::Call(callee, _) => self.written_one(callee, method),
            _ => None,
        };
        if !args.is_empty() {
            self.report(errors::arity(method, 0, args.len(), call.span));
            self.unchecked(args);
            return Some(Type::Error);
        }
        let target = written.or_else(|| match expect.map(|e| &e.ty) {
            Some(Type::Option(inner)) if method == "try_into" => Some((**inner).clone()),
            Some(ty) if method != "try_into" && self.is_number(ty) => Some(ty.clone()),
            _ => None,
        });
        let Some(target) = target else {
            let d = errors::cannot_infer("U", name.span)
                .with_help(format!("write the target: `x.{method}<u8>()`"));
            self.report(d);
            return Some(Type::Error);
        };
        if !self.conversion_fits(method, receiver, &target, name.span) {
            return Some(Type::Error);
        }
        Some(match method {
            "try_into" => Type::Option(Box::new(target)),
            _ => target,
        })
    }

    /// A number, or a type parameter that is one at every instance.
    fn is_number(&self, ty: &Type) -> bool {
        match ty {
            Type::Param(p) => self.has_num(p),
            _ => ty.is_numeric(),
        }
    }

    /// True if `method` converts `from` to `to`; E0408 otherwise.
    fn conversion_fits(&mut self, method: &str, from: &Type, to: &Type, span: Span) -> bool {
        let float_from = from.is_float() || matches!(from, Type::Param(p) if self.has_float(p));
        let ok = match method {
            "try_into" if *to == Type::Char => from.is_integer(),
            "truncate" => from.is_integer() && to.is_integer(),
            "floor" | "ceil" => float_from && self.is_number(to),
            _ => self.is_number(to),
        };
        if ok {
            return true;
        }
        let message = format!("`{method}` can't convert `{from}` to `{to}`");
        let help = match method {
            "truncate" => {
                "`truncate` keeps the low bits of an integer; from a float, write `round`, `floor`, `ceil` or `saturate`"
            }
            "floor" | "ceil" => "`floor` and `ceil` round a float; an integer is already whole",
            _ => "conversions go between numbers; `try_into<char>` takes an integer",
        };
        self.report(Diagnostic::error("E0408", message, span).with_help(help));
        false
    }
}

/// E0469: `from as to` can lose information, with the methods that fit.
fn lossy_cast(from: &Type, to: &Type, span: Span) -> Diagnostic {
    let message = format!("`as` can lose information: `{from}` to `{to}`");
    let help = match (from, to) {
        (Type::Param(_), _) | (_, Type::Param(_)) => format!(
            "name the conversion, which works at every instance: `x.saturate<{to}>()`, `x.round<{to}>()` or `x.try_into<{to}>()`"
        ),
        (_, Type::Char) => {
            "`x.try_into<char>()` gives an `Option`, `None` unless `x` is a valid code point"
                .to_string()
        }
        (Type::Char, _) => format!(
            "convert the code point: `(c as u32).truncate<{to}>()` or `(c as u32).try_into<{to}>()`"
        ),
        (f, _) if f.is_float() && to.is_integer() => format!(
            "`x.round<{to}>()`, `x.floor<{to}>()` and `x.ceil<{to}>()` round and panic out of range; `x.saturate<{to}>()` rounds toward zero and clamps; `x.try_into<{to}>()` gives an `Option`"
        ),
        (_, t) if t.is_float() => format!(
            "`x.round<{to}>()` gives the nearest `{to}`; `x.try_into<{to}>()` gives an `Option`, `None` unless it is exact"
        ),
        _ => format!(
            "`x.truncate<{to}>()` keeps the low bits; `x.saturate<{to}>()` clamps to the range; `x.try_into<{to}>()` gives an `Option`"
        ),
    };
    Diagnostic::error("E0469", message, span).with_help(help)
}

#[cfg(test)]
mod tests {
    use super::lossless;
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type};

    #[test]
    fn only_lossless_pairs_are_casts() {
        assert!(lossless(&Type::U8, &Type::I16));
        assert!(lossless(&Type::I32, &Type::F64));
        assert!(lossless(&Type::Char, &Type::U32));
        assert!(lossless(&Type::U8, &Type::Char));
        assert!(!lossless(&Type::I8, &Type::U64));
        assert!(!lossless(&Type::I64, &Type::F64));
        assert!(!lossless(&Type::U32, &Type::F32));
        assert!(!lossless(&Type::F64, &Type::F32));
        assert!(!lossless(&Type::Char, &Type::U8));
    }

    #[test]
    fn the_methods_take_their_target() {
        let main = |body: &str| format!("fun main() {{ let n: i64 = 300; {body} }}");
        assert_eq!(let_type(&main("let a = n.truncate<u8>();"), "a"), Type::U8);
        let opt = Type::Option(Box::new(Type::Char));
        assert_eq!(let_type(&main("let c = n.try_into<char>();"), "c"), opt);
        assert_eq!(
            let_type(&main("let r: u16 = n.saturate();"), "r"),
            Type::U16
        );
        assert_eq!(let_type(&main("let f = 2.5.round<i32>();"), "f"), Type::I32);
        assert_eq!(errors(&main("let a = n.truncate();"))[0].0, "E0412");
        assert_eq!(errors(&main("let a = 2.5.truncate<u8>();"))[0].0, "E0408");
        assert_eq!(errors(&main("let a = n.floor<i64>();"))[0].0, "E0408");
        assert_eq!(errors(&main("let a = n.round<u8>(1);"))[0].0, "E0405");
        assert_eq!(errors(&main("let a = n as u8;")), [("E0469", "n as u8")]);
    }
}
