//! Derive blocks: `derive I for X { … }`,
//! written in interface `I`'s own module, gives `I`'s items to the types `X`
//! covers: one type (`i64`, `T[]`), or every struct, enum or tuple whose
//! parts meet `I`. A block's items are methods of a hidden owner,
//! `<I>@<label>`, generic over the block's parameters (`Self` for a struct,
//! enum or tuple block), so each body is checked once and instantiated per
//! type like any generic method. Satisfaction, with E0450-style paths, is in
//! `check::derive_meets`; the field intrinsics in `check::derive_intrinsics`.

use std::collections::HashMap;

use super::applied::Applied;
use super::derive_match::{block_type, label, matches, names, overlap};
use super::generics::{Bounds, Generic};
use super::instances::Instance;
use super::methods::Method;
use super::tuples::elements;
use super::{Checker, SELF, Type, errors};
use crate::ast::{DeriveDecl, DeriveTarget, Item, Program};
use crate::span::Span;

/// The prelude's derivable `Eq`: what `T: Eq`
/// (the `eq` flag of a bound) means.
pub const EQ: &str = "$prelude.Eq";

/// The prelude's `Key`, which extends `Eq`: map keys and set values.
pub const KEY: &str = "$prelude.Key";

/// The prelude's `Ord`, which extends `Eq`: what `T: Ord` means.
pub const ORD: &str = "$prelude.Ord";

/// The prelude's `Debug`: every type's debug form, met without asking.
pub const DEBUG: &str = "$prelude.Debug";

/// The prelude's `Display`: the text `print` and `"{x}"` show, written by hand.
pub const DISPLAY: &str = "$prelude.Display";

/// True if a parameter with `bounds` has `==`, and so `Eq`'s items:
/// `Eq`, or `Num`/`Float`, which give `==` too.
pub(super) fn implies_eq(bounds: &Bounds) -> bool {
    bounds.eq
        || bounds
            .interfaces
            .iter()
            .any(|a| a.key == "$prelude.Num" || a.key == "$prelude.Float")
}

/// True for the types whose `==` goes through `Eq`'s blocks or their own
/// `eq`: structs, enums, tuples, arrays and `Option`.
/// The rest compare inline, or have no `==` at all.
pub(super) fn compared_through_eq(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Struct(..) | Type::Enum(..) | Type::Array(_) | Type::Option(_)
    )
}

/// `Eq` as a bound.
pub(super) fn eq_bound() -> Applied {
    Applied {
        key: EQ.to_string(),
        args: Vec::new(),
    }
}

/// What a block covers.
#[derive(Debug, Clone, PartialEq)]
pub enum Covers {
    /// One type, written with the block's parameters: `Option<T>`.
    Type(Type),
    /// Every struct whose fields meet the interface.
    Struct,
    /// Every enum whose payloads meet the interface.
    Enum,
    /// Every tuple whose elements meet the interface.
    Tuple,
    /// Every `extern type` handle, `CPtr` included.
    Handle,
}

/// One `derive` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The interface's key.
    pub interface: String,
    pub covers: Covers,
    /// Its parameters: the written ones, or `Self` for a struct, enum or tuple block.
    pub(crate) generics: Vec<Generic>,
    /// The hidden owner of its items: `std.$json_value.Json@i64`.
    pub owner: String,
    /// The whole block, from `derive` to its `}`.
    pub span: Span,
    /// An `impl I for f64` block for a built-in type, which doesn't make
    /// `I` derivable; its items are `items`, the rest the interface's defaults.
    pub written: bool,
    /// The interface's arguments, in the block's parameters: `impl
    /// Source<i64> for i64[]`. Empty for a derive block.
    pub args: Vec<Type>,
    /// The items the block defines.
    pub items: Vec<String>,
}

impl Block {
    /// The type its items take as `self`: the covered type, written with
    /// the block's parameters, or `Self` for a struct, enum or tuple block.
    pub fn self_type(&self) -> Type {
        match &self.covers {
            Covers::Type(t) => t.clone(),
            _ => Type::Param(SELF.to_string()),
        }
    }
}

