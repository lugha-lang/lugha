//! Generic interfaces: an interface with its
//! type arguments and its items at those arguments. Bounds that pass
//! arguments are in `check::bound_args`.

use std::fmt;

use super::generics::{Generic, subst};
use super::interfaces::Requirement;
use super::{Checker, Type};
use crate::ast::TypeParam;

/// An interface with its type arguments: `Source<i64>`, or `Shape` with none.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Applied {
    /// The interface's key.
    pub key: String,
    /// Its type arguments, one per parameter; empty for a plain interface.
    pub args: Vec<Type>,
}

impl Applied {
    /// `self` with `Self` in its arguments (from a default, `R = Self`)
    /// standing for `subject`: the bounded parameter or the block's type.
    pub(super) fn with_self(mut self, subject: &Type) -> Applied {
        let map = std::collections::HashMap::from([(super::SELF.to_string(), subject.clone())]);
        self.args = self.args.iter().map(|a| a.substitute(&map)).collect();
        self
    }

    /// The interface of an interface value's type, `None` for any other type.
    pub(super) fn of(ty: &Type) -> Option<Applied> {
        let Type::Interface(key, args) = ty else {
            return None;
        };
        Some(Applied {
            key: key.clone(),
            args: args.clone(),
        })
    }

    /// The type of this interface's values.
    pub(super) fn ty(&self) -> Type {
        Type::Interface(self.key.clone(), self.args.clone())
    }

    /// The same interface with `generics` replaced by `args` in its arguments.
    pub(super) fn subst(&self, generics: &[Generic], args: &[Type]) -> Applied {
        Applied {
            key: self.key.clone(),
            args: self.args.iter().map(|a| subst(a, generics, args)).collect(),
        }
    }
}

impl fmt::Display for Applied {
    /// As written: `Source<i64>`, `Shape`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ty())
    }
}

impl Checker {
    /// The items of `applied`, with the interface's parameters replaced by
    /// its arguments.
    pub(super) fn items(&self, applied: &Applied) -> Vec<Requirement> {
        let Some(info) = self.interfaces.get(&applied.key) else {
            return Vec::new();
        };
        info.items
            .iter()
            .map(|r| Requirement {
                params: r
                    .params
                    .iter()
                    .map(|t| subst(t, &info.generics, &applied.args))
                    .collect(),
                ret: subst(&r.ret, &info.generics, &applied.args),
                ..r.clone()
            })
            .collect()
    }

    /// An interface value's items: those of every interface it extends,
    /// transitively and each once, ancestors first in declaration order,
    /// then its own; each with the interface that declares it, at
    /// `applied`'s arguments.
    pub(super) fn value_items(&self, applied: &Applied) -> Vec<(Applied, Requirement)> {
        let mut out = Vec::new();
        self.collect_value_items(applied, &mut out, 0);
        out
    }

    fn collect_value_items(
        &self,
        applied: &Applied,
        out: &mut Vec<(Applied, Requirement)>,
        depth: usize,
    ) {
        // A cycle is reported where the interfaces are declared; stop here.
        let Some(info) = self.interfaces.get(&applied.key).filter(|_| depth < 32) else {
            return;
        };
        for parent in &info.extends {
            let args = parent
                .args
                .iter()
                .map(|t| subst(t, &info.generics, &applied.args));
            let parent = Applied {
                key: parent.key.clone(),
                args: args.collect(),
            };
            self.collect_value_items(&parent, out, depth + 1);
        }
        for req in self.items(applied) {
            if !out.iter().any(|(_, r)| r.name == req.name) {
                out.push((applied.clone(), req));
            }
        }
    }

    /// The first item of `key` or an interface it extends that keeps its
    /// values from existing: an associated function, or one using `Self`.
    pub(super) fn value_blockers(&self, key: &str) -> (Option<String>, Option<String>) {
        let mut todo = vec![key.to_string()];
        let mut seen: Vec<String> = Vec::new();
        let (mut not_value, mut uses_self) = (None, None);
        while let Some(k) = todo.pop() {
            if seen.contains(&k) {
                continue;
            }
            if let Some(info) = self.interfaces.get(&k) {
                not_value = not_value.or_else(|| info.not_value.clone());
                uses_self = uses_self.or_else(|| info.uses_self.clone());
                todo.extend(info.extends.iter().map(|p| p.key.clone()));
            }
            seen.push(k);
        }
        (not_value, uses_self)
    }

