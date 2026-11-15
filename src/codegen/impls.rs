//! Items of interfaces met through `impl I for T`: the block's method for
//! a type, which `modules::impls` named `item@tag`. A type whose block
//! leaves an item to the interface's default has no such method, and the
//! callers fall back to the default's instance.

use super::lower::Lowerer;
use crate::ast::Expr;
use crate::check::{Type, find_impl};

impl Lowerer<'_> {
    /// The name of `ty`'s method for item `name` of interface `key` at
    /// `args`: its block's, when `ty` implements the interface through
    /// one, else `name` itself.
    pub(super) fn block_method(&self, ty: &Type, key: &str, args: &[Type], name: &str) -> String {
        let (owner, targs) = match ty {
            Type::Struct(owner, targs) | Type::Enum(owner, targs) => (owner, targs.as_slice()),
            Type::Handle(owner) => (owner, &[][..]),
            _ => return name.to_string(),
        };
        match find_impl(&self.impls, owner, targs, key, args) {
            Some(entry) => entry.method(name),
            None => name.to_string(),
        }
    }

    /// For a call the checker found to be a nominal interface's item: the
    /// name of `ty`'s method for item `name` (which may not exist, when
    /// the default stands in).
    pub(super) fn impl_method(&self, call: &Expr, ty: &Type, name: &str) -> Option<String> {
        let (key, args) = self.nominal_calls.get(&call.id)?;
        let args: Vec<Type> = args.iter().map(|a| self.concrete(a)).collect();
        Some(self.block_method(ty, key, &args, name))
    }
}
