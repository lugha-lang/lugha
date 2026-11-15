//! Nominal interfaces: a type implements an interface only through an
//! `impl I for T { … }` block. Each block is an entry of the table here,
//! by the type's key; its methods are ordinary methods of `T`, renamed
//! `item@tag` by `modules::impls` so they never clash with `T`'s own or
//! another interface's. The prelude's interfaces and derivable ones are
//! still met by their methods' names.

use std::collections::HashMap;

use super::applied::Applied;
use super::generics::{Generic, params, subst, with_args};
use super::interfaces::Requirement;
use super::methods::Method;
use super::satisfy::shown;
use super::{Checker, Type, errors};
use crate::ast::ImplDecl;
use crate::modules::impls::item_name;
use crate::span::Span;

/// One `impl I for T` block.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplEntry {
    /// The interface's key.
    pub key: String,
    /// The interface's arguments, in the block's type parameters.
    pub args: Vec<Type>,
    /// The block's type parameters, `T` in `impl Source<T> for Stack<T>`,
    /// which the type's arguments replace.
    pub generics: Vec<String>,
    /// What the block's method names carry after `@` (`modules::impls`).
    pub tag: String,
    /// The interface as written in the block.
    pub span: Span,
}

impl ImplEntry {
    /// The interface's arguments when the type's are `targs`.
    pub fn args_at(&self, targs: &[Type]) -> Vec<Type> {
        if self.generics.is_empty() {
            return self.args.clone();
        }
        let map: HashMap<String, Type> =
            self.generics.iter().cloned().zip(targs.to_vec()).collect();
        self.args.iter().map(|a| a.substitute(&map)).collect()
    }

    /// The name of the block's method for interface item `item`.
    pub fn method(&self, item: &str) -> String {
        format!("{item}@{}", self.tag)
    }
}

/// Every `impl I for T` block, by `T`'s key.
pub type Impls = HashMap<String, Vec<ImplEntry>>;

/// The block of `impls` by which `owner` at arguments `targs` implements
/// interface `key` at `args`.
pub fn find<'a>(
    impls: &'a Impls,
    owner: &str,
    targs: &[Type],
    key: &str,
    args: &[Type],
) -> Option<&'a ImplEntry> {
    impls
        .get(owner)?
        .iter()
        .find(|e| e.key == key && e.args_at(targs) == args)
}

/// The module part of an item's key: `geo` for `geo.Circle`, `""` for an
/// item of the main file.
fn module_of(key: &str) -> &str {
    key.rsplit_once('.').map_or("", |(m, _)| m)
}

impl Checker {
    /// E0422: `ty` has no `impl` of `interface`, at `span`.
    pub(super) fn report_not_implemented(&mut self, ty: &Type, interface: &str, span: Span) {
        let declared = match ty {
            Type::Struct(k, a) | Type::Enum(k, a) if !a.is_empty() => {
                Some(with_args(ty, params(&self.generics_of(k))))
            }
            Type::Struct(..) | Type::Enum(..) | Type::Handle(_) => Some(ty.clone()),
            // A built-in type can't have a block yet.
            _ => None,
        };
        self.report(errors::not_implemented(
            ty,
            declared.as_ref(),
            interface,
            span,
        ));
    }

    /// True for an interface a type meets only through `impl I for T`:
    /// any but the prelude's and the derivable ones, which the compiler
    /// still finds by their methods' names.
    pub(super) fn nominal(&self, key: &str) -> bool {
        module_of(key) != "$prelude" && !self.derives.derivable(key)
    }

