//! Qualified calls: `Shape.area(c)` calls item `area` of interface
//! `Shape` on `c`, whatever else `c`'s type has by that name. The first
//! argument is the receiver; it must implement the interface (E0422).

use super::applied::Applied;
use super::assign::PlaceUse;
use super::expr::Expect;
use super::methods::Method;
use super::{Checker, Type, errors};
use crate::ast::{Bound, Expr, ExprKind, Ident};

impl Checker {
    /// The key of the interface `base` names, `Shape` or `m.Shape`, unless
    /// a local has that name.
    pub(super) fn interface_named_by(&self, base: &Expr) -> Option<String> {
        let (module, name) = match &base.kind {
            ExprKind::Name(n) if self.local(n).is_none() => (None, n.clone()),
            ExprKind::Field(m, n) => match &m.kind {
                ExprKind::Name(module) if self.local(module).is_none() => {
                    let module = Ident {
                        name: module.clone(),
                        span: m.span,
                    };
                    (Some(module), n.name.clone())
                }
                _ => return None,
            },
            _ => return None,
        };
        let bound = Bound {
            module,
            name: Ident {
                name,
                span: base.span,
            },
            args: Vec::new(),
            span: base.span,
        };
        self.interface_key(&bound)
    }

    /// `I.name(x, args…)` for interface `key`: `name` must be a method of
    /// `I` (E0415) and `x` must meet `I` (E0422).
    pub(super) fn qualified_call(
        &mut self,
        call: &Expr,
        key: &str,
        name: &Ident,
        args: &[Expr],
        expect: Option<&Expect>,
    ) -> Type {
        let shown = Type::Interface(key.to_string(), Vec::new());
        let req = self
            .interfaces
            .get(key)
            .and_then(|i| i.items.iter().find(|r| r.name == name.name))
            .cloned();
        let Some(req) = req.filter(|r| r.receiver.is_some()) else {
            let help = format!(
                "`{}.{}(x, …)` calls a method of the interface on `x`",
                shown, name.name
            );
            self.report(errors::no_method(&name.name, &shown, name.span, Some(help)));
            return self.unchecked(args);
        };
        let Some((first, rest)) = args.split_first() else {
            let d = errors::arity(&name.name, req.params.len() + 1, 0, call.span);
            self.report(d);
            return Type::Error;
        };
        let receiver = self.value(first, None);
        let Some(applied) = self.met(&receiver, key, first) else {
            return self.unchecked(rest);
        };
        self.qualified.insert(call.id);
        let method = match &receiver {
            Type::Param(p) => self.bound_item_call(call, p, &applied, &name.name),
            Type::Interface(..) => match self.through_interface(call, &applied, name) {
                Some(m) => m,
                None => return self.unchecked(rest),
            },
            _ if let Some((block, targs)) = self.builtin_block(key, &receiver) => {
                self.block_call(call, &receiver, (&block, &targs), &name.name)
            }
            _ => {
                let m = self.item_call(call, &receiver, &applied, &name.name);
                if m.generics.is_empty() && !m.key.is_empty() {
                    self.used(call, &m.key, &[]);
                }
                m
            }
        };
        let bound = match &receiver {
            Type::Struct(_, targs) | Type::Enum(_, targs) => targs.clone(),
            _ => Vec::new(),
        };
        let ret = self.arguments(call, &name.name, &method, rest, (&bound, expect));
        if method.receiver == Some(true) {
            self.check_place_for(first, PlaceUse::Receiver(&name.name));
        }
        ret
    }

    /// The interface `key`, at its arguments, as `ty` meets it: through a
    /// block, a bound, or as the interface's own value. E0422 otherwise,
    /// at `at`.
    fn met(&mut self, ty: &Type, key: &str, at: &Expr) -> Option<Applied> {
        let found = match ty {
            Type::Error | Type::Never => return None,
            Type::Param(p) => self.bounds(p).interfaces.into_iter().find(|a| a.key == key),
            Type::Interface(k, args) if k == key => Some(Applied {
                key: key.to_string(),
                args: args.clone(),
            }),
            Type::Struct(owner, targs) | Type::Enum(owner, targs) => self
                .impls
                .get(owner)
                .and_then(|es| es.iter().find(|e| e.key == key))
                .map(|e| Applied {
                    key: key.to_string(),
                    args: e.args_at(targs),
                }),
            Type::Handle(owner) => self
                .impls
                .get(owner)
                .and_then(|es| es.iter().find(|e| e.key == key))
                .map(|e| Applied {
                    key: key.to_string(),
                    args: e.args.clone(),
                }),
            _ => None,
        };
        // A built-in type through its block: `Show.show(2.5)`.
        let found = found.or_else(|| {
            let (block, args) = self.builtin_block(key, ty)?;
            Some(Self::block_applied(&block, &args))
        });
        if found.is_none() {
            let shown = Type::Interface(key.to_string(), Vec::new());
            self.report_not_implemented(ty, &shown.to_string(), at.span);
        }
        found
    }

    /// A qualified call of `name` through type parameter `param`'s bound
    /// `applied`: each instance calls the concrete type's item.
    fn bound_item_call(
        &mut self,
        call: &Expr,
        param: &str,
        applied: &Applied,
        name: &str,
    ) -> Method {
        let req = self
            .items_for(applied, &Type::Param(param.to_string()))
            .into_iter()
            .find(|r| r.name == name)
            .expect("the bound declares the item");
        self.bound_used(call, param, name);
        if self.nominal(&applied.key) {
            self.nominal_calls.insert(call.id, applied.clone());
        }
        self.default_callee(call, &req, applied.args.clone());
        Method {
            receiver: req.receiver,
            params: req.params,
            muts: req.muts,
            ret: req.ret,
            span: req.span,
            generics: Vec::new(),
            outer: 0,
            key: String::new(),
        }
    }
}
