//! Built-in types implementing interfaces: `impl Show for f64 { … }`,
//! `impl Show for Point[]` and `impl<T: Show> Show for T[]`. Such a block is
//! recorded like a derive block for one type (`check::derive`), flagged as
//! written: its items are methods of a hidden owner, generic over the
//! block's parameters, checked once and instantiated per type. It doesn't
//! make its interface derivable. Its home is the interface's module, or
//! the module of a program type the block's type is built from.

use super::applied::Applied;
use super::derive::{Block, Covers, block_type, label, names, overlap};
use super::methods::Method;
use super::{Checker, Type, errors};
use crate::ast::{AppliedType, Expr, Ident, ImplDecl, Item, Program, TypeKind};
use crate::modules::impls::item_name;

/// The prelude's types that a block written `impl I for Name<…>` may be
/// for, when no type of the program has that name.
const NAMED: [&str; 4] = ["Option", "Result", "Map", "Set"];

/// The parameters `ty` mentions, each once, in order.
fn params_in(ty: &Type, out: &mut Vec<String>) {
    match ty {
        Type::Param(n) if !out.contains(n) => out.push(n.clone()),
        Type::Array(t) | Type::Option(t) | Type::Ref(t) | Type::Task(t) | Type::Chan(t) => {
            params_in(t, out)
        }
        Type::Struct(_, args) | Type::Enum(_, args) | Type::Interface(_, args) => {
            args.iter().for_each(|a| params_in(a, out))
        }
        Type::Fun(ps, r) => {
            ps.iter().for_each(|p| params_in(p, out));
            params_in(r, out);
        }
        _ => {}
    }
}

/// The keys of the program's own structs, enums and extern types in `ty`.
fn program_types(ty: &Type, out: &mut Vec<String>) {
    match ty {
        Type::Struct(k, args) | Type::Enum(k, args) => {
            if !k.starts_with('$') {
                out.push(k.clone());
            }
            args.iter().for_each(|a| program_types(a, out));
        }
        // `CPtr` is the prelude's.
        Type::Handle(k) if k != super::input::CPTR => out.push(k.clone()),
        Type::Array(t) | Type::Option(t) => program_types(t, out),
        _ => {}
    }
}

/// The module part of an item's key: `geo` for `geo.Point`.
fn module_of(key: &str) -> &str {
    key.rsplit_once('.').map_or("", |(m, _)| m)
}

impl Checker {
    /// The type a block for a built-in type is for, as written: its
    /// `target`, or `Option<Point>` written like a struct's block when no
    /// type of the program is named `Option`.
    pub(super) fn builtin_target(&self, i: &ImplDecl) -> Option<crate::ast::Type> {
        if let Some(t) = &i.target {
            return Some((**t).clone());
        }
        i.interface.as_ref()?;
        let name = &i.ty.name;
        let own = i.module.is_some() || self.named_type(&self.key(name)).is_some();
        if own || !NAMED.contains(&name.as_str()) {
            return None;
        }
        // Its "parameters" are the types it is for: `Option<Point>`.
        let args: Vec<crate::ast::Type> = i
            .params
            .iter()
            .map(|p| crate::ast::Type {
                kind: TypeKind::Named(p.name.name.clone()),
                span: p.name.span,
            })
            .collect();
        let kind = match (name.as_str(), <[crate::ast::Type; 1]>::try_from(args)) {
            ("Option", Ok([inner])) => TypeKind::Option(Box::new(inner)),
            (_, args) => TypeKind::Applied(Box::new(AppliedType {
                module: None,
                name: i.ty.clone(),
                args: args.map_or_else(|a| a, Vec::from),
            })),
        };
        Some(crate::ast::Type {
            kind,
            span: i.ty.span,
        })
    }

    /// Pass 2, after the derive blocks: every block for a built-in type.
    pub(super) fn collect_builtin_impls(&mut self, program: &Program) {
        for item in &program.items {
            let Item::Impl(i) = item else { continue };
            if let Some(target) = self.builtin_target(i) {
                self.collect_builtin_impl(i, &target);
            }
        }
        self.type_params.clear();
    }

