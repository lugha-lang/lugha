//! Function types as interfaces: `fun(A, B): R` in a bound, an interface's
//! list of what it requires, or an `impl … for` header is the prelude's
//! `$Fun2<A, B, R>`, whose one method, `call`, is the call. Every function
//! value implements its own signature's, and a value whose type has `call`
//! through one of them is called as a function: `f(x)` is `f.call(x)`.

use super::{Checker, FunOrigin, OwnFun, Type};
use crate::ast::{Expr, ExprKind, Ident};

/// `callee(args)` as `callee.call(args)`: the member call has the call's
/// id, as a qualified call's rewriting does, so codegen rebuilds the same.
pub fn through_call(call: &Expr, callee: &Expr) -> Expr {
    let name = Ident {
        name: "call".to_string(),
        span: callee.span,
    };
    Expr {
        kind: ExprKind::Field(Box::new(callee.clone()), name),
        ..call.clone()
    }
}

/// The prefix of the prelude's function interfaces, `$Fun0` to `$Fun8`.
const PREFIX: &str = "$Fun";

/// True if `name` is one of the prelude's function interfaces.
pub(super) fn is_fun_interface(name: &str) -> bool {
    name.strip_prefix(PREFIX)
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// True for the key of one of the prelude's function interfaces.
pub(super) fn is_fun_key(key: &str) -> bool {
    key.strip_prefix("$prelude.").is_some_and(is_fun_interface)
}

/// The key of the prelude's function interface for `arity` parameters.
pub(super) fn fun_interface_key(arity: usize) -> String {
    format!("$prelude.{PREFIX}{arity}")
}

/// The parameters and result a function interface's arguments describe, if
/// `key` is one: `$Fun2<A, B, R>` gives `([A, B], R)`.
pub(super) fn fun_signature<'a>(key: &str, args: &'a [Type]) -> Option<(&'a [Type], &'a Type)> {
    let name = key.strip_prefix("$prelude.")?;
    if !is_fun_interface(name) {
        return None;
    }
    let (ret, params) = args.split_last()?;
    Some((params, ret))
}

impl Checker {
    /// True if a value of type `ty` is called through `call`: a type
    /// parameter, an interface value or a struct with a function
    /// interface's `call`.
    pub(super) fn called_through_call(&self, ty: &Type, at: crate::span::Span) -> bool {
        let is_fun = |key: &str| key.strip_prefix("$prelude.").is_some_and(is_fun_interface);
        match ty {
            Type::Param(name) => self
                .bounds(name)
                .interfaces
                .iter()
                .any(|a| self.ancestors(&a.key).iter().any(|k| is_fun(k))),
            Type::Interface(key, _) => self.ancestors(key).iter().any(|k| is_fun(k)),
            Type::Struct(..) | Type::Enum(..) => self
                .implemented_with(ty, "call", at)
                .iter()
                .any(|a| is_fun(&a.key)),
            _ => false,
        }
    }

    /// What a function value of signature `(params, ret)` lacks to be a
    /// value of `interface`: nothing when every interface it is or extends
    /// is that signature's function interface or declares no items.
    pub(super) fn fun_unmet(
        &self,
        (params, ret): (&[Type], &Type),
        interface: &super::applied::Applied,
    ) -> Vec<String> {
        let mut todo = vec![interface.clone()];
        let mut met = false;
        let mut problems = Vec::new();
        while let Some(a) = todo.pop() {
            if let Some(wanted) = fun_signature(&a.key, &a.args) {
                if wanted == (params, ret) {
                    met = true;
                } else {
                    problems.push(format!("it isn't a `{}`", a.ty()));
                }
                continue;
            }
            let Some(info) = self.interfaces.get(&a.key) else {
                continue;
            };
            if !info.items.is_empty() {
                problems.push(format!("a function has no `{}` items", a.ty()));
            }
            todo.extend(info.extends.iter().cloned());
        }
        if !met && problems.is_empty() {
            problems.push(format!("`{}` isn't a function interface", interface.ty()));
        }
        problems
    }

    /// `expr`, of struct or enum type `ty`, where function type `wanted`
    /// is expected: converted when `ty` implements that signature's
    /// interface (E0422 otherwise).
    pub(super) fn fun_value_of(
        &mut self,
        expr: &Expr,
        ty: &Type,
        (params, ret): (&[Type], &Type),
        wanted: &Type,
    ) -> Type {
        let mut args = params.to_vec();
        args.push(ret.clone());
        let interface = super::applied::Applied {
            key: fun_interface_key(params.len()),
            args,
        };
        if !self.satisfies(ty, &interface, expr.span) {
            return Type::Error;
        }
        self.conversions.insert(expr.id, wanted.clone());
        self.conversion_uses(expr, ty, &interface);
        wanted.clone()
    }

