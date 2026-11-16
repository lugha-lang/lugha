//! Interfaces at work: calls through a type
//! parameter's bounds (`s.area()`, `T.make(…)`, E0421) and whether a type
//! argument satisfies an interface bound (E0422), structurally.

use super::applied::Applied;
use super::generics::subst;
use super::interfaces::Requirement;
use super::methods::Method;
use super::{Checker, Type, errors};
use crate::ast::{Expr, Ident};

/// What a type offers for one requirement.
enum Offer {
    Missing,
    Mismatched,
    Matches,
}

impl Checker {
    /// The item `name` that one of `param`'s interface bounds declares.
    pub(super) fn bound_item(&self, param: &str, name: &str) -> Option<Requirement> {
        self.bound_entry(param, name).map(|(req, _)| req)
    }

    /// The item `name` that one of `param`'s bounds declares, with that bound.
    pub(super) fn bound_entry(&self, param: &str, name: &str) -> Option<(Requirement, Applied)> {
        let bounds = self.bounds(param);
        let me = Type::Param(param.to_string());
        // `T: Eq` (and `Num`, which gives `==`) is the prelude's `Eq`.
        let implied = super::derive::implies_eq(&bounds).then(super::derive::eq_bound);
        bounds.interfaces.into_iter().chain(implied).find_map(|a| {
            let req = self
                .items_for(&a, &me)
                .into_iter()
                .find(|r| r.name == name)?;
            Some((req, a))
        })
    }

