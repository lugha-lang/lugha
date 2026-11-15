//! Interface items on concrete types: `c.area()` on a struct or enum
//! without its own `area` calls the item of the one interface in sight
//! that the type implements and that declares it (E0460 for more than
//! one): the block's method, or else the interface's default. A default
//! of the prelude's interfaces, which types meet by their methods' names,
//! is found the same way among those the type satisfies.

use std::collections::HashMap;

use super::applied::Applied;
use super::generics::params;
use super::methods::Method;
use super::{Checker, Type, errors};
use crate::ast::{Expr, Ident};
use crate::span::Span;

impl Checker {
    /// `x.name(…)` on `receiver`, a struct or enum lacking `name`: the
    /// item's method, after recording the call's interface (and, for a
    /// default, its callee and instance). `None` when no interface in
    /// sight supplies it (E0415 follows); `Some(None)` after E0460.
    pub(super) fn concrete_default(
        &mut self,
        call: &Expr,
        receiver: &Type,
        name: &Ident,
    ) -> Option<Option<Method>> {
        if !matches!(
            receiver,
            Type::Struct(..) | Type::Enum(..) | Type::Handle(_)
        ) {
            return None;
        }
        let mut found = self.implemented_with(receiver, &name.name, name.span);
        if found.is_empty() {
            found = self.default_suppliers(receiver, &name.name, name);
        }
        let applied = match found.len() {
            0 => return None,
            1 => found.remove(0),
            _ => {
                let names = [found[0].to_string(), found[1].to_string()];
                self.report(errors::ambiguous_item(&name.name, receiver, &names, call));
                return Some(None);
            }
        };
        Some(Some(self.item_call(call, receiver, &applied, &name.name)))
    }

    /// A call of item `name` of `applied` on `receiver`, which meets it:
    /// records the call's interface, and the default's callee and instance
    /// when the type has no method for it. The method to check the call
    /// against.
    pub(super) fn item_call(
        &mut self,
        call: &Expr,
        receiver: &Type,
        applied: &Applied,
        name: &str,
    ) -> Method {
        let nominal = self.impl_item(receiver, applied, name);
        if nominal.is_some() {
            self.nominal_calls.insert(call.id, applied.clone());
        }
        if let (
            Some(Some(method)),
            Type::Struct(owner, _) | Type::Enum(owner, _) | Type::Handle(owner),
        ) = (&nominal, receiver)
        {
            return self.methods[owner][method].clone();
        }
        let req = self
            .items_for(applied, receiver)
            .into_iter()
            .find(|r| r.name == name)
            .expect("a supplier declares the item");
        let key = format!("{}.{}", applied.key, name);
        self.callees.insert(call.id, key.clone());
        if !applied.args.is_empty() {
            self.default_args.insert(call.id, applied.args.clone());
        }
        let instance: Vec<Type> = std::iter::once(receiver.clone())
            .chain(applied.args.iter().cloned())
            .collect();
        self.used(call, &key, &instance);
        Method {
            receiver: req.receiver,
            params: req.params,
            ret: req.ret,
            span: req.span,
            generics: Vec::new(),
            outer: 0,
            key: String::new(),
        }
    }

    /// The prelude's interfaces in sight, sorted by key, that give `name`
    /// a default and that `ty` satisfies, at the arguments `ty`'s methods
    /// give them.
    fn default_suppliers(&self, ty: &Type, name: &str, at: &Ident) -> Vec<Applied> {
        let mut keys: Vec<&String> = self
            .interfaces
            .iter()
            .filter(|(key, _)| !self.nominal(key))
            .filter(|(_, info)| {
                info.items
                    .iter()
                    .any(|r| r.name == name && r.default.is_some() && r.receiver.is_some())
            })
            .map(|(key, _)| key)
            .filter(|key| self.interface_in_sight(key, at.span))
            .collect();
        keys.sort();
        keys.into_iter()
            .filter_map(|key| {
                let generics = self.interfaces[key].generics.clone();
                let names: Vec<&str> = generics.iter().map(|g| g.name.as_str()).collect();
                let mut bound = HashMap::new();
                let open = Applied {
                    key: key.clone(),
                    args: params(&generics),
                };
                self.read_off(&open, ty, &names, &mut bound);
                let args = names.iter().map(|n| bound.get(*n).cloned());
                let applied = Applied {
                    key: key.clone(),
                    args: args.collect::<Option<Vec<Type>>>()?,
                };
                self.unmet(ty, &applied).is_empty().then_some(applied)
            })
            .collect()
    }

