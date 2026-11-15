//! Operators in codegen. An operator the checker gave a block calls that
//! block's method (`lugha_fn_Vec2.add@Add`, or a built-in type's block's
//! instance), receiver first like any `self` method. Everything else
//! (numbers, strings, and so every instance of a `T: Num` body at a number
//! type) keeps the built-in operator, so `add<i64>` is a plain checked `add`.

use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, ValueKind};

use super::CodegenError;
use super::instances::instance_name;
use super::lower::{Lowerer, POSITIONED};
use super::methods::{Method, key};
use crate::ast::Expr;
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// `lhs op rhs` (or `-lhs`) as a call of its operator block's method,
    /// when the checker chose one; `None` for the built-in operators.
    pub(super) fn operator(
        &mut self,
        expr: &Expr,
        name: &str,
        lhs: &Expr,
        rhs: Option<&Expr>,
    ) -> Result<Option<BasicValueEnum<'ctx>>, CodegenError> {
        let ty = self.ty(lhs);
        // A built-in type uses its block only where the checker chose one
        // that exists at this type; a `T: Add` instance at `string` doesn't.
        let program = matches!(ty, Type::Struct(..) | Type::Enum(..) | Type::Handle(_));
        if !program && self.built_in_block(expr, &ty, name).is_none() {
            return Ok(None);
        }
        let args: Vec<Expr> = rhs.into_iter().cloned().collect();
        let current = self.get(lhs, &ty)?;
        self.operator_on(expr, (current, &ty), name, &args)
            .map(Some)
    }

    /// Calls `ty`'s operator block's method `name` on `current` with
    /// `args`, for the operator (or compound assignment) whose checker id
    /// is `call`'s.
    pub(super) fn operator_on(
        &mut self,
        call: &Expr,
        (current, ty): (BasicValueEnum<'ctx>, &Type),
        name: &str,
        args: &[Expr],
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let method = self.operator_method(call, ty, name);
        // A `self` receiver is a pointer to a copy, as for any method call.
        let slot = self.entry_alloca(ty, "self");
        self.builder.build_store(slot, current).expect(POSITIONED);
        let mut values: Vec<BasicMetadataValueEnum> = vec![slot.into()];
        values.extend(self.arguments(args, &method.signature.params, false)?);
        let site = self
            .builder
            .build_call(method.signature.function, &values, name)
            .expect(POSITIONED);
        match site.try_as_basic_value() {
            ValueKind::Basic(result) => Ok(result),
            ValueKind::Instruction(_) => unreachable!("an operator method returns a value"),
        }
    }

    /// The instance of a built-in type's operator block that `call` reaches,
    /// when the checker chose one there.
    fn built_in_block(&self, call: &Expr, ty: &Type, name: &str) -> Option<Method<'ctx>> {
        let interface = self.derived.get(&call.id)?;
        crate::check::operator_interface(name)?;
        let at: Option<Vec<Type>> = self
            .nominal_calls
            .get(&call.id)
            .map(|(_, a)| a.iter().map(|t| self.concrete(t)).collect());
        let target = self
            .derives
            .target_at(interface, at.as_deref(), name, ty, |_| false)?;
        self.monos
            .get(&instance_name(&target.key, &target.args))
            .cloned()
    }

    /// The method of `ty`'s operator block for item `name` that `call`
    /// reaches: a program type's `impl` block's (an instance for a generic
    /// type), or a built-in type's block's instance.
    fn operator_method(&self, call: &Expr, ty: &Type, name: &str) -> Method<'ctx> {
        let owned = match ty {
            Type::Struct(owner, args) | Type::Enum(owner, args) => Some((owner, args.as_slice())),
            Type::Handle(owner) => Some((owner, &[][..])),
            _ => None,
        };
        // A built-in type's block (a tuple's, a number's): its instance.
        if let Some(m) = self.built_in_block(call, ty, name) {
            return m;
        }
        if let Some((owner, args)) = owned {
            let method = self
                .impl_method(call, ty, name)
                .unwrap_or_else(|| name.to_string());
            let k = key(owner, &method);
            let by_type = || {
                (!args.is_empty())
                    .then(|| self.monos.get(&instance_name(&k, args)).cloned())
                    .flatten()
            };
            return self
                .instance_of(call, &k)
                .or_else(by_type)
                .or_else(|| self.methods.get(&k).cloned())
                .or_else(|| self.monos.get(&k).cloned())
                .unwrap_or_else(|| panic!("checked: `{k}` is an operator method"));
        }
        panic!("checked: `{ty}` has an operator method `{name}`")
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::ir;

    const VEC2: &str = "struct Vec2 { x: f64, y: f64 }\n\
        impl Add for Vec2 { fun add(self, o: Vec2): Vec2 = Vec2 { x: self.x + o.x, y: self.y + o.y }; }\n\
        impl Neg for Vec2 { fun neg(self): Vec2 = Vec2 { x: -self.x, y: -self.y }; }\n";

    #[test]
    fn user_types_call_their_blocks_methods() {
        let ir = ir(&format!(
            "{VEC2}fun main() {{ let mut a = Vec2 {{ x: 1.0, y: 2.0 }}; let b = a + -a; a += b; }}"
        ));
        let main = &ir[ir.find("define void @lugha_fn_main").unwrap()..];
        assert_eq!(
            main.matches("call void @\"lugha_fn_Vec2.add@Add\"(ptr sret(%Vec2)")
                .count(),
            2,
            "{ir}"
        );
        assert!(main.contains("@\"lugha_fn_Vec2.neg@Neg\"("), "{ir}");
    }

    #[test]
    fn each_right_hand_type_and_a_number_on_the_left_has_its_block() {
        let ir = ir("struct V { x: f64 }\n\
            impl Mul for V { fun mul(self, w: V): V = w; }\n\
            impl Mul<f64> for V { fun mul(self, k: f64): V = self; }\n\
            impl Mul<V> for f64 { fun mul(self, v: V): V = v; }\n\
            fun main() { let v = V { x: 1.0 }; let a = v * v; let b = v * 2.0; let c = 2.0 * v; }");
        assert!(ir.contains("@\"lugha_fn_V.mul@Mul\"("), "{ir}");
        assert!(ir.contains("@\"lugha_fn_V.mul@Mul<f64>\"("), "{ir}");
        assert!(ir.contains("Mul<V>@f64.mul"), "{ir}");
    }

    #[test]
    fn brackets_on_user_types_call_index_and_set_index() {
        let ir = ir("struct B { xs: i64[] }\nimpl B {\n\
            fun index(self, i: i64): i64 = self.xs[i];\n\
            fun set_index(mut self, i: i64, v: i64) { self.xs[i] = v; }\n}\n\
            fun main() { let mut b = B { xs: [1, 2] }; b[0] = b[1]; }");
        let main = &ir[ir.find("define void @lugha_fn_main").unwrap()..];
        assert!(main.contains("call i64 @lugha_fn_B.index("), "{ir}");
        assert!(main.contains("call void @lugha_fn_B.set_index("), "{ir}");
    }
}