    /// True if `ancestor` is `of` or an interface it extends, transitively.
    pub(super) fn extends_or_is(&self, of: &str, ancestor: &str) -> bool {
        let mut todo = vec![of.to_string()];
        let mut seen: Vec<String> = Vec::new();
        while let Some(key) = todo.pop() {
            if key == ancestor {
                return true;
            }
            if seen.contains(&key) {
                continue;
            }
            if let Some(info) = self.interfaces.get(&key) {
                todo.extend(info.extends.iter().map(|p| p.key.clone()));
            }
            seen.push(key);
        }
        false
    }

    /// The parameters of interface `key`; none for a plain one.
    pub(super) fn interface_generics(&self, key: &str) -> Vec<Generic> {
        self.interfaces
            .get(key)
            .map(|i| i.generics.clone())
            .unwrap_or_default()
    }

    /// Runs `f`, dropping what it reports: for resolving declarations
    /// early, before the pass that checks them.
    pub(super) fn quietly<T>(&mut self, f: impl FnOnce(&mut Checker) -> T) -> T {
        let reported = self.diagnostics.len();
        let result = f(self);
        self.diagnostics.truncate(reported);
        result
    }

    /// The parameters `params` as written, with `outer` in scope for their
    /// bounds' arguments: names first, then each bound resolved quietly.
    pub(super) fn generics_with(
        &mut self,
        params: &[TypeParam],
        outer: &[Generic],
    ) -> Vec<Generic> {
        let names: Vec<Generic> = params
            .iter()
            .map(|p| Generic {
                name: p.name.name.clone(),
                bounds: Default::default(),
            })
            .collect();
        let saved = std::mem::replace(&mut self.type_params, [outer, &names].concat());
        let generics = params
            .iter()
            .map(|p| Generic {
                name: p.name.name.clone(),
                bounds: self.bounds_of(p),
            })
            .collect();
        self.type_params = saved;
        generics
    }
}

#[cfg(test)]
pub(super) mod tests {
    use crate::check::test_util::{diagnostics, errors, ok};

    pub(in crate::check) const SOURCE: &str = "interface Source<T> { fun next(mut self): Option<T>; }\n\
        struct Count { n: i64, end: i64 }\n\
        impl Source<i64> for Count {\n\
            fun next(mut self): Option<i64> {\n\
                if self.n == self.end { return None; }\n\
                self.n += 1;\n\
                Some(self.n)\n\
            }\n\
        }\n\
        struct Repeat<T> { x: T, left: i64 }\n\
        impl Source<T> for Repeat<T> {\n\
            fun next(mut self): Option<T> {\n\
                if self.left == 0 { return None; }\n\
                self.left -= 1;\n\
                Some(self.x)\n\
            }\n\
        }\n";

    pub(in crate::check) fn e(body: &str) -> Vec<(&'static str, String)> {
        errors(&format!("{SOURCE}{body}\nfun main() {{}}"))
            .into_iter()
            .map(|(c, t)| (c, t.to_string()))
            .collect()
    }