    /// Whether interface `key` is declared here, in an imported module or
    /// in the prelude, and visible at `at`.
    pub(super) fn interface_in_sight(&self, key: &str, at: Span) -> bool {
        let module = key.rsplit_once('.').map_or("", |(m, _)| m);
        let here = |path: &[String]| path.join(".") == module;
        let near =
            module == "$prelude" || here(&self.module) || self.imports.values().any(|p| here(p));
        let Some(owner) = self.privacy.items.get(key) else {
            return near;
        };
        let name = key.rsplit('.').next().unwrap_or(key);
        near && self.denied(name, owner, owner.level, at, None).is_none()
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{errors, ok};

    const DESCRIBE: &str = "interface Describe {\n\
        fun name(self): string;\n\
        fun describe(self): string = \"this is {self.name()}\";\n\
        }\n";

    #[test]
    fn a_default_is_called_on_structs_and_enums() {
        ok(&format!(
            "{DESCRIBE}struct P {{ x: i64 }}\nimpl Describe for P {{ fun name(self): string = \"p\"; }}\n\
             enum L {{ Red, Green }}\nimpl Describe for L {{ fun name(self): string = \"l\"; }}\n\
             fun main() {{ let s: string = P {{ x: 1 }}.describe(); println(L.Red.describe()); }}"
        ));
    }

    #[test]
    fn the_types_own_method_wins() {
        ok(&format!(
            "{DESCRIBE}struct P {{ x: i64 }}\nimpl Describe for P {{ fun name(self): string = \"p\"; }}\n\
             impl P {{ fun describe(self): i64 = 1; }}\n\
             fun main() {{ let n: i64 = P {{ x: 1 }}.describe(); }}"
        ));
    }

    #[test]
    fn a_generic_interfaces_arguments_come_from_the_type() {
        ok("interface Source<T> {\n\
            fun next(mut self): Option<T>;\n\
            fun first(mut self): T = match self.next() { Some(x) => x, None => panic(\"empty\") };\n\
            }\n\
            struct C { n: i64 }\n\
            impl Source<i64> for C { fun next(mut self): Option<i64> = Some(self.n); }\n\
            fun main() { let mut c = C { n: 4 }; let n: i64 = c.first(); }");
    }

    #[test]
    fn two_suppliers_are_e0460() {
        let src = format!(
            "{DESCRIBE}interface Other {{ fun describe(self): string = \"other\"; }}\n\
             struct P {{ x: i64 }}\nimpl Describe for P {{ fun name(self): string = \"p\"; }}\n\
             impl Other for P {{}}\n\
             fun main() {{ println(P {{ x: 1 }}.describe()); println(Other.describe(P {{ x: 1 }})); }}"
        );
        assert_eq!(errors(&src), [("E0460", "describe")]);
    }

    #[test]
    fn no_supplier_is_still_e0415() {
        // `Q` doesn't implement `Describe`.
        let src = format!(
            "{DESCRIBE}struct Q {{ x: i64 }}\nfun main() {{ println(Q {{ x: 1 }}.describe()); }}"
        );
        assert_eq!(errors(&src), [("E0415", "describe")]);
        let src = format!("{DESCRIBE}fun main() {{ let n = 1; println(n.describe()); }}");
        assert_eq!(errors(&src), [("E0415", "describe")]);
    }

    #[test]
    fn self_in_an_impl_checks_as_the_type() {
        ok("struct P { x: i64 }\n\
            impl P { fun o(): Self = Self { x: 0 }; fun up(self): Self = Self { x: self.x + 1 }; }\n\
            enum L { Red, Green }\n\
            impl L { fun flip(self): Self = match self { Red => Self.Green, Green => Self.Red }; }\n\
            fun main() { let p: P = P.o().up(); let l: L = L.Red.flip(); }");
        // `Self` in a declaration is the declared type.
        ok("struct N { kids: Self[] }\nfun main() {}");
        assert_eq!(
            errors("fun f(): Self = 1;\nfun main() {}"),
            [("E0305", "Self")]
        );
    }
}