    fn collect_builtin_impl(&mut self, i: &ImplDecl, target: &crate::ast::Type) {
        let Some(b) = &i.interface else { return };
        let Some(key) = self.interface_key(b) else {
            let is_type = b.module.is_none() && self.is_type_name(&b.name.name);
            self.report(errors::unknown_bound(&b.name.name, b.span, is_type));
            return;
        };
        self.item_visible(&key, b.span);
        self.define_bound(b, &key);
        let written = b.name.name.as_str();
        // `Option<Point>` written as a struct's block names no parameters.
        let declared = if i.target.is_some() {
            &i.params[..]
        } else {
            &[]
        };
        let generics = self.generics_with(declared, &[]);
        self.type_params = generics.clone();
        let ty = self.resolve(target);
        if ty == Type::Error {
            return;
        }
        let misplaced = |why: String| errors::misplaced_impl(written, why, target.span);
        if let Some(why) = self.not_for_block(&ty) {
            self.report(misplaced(why));
            return;
        }
        let mut used = Vec::new();
        params_in(&ty, &mut used);
        if let Some(g) = generics.iter().find(|g| !used.contains(&g.name)) {
            let why = format!("has `{}`, which `{ty}` doesn't use", g.name);
            self.report(misplaced(why));
            return;
        }
        let prelude = [super::derive::EQ, super::derive::KEY, super::derive::ORD];
        if prelude.contains(&key.as_str()) {
            let why = format!(
                "is for `{ty}`: the prelude decides `Eq`, `Key` and `Ord` for built-in types"
            );
            self.report(errors::misplaced_impl(written, why, b.span));
            return;
        }
        if !self.by_block(&key) {
            let why = format!("names `{written}`, which a type still meets by its methods' names");
            self.report(errors::misplaced_impl(written, why, b.span));
            return;
        }
        let here = self.module.join(".");
        let mut owners = Vec::new();
        program_types(&ty, &mut owners);
        if module_of(&key) != here && !owners.iter().any(|o| module_of(o) == here) {
            let why = match owners.is_empty() {
                true => format!("must be written in the module that declares `{written}`"),
                false => format!(
                    "must be written in the module that declares `{written}` or a type in `{ty}`"
                ),
            };
            self.report(errors::misplaced_impl(written, why, i.span));
            return;
        }
        self.bound_args(&key, b);
        let Some(applied) = self.applied_bound(key.clone(), b) else {
            return;
        };
        let covers = Covers::Type(ty.clone());
        let mut owner = super::qualify(&self.module, &format!("{}@{}", i.tag, label(&covers)));
        if self.derives.blocks.iter().any(|b| b.owner == owner) {
            owner = super::qualify(&self.module, &format!("{}@{}", i.tag, block_type(&covers)));
        }
        let mut block = Block {
            interface: key.clone(),
            covers,
            generics: generics.clone(),
            owner: owner.clone(),
            span: i.span,
            written: true,
            args: applied.args.clone(),
            items: Vec::new(),
        };
        let first = self.derives.blocks.iter().find(|o| {
            o.interface == key
                && matches!(&o.covers, Covers::Type(x) if overlap(x, &ty, &names(o), &names(&block)))
        });
        if let Some(first) = first {
            let shown = super::derive_with::shown_interface(&key);
            let d = errors::duplicate_impl(&shown, &ty, b.span, first.span);
            self.report(d);
            return;
        }
        let wanted = self.items_for(&applied, &ty);
        let mut problems = Vec::new();
        for f in &i.methods {
            let name = item_name(&f.name.name);
            let params: Vec<Type> = f.params.iter().map(|p| self.resolve(&p.ty)).collect();
            let ret = f.ret.as_ref().map_or(Type::Unit, |t| self.resolve(t));
            let receiver = f.receiver.map(|r| r.mutable);
            let Some(req) = wanted.iter().find(|r| r.name == name) else {
                problems.push(format!("`{name}` isn't an item of `{applied}`"));
                continue;
            };
            if req.receiver != receiver || req.params != params || req.ret != ret {
                problems.push(format!(
                    "`{name}` should be `{}`",
                    super::satisfy::shown(req)
                ));
                continue;
            }
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
                key: format!("{owner}.{name}"),
            };
            let methods = self.methods.entry(owner.clone()).or_default();
            methods.insert(name.to_string(), method);
            block.items.push(name.to_string());
        }
        for r in &wanted {
            if r.default.is_none() && !block.items.contains(&r.name) {
                problems.push(format!("no method `{}`", r.name));
            }
        }
        if !problems.is_empty() {
            let d = errors::bad_impl(&ty, &applied.to_string(), &problems, b.span);
            self.report(d);
        }
        self.derives.blocks.push(block);
    }

    /// Why no block may be for `ty`, if none may: a function, an interface
    /// value, a box, a task, a channel, an actor, a bare parameter, or a
    /// program's own struct or enum, whose block is written by its name.
    fn not_for_block(&self, ty: &Type) -> Option<String> {
        let what = match ty {
            Type::Fun(..) => "a function type",
            Type::Interface(..) => "an interface type",
            Type::Ref(_) => "a `Ref`",
            Type::Task(_) | Type::Chan(_) | Type::Actor(_) => "a task, channel or actor",
            Type::Param(_) => {
                return Some(format!(
                    "is for every type `{ty}`: a block is for one kind of type"
                ));
            }
            Type::Struct(k, _) | Type::Enum(k, _) | Type::Handle(k) if !k.starts_with('$') => {
                return Some(format!(
                    "is for `{ty}`: a struct's or enum's block writes its parameters after its name, without `impl<…>`"
                ));
            }
            _ => return None,
        };
        Some(format!(
            "is for `{ty}`: {what} can't implement an interface"
        ))
    }

    /// Checks every block for a built-in type's bodies once, `self` of the
    /// block's type.
    pub(super) fn builtin_impl_bodies(&mut self, program: &Program) {
        for item in &program.items {
            let Item::Impl(i) = item else { continue };
            let Some(block) = self
                .derives
                .blocks
                .iter()
                .find(|b| b.span == i.span && b.written)
            else {
                continue;
            };
            let block = block.clone();
            let me = block.self_type();
            for f in &i.methods {
                let found = self
                    .methods
                    .get(&block.owner)
                    .and_then(|m| m.get(item_name(&f.name.name)));
                let Some(m) = found.filter(|m| m.span == f.name.span).cloned() else {
                    continue;
                };
                self.owner = m.key.clone();
                self.type_params = m.generics.clone();
                self.body(f, m.params, m.ret, Some(me.clone()));
            }
        }
    }

    /// The written or derived block of `interface` covering built-in type
    /// `ty`, if any.
    pub(super) fn builtin_block(&self, interface: &str, ty: &Type) -> Option<(Block, Vec<Type>)> {
        if matches!(
            ty,
            Type::Struct(..) | Type::Enum(..) | Type::Handle(_) | Type::Param(_)
        ) && super::tuples::elements(ty).is_none()
            && !matches!(ty, Type::Struct(k, _) | Type::Enum(k, _) if k.starts_with('$'))
        {
            return None;
        }
        let (block, args) = self.derives.find(interface, ty, |_| false)?;
        matches!(block.covers, Covers::Type(_)).then(|| (block.clone(), args))
    }

    /// True if built-in type `ty` implements `interface` through a block.
    pub(super) fn builtin_implements(&self, ty: &Type, interface: &Applied) -> bool {
        self.builtin_block(&interface.key, ty)
            .is_some_and(|(b, args)| {
                b.args.is_empty()
                    || b.args
                        .iter()
                        .map(|a| super::generics::subst(a, &b.generics, &args))
                        .eq(interface.args.iter().cloned())
            })
    }

    /// The interface `block` gives built-in type `ty` at `args`, the
    /// block's parameters.
    pub(super) fn block_applied(block: &Block, args: &[Type]) -> Applied {
        Applied {
            key: block.interface.clone(),
            args: block
                .args
                .iter()
                .map(|a| super::generics::subst(a, &block.generics, args))
                .collect(),
        }
    }

    /// `x.name(…)` on built-in `receiver` through the one block in sight
    /// that defines or defaults item `name`: `None` when no block does,
    /// `Some(None)` when two do (reported: the call must be qualified).
    pub(super) fn block_item(
        &mut self,
        call: &Expr,
        receiver: &Type,
        name: &Ident,
    ) -> Option<Option<Method>> {
        if matches!(receiver, Type::Param(_) | Type::Interface(..) | Type::Error) {
            return None;
        }
        let mut keys: Vec<String> = self
            .derives
            .blocks
            .iter()
            .map(|b| b.interface.clone())
            .filter(|k| {
                self.interfaces.get(k).is_some_and(|i| {
                    i.items
                        .iter()
                        .any(|r| r.name == name.name && r.receiver.is_some())
                })
            })
            .filter(|k| self.interface_in_sight(k, name.span))
            .collect();
        keys.sort();
        keys.dedup();
        let mut found: Vec<(Block, Vec<Type>)> = keys
            .iter()
            .filter_map(|k| self.builtin_block(k, receiver))
            .collect();
        let (block, args) = match found.len() {
            0 => return None,
            1 => found.remove(0),
            _ => {
                let names =
                    [0, 1].map(|n| Self::block_applied(&found[n].0, &found[n].1).to_string());
                self.report(errors::ambiguous_item(&name.name, receiver, &names, call));
                return Some(None);
            }
        };
        Some(Some(self.block_call(
            call,
            receiver,
            (&block, &args),
            &name.name,
        )))
    }

    /// A call of item `name` on built-in `receiver` through `block` at
    /// `args`: the block's method, listed, or the interface's default.
    /// The method to check the call against.
    pub(super) fn block_call(
        &mut self,
        call: &Expr,
        receiver: &Type,
        (block, args): (&Block, &[Type]),
        name: &str,
    ) -> Method {
        let applied = Self::block_applied(block, args);
        // Codegen finds the block's item from the interface (or the default).
        self.derived.insert(call.id, block.interface.clone());
        if !block.items.iter().any(|i| i == name) {
            return self.item_call(call, receiver, &applied, name);
        }
        self.used(call, &format!("{}.{name}", block.owner), args);
        let req = self
            .items_for(&applied, receiver)
            .into_iter()
            .find(|r| r.name == name)
            .expect("the block's interface declares the item");
        // Go to definition leads to the block's method.
        let span = self
            .methods
            .get(&block.owner)
            .and_then(|m| m.get(name))
            .map_or(req.span, |m| m.span);
        Method {
            receiver: req.receiver,
            params: req.params,
            muts: req.muts,
            ret: req.ret,
            span,
            generics: Vec::new(),
            outer: 0,
            key: String::new(),
        }
    }
}
