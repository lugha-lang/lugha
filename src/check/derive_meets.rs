//! Meeting a derivable interface: a type
//! meets it through the derive block that covers it, when the block's
//! parameters meet their bounds and, for a struct, enum or tuple block,
//! every part meets the interface in turn (recursive types included).
//! A miss names the path to the first part that doesn't, as E0450 always has.

use std::collections::HashSet;

use super::derive::Covers;
use super::generics::subst;
use super::tuples::elements;
use super::{Checker, Type, errors};
use crate::span::Span;

/// Why a part doesn't meet a derivable interface.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Unmet {
    /// No block covers this type.
    Missing(Type),
    /// A type parameter without the interface among its bounds.
    Unbounded(String),
    /// A block's parameter misses another derivable bound: `Map<bool, V>`'s
    /// keys, for one.
    Bound(String, Type),
    /// A C union (`true`) or a struct holding one (`false`).
    Union(bool),
    /// A struct or enum that didn't ask for the interface (E0422).
    NotAsked(Type),
}

impl Checker {
    /// E0450 for `Json`, E0451 for any other interface, E0422 for a type
    /// that didn't ask for it, unless `arg` meets derivable interface
    /// `interface`.
    pub(super) fn meets_derived(&mut self, interface: &str, arg: &Type, span: Span) -> bool {
        let Some((path, why)) = self.unmet_derived(interface, arg, "", &mut HashSet::new()) else {
            return true;
        };
        // A type that didn't ask, but whose parts couldn't meet it if it did:
        // name that part, rather than suggest a `with` that would fail.
        let (path, why) = match &why {
            Unmet::NotAsked(Type::Struct(owner, _) | Type::Enum(owner, _)) => {
                let at = (owner.clone(), interface.to_string());
                self.asking.asked.insert(at.clone(), Span::new(0, 0));
                let asked = self.unmet_derived(interface, arg, "", &mut HashSet::new());
                self.asking.asked.remove(&at);
                match asked {
                    Some((inner, deeper)) if !matches!(deeper, Unmet::NotAsked(_)) => {
                        (inner, deeper)
                    }
                    _ => (path, why),
                }
            }
            _ => (path, why),
        };
        let name = interface.rsplit('.').next().unwrap_or(interface);
        let d = match &why {
            Unmet::NotAsked(part) => {
                let shown = super::derive_with::shown_interface(interface);
                errors::not_asked(arg, part, (&path, &shown), span)
            }
            _ if interface == super::json::BOUND => {
                errors::not_json(arg, &path, &super::json::describe(&why), span)
            }
            // Floats are `Eq` but never keys: say why, not what is.
            Unmet::Missing(Type::F32 | Type::F64) if interface == super::derive::KEY => {
                errors::float_key(arg, &path, span)
            }
            Unmet::Missing(float @ (Type::F32 | Type::F64)) if interface == super::derive::ORD => {
                errors::float_not_ord(arg, float, &path, span)
            }
            _ => {
                let covered = self.derives.covered(interface);
                let what = describe(name, &why);
                errors::not_derived(arg, name, (&path, &what), &covered, span)
            }
        };
        self.report(d);
        false
    }

    /// After every module's declarations: the types that implement a
    /// derivable interface by hand, so calls reach their block's methods
    /// instead of a derive block's. Such an `impl` supplies every item of
    /// the interface, its defaults included.
    pub(super) fn derive_owners(&mut self) {
        let mut own = Vec::new();
        for (owner, entries) in &self.impls {
            for e in entries {
                if !self.derives.derivable(&e.key) {
                    continue;
                }
                for r in &self.interfaces[&e.key].items {
                    own.push((owner.clone(), r.name.clone()));
                }
            }
        }
        self.derives.own.extend(own);
    }

