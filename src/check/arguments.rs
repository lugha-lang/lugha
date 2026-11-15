//! Arguments of method and associated-function calls: arity, then each argument, inferring the
//! method's type arguments.

use super::expr::Expect;
use super::generics::subst;
use super::methods::Method;
use super::{Checker, Type, errors};
use crate::ast::Expr;

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
        let bound = m
            .generics
            .iter()
            .map(|g| g.name.clone())
            .zip(outer.iter().cloned())
            .collect();
        match self.infer(call, &m.generics, bound, Some((&m.ret, expect)), &inputs) {
            Some(targs) => {
                self.settle_mut(&passed, args, (&m.params, &m.generics, &targs));
                self.used(call, &m.key, &targs);
                subst(&m.ret, &m.generics, &targs)
            }
            None => Type::Error,
        }
    }

    pub(super) fn unchecked(&mut self, args: &[Expr]) -> Type {
        for arg in args {
            self.expr(arg, None);
        }
        Type::Error
    }
}
