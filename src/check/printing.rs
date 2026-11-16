//! Printing values. `print`, `println`, `eprint`, `eprintln`,
//! `to_string` and `"{x}"` show a value's `Display`: the numbers', `bool`'s,
//! `char`'s and `string`'s own text, or a type's `impl Display`. Every
//! value has a debug form, `Debug`: `x.debug()`, `debug(x)` and the values
//! a panic shows. This module checks both and lists the instances each
//! printed type reaches.

use std::collections::HashSet;

use super::applied::Applied;
use super::instances::Instance;
use super::{Checker, Type, errors};
use crate::ast::Expr;

/// The key of a print site's use: its one argument is the printed type,
/// whose `Display` it calls.
pub(super) const PRINT: &str = "$print";

/// The key of a debug site's use: its one argument is the type whose
/// debug form it shows.
pub(super) const DEBUG_USE: &str = "$debug";

/// True for the types codegen prints without any helper.
pub(super) fn plain(ty: &Type) -> bool {
    ty.is_numeric()
        || matches!(
            ty,
            Type::Bool | Type::String | Type::Char | Type::Error | Type::Never
        )
}

/// The prelude's `Display`.
fn display() -> Applied {
    Applied {
        key: super::derive::DISPLAY.to_string(),
        args: Vec::new(),
    }
}

impl Checker {
    /// True if a value of `ty` has `Display`, so it prints; E0422 at
    /// `span` otherwise, with help to write the block or show its debug form.
    pub(super) fn displayed(&mut self, ty: &Type, span: crate::span::Span) -> bool {
        if plain(ty) || self.unmet(ty, &display()).is_empty() {
            return true;
        }
        self.report(errors::not_display(ty, span));
        false
    }

    /// Records that the body being checked prints a value of `ty` through
    /// its `Display`, so each instance lists the `show` it reaches.
    pub(super) fn printed(&mut self, at: &Expr, ty: &Type) {
        if !plain(ty) {
            self.used(at, PRINT, std::slice::from_ref(ty));
        }
    }

    /// Records that the body being checked shows a value of `ty` in its
    /// debug form, so each instance lists the `debug` it reaches.
    pub(super) fn debug_used(&mut self, at: &Expr, ty: &Type) {
        if !plain(ty) {
            self.used(at, DEBUG_USE, std::slice::from_ref(ty));
        }
    }

    /// `debug(x)`: shows `x`'s debug form on stderr and gives `x` back.
    pub(super) fn debug_intrinsic(&mut self, call: &Expr, args: &[Expr]) -> Type {
        let [arg] = args else {
            self.report(errors::arity("debug", 1, args.len(), call.span));
            return self.unchecked_args(args);
        };
        let ty = self.value(arg, None);
        self.debug_used(arg, &ty);
        self.debug_calls.push(call.span);
        ty
    }

    /// `x.debug()` on any value without a method of that name of its own:
    /// its debug form. `None` when the type has its own `debug`.
    pub(super) fn debug_call(
        &mut self,
        call: &Expr,
        receiver: &Type,
        args: &[Expr],
    ) -> Option<Type> {
        if let Type::Struct(o, _) | Type::Enum(o, _) | Type::Handle(o) = receiver
            && self.methods.get(o).is_some_and(|m| m.contains_key("debug"))
        {
            return None;
        }
        if !args.is_empty() {
            self.report(errors::arity("debug", 0, args.len(), call.span));
            return Some(self.unchecked_args(args));
        }
        self.derived
            .insert(call.id, super::derive::DEBUG.to_string());
        self.debug_used(call, receiver);
        Some(Type::String)
    }

    /// The `show` instance printing a value of concrete type `ty` reaches:
    /// its `impl Display`'s, when generic or the prelude's, or with `every`
    /// any (for the purity of constants: it is compiled anyway, but runs).
    /// An interface value's goes through its table.
    pub(super) fn display_instances(&self, ty: &Type, every: bool) -> Vec<Instance> {
        let key = super::derive::DISPLAY;
        if let Some(target) = self.derives.target(key, "show", ty, |_| false) {
            return vec![target];
        }
        let Some(Some(name)) = self.impl_item(ty, &display(), "show") else {
            return Vec::new();
        };
        let (Type::Struct(owner, args) | Type::Enum(owner, args)) = ty else {
            let Type::Handle(owner) = ty else {
                return Vec::new();
            };
            let m = &self.methods[owner][&name];
            return vec![Instance {
                key: m.key.clone(),
                args: Vec::new(),
            }];
        };
        let m = &self.methods[owner][&name];
        let listed = every || !args.is_empty() || super::instances::lazy_key(&m.key);
        listed
            .then(|| Instance {
                key: m.key.clone(),
                args: args.clone(),
            })
            .into_iter()
            .collect()
    }

    /// The `debug` instances showing a value of concrete type `ty` reaches:
    /// its `impl Debug`'s or its block's. An interface value reaches those
    /// of every type converted to it, a box its contents'. The numbers,
    /// `bool`, `char`, `string` and the fixed forms need none.
    pub(super) fn debug_instances(&self, ty: &Type, every: bool) -> Vec<Instance> {
        let mut found = Vec::new();
        self.reach_debug(ty, every, &mut HashSet::new(), &mut found);
        found
    }

    fn reach_debug(
        &self,
        ty: &Type,
        every: bool,
        seen: &mut HashSet<Type>,
        found: &mut Vec<Instance>,
    ) {
        if plain(ty) || !seen.insert(ty.clone()) {
            return;
        }
        match ty {
            Type::Ref(inner) => self.reach_debug(inner, every, seen, found),
            Type::Interface(key, _) => {
                for (id, target) in &self.conversions {
                    let Type::Interface(k, _) = target else {
                        continue;
                    };
                    let source = self.types.get(id.0 as usize).cloned().flatten();
                    if let Some(source) = source.filter(|_| k == key) {
                        self.reach_debug(&source, every, seen, found);
                    }
                }
            }
            _ => found.extend(self.debug_instance(ty, every)),
        }
    }