    /// Records `impl I for T` block `i` of `owner` (type `ty`, the block's
    /// parameters `outer`): E0315 for an unknown interface, E0459 for a
    /// block in the wrong place, E0461 for a second one. False if its
    /// methods shouldn't be registered.
    pub(super) fn register_impl(
        &mut self,
        i: &ImplDecl,
        owner: &str,
        ty: &Type,
        outer: &[Generic],
    ) -> bool {
        let Some(b) = &i.interface else {
            return true;
        };
        let Some(key) = self.interface_key(b) else {
            let is_type = b.module.is_none() && self.is_type_name(&b.name.name);
            self.report(errors::unknown_bound(&b.name.name, b.span, is_type));
            return false;
        };
        self.item_visible(&key, b.span);
        let written = &b.name.name;
        if !matches!(ty, Type::Struct(..) | Type::Enum(..) | Type::Handle(_)) {
            let why = format!(
                "is for `{ty}`: only a struct, an enum, an actor or an extern type can implement an interface"
            );
            self.report(errors::misplaced_impl(written, why, i.ty.span));
            return false;
        }
        if !self.nominal(&key) {
            let why = format!(
                "names `{written}`, which a type still meets by its methods' names: write them in `impl {} {{ … }}`",
                i.ty.name
            );
            self.report(errors::misplaced_impl(written, why, b.span));
            return false;
        }
        let here = self.module.join(".");
        if module_of(&key) != here && module_of(owner) != here {
            let why = format!(
                "must be written in the module that declares `{written}` or the one that declares `{ty}`"
            );
            self.report(errors::misplaced_impl(written, why, i.span));
            return false;
        }
        let saved = std::mem::replace(&mut self.type_params, outer.to_vec());
        self.bound_args(&key, b);
        let applied = self.applied_bound(key.clone(), b);
        self.type_params = saved;
        let Some(applied) = applied else {
            return false;
        };
        let entry = ImplEntry {
            key,
            args: applied.args,
            generics: outer.iter().map(|g| g.name.clone()).collect(),
            tag: i.tag.to_string(),
            span: b.span,
        };
        // Two blocks are the same when their arguments agree for every
        // argument list of the type.
        let probe: Vec<Type> = (0..outer.len())
            .map(|n| Type::Param(format!("${n}")))
            .collect();
        let entries = self.impls.entry(owner.to_string()).or_default();
        if let Some(first) = entries
            .iter()
            .find(|e| e.key == entry.key && e.args_at(&probe) == entry.args_at(&probe))
        {
            let first = first.span;
            let shown = Applied {
                key: entry.key.clone(),
                args: entry.args.clone(),
            };
            let d = errors::duplicate_impl(&shown.to_string(), ty, b.span, first);
            self.report(d);
            return false;
        }
        entries.push(entry);
        true
    }

    /// E0422 unless the registered block `i` of `owner` (type `ty`)
    /// defines every item of its interface that has no default, each with
    /// the interface's signature, and nothing else.
    pub(super) fn check_impl(&mut self, i: &ImplDecl, owner: &str, ty: &Type) {
        let Some(b) = &i.interface else { return };
        let Some(entry) = self
            .impls
            .get(owner)
            .and_then(|es| es.iter().find(|e| e.span == b.span))
            .cloned()
        else {
            return;
        };
        let applied = Applied {
            key: entry.key.clone(),
            args: entry.args.clone(),
        };
        let params: Vec<Type> = entry.generics.iter().cloned().map(Type::Param).collect();
        let self_ty = with_args(ty, params);
        let reqs = self.items_for(&applied, &self_ty);
        let mut problems = Vec::new();
        for req in &reqs {
            match self
                .methods
                .get(owner)
                .and_then(|m| m.get(&entry.method(&req.name)))
            {
                None if req.default.is_some() => {}
                None => problems.push(format!("no method `{}`", req.name)),
                Some(m) if !offers(m, req) => {
                    problems.push(format!("`{}` should be `{}`", req.name, shown(req)));
                }
                Some(_) => {}
            }
        }
        for f in &i.methods {
            let item = item_name(&f.name.name);
            if !reqs.iter().any(|r| r.name == item) {
                problems.push(format!("`{item}` isn't an item of `{applied}`"));
            }
        }
        if !problems.is_empty() {
            let d = errors::bad_impl(&self_ty, &applied.to_string(), &problems, b.span);
            self.report(d);
        }
    }

    /// The block by which `ty` implements `interface`, if `ty` is a struct,
    /// enum or extern type with one.
    pub(super) fn impl_for(&self, ty: &Type, interface: &Applied) -> Option<&ImplEntry> {
        let (owner, targs) = match ty {
            Type::Struct(owner, targs) | Type::Enum(owner, targs) => (owner, targs.as_slice()),
            Type::Handle(owner) => (owner, &[][..]),
            _ => return None,
        };
        find(&self.impls, owner, targs, &interface.key, &interface.args)
    }

