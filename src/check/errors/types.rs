//! E04xx: types.

use super::with_reason;
use crate::check::Type;
use crate::diagnostic::{Diagnostic, Label};
use crate::span::Span;

pub(in crate::check) fn wrong_literal(
    is_float: bool,
    expected: Type,
    span: Span,
    reason: Option<&Label>,
) -> Diagnostic {
    let kind = if is_float { "float" } else { "integer" };
    let d = Diagnostic::error(
        "E0401",
        format!("{kind} literal where {expected} expected"),
        span,
    )
    .with_primary_label(format!("expected {expected}"));
    with_reason(d, reason)
}

pub(in crate::check) fn out_of_range(ty: Type, span: Span) -> Diagnostic {
    let (max, neg) = ty.literal_range();
    let min = if neg == 0 {
        "0".to_string()
    } else {
        format!("-{neg}")
    };
    Diagnostic::error(
        "E0402",
        format!("integer literal out of range for {ty}"),
        span,
    )
    .with_help(format!("{ty} holds {min} to {max}"))
}

/// E0402 at a literal typed as a type parameter, out of range at one instance.
pub(in crate::check) fn out_of_range_in(ty: Type, span: Span, instance: &str) -> Diagnostic {
    let (max, neg) = ty.literal_range();
    let min = if neg == 0 {
        "0".to_string()
    } else {
        format!("-{neg}")
    };
    Diagnostic::error(
        "E0402",
        format!("integer literal out of range for {ty} in the instance {instance}"),
        span,
    )
    .with_help(format!(
        "{ty} holds {min} to {max}; this literal is {ty} in {instance}"
    ))
}

pub(in crate::check) fn mismatch(
    expected: Type,
    found: Type,
    span: Span,
    reason: Option<&Label>,
) -> Diagnostic {
    let mut d = Diagnostic::error("E0403", format!("expected {expected}, found {found}"), span)
        .with_primary_label(format!("expected {expected}"));
    if let Some(help) = state_or_handle(&expected, &found) {
        d = d.with_help(help);
    }
    with_reason(d, reason)
}

/// When `expected` and `found` differ only where one is an actor's state
/// and the other its handle, which print alike: which is which.
fn state_or_handle(expected: &Type, found: &Type) -> Option<String> {
    let (state, handle) = clash(expected, found)?;
    let name = state.rsplit('.').next().unwrap_or(&state);
    let advice = match handle {
        Side::Found => "the code expects its state but has a handle to it",
        Side::Expected => {
            "the code expects a handle to it but has its state; write `Self` where the state is meant"
        }
    };
    Some(format!(
        "two types print as `{name}` here: inside `actor {name}`, `Self` is its state and `{name}` a handle to it, as everywhere; {advice}"
    ))
}

/// Which side holds the actor's handle where the other holds its state.
enum Side {
    Expected,
    Found,
}

/// The actor whose state one type has where the other has its handle, at
/// the first such place.
fn clash(expected: &Type, found: &Type) -> Option<(String, Side)> {
    match (expected, found) {
        (Type::Struct(a, _), Type::Actor(b, _)) if a == b => Some((a.clone(), Side::Found)),
        (Type::Actor(a, _), Type::Struct(b, _)) if a == b => Some((a.clone(), Side::Expected)),
        (Type::Array(x), Type::Array(y))
        | (Type::Option(x), Type::Option(y))
        | (Type::Ref(x), Type::Ref(y))
        | (Type::Weak(x), Type::Weak(y))
        | (Type::Task(x), Type::Task(y))
        | (Type::Chan(_, x), Type::Chan(_, y)) => clash(x, y),
        (Type::Struct(_, xs), Type::Struct(_, ys))
        | (Type::Enum(_, xs), Type::Enum(_, ys))
        | (Type::Actor(_, xs), Type::Actor(_, ys))
        | (Type::Interface(_, xs), Type::Interface(_, ys)) => {
            xs.iter().zip(ys).find_map(|(x, y)| clash(x, y))
        }
        (Type::Fun(xs, x), Type::Fun(ys, y)) => xs
            .iter()
            .zip(ys)
            .find_map(|(a, b)| clash(a, b))
            .or_else(|| clash(x, y)),
        _ => None,
    }
}

pub(in crate::check) fn branch_mismatch(
    span: Span,
    then: (Span, Type),
    else_: (Span, Type),
) -> Diagnostic {
    Diagnostic::error("E0403", "`if` and `else` have different types", span)
        .with_label(then.0, format!("this is {}", then.1))
        .with_label(else_.0, format!("this is {}", else_.1))
}

pub(in crate::check) fn bad_operands(op: &str, types: &[Type], span: Span) -> Diagnostic {
    let list = types
        .iter()
        .map(Type::to_string)
        .collect::<Vec<_>>()
        .join(" and ");
    Diagnostic::error("E0404", format!("cannot apply `{op}` to {list}"), span)
}