/// Every derive block, by interface (`Checked::derives`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Derives {
    pub(super) blocks: Vec<Block>,
    /// Struct and enum keys with their own item of a derivable interface,
    /// by item name: calls reach those instead of a block's.
    pub(super) own: std::collections::HashSet<(String, String)>,
}

impl Derives {
    /// True if interface `key` has derive blocks: it is met through them.
    pub fn derivable(&self, key: &str) -> bool {
        self.blocks.iter().any(|b| b.interface == key && !b.written)
    }

    /// True if interface `key` has any block: a derive block, or an `impl`
    /// block for a built-in type.
    pub fn has_blocks(&self, key: &str) -> bool {
        self.blocks.iter().any(|b| b.interface == key)
    }

    /// The block of `interface` that covers `ty`, with its parameters'
    /// types: a block for `ty`'s own type first, then the struct, enum or
    /// tuple block. `holds_union` keeps C unions out of struct blocks.
    pub fn find(
        &self,
        interface: &str,
        ty: &Type,
        holds_union: impl Fn(&str) -> bool,
    ) -> Option<(&Block, Vec<Type>)> {
        self.find_at(interface, None, ty, holds_union)
    }

    /// Every written block of `interface` whose type covers `ty`, with
    /// what its parameters stand for there: one per argument list.
    pub fn blocks_for(&self, interface: &str, ty: &Type) -> Vec<(&Block, Vec<Type>)> {
        self.blocks
            .iter()
            .filter(|b| b.interface == interface && b.written)
            .filter_map(|b| {
                let Covers::Type(pattern) = &b.covers else {
                    return None;
                };
                let names: Vec<&str> = b.generics.iter().map(|g| g.name.as_str()).collect();
                let mut bound = HashMap::new();
                if !matches(pattern, ty, &names, &mut bound) {
                    return None;
                }
                let args = names
                    .iter()
                    .map(|n| bound.get(*n).cloned())
                    .collect::<Option<_>>()?;
                Some((b, args))
            })
            .collect()
    }

    /// `find`, among the blocks whose interface arguments are `at`, when
    /// given: `impl Mul<Vec2> for f64` beside `impl Mul<Mat> for f64`.
    pub fn find_at(
        &self,
        interface: &str,
        at: Option<&[Type]>,
        ty: &Type,
        holds_union: impl Fn(&str) -> bool,
    ) -> Option<(&Block, Vec<Type>)> {
        let mine = || self.blocks.iter().filter(|b| b.interface == interface);
        for b in mine() {
            if let Covers::Type(pattern) = &b.covers {
                let names: Vec<&str> = b.generics.iter().map(|g| g.name.as_str()).collect();
                let mut bound = HashMap::new();
                // The block's arguments bind its parameters as its type does.
                let fits_at = |bound: &mut HashMap<String, Type>| match at {
                    Some(at) => {
                        b.args.len() == at.len()
                            && b.args
                                .iter()
                                .zip(at)
                                .all(|(p, t)| matches(p, t, &names, bound))
                    }
                    None => true,
                };
                if matches(pattern, ty, &names, &mut bound) && fits_at(&mut bound) {
                    let Some(args) = names.iter().map(|n| bound.get(*n).cloned()).collect() else {
                        continue;
                    };
                    return Some((b, args));
                }
            }
        }
        if at.is_some_and(|a| !a.is_empty()) {
            return None;
        }
        let kind = match ty {
            Type::Struct(..) if elements(ty).is_some() => Covers::Tuple,
            Type::Struct(key, _) if !holds_union(key) => Covers::Struct,
            // `Option` is an enum too, unless a block names it.
            Type::Enum(..) | Type::Option(_) => Covers::Enum,
            Type::Handle(_) => Covers::Handle,
            _ => return None,
        };
        let b = mine().find(|b| b.covers == kind)?;
        Some((b, vec![ty.clone()]))
    }

    /// True if `interface` has a block for every struct (`Covers::Struct`)
    /// or every enum (`Covers::Enum`).
    pub(super) fn covers(&self, interface: &str, kind: &Covers) -> bool {
        self.blocks
            .iter()
            .any(|b| b.interface == interface && b.covers == *kind)
    }

