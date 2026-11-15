//! Derive blocks' diagnostics: E0451 for a
//! type that doesn't meet a derivable interface, E0452 for a misplaced or
//! overlapping block, E0453 for a misused field intrinsic. A block's item
//! that doesn't match its interface is E0422, as for any type. E0450 is a type
//! that `json.Json` needs to be data but isn't.

use crate::check::Type;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// E0451: `ty` doesn't meet derivable interface `interface`; `path` leads
/// to the part at fault (empty for `ty` itself), `what` says why, and
/// `covered` lists the types the interface has blocks for.
pub(in crate::check) fn not_derived(
    ty: &Type,
    interface: &str,
    (path, what): (&str, &str),
    covered: &[String],
    span: Span,
) -> Diagnostic {
    let place = if path.is_empty() {
        "it is".to_string()
    } else {
        format!("`{}` is", path.trim_start_matches('.'))
    };
    Diagnostic::error(
        "E0451",
        format!("`{ty}` doesn't meet `{interface}`: {place} {what}"),
        span,
    )
    .with_help(format!("`{interface}` has blocks for {}", list(covered)))
}

/// `a`, `a and b`, `a, b and c`.
fn list(items: &[String]) -> String {
    match items {
        [] => "nothing".to_string(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// E0452: a block for an interface this module doesn't declare.
pub(in crate::check) fn derive_elsewhere(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0452",
        format!("a `derive` block for `{name}` must be in the module that declares `{name}`"),
        span,
    )
    .with_help("only an interface's own module says how types meet it; a type's own methods can replace an item instead")
}

/// E0452: two blocks of `interface` that both cover `ty`.
pub(in crate::check) fn derive_overlap(interface: &str, ty: &str, span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0452",
        format!("two `derive {interface}` blocks cover `{ty}`"),
        span,
    )
    .with_help("each type gets its items from one block: remove or narrow one of them")
}

/// E0422: a block whose items don't match its interface's; `problems` as
/// for any type that doesn't satisfy one.
pub(in crate::check) fn block_unsatisfied(
    block: &str,
    interface: &str,
    problems: &[String],
    span: Span,
) -> Diagnostic {
    Diagnostic::error(
        "E0422",
        format!("`{block}` doesn't match `{interface}`"),
        span,
    )
    .with_help(problems.join("; "))
}

/// E0453: a field intrinsic used wrongly; `why` says how.
pub(in crate::check) fn bad_intrinsic(op: &str, why: &str, span: Span) -> Diagnostic {
    Diagnostic::error("E0453", format!("`derive.{op}` {why}"), span).with_help(
        "`derive.fields` and `derive.build` work in `derive … for struct` and `for tuple` blocks; `derive.variant`, `variants`, `build_variant` and `index` in `for enum` blocks; `derive.fold` in all three; `derive.address` in `for handle` blocks; `f` names a function of this module generic over one `F` with the block's interface",
    )
}

/// E0453: a local named `derive` inside a derive block, where `derive.` means the intrinsics.
pub(in crate::check) fn derive_local(span: Span) -> Diagnostic {
    Diagnostic::error(
        "E0453",
        "a local can't be named `derive` inside a derive block",
        span,
    )
    .with_help("inside a block `derive.` means the field intrinsics: rename the local")
}

/// A type where `json.Json` is required that holds something that isn't
/// data: `path` leads to it (empty for the type itself).
pub(in crate::check) fn not_json(ty: &Type, path: &str, what: &str, span: Span) -> Diagnostic {
    let place = if path.is_empty() {
        "it is".to_string()
    } else {
        format!("`{}` is", path.trim_start_matches('.'))
    };
    Diagnostic::error(
        "E0450",
        format!("`{ty}` can't be turned into JSON: {place} {what}"),
        span,
    )
    .with_help(
        "JSON holds data: numbers, `bool`, `char`, `string`, arrays, tuples, maps with string or integer keys, sets, `Option`, and structs and enums of these that ask for it with `with json.Json`",
    )
}

/// E0451: `ty` isn't `Key` because of a float, at `path` inside it.
pub(in crate::check) fn float_key(ty: &Type, path: &str, span: Span) -> Diagnostic {
    let (place, instead) = if path.is_empty() {
        ("it is a float".to_string(), "use an integer")
    } else {
        (
            format!("`{}` is a float", path.trim_start_matches('.')),
            "key by the other parts, or hold the number as an integer (cents, `x.to_bits()`)",
        )
    };
    Diagnostic::error("E0451", format!("`{ty}` can't be a key: {place}"), span).with_help(format!(
        "floats compare with `==` but aren't `Key`: `NaN != NaN`, so a float key could never be found again; {instead}"
    ))
}

/// E0422: `ty` meets derivable interface `interface` only by asking, and
/// `part`, at `path` inside it (or `ty` itself), doesn't.
pub(in crate::check) fn not_asked(
    ty: &Type,
    part: &Type,
    (path, interface): (&str, &str),
    span: Span,
) -> Diagnostic {
    let message = if path.is_empty() {
        format!("`{ty}` doesn't implement `{interface}`")
    } else {
        let at = path.trim_start_matches('.');
        format!("`{ty}` doesn't implement `{interface}`: `{at}` is a `{part}`, which doesn't")
    };
    // The declaration's name, without the arguments of this use.
    let declared = match part {
        Type::Struct(k, _) | Type::Enum(k, _) => k.rsplit('.').next().unwrap_or(k).to_string(),
        _ => part.to_string(),
    };
    Diagnostic::error("E0422", message, span).with_help(format!(
        "add `with {interface}` after `{declared}`'s declaration, or implement it by hand with `impl {interface} for {declared}`"
    ))
}

/// E0462: `interface` can't be derived for `ty`, which is `kind` ("a
/// struct" or "an enum"); `covered` lists what its blocks cover, and
/// `by_name` says the interface is still met by methods' names.
pub(in crate::check) fn not_derivable(
    interface: &str,
    (ty, kind): (&str, &str),
    covered: &[String],
    by_name: bool,
    span: Span,
) -> Diagnostic {
    let help = if by_name {
        format!(
            "a type meets `{interface}` with its own methods of the interface's item names, in `impl {ty} {{ … }}`"
        )
    } else if covered.is_empty() {
        format!("`{interface}` has no derive blocks: write `impl {interface} for {ty} {{ … }}`")
    } else {
        format!(
            "`{interface}` has derive blocks for {} only: write `impl {interface} for {ty} {{ … }}`",
            list(covered)
        )
    };
    Diagnostic::error(
        "E0462",
        format!("`{interface}` can't be derived for `{ty}`, {kind}"),
        span,
    )
    .with_help(help)
}

/// E0463: `ty` both derives `interface` (at `span`) and implements it by
/// hand (at `written`).
pub(in crate::check) fn derived_and_written(
    ty: &str,
    interface: &str,
    span: Span,
    written: Span,
) -> Diagnostic {
    Diagnostic::error(
        "E0463",
        format!("`{ty}` both derives and implements `{interface}`"),
        span,
    )
    .with_label(written, "implemented by hand here")
    .with_help(format!(
        "keep one: remove `{interface}` from `with`, or the `impl` block"
    ))
}