pub(in crate::check) fn arity(name: &str, expected: usize, given: usize, span: Span) -> Diagnostic {
    let s = if expected == 1 { "" } else { "s" };
    let were = if given == 1 { "was" } else { "were" };
    let message = format!("`{name}` takes {expected} argument{s} but {given} {were} given");
    Diagnostic::error("E0405", message, span)
}

pub(in crate::check) fn not_function(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error("E0406", format!("`{name}` is not a function"), span)
}

pub(in crate::check) fn intrinsic_argument(name: &str, found: Type, span: Span) -> Diagnostic {
    let expected = match name {
        "print" | "println" | "eprint" | "eprintln" => "a number, bool or string",
        "to_string" => "a number or bool",
        _ => "a string",
    };
    Diagnostic::error(
        "E0403",
        format!("`{name}` expects {expected}, found {found}"),
        span,
    )
}

pub(in crate::check) fn swap_types(first: Type, second: Type, span: Span, at: Span) -> Diagnostic {
    Diagnostic::error(
        "E0403",
        format!("`swap` expects two places of the same type, found {first} and {second}"),
        span,
    )
    .with_label(at, format!("this is {first}"))
}

pub(in crate::check) fn no_field(name: &str, ty: Type, span: Span) -> Diagnostic {
    Diagnostic::error("E0410", format!("no field `{name}` on `{ty}`"), span)
}

pub(in crate::check) fn not_indexable(ty: Type, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0411",
        format!("cannot index into a value of type `{ty}`"),
        span,
    )
}

pub(in crate::check) fn empty_array(span: Span) -> Diagnostic {
    Diagnostic::error("E0412", "cannot infer the element type of `[]`", span)
        .with_help("annotate it: `let xs: i64[] = [];`")
}

/// E0411: a loop over `ty`, which isn't an iterator; `own_next` when it
/// has a method `next` but no `impl Iter`, which the help then names.
pub(in crate::check) fn not_iterable(ty: &Type, own_next: bool, span: Span) -> Diagnostic {
    let help = match own_next {
        true => format!(
            "`{ty}` has a method `next`, but a loop needs `impl Iter<T> for {ty} {{ fun next(mut self): Option<T> … }}`"
        ),
        false => "`for … of` walks arrays, strings, views, channels and any type with `impl Iter<T> for …`".to_string(),
    };
    Diagnostic::error("E0411", format!("cannot iterate over `{ty}`"), span).with_help(help)
}

pub(in crate::check) fn extern_array(span: Span) -> Diagnostic {
    Diagnostic::error("E0409", "arrays can't cross the C boundary", span)
        .with_help("pass a number or a string instead")
}

pub(in crate::check) fn not_callable(span: Span) -> Diagnostic {
    Diagnostic::error("E0406", "this expression is not a function", span)
}

pub(in crate::check) fn not_value(name: &str, span: Span, help: &str) -> Diagnostic {
    Diagnostic::error("E0406", format!("`{name}` can't be used as a value"), span).with_help(help)
}

pub(in crate::check) fn bad_cast(from: Type, to: Type, span: Span) -> Diagnostic {
    let d = Diagnostic::error("E0408", format!("cannot cast `{from}` to `{to}`"), span);
    match (from, to) {
        (Type::Bool, _) => d.with_help("write `if b { 1 } else { 0 }`"),
        (_, Type::Bool) => d.with_help("compare instead, e.g. `x != 0`"),
        _ => d,
    }
}

pub(in crate::check) fn extern_struct(span: Span) -> Diagnostic {
    Diagnostic::error("E0409", "this struct can't cross the C boundary", span).with_help(
        "declare it `extern struct` to give it C's layout (numbers, `bool`, handles and other extern structs), or pass the fields one by one",
    )
}

