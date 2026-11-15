//! `impl` blocks for a type this module doesn't declare. An interface's
//! block may be for another module's type (`impl Shape for geo.Circle`),
//! in the interface's module; a block of a type's own methods may not
//! (E0466): `impl f64 { … }` and `impl geo.W { … }` hold nothing, and an
//! operator with such a type on the left is a block of its interface,
//! `impl Mul<V> for f64`, in `V`'s module.

use super::{Checker, Type};
use crate::ast::{AppliedType, ImplDecl, TypeKind};
use crate::diagnostic::Diagnostic;

/// The number types' names, as an `impl` may write them.
const NUMBERS: [&str; 10] = [
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
];

/// True for an `impl` of a type the module can't declare: another module's
/// (`impl geo.W`) or a number (`impl f64`).
pub(super) fn foreign(i: &ImplDecl) -> bool {
    i.module.is_some() || NUMBERS.contains(&i.ty.name.as_str())
}

impl Checker {
    /// The owner of an interface's block for another module's type, with
    /// its type: `None` (reported) when the type doesn't exist, or when the
    /// block holds the type's own methods (E0466).
    pub(super) fn foreign_owner(&mut self, i: &ImplDecl) -> Option<(String, Type)> {
        if i.interface.is_none() {
            self.report(own_methods_elsewhere(i));
            return None;
        }
        let module = i.module.as_ref()?;
        let written = crate::ast::Type {
            kind: TypeKind::Applied(Box::new(AppliedType {
                module: Some(module.clone()),
                name: i.ty.clone(),
                args: Vec::new(),
            })),
            span: i.ty.span,
        };
        // A generic type takes the `impl`'s parameters, checked after.
        let saved = std::mem::take(&mut self.type_params);
        let ty = self.resolve(&written);
        self.type_params = saved;
        let owner = match &ty {
            Type::Struct(k, _) | Type::Enum(k, _) | Type::Handle(k) => k.clone(),
            _ => return None,
        };
        self.impl_owners.insert(i.span, ty.clone());
        Some((owner, ty))
    }
}

/// E0466: `impl T { … }` for a type `T` this module doesn't declare.
pub(super) fn own_methods_elsewhere(i: &ImplDecl) -> Diagnostic {
    let ty = match &i.module {
        Some(m) => format!("{}.{}", m.name, i.ty.name),
        None => i.ty.name.clone(),
    };
    let message = format!("methods for `{ty}`, which this module doesn't declare");
    let help = match i.module {
        Some(_) => format!(
            "write them in `{ty}`'s own module; for an operator with `{ty}` on the left, write `impl Mul<V> for {ty} {{ … }}` in `V`'s module"
        ),
        None => format!(
            "a built-in type has no methods of a program's; for `{ty} * v`, write `impl Mul<V> for {ty} {{ fun mul(self, v: V): … }}` in `V`'s module, and for anything else a function"
        ),
    };
    Diagnostic::error("E0466", message, i.ty.span).with_help(help)
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{errors, ok};

    #[test]
    fn a_block_of_own_methods_for_a_number_is_e0466() {
        assert_eq!(
            errors("struct V { x: f64 }\nimpl f64 { fun mul(self, v: V): V = v; }\nfun main() {}"),
            [("E0466", "f64")]
        );
        ok(
            "struct V { x: f64 }\nimpl Mul<V> for f64 { fun mul(self, v: V): V = v; }\n\
            fun main() { let v = 2.0 * V { x: 1.0 }; }",
        );
    }
}
