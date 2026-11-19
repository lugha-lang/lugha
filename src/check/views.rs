//! Views of C's memory: `View<T>` reads it, `MutView<T>` writes it too,
//! each a pointer and a count of `T`s. A view lives only as long as the call
//! it was given to, since C decides when its memory goes: it may be a
//! parameter or a local, used, indexed, sliced and passed down, but never
//! stored, returned, captured, sent or made a type argument (E0483).

use super::{Checker, Type, errors};
use crate::ast::{Expr, Ident, Type as Annotation};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// What codegen finds `p.view(n, f)` by in `Checked::callees`.
pub const VIEW_OF: &str = "$CPtr.view";

/// True if `ty` is a view or holds one anywhere.
pub(super) fn holds_view(ty: &Type) -> bool {
    matches!(ty, Type::View(..)) || nested_view(ty)
}

/// True if a view is inside `ty`, where it could outlive its call: an
/// element, payload, type argument or result. A function's parameter may
/// be one: a view is passed as an argument.
fn nested_view(ty: &Type) -> bool {
    match ty {
        Type::Fun(params, ret) => holds_view(ret) || params.iter().any(nested_view),
        Type::Array(t) | Type::Option(t) | Type::Ref(t) | Type::Chan(_, t) | Type::Task(t) => {
            holds_view(t)
        }
        Type::Struct(_, ts) | Type::Enum(_, ts) | Type::Interface(_, ts) | Type::Actor(_, ts) => {
            ts.iter().any(holds_view)
        }
        _ => false,
    }
}

impl Checker {
    /// A written `View<T>` or `MutView<T>`; `None` when `name` is neither or
    /// the module declares its own. `T` is an integer, a float or an extern
    /// struct (E0409), whose fields `plain_elements` checks once every
    /// struct is known: C's bytes must be a valid `T` whatever they are.
    pub(super) fn resolve_view(
        &mut self,
        name: &str,
        args: &[Annotation],
        span: Span,
    ) -> Option<Type> {
        if !matches!(name, "View" | "MutView")
            || self.local(name).is_some()
            || self.named_type(&self.key(name)).is_some()
        {
            return None;
        }
        let [arg] = args else {
            self.report(errors::type_args(name, 1, args.len(), span));
            return Some(Type::Error);
        };
        let element = self.resolve(arg);
        let c_struct = matches!(&element, Type::Struct(k, _) if self.c_structs.contains(k));
        if !(element.is_numeric() || c_struct || element == Type::Error) {
            self.report(errors::view_element(&element, arg.span));
            return Some(Type::Error);
        }
        Some(Type::View(name == "MutView", Box::new(element)))
    }

    /// A value of type `ty` made at `span`: a view inside it is E0483.
    pub(super) fn view_in_value(&mut self, span: Span, ty: &Type) {
        if nested_view(ty) {
            let what = format!("this `{ty}` would hold one");
            self.report(errors::view_escapes(&what, span));
        }
    }

    /// A field, payload, captured local or declared result of type `ty`
    /// (`what` says which): a view there is E0483.
    pub(super) fn view_kept(&mut self, ty: &Type, what: &str, span: Span) {
        if holds_view(ty) {
            self.report(errors::view_escapes(what, span));
        }
    }

    /// `p.view(n, f)` on a `CPtr`: `f` called with a view of `n` `T`s at
    /// `p`, its parameter's written type saying which kind and `T`; the
    /// call's value is `f`'s.
    /// `None` for any other method.
    pub(super) fn cptr_view(
        &mut self,
        call: &Expr,
        (receiver, name): (&Type, &Ident),
        args: &[Expr],
    ) -> Option<Type> {
        if !matches!(receiver, Type::Handle(k) if k == "CPtr") || name.name != "view" {
            return None;
        }
        let [count, f] = args else {
            self.report(errors::arity(&name.name, 2, args.len(), call.span));
            return Some(self.unchecked(args));
        };
        // C's memory is outside the program: no constant reads it.
        self.effect("view", "uses C memory", name.span);
        self.expect_type(count, &super::expr::Expect::of(Type::U64));
        let ty = self.value(f, None);
        Some(match &ty {
            Type::Fun(params, ret) if matches!(params.as_slice(), [Type::View(..)]) => {
                self.callees.insert(call.id, VIEW_OF.to_string());
                (**ret).clone()
            }
            Type::Error | Type::Never => Type::Error,
            _ => {
                self.report(errors::view_function(&ty, f.span));
                Type::Error
            }
        })
    }
}

