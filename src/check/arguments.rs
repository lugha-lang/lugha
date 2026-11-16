//! Arguments of method and associated-function calls: arity, then each argument, inferring the
//! method's type arguments.

use super::expr::Expect;
use super::generics::subst;
use super::infer::Bound;
use super::methods::Method;
use super::{Checker, Type, errors};
use crate::ast::{Expr, ExprKind};

impl Checker {
    /// Arity, then the arguments, inferring the method's type arguments
    /// after the `impl`'s are bound to `outer`.
    pub(super) fn arguments(
        &mut self,
        call: &Expr,
        name: &str,
        m: &Method,
        args: &[Expr],
        (outer, expect): (&[Type], Option<&Expect>),
    ) -> Type {
        let written = self.written_on_method(call, name, m);
        if args.len() != m.params.len() {
            self.report(errors::arity(name, m.params.len(), args.len(), call.span));
            self.unchecked(args);
            return if m.generics.is_empty() {
                m.ret.clone()
            } else {
                Type::Error
            };
        }
        let (inputs, passed) = self.mut_inputs(&m.muts, args, &m.params);
        let mut bound: super::infer::Bound = m
            .generics
            .iter()
            .map(|g| g.name.clone())
            .zip(outer.iter().cloned())
            .collect();
        bound.extend(written);
        match self.infer(call, &m.generics, bound, Some((&m.ret, expect)), &inputs) {
            Some(targs) => {
                self.settle_mut(&passed, args, (&m.params, &m.generics, &targs));
                self.used(call, &m.key, &targs);
                subst(&m.ret, &m.generics, &targs)
            }
            None => Type::Error,
        }
    }

    /// What the type arguments written on a method call bind: those after
    /// the method's name bind its own parameters (`xs.map<string>(f)`), and
    /// for an associated function those after the type's name bind the
    /// type's (`Stack<i64>.new()`).
    fn written_on_method(&mut self, call: &Expr, name: &str, m: &Method) -> Bound {
        let ExprKind::Call(callee, _) = &call.kind else {
            return Bound::new();
        };
        let mut bound = Bound::new();
        if let ExprKind::Field(base, _) = &callee.kind
            && m.receiver.is_none()
            && base.type_args.is_some()
        {
            let owner = written_type_name(base);
            bound.extend(self.written(base, &owner, &m.generics[..m.outer]));
        }
        bound.extend(self.written(callee, name, &m.generics[m.outer..]));
        bound
    }

    pub(super) fn unchecked(&mut self, args: &[Expr]) -> Type {
        for arg in args {
            self.expr(arg, None);
        }
        Type::Error
    }
}

/// The type's name in `Stack<i64>.new()` or `m.Stack<i64>.new()`.
fn written_type_name(base: &Expr) -> String {
    match &base.kind {
        ExprKind::Name(name) => name.clone(),
        ExprKind::Field(_, member) => member.name.clone(),
        _ => "this".to_string(),
    }
}
