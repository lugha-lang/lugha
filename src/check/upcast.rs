//! Converting between interfaces: a value of interface type `J` becomes an
//! `I` only when `J` is `I` or extends it, at any depth; having items of
//! `I`'s names doesn't count. Interfaces still met by their methods' names
//! (`Iter`, the derivable ones) convert by items.

use super::applied::Applied;
use super::{Checker, Type};

impl Checker {
    /// What `from` lacks to convert to `to`, both interfaces at their
    /// arguments, one entry per item: empty when it declares them all.
    pub(super) fn undeclared(&self, from: &Applied, to: &Applied) -> Vec<String> {
        // A nominal interface is only the value of one that extends it.
        if self.converts_by_ancestry(&to.key) && !self.extends_or_is(&from.key, &to.key) {
            return vec![format!("`{from}` doesn't extend `{to}`")];
        }
        let declared: Vec<_> = self.value_items(from).into_iter().map(|(_, r)| r).collect();
        let mut problems = Vec::new();
        for (_, req) in self.value_items(to) {
            match declared.iter().find(|d| d.name == req.name) {
                Some(d) if d.same(&req) => {}
                Some(_) => problems.push(format!("`{}` is declared differently", req.name)),
                None => problems.push(format!("no `{}`", req.name)),
            }
        }
        problems
    }

    /// True for an interface whose values come only from interfaces
    /// extending it: every nominal one but the operators'.
    pub(super) fn converts_by_ancestry(&self, key: &str) -> bool {
        self.nominal(key) && !super::impls::is_operator(key)
    }

    /// True if a value of type `from`, another interface than `to`, converts
    /// to `to`; E0422 naming what it lacks otherwise.
    pub(super) fn upcasts(&mut self, from: &Type, to: &Applied, span: crate::span::Span) -> bool {
        let source = Applied::of(from).expect("an interface type");
        // A nominal interface asks for ancestry, not items.
        if self.converts_by_ancestry(&to.key) && !self.extends_or_is(&source.key, &to.key) {
            self.report(super::errors::not_extending(from, &to.to_string(), span));
            return false;
        }
        let problems = self.undeclared(&source, to);
        if problems.is_empty() {
            return true;
        }
        self.report(super::errors::undeclared(
            from,
            &to.to_string(),
            &problems,
            span,
        ));
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{diagnostics, errors, ok};

    const PETS: &str = "interface Named { fun name(self): string; }\n\
        interface Pet: Named { fun sound(self): string; }\n\
        interface Label { fun name(self): string; }\n\
        interface Odd { fun name(self): i64; }\n\
        interface Sized { fun size(self): i64; }\n\
        interface Source<T>: Sized { fun next(mut self): Option<T>; }\n\
        struct Dog { n: string }\n\
        impl Pet for Dog { fun sound(self): string = \"woof\"; }\n\
        impl Named for Dog { fun name(self): string = self.n; }\n\
        struct Wrap { n: Named }\n";

    fn codes(body: &str) -> Vec<&'static str> {
        let src = format!("{PETS}{body}\nfun main() {{}}");
        errors(&src).into_iter().map(|(c, _)| c).collect()
    }

    #[test]
    fn a_value_converts_to_an_interface_it_extends() {
        ok(&format!(
            "{PETS}fun take(n: Named) {{}}\n\
             fun back(p: Pet): Named = p;\n\
             fun all(p: Pet): Named[] = [p, Dog {{ n: \"d\" }}];\n\
             fun some(p: Pet): Option<Named> = Some(p);\n\
             fun field(p: Pet): Wrap = Wrap {{ n: p }};\n\
             fun sized(s: Source<i64>): Sized = s;\n\
             fun main() {{ let p: Pet = Dog {{ n: \"d\" }}; let n: Named = p; take(p); }}"
        ));
    }

    #[test]
    fn only_extending_converts_never_matching_names() {
        // Down the tree, sideways by names, and by a different item: E0422.
        assert_eq!(codes("fun f(n: Named): Pet = n;"), ["E0422"]);
        assert_eq!(codes("fun f(l: Label): Named = l;"), ["E0422"]);
        assert_eq!(codes("fun f(o: Odd): Named = o;"), ["E0422"]);
        let src = format!("{PETS}fun f(l: Label): Named = l;\nfun main() {{}}");
        let d = &diagnostics(&src)[0];
        assert!(
            d.message.contains("only an interface that extends"),
            "{}",
            d.message
        );
        // As a type argument too.
        assert_eq!(
            codes("fun g<T: Named>(x: T) {}\nfun f(l: Label) { g(l); }"),
            ["E0422"]
        );
        ok(&format!(
            "{PETS}fun g<T: Named>(x: T) {{}}\nfun f(p: Pet) {{ g(p); }}\nfun main() {{}}"
        ));
    }

    #[test]
    fn other_arguments_and_containers_stay_e0403() {
        assert_eq!(
            codes("fun f(s: Source<i64>): Source<string> = s;"),
            ["E0403"]
        );
        assert_eq!(codes("fun f(ps: Pet[]): Named[] = ps;"), ["E0403"]);
    }

    #[test]
    fn question_mark_converts_interface_errors() {
        let base = "interface Problem { fun text(self): string; }\n\
            interface Parse: Problem { fun at(self): i64; }\n\
            interface Other { fun at(self): i64; }\n\
            fun p(): Result<i64, Parse> = Ok(1);\n\
            fun q(): Result<i64, Other> = Ok(1);\n";
        ok(&format!(
            "{base}fun r(): Result<i64, Problem> {{ let n = p()?; Ok(n) }}\nfun main() {{}}"
        ));
        let src = format!(
            "{base}fun r(): Result<i64, Problem> {{ let n = q()?; Ok(n) }}\nfun main() {{}}"
        );
        assert_eq!(
            errors(&src).iter().map(|(c, _)| *c).collect::<Vec<_>>(),
            ["E0424"]
        );
    }
}