/// E0409 for each written view of an extern struct holding anything but
/// integers, floats and such structs (a `bool`, `char`, handle or `CPtr`):
/// C's bytes might not be a valid one.
pub(super) fn plain_elements(checker: &Checker) -> Vec<Diagnostic> {
    let mut found: Vec<(Span, Type)> = Vec::new();
    for (span, ty) in &checker.annotations {
        each_view_element(ty, &mut |element| {
            if let Type::Struct(key, _) = element
                && !plain(checker, key, &mut Vec::new())
            {
                found.push((*span, element.clone()));
            }
        });
    }
    // One error per place, in source order.
    found.sort_by_key(|(span, _)| (span.start, span.end));
    found.dedup_by_key(|(span, _)| *span);
    found
        .into_iter()
        .map(|(span, element)| errors::view_element(&element, span))
        .collect()
}

/// Calls `f` with the element type of every view in `ty`.
fn each_view_element(ty: &Type, f: &mut impl FnMut(&Type)) {
    match ty {
        Type::View(_, element) => f(element),
        Type::Fun(params, ret) => {
            params.iter().for_each(|p| each_view_element(p, f));
            each_view_element(ret, f);
        }
        _ => {}
    }
}

/// True if extern struct `key` holds only integers, floats and plain
/// extern structs, whatever their depth.
fn plain(checker: &Checker, key: &str, path: &mut Vec<String>) -> bool {
    if path.iter().any(|k| k == key) {
        return true;
    }
    let Some(info) = checker.structs.get(key) else {
        return false;
    };
    path.push(key.to_string());
    let ok = info.fields.iter().all(|(_, t)| match t {
        Type::Struct(k, _) if checker.c_structs.contains(k) => plain(checker, k, path),
        t => t.is_numeric(),
    });
    path.pop();
    ok
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type, ok};

    #[test]
    fn a_view_is_named_sliced_iterated_and_passed_down() {
        ok(
            "fun sum(v: View<i32>): i64 { let mut s = 0; for x of v { s += x as i64; } s }\n\
            fun fill(v: MutView<i32>) { for i in 0..v.len { v[i] = 1; } }\n\
            fun both(v: MutView<i32>): i64 { let head = v.slice(0, 2); fill(v); sum(head) + sum(v) }\n\
            fun main() {}",
        );
    }

    #[test]
    fn a_cptr_view_is_its_functions_value() {
        let src = "fun main() { let n = CPtr.null.view(0, fun(v: View<u8>) = v.len); }";
        assert_eq!(let_type(src, "n"), Type::I64);
        assert_eq!(
            errors("fun main() { CPtr.null.view(0, fun(n: i64) = n); }"),
            [("E0403", "fun(n: i64) = n")]
        );
    }

    #[test]
    fn a_view_never_leaves_its_call() {
        let codes = |src: &str| -> Vec<&'static str> {
            errors(src).into_iter().map(|(code, _)| code).collect()
        };
        let cases = [
            "fun f(v: View<u8>): View<u8> = v;",
            "fun f(v: View<u8>) { let o = Some(v); }",
            "fun f(v: View<u8>) { let g = fun(): u8 = v[0]; }",
            "fun id<T>(x: T): T = x;\nfun f(v: View<u8>) { let w = id(v); }",
            "struct S { v: View<u8> }",
        ];
        for case in cases {
            let found = codes(&format!("{case}\nfun main() {{}}"));
            assert!(found.contains(&"E0483"), "{case}: {found:?}");
        }
    }

    #[test]
    fn a_views_elements_are_plain_data() {
        ok("extern struct P { a: i32, b: f64 }\nfun f(v: View<P>) {}\nfun main() {}");
        assert_eq!(
            errors("fun f(v: View<bool>) {}\nfun main() {}"),
            [("E0409", "bool")]
        );
    }
}
