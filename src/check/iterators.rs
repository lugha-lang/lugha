//! Iterators: the prelude's `Iter<T>`, met only through `impl Iter<T> for
//! X`, and `for x of e` over such an `e`: never over a type that merely has
//! a method named `next`.

use super::applied::Applied;
use super::fun_interfaces::is_fun_interface;
use super::generics::subst;
use super::interfaces::Requirement;
use super::{Binding, Checker, Type, errors};

use crate::ast::{Block, Expr, Ident};

/// The prelude's iterator interface, met only through `impl Iter<T> for …`.
pub const ITER: &str = "$prelude.Iter";

/// What codegen needs for one iterator loop (`Checked.iterations`).
#[derive(Debug, Clone, PartialEq)]
pub struct Iteration {
    /// The type `next` yields inside its `Some`.
    pub element: Type,
    /// The bound interface whose default `next` a type parameter may use:
    /// its key and arguments, in the loop's own parameters.
    pub default: Option<(String, Vec<Type>)>,
}

/// The prelude's operator bounds.
pub(super) fn is_operator_bound(name: &str) -> bool {
    matches!(
        name,
        "Add" | "Sub" | "Mul" | "Div" | "Rem" | "Neg" | "Num" | "Float"
    )
}

impl Checker {
    /// `key`, the interface a bare `name` would mean here, or the prelude's
    /// `Iter` when the module declares nothing by that name.
    pub(super) fn or_prelude_interface(
        &self,
        key: String,
        module: Option<&Ident>,
        name: &str,
    ) -> String {
        let declared = self.interfaces.contains_key(&key) || self.named_type(&key).is_some();
        // `Iter`, `Display`, `Debug`, `Eq`, `Key`, `Ord`, `FromEntries`,
        // `FromItems` and the operator bounds are the prelude's interfaces.
        let prelude = format!("$prelude.{name}");
        let known = (matches!(
            name,
            "Iter"
                | "Display"
                | "Debug"
                | "Eq"
                | "Key"
                | "Ord"
                | "FromEntries"
                | "FromItems"
                | "Error"
                | "Console"
                | "Fs"
                | "Dir"
                | "Clock"
                | "Env"
                | "Proc"
                | "Entropy"
                | "File"
                | "Net"
                | "Listener"
                | "Conn"
                | "Udp"
                | "Sendable"
        ) || is_operator_bound(name)
            || is_fun_interface(name))
            && self.interfaces.contains_key(&prelude);
        if module.is_none() && known && !declared {
            return prelude;
        }
        key
    }

    /// `for var of iter { body }` where `iter`, of type `ty`, isn't an
    /// array: `var` is an immutable element, and the body may change `iter`,
    /// since the loop works on a copy.
    pub(super) fn iterator_loop(&mut self, var: &Ident, iter: &Expr, ty: &Type, body: &Block) {
        let element = self.iterator_element(iter, ty);
        self.push();
        self.declare(var, element, Binding::LoopVar);
        self.loop_body(body);
        self.pop();
    }

    /// The element type of a loop over `iter`, of type `ty`, which isn't an
    /// array: E0411 unless `ty` implements `Iter<T>`.
    fn iterator_element(&mut self, iter: &Expr, ty: &Type) -> Type {
        let Some(iteration) = self.next_of(ty) else {
            // A `next` of the iterator's shape, missing only its block.
            let own_next = self.offered(ty, "next").is_some_and(|r| {
                r.receiver == Some(true) && r.params.is_empty() && matches!(r.ret, Type::Option(_))
            });
            self.report(errors::not_iterable(ty, own_next, iter.span));
            return Type::Error;
        };
        match ty {
            Type::Param(p) => self.bound_used(iter, p, "next"),
            // The block's `next`, which the loop calls.
            Type::Struct(owner, args) | Type::Enum(owner, args) => {
                if let Some((_, method)) = self.iter_block(owner, args) {
                    let key = self.methods[owner][&method].key.clone();
                    self.used(iter, &key, args);
                }
            }
            _ => {}
        }
        let element = iteration.element.clone();
        self.iterations.insert(iter.id, iteration);
        element
    }

    /// The `T` of `ty`'s `next(mut self): Option<T>`, and for a type
    /// parameter the bound interface whose default it may be.
    pub(super) fn next_of(&self, ty: &Type) -> Option<Iteration> {
        if matches!(ty, Type::Error | Type::Never) {
            return None;
        }
        // A channel's receiver gives what it receives until closed and empty.
        if let Type::Chan(super::End::Recv, element) = ty {
            return Some(Iteration {
                element: (**element).clone(),
                default: None,
            });
        }
        let offered = match ty {
            Type::Struct(owner, args) | Type::Enum(owner, args) => self.iter_block(owner, args)?.0,
            Type::Handle(owner) => self.iter_block(owner, &[])?.0,
            // An interface value is an iterator when its interface is `Iter`
            // or extends it; a type parameter when a bound does.
            Type::Interface(key, _) if !self.extends_or_is(key, ITER) => return None,
            // `next` may be an ancestor's item: `interface Source<T>: Iter<T>`.
            Type::Interface(..) => {
                let applied = Applied::of(ty)?;
                let items = self.value_items(&applied).into_iter().map(|(_, r)| r);
                items.into_iter().find(|r| r.name == "next")?
            }
            Type::Param(p) => {
                let (req, bound) = self.bound_entry(p, "next")?;
                if !self.extends_or_is(&bound.key, ITER) {
                    return None;
                }
                req
            }
            _ => self.offered(ty, "next")?,
        };
        let Type::Option(element) = offered.ret else {
            return None;
        };
        if offered.receiver != Some(true) || !offered.params.is_empty() {
            return None;
        }
        let default = match ty {
            Type::Param(p) => self
                .bound_entry(p, "next")
                .filter(|(req, _)| req.default.is_some())
                .map(|(_, a): (_, Applied)| (a.key, a.args)),
            _ => None,
        };
        Some(Iteration {
            element: *element,
            default,
        })
    }