    /// A closure literal, typed `ty`, outside generic code: which function
    /// it is, for an instance that takes it.
    pub(super) fn closure_origin(&mut self, expr: &Expr, ty: &Type) {
        if matches!(ty, Type::Fun(..)) && self.type_params.is_empty() {
            // Numbered within its function, in the order closures are met.
            let earlier = self
                .fun_origins
                .values()
                .filter(|o| matches!(o, FunOrigin::Closure(_, w) if w.ends_with(&format!(" in {}", self.owner))))
                .map(|o| match o {
                    FunOrigin::Closure(id, _) => *id,
                    FunOrigin::Named(_) => 0,
                })
                .collect::<std::collections::HashSet<_>>()
                .len();
            let label = format!("closure {} in {}", earlier + 1, self.owner);
            self.fun_origins
                .insert(expr.id, FunOrigin::Closure(expr.id.0, label));
        }
    }

    /// Function `key` used as a value, typed `ty`: which function, unless
    /// it is generic (its instance is chosen by context).
    pub(super) fn named_origin(&mut self, expr: &Expr, key: &str, ty: &Type) {
        let plain = self
            .functions
            .get(key)
            .is_some_and(|f| f.generics.is_empty());
        if matches!(ty, Type::Fun(..)) && plain && !self.instances.contains_key(&expr.id) {
            self.fun_origins
                .insert(expr.id, FunOrigin::Named(key.to_string()));
        }
    }

    /// `let name = init;`, immutable and without a written type: the local
    /// holds the function `init` does, if known.
    pub(super) fn local_origin(&mut self, erased: bool, name: &Ident, init: &Expr) {
        if erased {
            return;
        }
        if let Some(origin) = self.fun_origins.get(&init.id).cloned() {
            self.local_origins.insert(name.span, origin);
        }
    }

    /// A read of the local declared at `declared`.
    pub(super) fn read_origin(&mut self, expr: &Expr, declared: crate::span::Span) {
        if let Some(origin) = self.local_origins.get(&declared).cloned() {
            self.fun_origins.insert(expr.id, origin);
        }
    }

    /// `found`, the type of argument `e` for a parameter of type `want`:
    /// the function's own type when `want` is one of `own`, the type
    /// parameters taken as exact functions, and `e` is a known function.
    pub(super) fn own_input(&self, e: &Expr, want: &Type, found: Type, own: &[&str]) -> Type {
        let (Type::Param(name), Type::Fun(params, ret)) = (want, &found) else {
            return found;
        };
        match self.fun_origins.get(&e.id) {
            Some(origin) if own.contains(&name.as_str()) => Type::Own(Box::new(OwnFun {
                origin: origin.clone(),
                params: params.clone(),
                ret: (**ret).clone(),
            })),
            _ => found,
        }
    }