    /// `x.m(…)` (`on_value`) or `T.f(…)` on type parameter `param`: the
    /// signature its bounds declare, as a method to check the call against.
    /// E0421 if no bound declares it, E0415 for the wrong kind of call.
    pub(super) fn through_bound(
        &mut self,
        call: &Expr,
        param: &str,
        name: &Ident,
        on_value: bool,
    ) -> Option<Method> {
        let Some((req, applied)) = self.bound_entry(param, &name.name) else {
            self.report(errors::no_bound_declares(param, &name.name, name.span));
            return None;
        };
        if req.receiver.is_some() != on_value {
            let ty = Type::Param(param.to_string());
            let help = if on_value {
                format!("call it on the type: `{param}.{}(…)`", name.name)
            } else {
                format!("call it on a value: `x.{}(…)`", name.name)
            };
            self.report(errors::no_method(&name.name, &ty, name.span, Some(help)));
            return None;
        }
        self.bound_used(call, param, &name.name);
        if self.by_block(&applied.key) {
            self.nominal_calls.insert(call.id, applied.clone());
        }
        // Codegen finds a derivable interface's item through its blocks, and
        // a built-in type's through its `impl` block.
        if self.derives.has_blocks(&applied.key) {
            self.derived.insert(call.id, applied.key.clone());
        }
        self.default_callee(call, &req, applied.args);
        Some(Method {
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

    /// E0422 unless `arg` satisfies `interface`: every item it declares
    /// has a visible method or associated function with the same name,
    /// receiver kind and types.
    pub(super) fn satisfies(
        &mut self,
        arg: &Type,
        interface: &Applied,
        span: crate::span::Span,
    ) -> bool {
        let problems = self.unmet(arg, interface);
        if problems.is_empty() || matches!(arg, Type::Error) {
            return true;
        }
        let shown = interface.to_string();
        // A value printed through a `Console`: the bound or the debug form.
        if interface.key == super::derive::DISPLAY && matches!(arg, Type::Param(_)) {
            self.report(errors::not_display(arg, span));
            return false;
        }
        // A block that exists but gives another type says so.
        let gives_other = self.impl_for(arg, interface).is_some();
        if self.nominal(&interface.key)
            && !gives_other
            && !matches!(arg, Type::Param(_) | Type::Interface(..))
        {
            self.report_not_implemented(arg, &shown, span);
        } else {
            self.report(errors::unsatisfied(arg, &shown, &problems, span));
        }
        false
    }

    /// What `arg` lacks to satisfy `interface`, one entry per item: empty
    /// when it satisfies it.
    pub(super) fn unmet(&self, arg: &Type, interface: &Applied) -> Vec<String> {
        // The numbers and `string` meet the prelude's operator bounds as
        // built-ins, with themselves on the right: `i64 * f64` is a block's.
        let other_right = super::impls::is_operator(&interface.key)
            && interface.args.first().is_some_and(|r| r != arg);
        if !other_right && let Some(problems) = self.builtin_operator(arg, &interface.key) {
            return problems;
        }
        // A built-in type through its block: `impl Show for f64`.
        if self.builtin_implements(arg, interface) {
            return Vec::new();
        }
        // An interface value meets a prelude interface met only by `impl`
        // (`Error`) when it is its own or one it extends.
        if let Type::Interface(key, _) = arg
            && self.prelude_nominal(&interface.key)
        {
            return match self.extends_or_is(key, &interface.key) {
                true => Vec::new(),
                false => vec![format!("`{key}` doesn't extend `{interface}`")],
            };
        }
        // A nominal interface is met only through `impl I for T`; a type
        // parameter still offers its items.
        if self.nominal(&interface.key) && !matches!(arg, Type::Param(_) | Type::Interface(..)) {
            return match self.impl_for(arg, interface) {
                Some(entry) if super::impls::is_operator(&interface.key) => {
                    // A bound promises `arg` back; the block may give another type.
                    let name = &self.interfaces[&interface.key].items[0].name;
                    let gives = match arg {
                        Type::Struct(o, targs) | Type::Enum(o, targs) => self
                            .methods
                            .get(o)
                            .and_then(|m| m.get(&entry.method(name)))
                            .map(|m| subst(&m.ret, &m.generics[..m.outer], targs)),
                        Type::Handle(o) => self
                            .methods
                            .get(o)
                            .and_then(|m| m.get(&entry.method(name)))
                            .map(|m| m.ret.clone()),
                        _ => None,
                    };
                    match gives {
                        Some(ret) if ret != *arg => {
                            vec![format!("its `impl {interface}` gives {ret}, not {arg}")]
                        }
                        _ => Vec::new(),
                    }
                }
                Some(_) => Vec::new(),
                None => vec![format!("no `impl {interface} for {arg}`")],
            };
        }
        let mut problems = Vec::new();
        for req in &self.items_for(interface, arg) {
            match self.offer(arg, req) {
                Offer::Matches => {}
                Offer::Missing if req.default.is_some() => {}
                Offer::Missing => problems.push(format!("no method `{}`", req.name)),
                Offer::Mismatched => {
                    problems.push(format!("`{}` should be `{}`", req.name, shown(req)));
                }
            }
        }
        problems
    }

    /// What `arg` offers for `req`: a parameter offers its bounds' items, a
    /// struct or enum its visible methods (an `impl`'s parameters
    /// substituted), and every other type nothing.
    fn offer(&self, arg: &Type, req: &Requirement) -> Offer {
        self.offer_one(arg, req)
    }

    /// `offer` for a method of one name.
    fn offer_one(&self, arg: &Type, req: &Requirement) -> Offer {
        let found = match arg {
            Type::Param(p) => match self.bound_item(p, &req.name) {
                Some(item) if item.same(req) => return Offer::Matches,
                Some(_) => return Offer::Mismatched,
                None => return Offer::Missing,
            },
            // An interface value offers what its interface declares, its
            // ancestors' items included.
            Type::Interface(..) => {
                let applied = Applied::of(arg).expect("an interface type");
                let items: Vec<Requirement> = self
                    .value_items(&applied)
                    .into_iter()
                    .map(|(_, r)| r)
                    .collect();
                return match items.iter().find(|r| r.name == req.name) {
                    Some(item) if item.same(req) => Offer::Matches,
                    Some(_) => Offer::Mismatched,
                    None => Offer::Missing,
                };
            }
            Type::Struct(owner, args) | Type::Enum(owner, args) => {
                let method = self.methods.get(owner).and_then(|m| m.get(&req.name));
                match method {
                    Some(m) if self.method_usable(owner, &req.name) => (m, args),
                    _ => return Offer::Missing,
                }
            }
            _ => return Offer::Missing,
        };
        let (m, args) = found;
        // A method with type parameters of its own never matches: interface
        // signatures are concrete.
        if m.generics.len() > m.outer {
            return Offer::Mismatched;
        }
        let outer = &m.generics[..m.outer];
        let offered = Requirement {
            name: req.name.clone(),
            receiver: m.receiver,
            params: m.params.iter().map(|t| subst(t, outer, args)).collect(),
            muts: m.muts.clone(),
            ret: subst(&m.ret, outer, args),
            span: m.span,
            default: None,
        };
        if offered.same(req) {
            Offer::Matches
        } else {
            Offer::Mismatched
        }
    }
}

/// `fun area(self): f64`, as an interface declares it.
pub(super) fn shown(req: &Requirement) -> String {
    let receiver = req.receiver.map(|m| if m { "mut self" } else { "self" });
    let params = req.params.iter().enumerate().map(|(i, t)| {
        match req.muts.get(i).copied().unwrap_or(false) {
            true => format!("mut {t}"),
            false => t.to_string(),
        }
    });
    let list: Vec<String> = receiver
        .map(String::from)
        .into_iter()
        .chain(params)
        .collect();
    let ret = if req.ret == Type::Unit {
        String::new()
    } else {
        format!(": {}", req.ret)
    };
    format!("fun {}({}){ret}", req.name, list.join(", "))
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{diagnostics, errors, ok};

    const SHAPES: &str = "interface Shape { fun area(self): f64; fun name(self): string; }\n\
        struct Circle { r: f64 }\n\
        impl Shape for Circle {\n\
            fun area(self): f64 = 3.0 * self.r * self.r;\n\
            fun name(self): string = \"circle\";\n\
        }\n\
        impl Make for Circle { fun make(r: f64): Circle = Circle { r: r }; }\n\
        impl Grow for Circle { fun grow(mut self, by: f64) { self.r = self.r + by; } }\n\
        interface Make { fun make(r: f64): Circle; }\n\
        interface Grow { fun grow(mut self, by: f64); }\n\
        fun total<T: Shape>(xs: T[]): f64 { let mut sum = 0.0; for x of xs { sum += x.area(); } sum }\n";

    fn e(body: &str) -> Vec<(&'static str, String)> {
        let src = format!("{SHAPES}{body}\nfun main() {{}}");
        errors(&src)
            .into_iter()
            .map(|(c, t)| (c, t.to_string()))
            .collect()
    }

    #[test]
    fn calls_go_through_the_bounds() {
        ok(&format!(
            "{SHAPES}fun f<T: Shape>(s: T): f64 = s.area();\n\
             fun g<T: Make>(): Circle = T.make(1.0);\n\
             fun h<T: Grow>(s: T) {{ let mut t = s; t.grow(1.0); }}\n\
             fun main() {{ let n = total([Circle {{ r: 1.0 }}]); }}"
        ));
        assert_eq!(
            e("fun f<T: Shape>(s: T): i64 = s.area();"),
            [("E0403", "s.area()".into())]
        );
        assert_eq!(
            e("fun f<T: Grow>(s: T) { let mut t = s; t.grow(\"x\"); }"),
            [("E0403", "\"x\"".into())]
        );
        assert_eq!(e("fun f<T: Grow>(s: T) { s.grow(1.0); }")[0].0, "E0501");
    }

    #[test]
    fn a_method_no_bound_declares_is_e0421() {
        let src = format!("{SHAPES}fun f<T: Shape>(s: T): f64 = s.perimeter();\nfun main() {{}}");
        assert_eq!(errors(&src), [("E0421", "perimeter")]);
        let help = diagnostics(&src)[0].help.clone().expect("a help");
        assert!(
            help.contains("add an interface that declares `perimeter`"),
            "{help}"
        );
        assert_eq!(
            e("fun f<T: Make>(s: T): Circle = s.make(1.0);")[0].0,
            "E0415"
        );
    }

    #[test]
    fn unsatisfied_bounds_are_e0422() {
        let at = |body: &str| e(body).into_iter().map(|(c, _)| c).collect::<Vec<_>>();
        assert_eq!(at("fun f() { let n = total([1, 2]); }"), ["E0422"]);
        let d = diagnostics(&format!(
            "{SHAPES}fun f() {{ let n = total([1, 2]); }}\nfun main() {{}}"
        ));
        assert_eq!(d[0].message, "`i64` doesn't implement `Shape`");
        assert!(
            d[0].help
                .as_deref()
                .unwrap()
                .contains("add `impl Shape for i64 { … }` in the module that declares `Shape`"),
            "{:?}",
            d[0].help
        );
        // Methods of the right shape don't make a type a `Shape`.
        let alike = "struct Sq { s: f64 }\nimpl Sq { fun area(self): f64 = 1.0; fun name(self): string = \"sq\"; }\n";
        assert_eq!(
            at(&format!(
                "{alike}fun f() {{ let n = total([Sq {{ s: 1.0 }}]); }}"
            )),
            ["E0422"]
        );
        let wrong = "struct Sq { s: f64 }\nimpl Shape for Sq { fun area(self): i64 = 1; fun name(self): string = \"sq\"; }\n";
        assert_eq!(
            at(&format!(
                "{wrong}fun f() {{ let n = total([Sq {{ s: 1.0 }}]); }}"
            )),
            ["E0422"]
        );
        let d = diagnostics(&format!(
            "{SHAPES}{wrong}fun f() {{ let n = total([Sq {{ s: 1.0 }}]); }}\nfun main() {{}}"
        ));
        assert!(
            d[0].help
                .as_deref()
                .unwrap()
                .contains("`area` should be `fun area(self): f64`"),
            "{:?}",
            d[0].help
        );
        let kind = "struct K {}\nimpl K { fun grow(self, by: f64) {} }\nfun g<T: Grow>(x: T) {}\n";
        assert_eq!(at(&format!("{kind}fun f() {{ g(K {{}}); }}")), ["E0422"]);
    }

    #[test]
    fn another_modules_private_method_doesnt_count() {
        use crate::check::modules::test_util::errors;
        let found = errors(&[
            (
                "main.la",
                "import geo;\ninterface Shape { fun area(self): f64; }\n\
                 fun a<T: Shape>(x: T): f64 = x.area();\n\
                 fun main() { let n = a(geo.Sq { s: 1.0 }); }\n",
            ),
            (
                "geo.la",
                "struct Sq { s: f64 }\nimpl Sq { private fun area(self): f64 = self.s; }\n",
            ),
        ]);
        assert_eq!(found.iter().map(|(c, _)| *c).collect::<Vec<_>>(), ["E0422"]);
    }

    #[test]
    fn generic_types_and_parameters_satisfy_too() {
        ok("interface Sized { fun size(self): i64; }\n\
            struct Stack<T> { items: T[] }\n\
            impl Sized for Stack<T> { fun size(self): i64 = self.items.len; }\n\
            fun count<T: Sized>(x: T): i64 = x.size();\n\
            fun pass<U: Sized>(u: U): i64 = count(u);\n\
            fun main() { let s = Stack { items: [1] }; let n = count(s); let m = pass(s); }");
    }
}