pub(in crate::check) fn missing_fields(missing: &[&str], name: &str, span: Span) -> Diagnostic {
    let s = if missing.len() == 1 { "" } else { "s" };
    let list = missing
        .iter()
        .map(|f| format!("`{f}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Diagnostic::error(
        "E0413",
        format!("missing field{s} {list} in `{name}`"),
        span,
    )
}

pub(in crate::check) fn field_given_twice(field: &str, span: Span, first: Span) -> Diagnostic {
    Diagnostic::error("E0414", format!("field `{field}` is given twice"), span)
        .with_label(first, "first given here")
}

pub(in crate::check) fn no_variant(variant: &str, ty: &Type, span: Span) -> Diagnostic {
    Diagnostic::error("E0415", format!("no variant `{variant}` on `{ty}`"), span)
}

/// A bare name in a pattern that names a variant of the matched type.
pub(in crate::check) fn undotted_variant(variant: &str, ty: &Type, span: Span) -> Diagnostic {
    let message =
        format!("a variant in a pattern takes a leading dot: `{variant}` is a variant of `{ty}`");
    Diagnostic::error("E0468", message, span).with_help(format!(
        "write `.{variant}`; a bare name in a pattern is always a new binding"
    ))
}

pub(in crate::check) fn payload_count(
    what: &str,
    expected: usize,
    given: usize,
    span: Span,
) -> Diagnostic {
    let s = if expected == 1 { "" } else { "s" };
    Diagnostic::error(
        "E0416",
        format!(
            "`{what}` has {expected} payload{s}, but {given} {} given",
            if given == 1 { "was" } else { "were" }
        ),
        span,
    )
}

pub(in crate::check) fn cannot_infer_none(span: Span) -> Diagnostic {
    Diagnostic::error("E0412", "cannot infer the type of `None`", span)
        .with_help("annotate it: `let x: Option<i64> = None;`")
}

/// E0417 listing at most three missing cases, then "and N more".
pub(in crate::check) fn non_exhaustive(missing: &[String], span: Span) -> Diagnostic {
    let mut list = missing
        .iter()
        .take(3)
        .map(|m| format!("`{m}`"))
        .collect::<Vec<_>>()
        .join(", ");
    if missing.len() > 3 {
        list += &format!(" and {} more", missing.len() - 3);
    }
    Diagnostic::error(
        "E0417",
        format!("non-exhaustive match: {list} not covered"),
        span,
    )
    .with_help("add the missing arms, or `_ => …`")
}

pub(in crate::check) fn non_exhaustive_values(ty: &Type, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0417",
        format!("non-exhaustive match: other values of `{ty}` are not covered"),
        span,
    )
    .with_help("add `_ => …` as the last arm")
}

pub(in crate::check) fn pattern_mismatch(ty: &Type, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0403",
        format!("this pattern can't match a value of type `{ty}`"),
        span,
    )
}

pub(in crate::check) fn extern_enum(span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0409",
        "enums and `Option` can't cross the C boundary",
        span,
    )
    .with_help("pass a number or a string instead")
}

pub(in crate::check) fn no_method(
    name: &str,
    ty: &Type,
    span: Span,
    help: Option<String>,
) -> Diagnostic {
    let d = Diagnostic::error("E0415", format!("no method `{name}` on `{ty}`"), span);
    match help {
        Some(help) => d.with_help(help),
        None => d,
    }
}

/// E0422: a printed or interpolated value whose type has no `Display`.
pub(in crate::check) fn not_display(ty: &Type, span: Span) -> Diagnostic {
    let help = match ty {
        Type::Param(p) => {
            format!("add the bound `{p}: Display`, or show its debug form with `.debug()`")
        }
        Type::Struct(k, _) | Type::Enum(k, _) | Type::Handle(k)
            if !k.starts_with('$') && super::super::tuples::elements(ty).is_none() =>
        {
            // Written in the type's own module, where its name is bare.
            let bare = k.rsplit('.').next().unwrap_or(k);
            let shown = ty.to_string();
            let args = shown.find('<').map_or("", |at| &shown[at..]);
            format!(
                "write `impl Display for {bare}{args} {{ fun show(self): string = … }}`, or show its debug form with `.debug()`"
            )
        }
        _ => "it has no text of its own: show its debug form with `.debug()`".to_string(),
    };
    Diagnostic::error("E0422", format!("`{ty}` doesn't implement `Display`"), span).with_help(help)
}

/// E0409: a function type in an extern signature: C can't call Lugha functions.
pub(in crate::check) fn extern_fun(span: Span) -> Diagnostic {
    Diagnostic::error("E0409", "a function can't come back from C", span).with_help(
        "an extern may take a callback, `cb: fun(i64)`, but can't return one or take one inside another",
    )
}

/// E0412: a closure parameter whose type nothing gives.
pub(in crate::check) fn cannot_infer_param(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0412",
        format!("can't infer the type of parameter `{name}`"),
        span,
    )
    .with_help(format!(
        "write it: `{name}: i64`, or pass the closure where a function type is expected"
    ))
}

/// E0412: `return` in a closure whose return type isn't known yet.
pub(in crate::check) fn cannot_infer_return(span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0412",
        "can't infer the return type of a closure that uses `return`",
        span,
    )
    .with_help("write the return type: `fun(x: i64): i64 { … }`")
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::diagnostics;

    #[test]
    fn an_actors_state_and_handle_are_told_apart() {
        let found = diagnostics(
            "actor Holder {
                n: i64,
                fun new(): Holder = Holder { n: 0 };
            }
            fun main() {}",
        );
        let d = &found[0];
        assert_eq!(d.code, "E0403");
        assert_eq!(d.message, "expected Holder, found Holder");
        let help = d.help.as_deref().unwrap_or("");
        assert!(
            help.contains("inside `actor Holder`, `Self` is its state and `Holder` a handle to it, as everywhere; the code expects a handle to it but has its state; write `Self` where the state is meant"),
            "{help}"
        );
    }

    #[test]
    fn other_mismatches_get_no_such_help() {
        let found = diagnostics("fun main() { let x: i64 = \"a\"; }");
        assert!(
            found[0]
                .help
                .as_deref()
                .is_none_or(|h| !h.contains("actor"))
        );
    }
}