    pub(in crate::check) fn codes(body: &str) -> Vec<&'static str> {
        e(body).into_iter().map(|(c, _)| c).collect()
    }

    #[test]
    fn declarations_use_their_parameters() {
        ok(&format!("{SOURCE}fun main() {{}}"));
        assert_eq!(codes("interface I<T, T> { fun f(self): T; }"), ["E0316"]);
        assert_eq!(codes("interface I<Self> { fun f(self); }"), ["E0316"]);
        assert_eq!(codes("interface I<T> { fun f(self): Nope<T>; }"), ["E0305"]);
    }

    #[test]
    fn satisfaction_substitutes_the_arguments() {
        ok(&format!(
            "{SOURCE}fun a<S: Source<i64>>(s: S) {{}}\n\
             fun b<S: Source<string>>(s: S) {{}}\n\
             fun c<U: Source<i64>>(u: U) {{ a(u); }}\n\
             fun main() {{ a(Count {{ n: 0, end: 1 }}); b(Repeat {{ x: \"x\", left: 1 }}); }}"
        ));
        let src = format!(
            "{SOURCE}fun b<S: Source<string>>(s: S) {{}}\n\
             fun main() {{ b(Count {{ n: 0, end: 1 }}); }}"
        );
        assert_eq!(errors(&src).len(), 1);
        let d = &diagnostics(&src)[0];
        assert_eq!(d.code, "E0422");
        assert_eq!(d.message, "`Count` doesn't implement `Source<string>`");
        let help = d.help.as_deref().expect("a help");
        assert!(
            help.contains("add `impl Source<string> for Count"),
            "{help}"
        );
    }

    #[test]
    fn generic_interfaces_are_value_types() {
        ok(&format!(
            "{SOURCE}struct Pipe {{ from: Source<i64> }}\n\
             fun f(s: Source<i64>): Option<i64> {{ let mut t = s; t.next() }}\n\
             fun main() {{\n\
                 let s: Source<i64> = Count {{ n: 0, end: 1 }};\n\
                 let all: Source<i64>[] = [Count {{ n: 0, end: 1 }}, Repeat {{ x: 7, left: 1 }}];\n\
                 let p = Pipe {{ from: Count {{ n: 0, end: 1 }} }};\n\
                 let o: Option<Source<string>> = Some(Repeat {{ x: \"r\", left: 1 }});\n\
                 let n = f(s);\n\
             }}"
        ));
        assert_eq!(e("fun f(s: Source) {}"), [("E0418", "Source".into())]);
        assert_eq!(
            e("fun f(s: Source<i64, i64>) {}"),
            [("E0418", "Source<i64, i64>".into())]
        );
        let sorted = "interface Sorted<T: Ord> { fun get(self): T; }\nstruct P {}\n";
        assert_eq!(
            codes(&format!("{sorted}fun f(s: Sorted<P>) {{}}")),
            ["E0422"]
        );
    }

    #[test]
    fn conversion_and_calls_use_the_arguments() {
        let src =
            format!("{SOURCE}fun main() {{ let s: Source<string> = Count {{ n: 0, end: 1 }}; }}");
        let d = &diagnostics(&src)[0];
        assert_eq!(d.code, "E0422");
        let help = d.help.as_deref().expect("a help");
        assert!(help.contains("impl Source<string> for Count"), "{help}");
        assert_eq!(
            codes("fun w(s: Source<string>) {}\nfun f(s: Source<i64>) { w(s); }"),
            ["E0403"]
        );
        assert_eq!(
            codes("fun f(s: Source<i64>): string { let mut t = s; t.next().unwrap_or(0) }"),
            ["E0403"]
        );
        assert_eq!(codes("fun f(s: Source<i64>) { s.next(); }"), ["E0501"]);
    }

    #[test]
    fn values_satisfy_bounds_and_match_type_patterns() {
        ok(&format!(
            "{SOURCE}fun a<S: Source<i64>>(s: S) {{}}\n\
             fun d(s: Source<i64>): i64 = match s {{ c: Count => c.end, _: Repeat<i64> => 1, _ => 0 }};\n\
             fun main() {{ let s: Source<i64> = Count {{ n: 0, end: 1 }}; a(s); let n = d(s); }}"
        ));
        assert_eq!(
            codes("fun d(s: Source<string>): i64 = match s { c: Count => 1, _ => 0 };"),
            ["E0426"]
        );
    }

    #[test]
    fn an_extending_interface_is_a_value_with_its_ancestors_items() {
        let src = "interface Named { fun name(self): string; }\n\
            interface Greeter: Named { fun greet(self): string = \"hi {self.name()}\"; }\n\
            struct A {}\n\
            impl Named for A { fun name(self): string = \"a\"; }\n\
            impl Greeter for A {}\n\
            fun main() { let g: Greeter = A {}; let s = g.name() + g.greet(); let n: Named = g; }";
        let checked = crate::check::test_util::ok(src);
        let shape = &checked.interfaces["Greeter"];
        let names: Vec<&str> = shape.methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["name", "greet"]);
        assert_eq!(shape.methods[0].owner, "Named");
        // A plain value meets `Error` only by `impl`; an interface value only
        // by extending it.
        let src = "struct P {}\nimpl Display for P { fun show(self): string = \"p\"; }\n\
            fun main() { let d: Display = P {}; let e: Error = d; }";
        assert_eq!(crate::check::test_util::errors(src)[0].0, "E0422");
        crate::check::test_util::ok(
            "struct P {}\nimpl Display for P { fun show(self): string = \"p\"; }\nimpl Error for P {}\n\
             fun main() { let e: Error = P {}; let s = \"{e}\"; let d: Display = e; }",
        );
    }
}
