//! The prelude's blocks of a built-in type's own methods: `impl string { … }`,
//! `impl<T> T[] { … }`, `impl<T: Ord> T[] { … }`, `impl string[] { … }`,
//! `impl char { … }`, `impl<T> Option<T> { … }`. Each is recorded like a
//! block for a built-in type (`check::builtin_impls`), with no interface:
//! its methods are a hidden owner's, generic over the block's parameters
//! and then their own, and `s.trim()` calls the one whose block covers the
//! receiver (`helper_call`). Only the prelude writes them (E0466 elsewhere).

use super::derive::{Block, Covers};
use super::derive_match::label;
use super::methods::Method;
use super::{Checker, Type};
use crate::ast::ImplDecl;
use crate::modules::PRELUDE_PATH;

impl Checker {
    /// Records `impl<…> Type { … }`, a block of `target`'s own methods.
    pub(super) fn collect_own_block(&mut self, i: &ImplDecl, target: &crate::ast::Type) {
        if self.module.join(".") != PRELUDE_PATH {
            self.report(super::foreign_impls::own_methods_elsewhere(i));
            return;
        }
        let generics = self.generics_with(&i.params, &[]);
        self.type_params = generics.clone();
        let ty = self.resolve(target);
        let covers = Covers::Type(ty);
        // One owner per block: `$array`, then `$array@2` for the next.
        let base = format!("${}", label(&covers));
        let taken = |c: &Self, o: &str| c.derives.blocks.iter().any(|b| b.owner == o);
        let mut owner = super::qualify(&self.module, &base);
        let mut n = 2;
        while taken(self, &owner) {
            owner = super::qualify(&self.module, &format!("{base}@{n}"));
            n += 1;
        }
        let mut items = Vec::new();
        for f in &i.methods {
            let own = self.generics_with(&f.type_params, &generics);
            self.type_params = [generics.clone(), own].concat();
            self.check_params(&f.type_params, &generics);
            let params: Vec<Type> = f.params.iter().map(|p| self.resolve(&p.ty)).collect();
            let ret = f.ret.as_ref().map_or(Type::Unit, |t| self.resolve(t));
            let receiver = f.receiver.map(|r| r.mutable);
            let sig = super::Sig::of(
                &self.type_params,
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
                generics: self.type_params.clone(),
                outer: generics.len(),
                key: format!("{owner}.{}", f.name.name),
            };
            let methods = self.methods.entry(owner.clone()).or_default();
            methods.insert(f.name.name.clone(), method);
            items.push(f.name.name.clone());
        }
        self.type_params.clear();
        self.derives.blocks.push(Block {
            interface: String::new(),
            covers,
            generics,
            owner,
            span: i.span,
            written: true,
            args: Vec::new(),
            items,
        });
    }

    /// The prelude's own-method block covering built-in `receiver` that
    /// has method `name`, with what its parameters stand for there.
    pub(super) fn own_block(&self, receiver: &Type, name: &str) -> Option<(Block, Vec<Type>)> {
        self.derives
            .blocks_for("", receiver)
            .into_iter()
            .find(|(b, _)| b.items.iter().any(|i| i == name))
            .map(|(b, args)| (b.clone(), args))
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type, partial};

    #[test]
    fn the_preludes_blocks_give_built_in_types_their_methods() {
        let ty = |body: &str, name: &str| let_type(&format!("fun main() {{ {body} }}"), name);
        assert_eq!(
            ty("let a = [1].map(fun(n: i64): string = \"{n}\");", "a"),
            Type::Array(Box::new(Type::String))
        );
        assert_eq!(ty("let a = \" x \".trim();", "a"), Type::String);
        assert_eq!(ty("let a = Some(1).unwrap_or(2);", "a"), Type::I64);
        assert_eq!(
            ty("let a = ['a'].map(fun(c: char): char = c.to_upper());", "a"),
            Type::Array(Box::new(Type::Char))
        );
        // The compiler's built-ins come first; a block's bounds are the call's.
        assert_eq!(
            ty("let mut xs = [1]; xs.push(2); let a = xs.pop();", "a"),
            Type::Option(Box::new(Type::I64))
        );
        assert_eq!(errors("fun main() { let a = [2.5].sort(); }")[0].0, "E0451");
        assert_eq!(
            errors("impl string { fun shout(self): string = self; }\nfun main() {}")[0].0,
            "E0466"
        );
        assert_eq!(
            errors("fun main() { let a = str_trim(\" x\"); }")[0].0,
            "E0301"
        );
    }

    #[test]
    fn a_method_call_leads_to_the_preludes_method() {
        let src = "fun main() { let a = \" x \".trim(); }";
        let p = partial(src);
        let at = src.find("trim").expect("a call");
        let target = p
            .definitions
            .iter()
            .find(|(span, _)| span.start == at)
            .map(|(_, t)| t.clone());
        assert!(
            target.is_some(),
            "no definition for `trim`: {:?}",
            p.definitions.keys().collect::<Vec<_>>()
        );
    }
}
