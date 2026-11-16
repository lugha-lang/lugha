//! Type arguments written at a use: `max<i64>(a, b)`, `xs.map<string>(f)`,
//! `Stack<i64>.new()`, `Pair<u8> { … }`, `Option<i64>.None`.
//!
//! The written arguments bind their parameters before inference runs, so
//! `_` and the rest of inference work as they always did. Whatever use takes
//! them marks them taken; any left over are E0418, since nothing there has
//! type parameters.

use super::generics::Generic;
use super::infer::Bound;
use super::{Checker, Type, errors};
use crate::ast::{Expr, ExprKind};

impl Checker {
    /// The parameters of `generics` that the type arguments written on `at`
    /// bind, marking them taken. A wrong count is E0418 and binds nothing.
    pub(super) fn written(&mut self, at: &Expr, name: &str, generics: &[Generic]) -> Bound {
        let mut bound = Bound::new();
        let Some(written) = &at.type_args else {
            return bound;
        };
        self.written_used.insert(at.id);
        if written.args.len() != generics.len() {
            let (expected, given) = (generics.len(), written.args.len());
            self.report(errors::type_args(name, expected, given, written.span));
            return bound;
        }
        for (g, arg) in generics.iter().zip(&written.args) {
            // `_` leaves the parameter to inference.
            let Some(arg) = arg else { continue };
            let ty = self.resolve(arg);
            if ty != Type::Error {
                bound.insert(g.name.clone(), ty);
            }
        }
        bound
    }

    /// The one type argument written on a built-in generic type's name,
    /// `T` in `Chan<T>.new()`, `Ref<T>.new(v)` or `Option<T>.None`, marking
    /// it taken; `None` when none (or `_`) is written.
    pub(super) fn written_one(&mut self, base: &Expr, name: &str) -> Option<Type> {
        let param = [Generic {
            name: "T".to_string(),
            bounds: Default::default(),
        }];
        self.written(base, name, &param).remove("T")
    }

    /// E0418 for type arguments written on `e`, or on the callee and bases
    /// it is made of, that no use took. Nothing is reported when `e` already
    /// failed: its error says more.
    pub(super) fn untaken_type_args(&mut self, e: &Expr, ty: &Type) {
        let mut at = Some(e);
        while let Some(x) = at {
            if let Some(written) = &x.type_args
                && self.written_used.insert(x.id)
                && *ty != Type::Error
            {
                let name = written_name(x);
                self.report(errors::type_args(
                    &name,
                    0,
                    written.args.len(),
                    written.span,
                ));
            }
            at = match &x.kind {
                ExprKind::Call(callee, _) => Some(callee),
                ExprKind::Field(base, _) => Some(base),
                _ => None,
            };
        }
    }
}

/// The name type arguments were written after, for messages.
fn written_name(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Name(name) => name.clone(),
        ExprKind::Field(_, member) => member.name.clone(),
        ExprKind::StructLit(literal) => literal.name.name.clone(),
        _ => "this".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type, ok};

    const LIB: &str = "fun pick<T>(xs: T[]): Option<T> = if xs.len > 0 { Some(xs[0]) } else { None };\n\
        fun two<A, B>(a: A, b: B): A = a;\n\
        struct Pair<T> { a: T, b: T }\n\
        struct Stack<T> { items: T[] }\n\
        impl Stack<T> { fun new(): Stack<T> = Stack { items: [] }; }\n";

    fn ty(body: &str, name: &str) -> Type {
        let_type(&format!("{LIB}fun main() {{ {body} }}"), name)
    }

    fn e(body: &str) -> Vec<(&'static str, String)> {
        let src = format!("{LIB}fun main() {{ {body} }}");
        let found = errors(&src);
        found.into_iter().map(|(c, t)| (c, t.to_string())).collect()
    }

    #[test]
    fn written_arguments_bind_before_inference() {
        let opt = |t: Type| Type::Option(Box::new(t));
        assert_eq!(ty("let p = pick<string>([]);", "p"), opt(Type::String));
        assert_eq!(ty("let n = two<u8, _>(1, \"x\");", "n"), Type::U8);
        let s = Type::Struct("Stack".into(), vec![Type::F64]);
        assert_eq!(ty("let s = Stack<f64>.new();", "s"), s);
        let p = Type::Struct("Pair".into(), vec![Type::U8]);
        assert_eq!(ty("let p = Pair<u8> { a: 1, b: 2 };", "p"), p);
        assert_eq!(ty("let o = Option<i64>.None;", "o"), opt(Type::I64));
        assert_eq!(
            ty("let w = [1].map<string>(fun(n) = \"{n}\");", "w"),
            Type::Array(Box::new(Type::String))
        );
        ok(&format!(
            "{LIB}fun main() {{ let f = pick<i64>; let x = f([1]); }}"
        ));
    }

    #[test]
    fn a_wrong_count_or_an_unused_list_is_e0418() {
        assert_eq!(e("let n = two<u8>(1, 2);"), [("E0418", "<u8>".to_string())]);
        assert_eq!(e("let n = to_string<i64>(1);")[0].0, "E0418");
        assert_eq!(
            e("let s = \"ab\"; let n = s.len<i64>;"),
            [("E0418", "<i64>".to_string())]
        );
        assert_eq!(e("let p = pick<string>([1]);")[0].0, "E0401");
    }
}