    /// True if struct or enum `owner` has its own item `name` of a derivable
    /// interface.
    pub fn has_own(&self, owner: &str, name: &str) -> bool {
        self.own.contains(&(owner.to_string(), name.to_string()))
    }

    /// The types `interface` has blocks for, as written: `i64`, `T[]`, `struct`s.
    pub(super) fn covered(&self, interface: &str) -> Vec<String> {
        self.blocks
            .iter()
            .filter(|b| b.interface == interface && !b.written)
            .map(|b| match &b.covers {
                Covers::Type(t) => format!("`{}`", super::shown(&t.to_string())),
                Covers::Struct => "structs".to_string(),
                Covers::Enum => "enums".to_string(),
                Covers::Tuple => "tuples".to_string(),
                Covers::Handle => "handles".to_string(),
            })
            .collect()
    }

    /// The instance a call of item `name` of `interface` reaches at `ty`:
    /// `None` when `ty` has its own item (or no block covers it).
    pub fn target(
        &self,
        interface: &str,
        name: &str,
        ty: &Type,
        holds_union: impl Fn(&str) -> bool,
    ) -> Option<Instance> {
        self.target_at(interface, None, name, ty, holds_union)
    }

    /// `target`, through the block whose interface arguments are `at`.
    pub fn target_at(
        &self,
        interface: &str,
        at: Option<&[Type]>,
        name: &str,
        ty: &Type,
        holds_union: impl Fn(&str) -> bool,
    ) -> Option<Instance> {
        if let Type::Struct(owner, _) | Type::Enum(owner, _) = ty
            && self.own.contains(&(owner.clone(), name.to_string()))
        {
            return None;
        }
        let (block, args) = self.find_at(interface, at, ty, holds_union)?;
        // An item a written block leaves to the interface's default.
        if block.written && !block.items.iter().any(|i| i == name) {
            return None;
        }
        Some(Instance {
            key: format!("{}.{name}", block.owner),
            args,
        })
    }

    /// The hidden owner of the block declared at `span`.
    pub fn owner_at(&self, span: Span) -> Option<&str> {
        let block = self.blocks.iter().find(|b| b.span == span)?;
        Some(&block.owner)
    }

    /// The type a block's items take as `self`, at the block's arguments.
    pub fn self_type(&self, owner: &str, args: &[Type]) -> Option<Type> {
        let block = self.blocks.iter().find(|b| b.owner == owner)?;
        Some(match &block.covers {
            Covers::Type(pattern) => super::generics::subst(pattern, &block.generics, args),
            _ => args.first()?.clone(),
        })
    }
}

impl Checker {
    /// Pass 2, after methods: every derive block's items, as methods of its
    /// hidden owner, each checked against the interface's signature.
    pub(super) fn collect_derives(&mut self, program: &Program) {
        for item in &program.items {
            let Item::Derive(d) = item else { continue };
            self.collect_derive(d);
        }
        self.type_params.clear();
    }