    /// The path to the first part of `ty` that doesn't meet `interface`, and why.
    pub(super) fn unmet_derived(
        &self,
        interface: &str,
        ty: &Type,
        path: &str,
        seen: &mut HashSet<Type>,
    ) -> Option<(String, Unmet)> {
        let at = |why: Unmet| Some((path.to_string(), why));
        // Every type has a debug form.
        if matches!(ty, Type::Error | Type::Never)
            || interface == super::derive::DEBUG
            || !seen.insert(ty.clone())
        {
            return None;
        }
        // The kinds no `Sendable` block describes.
        if interface == super::sendable::SENDABLE
            && let Some(decided) = self.sendable_part(ty, path, seen)
        {
            return decided;
        }
        if let Type::Param(name) = ty {
            let bounds = self.bounds(name);
            // `T: Eq` (or `Num`) is written as a flag.
            let bound = bounds.interfaces.iter().any(|a| a.key == interface)
                || interface == super::derive::EQ && super::derive::implies_eq(&bounds);
            return if bound {
                None
            } else {
                at(Unmet::Unbounded(name.clone()))
            };
        }
        // A struct or enum that implements the interface by hand meets it.
        if let Type::Struct(owner, _) | Type::Enum(owner, _) = ty
            && self
                .impls
                .get(owner)
                .is_some_and(|es| es.iter().any(|e| e.key == interface))
        {
            return None;
        }
        let union = |key: &str| self.holds_union(key);
        let Some((block, args)) = self.derives.find(interface, ty, union) else {
            return match ty {
                Type::Struct(key, _) if self.holds_union(key) => {
                    at(Unmet::Union(self.c_unions.contains(key)))
                }
                _ => at(Unmet::Missing(ty.clone())),
            };
        };
        match &block.covers {
            Covers::Type(pattern) => {
                // An `Option` is its value's; anything else holds elements.
                let inner = match pattern {
                    Type::Option(_) => path.to_string(),
                    _ => format!("{path}[]"),
                };
                for (g, arg) in block.generics.iter().zip(&args) {
                    // `T: Eq` is written as the built-in flag.
                    let eq = g.bounds.eq.then(super::derive::eq_bound);
                    for b in g
                        .bounds
                        .interfaces
                        .iter()
                        .cloned()
                        .chain(eq)
                        .collect::<Vec<_>>()
                        .iter()
                    {
                        if b.key == interface {
                            if let found @ Some(_) =
                                self.unmet_derived(interface, arg, &inner, seen)
                            {
                                return found;
                            }
                        } else if self.derives.derivable(&b.key)
                            && self
                                .unmet_derived(&b.key, arg, path, &mut HashSet::new())
                                .is_some()
                        {
                            return at(Unmet::Bound(b.key.clone(), arg.clone()));
                        }
                    }
                }
                None
            }
            // A handle has no parts.
            Covers::Handle => None,
            Covers::Tuple => {
                let parts = elements(ty).unwrap_or_default().to_vec();
                parts.iter().enumerate().find_map(|(i, t)| {
                    self.unmet_derived(interface, t, &format!("{path}.{i}"), seen)
                })
            }
            Covers::Struct => {
                let (Type::Struct(key, targs) | Type::Enum(key, targs)) = ty else {
                    return None;
                };
                // The block is a template: a struct uses it when it asks.
                if !self.asks(key, interface) {
                    return at(Unmet::NotAsked(ty.clone()));
                }
                // Reported at the declaration already.
                if self
                    .asking
                    .broken
                    .contains(&(key.clone(), interface.to_string()))
                {
                    return None;
                }
                let info = self.structs.get(key)?;
                let fields: Vec<(String, Type)> = info
                    .fields
                    .iter()
                    .map(|(n, t)| (n.clone(), subst(t, &info.generics, targs)))
                    .collect();
                fields.iter().find_map(|(n, t)| {
                    self.unmet_derived(interface, t, &format!("{path}.{n}"), seen)
                })
            }
            Covers::Enum => {
                // `Option<T>` is `T`'s, at the same path.
                if let Type::Option(inner) = ty {
                    return self.unmet_derived(interface, inner, path, seen);
                }
                let (Type::Struct(key, targs) | Type::Enum(key, targs)) = ty else {
                    return None;
                };
                if !self.asks(key, interface) {
                    return at(Unmet::NotAsked(ty.clone()));
                }
                // Reported at the declaration already.
                if self
                    .asking
                    .broken
                    .contains(&(key.clone(), interface.to_string()))
                {
                    return None;
                }
                let variants = self.enum_variants(key, targs).unwrap_or_default();
                variants.iter().find_map(|(name, payloads)| {
                    payloads.iter().enumerate().find_map(|(i, t)| {
                        self.unmet_derived(interface, t, &format!("{path}.{name}.{i}"), seen)
                    })
                })
            }
        }
    }