    /// For a nominal `interface` and a struct, enum or extern type `ty`:
    /// `Some(Some(name))`, the method by which `ty`'s block defines
    /// `item`; `Some(None)` when the block leaves it to the default, or
    /// there is no block. `None` for any other interface or type, whose
    /// methods go by their names.
    pub(super) fn impl_item(
        &self,
        ty: &Type,
        interface: &Applied,
        item: &str,
    ) -> Option<Option<String>> {
        let owner = match ty {
            Type::Struct(o, _) | Type::Enum(o, _) | Type::Handle(o) => o,
            _ => return None,
        };
        if !self.nominal(&interface.key) {
            return None;
        }
        let Some(entry) = self.impl_for(ty, interface) else {
            return Some(None);
        };
        let name = entry.method(item);
        let has = self
            .methods
            .get(owner)
            .is_some_and(|m| m.contains_key(&name));
        Some(has.then_some(name))
    }

    /// `impl_item` for a call of `item` through a bound of type parameter
    /// `param`, in an instance whose `generics` are `args` and where
    /// `param` is `concrete`.
    pub(super) fn bound_impl_item(
        &self,
        (generics, args): (&[Generic], &[Type]),
        param: &str,
        item: &str,
        concrete: &Type,
    ) -> Option<Option<String>> {
        let bounds = &generics.iter().find(|g| g.name == param)?.bounds;
        let interface = bounds.interfaces.iter().find(|a| {
            self.nominal(&a.key)
                && self
                    .interfaces
                    .get(&a.key)
                    .is_some_and(|i| i.items.iter().any(|r| r.name == item))
        })?;
        let at = Applied {
            key: interface.key.clone(),
            args: interface
                .args
                .iter()
                .map(|a| subst(a, generics, args))
                .collect(),
        };
        self.impl_item(concrete, &at, item)
    }

    /// `T.name(…)` on a type without its own `name`: the method of the
    /// one implemented interface in sight that declares an associated
    /// function `name`, with its name in the type's table. `None` when
    /// there is none; `Some(None)` after E0460 for two.
    pub(super) fn associated_item(
        &mut self,
        ty: &Type,
        name: &crate::ast::Ident,
    ) -> Option<Option<(String, Method)>> {
        let owner = match ty {
            Type::Struct(o, _) | Type::Enum(o, _) | Type::Handle(o) => o.clone(),
            _ => return None,
        };
        let found: Vec<(String, String)> = self
            .impls
            .get(&owner)
            .into_iter()
            .flatten()
            .filter(|e| {
                self.interfaces.get(&e.key).is_some_and(|i| {
                    i.items
                        .iter()
                        .any(|r| r.name == name.name && r.receiver.is_none())
                })
            })
            .filter(|e| self.interface_in_sight(&e.key, name.span))
            .map(|e| (e.key.clone(), e.method(&name.name)))
            .collect();
        match found.as_slice() {
            [] => None,
            [(_, block)] => {
                let m = self.methods.get(&owner)?.get(block)?.clone();
                Some(Some((block.clone(), m)))
            }
            [(a, _), (b, _), ..] => {
                let names = [a.clone(), b.clone()];
                self.report(errors::ambiguous_item(&name.name, ty, &names, name.span));
                Some(None)
            }
        }
    }

    /// The nominal interfaces `ty` implements, at its arguments, that
    /// declare a method `item` and are in sight at `span`, sorted.
    pub(super) fn implemented_with(&self, ty: &Type, item: &str, span: Span) -> Vec<Applied> {
        let (owner, targs) = match ty {
            Type::Struct(owner, targs) | Type::Enum(owner, targs) => (owner, targs.as_slice()),
            Type::Handle(owner) => (owner, &[][..]),
            _ => return Vec::new(),
        };
        let mut found: Vec<Applied> = self
            .impls
            .get(owner)
            .into_iter()
            .flatten()
            .filter(|e| {
                self.interfaces.get(&e.key).is_some_and(|i| {
                    i.items
                        .iter()
                        .any(|r| r.name == item && r.receiver.is_some())
                })
            })
            .filter(|e| self.interface_in_sight(&e.key, span))
            .map(|e| Applied {
                key: e.key.clone(),
                args: e.args_at(targs),
            })
            .collect();
        found.sort_by_key(|a| a.to_string());
        found
    }
}