    fn collect_derive(&mut self, d: &DeriveDecl) {
        // Declared here, not named through an import.
        let interface = super::qualify(&self.module, &d.interface.name);
        if !self.interfaces.contains_key(&interface) {
            let d = errors::derive_elsewhere(&d.interface.name, d.interface.span);
            self.report(d);
            return;
        }
        let (generics, covers) = match &d.target {
            DeriveTarget::Type(t) => {
                let generics = self.generics_with(&d.type_params, &[]);
                self.type_params = generics.clone();
                (generics, Covers::Type(self.resolve(t)))
            }
            kind => {
                let bounds = Bounds {
                    interfaces: vec![Applied {
                        key: interface.clone(),
                        args: Vec::new(),
                    }],
                    ..Bounds::default()
                };
                let generics = vec![Generic {
                    name: SELF.to_string(),
                    bounds,
                }];
                let covers = match kind {
                    DeriveTarget::Struct => Covers::Struct,
                    DeriveTarget::Enum => Covers::Enum,
                    DeriveTarget::Handle => Covers::Handle,
                    _ => Covers::Tuple,
                };
                (generics, covers)
            }
        };
        self.type_params = generics.clone();
        let mut owner = format!("{interface}@{}", label(&covers));
        // Two blocks for one generic type at different arguments share a
        // label: the second is named by its whole type.
        if self.derives.blocks.iter().any(|b| b.owner == owner) {
            owner = format!("{interface}@{}", block_type(&covers));
        }
        let block = Block {
            interface: interface.clone(),
            covers: covers.clone(),
            generics: generics.clone(),
            owner: owner.clone(),
            span: d.span,
            written: false,
            args: Vec::new(),
            items: d.methods.iter().map(|f| f.name.name.clone()).collect(),
        };
        let me = block.self_type();
        let clash = self.derives.blocks.iter().find(|b| {
            b.interface == interface
                && match (&b.covers, &block.covers) {
                    (Covers::Type(x), Covers::Type(y)) => overlap(x, y, &names(b), &names(&block)),
                    (x, y) => x == y,
                }
        });
        if clash.is_some() {
            let shown = interface
                .rsplit('.')
                .next()
                .unwrap_or(&interface)
                .to_string();
            self.report(errors::derive_overlap(&shown, &block_type(&covers), d.span));
            return;
        }
        let wanted = self.items_for(
            &Applied {
                key: interface.clone(),
                args: Vec::new(),
            },
            &me,
        );
        let mut problems = Vec::new();
        for f in &d.methods {
            let params: Vec<Type> = f.params.iter().map(|p| self.resolve(&p.ty)).collect();
            let ret = f.ret.as_ref().map_or(Type::Unit, |t| self.resolve(t));
            let receiver = f.receiver.map(|r| r.mutable);
            let fits = wanted.iter().any(|r| {
                r.name == f.name.name
                    && r.receiver == receiver
                    && r.params == params
                    && r.ret == ret
            });
            if !fits || !f.type_params.is_empty() {
                problems.push(match wanted.iter().find(|r| r.name == f.name.name) {
                    Some(r) => format!("`{}` should be `{}`", r.name, super::satisfy::shown(r)),
                    None => format!("`{}` isn't one of its items", f.name.name),
                });
                continue;
            }
            // Hover shows an item like a method.
            let sig = super::Sig::of(
                &generics,
                generics.len(),
                receiver,
                (&f.params, &params),
                &ret,
            );
            self.hover.signatures.insert(f.name.span, sig);
            self.hover.function(f.name.span, f.name.span, None);
            let method = Method {
                receiver,
                params,
                muts: super::muts_of(&f.params),
                ret,
                span: f.name.span,
                generics: generics.clone(),
                outer: generics.len(),
                key: format!("{owner}.{}", f.name.name),
            };
            let methods = self.methods.entry(owner.clone()).or_default();
            methods.insert(f.name.name.clone(), method);
        }
        for r in &wanted {
            if !d.methods.iter().any(|f| f.name.name == r.name) {
                problems.push(format!("no method `{}`", r.name));
            }
        }
        if !problems.is_empty() {
            let shown = format!("derive {} for {}", d.interface.name, block_type(&covers));
            let d = errors::block_unsatisfied(&shown, &d.interface.name, &problems, d.span);
            self.report(d);
        }
        self.derives.blocks.push(block);
    }

    /// Checks every derive block's bodies once, with `self` of the block's type.
    pub(super) fn derive_bodies(&mut self, program: &Program) {
        for item in &program.items {
            let Item::Derive(d) = item else { continue };
            let Some(block) = self.derives.blocks.iter().find(|b| b.span == d.span) else {
                continue;
            };
            let block = block.clone();
            let me = block.self_type();
            for f in &d.methods {
                let found = self
                    .methods
                    .get(&block.owner)
                    .and_then(|m| m.get(&f.name.name));
                let Some(m) = found.filter(|m| m.span == f.name.span).cloned() else {
                    continue;
                };
                self.owner = m.key.clone();
                self.type_params = m.generics.clone();
                self.deriving = Some(block.clone());
                self.body(f, m.params, m.ret, Some(me.clone()));
            }
        }
        self.deriving = None;
    }
}