    /// Of `generics`, those an argument may make an exact function: bound
    /// by a function interface, and not part of the call's result, so an
    /// own type never leaves the instance.
    pub(super) fn own_params<'a>(
        &self,
        generics: &'a [super::generics::Generic],
        result: Option<&Type>,
    ) -> Vec<&'a str> {
        let is_fun = |key: &str| key.strip_prefix("$prelude.").is_some_and(is_fun_interface);
        generics
            .iter()
            .filter(|g| {
                g.bounds
                    .interfaces
                    .iter()
                    .any(|a| self.ancestors(&a.key).iter().any(|k| is_fun(k)))
            })
            .filter(|g| result.is_none_or(|r| !r.mentions_param(&g.name)))
            .map(|g| g.name.as_str())
            .collect()
    }

    /// `key` and every interface it extends, transitively.
    fn ancestors(&self, key: &str) -> Vec<String> {
        let mut seen = vec![key.to_string()];
        let mut at = 0;
        while at < seen.len() {
            let parents: Vec<String> = self
                .interfaces
                .get(&seen[at])
                .map(|i| i.extends.iter().map(|p| p.key.clone()).collect())
                .unwrap_or_default();
            for p in parents {
                if !seen.contains(&p) {
                    seen.push(p);
                }
            }
            at += 1;
        }
        seen
    }

    /// `f(args)` where `f` has `call`: checked as `f.call(args)`.
    pub(super) fn call_through(&mut self, call: &Expr, callee: &Expr, args: &[Expr]) -> Type {
        let member = through_call(call, callee);
        match self.method_call(call, &member, args, None) {
            Some(ty) => ty,
            None => self.unchecked_args(args),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fun_signature, is_fun_interface};
    use crate::check::test_util::{errors, let_type, ok};
    use crate::check::{FunOrigin, Type};

    const TWICE: &str = "fun twice<F: fun(i64): i64>(f: F, x: i64): i64 = f(f(x));\n";

    /// The type arguments the checker chose for the first call of `name`.
    fn instance(src: &str, name: &str) -> Vec<Type> {
        let checked = ok(src);
        let key = name.to_string();
        let mut found: Vec<(u32, Vec<Type>)> = checked
            .instances
            .iter()
            .filter(|(id, _)| checked.callees.get(id) == Some(&key))
            .map(|(id, args)| (id.0, args.clone()))
            .collect();
        found.sort_by_key(|(id, _)| *id);
        found.into_iter().next().map(|(_, a)| a).unwrap_or_default()
    }

    #[test]
    fn a_known_function_is_its_own_type_argument() {
        let src = format!(
            "{TWICE}fun double(x: i64): i64 = x * 2;\nfun main() {{ let k = 3; let f = fun(x: i64): i64 = x * k; let a = twice(f, 1); }}"
        );
        let [Type::Own(own)] = &instance(&src, "twice")[..] else {
            panic!("an own type")
        };
        assert!(
            matches!(&own.origin, FunOrigin::Closure(_, label) if label == "closure 1 in main")
        );
        let src = format!(
            "{TWICE}fun double(x: i64): i64 = x * 2;\nfun main() {{ let a = twice(double, 1); }}"
        );
        let [Type::Own(own)] = &instance(&src, "twice")[..] else {
            panic!("an own type")
        };
        assert_eq!(own.origin, FunOrigin::Named("double".into()));
    }

    #[test]
    fn a_function_typed_value_stays_the_interface() {
        // A parameter, an annotated local and a `let mut` hide which function it is.
        let fun = Type::Fun(vec![Type::I64], Box::new(Type::I64));
        let src = format!("{TWICE}fun run(g: fun(i64): i64): i64 = twice(g, 1);\nfun main() {{}}");
        assert_eq!(instance(&src, "twice"), vec![fun.clone()]);
        let src = format!(
            "{TWICE}fun main() {{ let g: fun(i64): i64 = fun(x: i64): i64 = x; let a = twice(g, 1); }}"
        );
        assert_eq!(instance(&src, "twice"), vec![fun.clone()]);
        let src = format!(
            "{TWICE}fun main() {{ let mut g = fun(x: i64): i64 = x; g = fun(x: i64): i64 = x + 1; let a = twice(g, 1); }}"
        );
        assert_eq!(instance(&src, "twice"), [fun]);
    }

    #[test]
    fn function_values_still_meet_as_their_signature() {
        let fun = Type::Fun(vec![Type::I64], Box::new(Type::I64));
        let src = "fun main() { let k = 1; let a = fun(x: i64): i64 = x + k; let b = fun(x: i64): i64 = x; let fs = [a, b]; let c = if k > 0 { a } else { b }; }";
        assert_eq!(let_type(src, "fs"), Type::Array(Box::new(fun.clone())));
        assert_eq!(let_type(src, "c"), fun);
    }

    #[test]
    fn interfaces_and_structs_may_be_functions() {
        ok("interface Pricing: fun(i64): i64 {}
            struct Scale { k: i64 }
            impl fun(i64): i64 for Scale { fun call(self, x: i64): i64 = x * self.k; }
            fun total(p: Pricing): i64 = p(1) + p(2);
            fun apply(f: fun(i64): i64): i64 = f(3);
            fun main() { let s = Scale { k: 2 }; let a = s(1) + apply(s) + total(fun(x: i64): i64 = x); }");
    }

    #[test]
    fn what_isnt_that_function_is_e0422() {
        assert_eq!(
            errors(&format!("{TWICE}fun main() {{ let a = twice(5, 1); }}")),
            [("E0422", "twice(5, 1)")]
        );
        assert_eq!(
            errors("interface Named { fun name(self): string; }\ninterface Job: fun(), Named {}\nfun run(j: Job) {}\nfun main() { run(fun() {}); }")[0].0,
            "E0422"
        );
        assert_eq!(
            errors("struct S {}\nfun apply(f: fun(i64): i64): i64 = f(3);\nfun main() { let a = apply(S {}); }")[0].0,
            "E0403"
        );
    }

    #[test]
    fn function_interfaces_are_named_by_arity() {
        assert!(is_fun_interface("$Fun0"));
        assert!(is_fun_interface("$Fun8"));
        assert!(!is_fun_interface("$Fun"));
        assert!(!is_fun_interface("Fun1"));
        let args = [Type::I64, Type::Bool];
        assert_eq!(
            fun_signature("$prelude.$Fun1", &args),
            Some((&[Type::I64][..], &Type::Bool))
        );
        assert_eq!(fun_signature("$prelude.Eq", &args), None);
    }
}