    /// The instance of `ty`'s own `debug`: its `impl Debug`'s method, or
    /// the item of the block covering it.
    fn debug_instance(&self, ty: &Type, every: bool) -> Option<Instance> {
        let debug = Applied {
            key: super::derive::DEBUG.to_string(),
            args: Vec::new(),
        };
        if let Some(entry) = self.impl_for(ty, &debug) {
            let (Type::Struct(owner, args) | Type::Enum(owner, args)) = ty else {
                let Type::Handle(owner) = ty else {
                    return None;
                };
                let m = self.methods.get(owner)?.get(&entry.method("debug"))?;
                return Some(Instance {
                    key: m.key.clone(),
                    args: Vec::new(),
                });
            };
            let m = self.methods.get(owner)?.get(&entry.method("debug"))?;
            let listed = every || !args.is_empty() || super::instances::lazy_key(&m.key);
            return listed.then(|| Instance {
                key: m.key.clone(),
                args: args.clone(),
            });
        }
        let union = |key: &str| self.holds_union(key);
        self.derives
            .target(super::derive::DEBUG, "debug", ty, union)
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{diagnostics, errors, ok};

    const TYPES: &str = "struct P { x: i64, s: string }\nenum E { A(f64), B }\n\
        struct Stack<T> { items: T[] }\n\
        impl Display for Stack<T> { fun show(self): string = \"stack\"; }\n\
        interface Shape { fun area(self): f64; }\nfun nothing() {}\n";

    fn monos(body: &str) -> Vec<String> {
        ok(&format!("{TYPES}{body}"))
            .monos
            .iter()
            .map(|m| {
                let args: Vec<String> = m.args.iter().map(Type::to_string).collect();
                format!("{}<{}>", m.key, args.join(", "))
            })
            .collect()
    }

    #[test]
    fn printing_needs_display_and_debug_shows_anything() {
        ok(&format!(
            "{TYPES}fun dump<T: Display>(x: T): string = to_string(x);\n\
             fun peek<T>(x: T): string = x.debug();\n\
             fun main(console: Console) {{\n\
             let p = P {{ x: 1, s: \"a\" }};\n\
             console.println(p.debug()); console.print(E.B.debug()); console.println([1].debug()); console.println(Some(2).debug());\n\
             let r: Result<i64, string> = Ok(1); console.eprintln(r.debug());\n\
             let t = \"{{Stack {{ items: [1] }}}} {{[E.A(1.0)].debug()}}\"; let u = to_string(\"s\");\n\
             let v = dump(Stack {{ items: [p] }}); let w = peek(p); let n = debug(1) + 1;\n\
             let f = fun(x: i64): i64 = x; console.println(f.debug());\n}}"
        ));
        let e = |body: &str| {
            errors(&format!("{TYPES}fun main(console: Console) {{ {body} }}"))
                .into_iter()
                .map(|(c, _)| c)
                .collect::<Vec<_>>()
        };
        for body in [
            "console.println(P { x: 1, s: \"a\" });",
            "let t = \"{[1]}\";",
            "let t = to_string(Some(1));",
            "console.println(nothing());",
            "console.println(E.B);",
        ] {
            assert_eq!(e(body), ["E0422"], "{body}");
        }
        let d = diagnostics(&format!(
            "{TYPES}fun main(console: Console) {{ console.println([1]); }}"
        ));
        assert_eq!(d[0].message, "`i64[]` doesn't implement `Display`");
        assert!(d[0].help.as_deref().unwrap().contains(".debug()"), "{d:?}");
        let d = diagnostics(&format!(
            "{TYPES}fun f<T>(console: Console, x: T) {{ console.println(x); }}\nfun main() {{}}"
        ));
        assert!(
            d[0].help.as_deref().unwrap().contains("T: Display"),
            "{d:?}"
        );
    }

    #[test]
    fn a_printed_generic_type_lists_its_show_and_debug() {
        let direct = monos("fun main(console: Console) { console.println(Stack { items: [1] }); }");
        assert!(
            direct
                .iter()
                .any(|m| m.starts_with("Stack.show@Display<i64")),
            "{direct:?}"
        );
        let generic = monos(
            "fun dump<T: Display>(console: Console, x: T) { console.println(x); }\nfun main(console: Console) { dump(console, Stack { items: [true] }); }",
        );
        assert!(
            generic
                .iter()
                .any(|m| m.starts_with("Stack.show@Display<bool")),
            "{generic:?}"
        );
        let debugged = monos("fun main() { let s = [P { x: 1, s: \"a\" }].debug(); }");
        assert!(
            debugged.iter().any(|m| m.contains("Debug@array.debug<P>")),
            "{debugged:?}"
        );
        assert!(
            debugged.iter().any(|m| m.contains("Debug@struct.debug<P>")),
            "{debugged:?}"
        );
    }

    #[test]
    fn interface_values_show_what_they_hold_and_unwrap_shows_the_error() {
        ok(&format!(
            "{TYPES}struct C {{ r: f64 }}\nimpl Shape for C {{ fun area(self): f64 = self.r; }}\n\
             fun main() {{ let s: Shape = C {{ r: 1.0 }}; let t = s.debug(); let u = [s].debug(); }}"
        ));
        let found =
            monos("fun main() { let r: Result<i64, Stack<i64>> = Ok(1); let n = r.unwrap(); }");
        assert!(
            found
                .iter()
                .any(|m| m.contains("Debug@struct.debug<Stack<i64>>")),
            "{found:?}"
        );
    }
}
