//! Order in codegen: `<`, `<=`, `>` and `>=` on a struct, enum or tuple
//! call its `Ord`'s `cmp`, written or derived, and the `Ordering` it
//! returns becomes −1, 0 or 1.

use inkwell::IntPredicate;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, IntValue, ValueKind};

use super::instances::instance_name;
use super::lower::{Lowerer, POSITIONED};
use super::methods::{Method, key};
use super::value::is_aggregate;
use crate::check::{ORD, ORDERING, Type};

/// Whether `<` on `ty` calls `Ord`'s `cmp`: a struct, enum or tuple.
pub(super) fn user_ordered(ty: &Type) -> bool {
    matches!(ty, Type::Struct(..) | Type::Enum(..))
}

impl<'ctx> Lowerer<'ctx> {
    /// −1, 0 or 1 as `a.cmp(b)` is `Less`, `Equal` or `Greater`.
    pub(super) fn user_order(
        &mut self,
        a: BasicValueEnum<'ctx>,
        b: BasicValueEnum<'ctx>,
        ty: &Type,
    ) -> IntValue<'ctx> {
        let method = self.cmp_method(ty);
        // `self` is a pointer to a copy; `other` is passed like any argument.
        let receiver = self.entry_alloca(ty, "self");
        self.builder.build_store(receiver, a).expect(POSITIONED);
        let other: BasicMetadataValueEnum = if is_aggregate(ty) {
            let slot = self.entry_alloca(ty, "other");
            self.builder.build_store(slot, b).expect(POSITIONED);
            slot.into()
        } else {
            b.into()
        };
        let site = self
            .builder
            .build_call(method.signature.function, &[receiver.into(), other], "cmp")
            .expect(POSITIONED);
        let ValueKind::Basic(ordering) = site.try_as_basic_value() else {
            unreachable!("checked: `cmp` returns an `Ordering`")
        };
        let tag = self
            .builder
            .build_extract_value(ordering.into_struct_value(), 0, "tag")
            .expect("an enum value starts with its tag")
            .into_int_value();
        // By variant name, not by assumed numbers.
        let ordering_ty = Type::Enum(ORDERING.to_string(), Vec::new());
        let (less, _) = self.variant(&ordering_ty, "Less");
        let (greater, _) = self.variant(&ordering_ty, "Greater");
        let i32_type = self.context.i32_type();
        let is = |this: &Self, index: u32, name: &str| {
            this.builder
                .build_int_compare(
                    IntPredicate::EQ,
                    tag,
                    tag.get_type().const_int(u64::from(index), false),
                    name,
                )
                .expect(POSITIONED)
        };
        let (below, above) = (is(self, less, "less"), is(self, greater, "greater"));
        let up = self
            .builder
            .build_select(
                above,
                i32_type.const_int(1, false),
                i32_type.const_zero(),
                "up",
            )
            .expect(POSITIONED);
        self.builder
            .build_select(below, i32_type.const_all_ones().into(), up, "order")
            .expect(POSITIONED)
            .into_int_value()
    }

    /// `ty`'s `cmp`: its `impl Ord`'s (an instance for a generic type),
    /// else the covering derive block's instance.
    fn cmp_method(&self, ty: &Type) -> Method<'ctx> {
        let (Type::Struct(owner, args) | Type::Enum(owner, args)) = ty else {
            unreachable!("checked: only structs, enums and tuples go through `Ord`")
        };
        if self.derives.has_own(owner, "cmp") {
            let k = key(owner, &self.block_method(ty, ORD, &[], "cmp"));
            let instance = (!args.is_empty())
                .then(|| self.monos.get(&instance_name(&k, args)).cloned())
                .flatten();
            return instance
                .or_else(|| self.methods.get(&k).cloned())
                .or_else(|| self.monos.get(&k).cloned())
                .unwrap_or_else(|| panic!("checked: `{k}` is listed for `{ty}`"));
        }
        let target = self
            .derives
            .target(ORD, "cmp", ty, |_| false)
            .unwrap_or_else(|| panic!("checked: `{ty}` is `Ord`"));
        let k = instance_name(&target.key, &target.args);
        self.monos
            .get(&k)
            .cloned()
            .unwrap_or_else(|| panic!("checked: `{k}` is listed"))
    }
}