/// True if method `m` has exactly the signature `req` asks for, both in
/// the block's type parameters.
fn offers(m: &Method, req: &Requirement) -> bool {
    m.generics.len() == m.outer
        && m.receiver == req.receiver
        && m.params == req.params
        && m.ret == req.ret
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{diagnostics, errors, ok};

    const SHAPE: &str = "interface Shape { fun area(self): f64; fun name(self): string = \"shape\"; }\n\
        interface Plot { fun area(self): f64; }\n\
        struct C { r: f64 }\n";

    fn codes(body: &str) -> Vec<&'static str> {
        errors(&format!("{SHAPE}{body}\nfun main() {{}}"))
            .into_iter()
            .map(|(c, _)| c)
            .collect()
    }

    #[test]
    fn a_block_implements_and_its_items_are_reached_every_way() {
        ok(&format!(
            "{SHAPE}impl C {{ fun area(self): i64 = 0; }}\n\
             impl Shape for C {{ fun area(self): f64 = self.r; }}\n\
             fun t<T: Shape>(x: T): f64 = x.area();\n\
             fun main() {{\n\
                 let c = C {{ r: 1.0 }};\n\
                 let own: i64 = c.area();\n\
                 let a: f64 = Shape.area(c) + t(c);\n\
                 let s: Shape = c;\n\
                 let n: string = c.name();\n\
             }}"
        ));
    }

    #[test]
    fn a_block_must_match_its_interface() {
        assert_eq!(
            codes("impl Shape for C { fun area(self): i64 = 1; }"),
            ["E0422"]
        );
        assert_eq!(codes("impl Shape for C {}"), ["E0422"]);
        let src = format!(
            "{SHAPE}impl Shape for C {{ fun area(self): f64 = 1.0; fun more(self) {{}} }}\nfun main() {{}}"
        );
        let help = diagnostics(&src)[0].help.clone().expect("a help");
        assert!(help.contains("`more` isn't an item of `Shape`"), "{help}");
    }

    #[test]
    fn a_misplaced_block_is_e0459() {
        assert_eq!(
            codes("impl Shape for i64 { fun area(self): f64 = 1.0; }"),
            ["E0459"]
        );
        assert_eq!(
            codes("impl Display for C { fun show(self): string = \"c\"; }"),
            ["E0459"]
        );
        use crate::check::modules::test_util::errors;
        let found = errors(&[
            (
                "main.la",
                "import geo;\nimport shapes;\n\
                 impl shapes.Shape for geo.Sq { fun area(self): f64 = 1.0; }\nfun main() {}\n",
            ),
            ("geo.la", "public struct Sq { s: f64 }\n"),
            (
                "shapes.la",
                "public interface Shape { fun area(self): f64; }\n",
            ),
        ]);
        assert_eq!(found.iter().map(|(c, _)| *c).collect::<Vec<_>>(), ["E0459"]);
    }

    #[test]
    fn two_blocks_of_one_interface_are_e0461() {
        let block = "impl Shape for C { fun area(self): f64 = 1.0; }\n";
        assert_eq!(codes(&format!("{block}{block}")), ["E0461"]);
        ok(
            "interface From<T> { fun from(x: T): Self; }\nstruct P { n: i64 }\n\
            impl From<i64> for P { fun from(x: i64): P = P { n: x }; }\n\
            impl From<bool> for P { fun from(x: bool): P = P { n: 0 }; }\nfun main() {}",
        );
    }

    #[test]
    fn a_name_two_implemented_interfaces_share_is_e0460() {
        let blocks = "impl Shape for C { fun area(self): f64 = 1.0; }\n\
                      impl Plot for C { fun area(self): f64 = 2.0; }\n";
        assert_eq!(
            codes(&format!(
                "{blocks}fun f() {{ let a = C {{ r: 1.0 }}.area(); }}"
            )),
            ["E0460"]
        );
        ok(&format!(
            "{SHAPE}{blocks}fun main() {{ let a = Plot.area(C {{ r: 1.0 }}); }}"
        ));
    }

    #[test]
    fn a_qualified_call_needs_the_block() {
        assert_eq!(
            codes("fun f() { let a = Shape.area(C { r: 1.0 }); }"),
            ["E0422"]
        );
        assert_eq!(
            codes("fun f() { let a = Shape.size(C { r: 1.0 }); }"),
            ["E0415"]
        );
    }
}