    /// `owner`'s block `impl Iter<T> for …` at the type's arguments `args`:
    /// its `next` as a requirement, and the method's name. `None` without
    /// exactly one such block.
    pub(super) fn iter_block(&self, owner: &str, args: &[Type]) -> Option<(Requirement, String)> {
        let blocks: Vec<_> = self
            .impls
            .get(owner)?
            .iter()
            .filter(|e| e.key == ITER)
            .collect();
        let [entry] = blocks.as_slice() else {
            return None;
        };
        let method = entry.method("next");
        let m = self.methods.get(owner)?.get(&method)?;
        let outer = &m.generics[..m.outer];
        let req = Requirement {
            name: "next".to_string(),
            receiver: m.receiver,
            params: m.params.iter().map(|t| subst(t, outer, args)).collect(),
            muts: m.muts.clone(),
            ret: subst(&m.ret, outer, args),
            span: m.span,
            default: None,
        };
        Some((req, method))
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{diagnostics, errors, let_type, ok};

    const LIB: &str = "struct Count { n: i64 }\n\
        impl Iter<i64> for Count { fun next(mut self): Option<i64> = None; }\n\
        struct Repeat<T> { x: T }\n\
        impl<T> Iter<T> for Repeat<T> { fun next(mut self): Option<T> = Some(self.x); }\n\
        interface Source<T>: Iter<T> { fun size(self): i64; }\n\
        impl<T> Source<T> for Repeat<T> { fun size(self): i64 = 1; }\n\
        struct Own { n: i64 }\n\
        impl Own { fun next(mut self): Option<i64> = None; }\n\
        struct Stuck { n: i64 }\n\
        impl Stuck { fun next(self): Option<i64> = None; }\n";

    fn element(setup: &str, iter: &str) -> Type {
        let src = format!("{LIB}fun main() {{ {setup} for x of ({iter}) {{ let y = x; }} }}");
        let_type(&src, "y")
    }

    fn codes(body: &str) -> Vec<&'static str> {
        let src = format!("{LIB}fun main() {{ {body} }}");
        errors(&src).into_iter().map(|(c, _)| c).collect()
    }

    #[test]
    fn what_implements_iter_iterates() {
        assert_eq!(element("", "Count { n: 0 }"), Type::I64);
        assert_eq!(element("", "Repeat { x: \"s\" }"), Type::String);
        assert_eq!(
            element("let it: Iter<i64> = Count { n: 0 };", "it"),
            Type::I64
        );
        // An interface extending `Iter`, as a value and as a bound.
        assert_eq!(
            element("let s: Source<string> = Repeat { x: \"s\" };", "s"),
            Type::String
        );
        ok(&format!(
            "{LIB}fun f<T, I: Iter<T>>(it: I): T[] {{ let mut out: T[] = []; for x of it {{ out.push(x); }} out }}\n\
             fun g<T, S: Source<T>>(s: S): i64 {{ let mut n = 0; for x of s {{ n += 1; }} n }}\n\
             fun main() {{ let xs = f(Count {{ n: 0 }}); let n = g(Repeat {{ x: 1 }}); }}"
        ));
    }

    #[test]
    fn a_method_merely_named_next_is_e0411_naming_the_block() {
        assert_eq!(codes("for x of (Own { n: 0 }) {}"), ["E0411"]);
        let src = format!("{LIB}fun main() {{ for x of (Own {{ n: 0 }}) {{}} }}");
        let help = diagnostics(&src)[0].help.clone().expect("a help");
        assert!(help.contains("impl Iter<T> for Own"), "{help}");
        // Nor does it convert to an `Iter` value.
        assert_eq!(codes("let it: Iter<i64> = Own { n: 0 };"), ["E0422"]);
        for iter in ["5", "Stuck { n: 0 }"] {
            assert_eq!(
                codes(&format!("for x of ({iter}) {{}}")),
                ["E0411"],
                "{iter}"
            );
        }
    }

    #[test]
    fn the_loop_works_on_a_copy() {
        assert_eq!(codes("for x of (Count { n: 0 }) { x = 1; }"), ["E0501"]);
        ok(&format!(
            "{LIB}fun main() {{ let mut c = Count {{ n: 0 }}; for x of c {{ c.n = 5; c = Count {{ n: 1 }}; }} }}"
        ));
    }

    #[test]
    fn iter_is_a_built_in_name() {
        // `for` uses the prelude's `Iter`, so no module declares its own.
        assert_eq!(
            errors(&format!(
                "{LIB}interface Iter {{ fun size(self): i64; }}\nfun main() {{}}"
            ))[0]
                .0,
            "E0302"
        );
        ok(&format!(
            "{LIB}fun f(it: Iter<i64>) {{}}\nfun g<I: Iter<i64>>(it: I) {{}}\nfun main() {{}}"
        ));
    }
}