    /// `Eq`'s item `name` (`eq` or `hash`) called on a value of concrete
    /// type `ty` without a method of its own by that name, when `ty` is
    /// `Eq`: the covering block's item.
    pub(super) fn eq_item(
        &mut self,
        call: &crate::ast::Expr,
        ty: &Type,
        name: &str,
    ) -> Option<super::methods::Method> {
        let eq = super::derive::eq_bound();
        let req = self
            .items_for(&eq, ty)
            .into_iter()
            .find(|r| r.name == name)?;
        if matches!(ty, Type::Param(_) | Type::Interface(..) | Type::Error)
            || self
                .unmet_derived(super::derive::EQ, ty, "", &mut HashSet::new())
                .is_some()
        {
            return None;
        }
        let union = |key: &str| self.holds_union(key);
        let target = self.derives.target(super::derive::EQ, name, ty, union)?;
        self.derived.insert(call.id, super::derive::EQ.to_string());
        self.used(call, &target.key, &target.args);
        Some(super::methods::Method {
            receiver: req.receiver,
            params: req.params,
            muts: req.muts,
            ret: req.ret,
            span: req.span,
            generics: Vec::new(),
            outer: 0,
            key: String::new(),
        })
    }

    /// The method of `owner`'s hand-written `impl Eq`, if it has one.
    fn written_eq(&self, owner: &str) -> Option<&super::methods::Method> {
        let entry = self
            .impls
            .get(owner)?
            .iter()
            .find(|e| e.key == super::derive::EQ)?;
        self.methods.get(owner)?.get(&entry.method("eq"))
    }

    /// The `eq` an `==` on concrete type `ty` calls:
    /// a struct's or enum's own (listed when generic or the prelude's), or
    /// the covering block's. Nothing for types compared inline.
    ///
    /// With `every`, a type's own plain (non-generic) `eq` too, for the
    /// purity of constants: it is compiled anyway, but runs.
    pub(super) fn eq_instances(&self, ty: &Type, every: bool) -> Vec<super::instances::Instance> {
        if !super::derive::compared_through_eq(ty) {
            return Vec::new();
        }
        if let Type::Struct(owner, targs) | Type::Enum(owner, targs) = ty
            && self
                .derives
                .own
                .contains(&(owner.clone(), "eq".to_string()))
            && let Some(m) = self.written_eq(owner)
        {
            let listed = every || !m.generics.is_empty() || super::instances::lazy_key(&m.key);
            return if listed {
                vec![super::instances::Instance {
                    key: m.key.clone(),
                    args: targs.clone(),
                }]
            } else {
                Vec::new()
            };
        }
        let union = |key: &str| self.holds_union(key);
        let found = self.derives.target(super::derive::EQ, "eq", ty, union);
        found.into_iter().collect()
    }

    /// The types of `ty`'s parts a struct, enum or tuple block reaches: its
    /// fields, elements, or every variant's payloads in order.
    pub(super) fn part_types(&self, ty: &Type) -> Vec<Type> {
        if let Some(parts) = elements(ty) {
            return parts.to_vec();
        }
        match ty {
            Type::Struct(key, args) => self.structs.get(key).map_or_else(Vec::new, |info| {
                info.fields
                    .iter()
                    .map(|(_, t)| subst(t, &info.generics, args))
                    .collect()
            }),
            Type::Option(inner) => vec![(**inner).clone()],
            Type::Enum(key, args) => self
                .enum_variants(key, args)
                .unwrap_or_default()
                .into_iter()
                .flat_map(|(_, p)| p)
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// What E0451 and E0450 call a union part: the union itself, or a struct
/// holding one.
pub(super) fn union_part(itself: bool) -> String {
    if itself {
        "a C union".to_string()
    } else {
        "a struct holding a union".to_string()
    }
}

/// What E0451 says a part is, for derivable interface `name`.
pub(super) fn describe(name: &str, why: &Unmet) -> String {
    match why {
        Unmet::Missing(ty) => format!("`{ty}`, which no `derive {name}` block covers"),
        Unmet::Unbounded(p) => format!("a type parameter without `{name}` (write `{p}: {name}`)"),
        Unmet::Bound(key, ty) => {
            let other = key.rsplit('.').next().unwrap_or(key);
            format!("`{ty}`, which doesn't meet `{other}`")
        }
        Unmet::Union(itself) => union_part(*itself),
        Unmet::NotAsked(ty) => format!("`{ty}`, which doesn't derive or implement `{name}`"),
    }
}
