//! Weak references in codegen. A `Weak<T>` is a pointer to its box's weak
//! cell (`runtime/memory/weak.c`), counted like any object. In a program that
//! uses `Weak`, every `Ref` box has one more word after its contents, its
//! cell or null, and its visit empties the cell when the box is freed.
//! Programs without `Weak` lay boxes out as before.

use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

use super::CodegenError;
use super::copy::mangle;
use super::lower::{Lowerer, POSITIONED};
use super::rc::RcOp;
use super::value::Value;
use crate::ast::Expr;
use crate::check::Type;

/// True if the program, by its expressions' `types`, makes weak
/// references: only then do its boxes carry a cell's word.
pub(super) fn uses_weak(types: &[Type]) -> bool {
    types.iter().any(|ty| matches!(ty, Type::Weak(_)))
}

impl<'ctx> Lowerer<'ctx> {
    /// Where a `Ref<contents>` box keeps its weak cell: after the
    /// contents, at the next word.
    pub(super) fn weak_slot(&self, contents: &Type) -> u64 {
        self.element_size(contents).div_ceil(8) * 8
    }

    /// The size of a `Ref<contents>` box.
    pub(super) fn box_size(&self, contents: &Type) -> u64 {
        if self.uses_weak {
            self.weak_slot(contents) + 8
        } else {
            self.element_size(contents).max(1)
        }
    }

    /// The visit of a `Ref<contents>` box: its contents', and with weak
    /// references its cell's too.
    pub(super) fn rc_ref_box_drop(&mut self, contents: &Type) -> Option<FunctionValue<'ctx>> {
        if !self.uses_weak {
            return self.rc_box_drop(contents);
        }
        let name = format!("lugha_rc_drop_wbox_{}", mangle(contents));
        if let Some(function) = self.module.get_function(&name) {
            return Some(function);
        }
        let contents = contents.clone();
        let slot = self.weak_slot(&contents);
        let counted = self.counted(&contents);
        Some(self.rc_visit_function(&name, |this, object, op| {
            if counted {
                let value = this.load_element(object, &contents);
                this.rc_value(value, &contents, op);
            }
            let visit = this.runtime(
                "lugha_rt_weak_visit",
                &[Type::String, Type::I64, Type::I64],
                None,
            );
            let i64_type = this.context.i64_type();
            // The runtime's `RC_MARK` is 1 and `RC_RELEASE` 0.
            let op = i64_type.const_int(u64::from(matches!(op, RcOp::Mark)), false);
            this.builder
                .build_call(
                    visit,
                    &[
                        object.into(),
                        i64_type.const_int(slot, false).into(),
                        op.into(),
                    ],
                    "",
                )
                .expect(POSITIONED);
        }))
    }

    /// `Weak.new(r)`, `call` being the whole call.
    pub(super) fn weak_new(
        &mut self,
        call: &Expr,
        args: &[Expr],
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let Type::Weak(inner) = self.ty(call) else {
            unreachable!("checked: `Weak.new` makes a `Weak`")
        };
        let box_ty = Type::Ref(inner.clone());
        let box_ptr = self.get(&args[0], &box_ty)?;
        let new = self.runtime(
            "lugha_rt_weak_new",
            &[Type::String, Type::I64],
            Some(Type::String),
        );
        let slot = self
            .context
            .i64_type()
            .const_int(self.weak_slot(&inner), false);
        Ok(self.call_basic(new, &[box_ptr, slot.into()], "weak"))
    }

    /// `w.get()`: `Some` box, held for the caller, or `None` once freed.
    pub(super) fn weak_get(
        &mut self,
        base: &Expr,
        inner: &Type,
    ) -> Result<Value<'ctx>, CodegenError> {
        let weak_ty = Type::Weak(Box::new(inner.clone()));
        let cell = self.get(base, &weak_ty)?;
        let get = self.runtime("lugha_rt_weak_get", &[Type::String], Some(Type::String));
        let box_ptr = self.call_basic(get, &[cell], "box").into_pointer_value();
        let option = Type::Option(Box::new(Type::Ref(Box::new(inner.clone()))));
        Ok(Value::Val(self.nullable_option(box_ptr, &option)))
    }

    /// `option`, an `Option` of a pointer: `None` for null, else `Some`.
    fn nullable_option(
        &mut self,
        pointer: PointerValue<'ctx>,
        option: &Type,
    ) -> BasicValueEnum<'ctx> {
        let is_null = self
            .builder
            .build_is_null(pointer, "gone")
            .expect(POSITIONED);
        let none = self.construct_values(option, "None", Vec::new());
        let some = self.construct_values(option, "Some", vec![pointer.into()]);
        self.builder
            .build_select(is_null, none, some, "weak.get")
            .expect(POSITIONED)
    }

    /// Calls `function`, which returns a value, with `args`.
    fn call_basic(
        &mut self,
        function: FunctionValue<'ctx>,
        args: &[BasicValueEnum<'ctx>],
        name: &str,
    ) -> BasicValueEnum<'ctx> {
        let args: Vec<_> = args.iter().map(|a| (*a).into()).collect();
        let site = self
            .builder
            .build_call(function, &args, name)
            .expect(POSITIONED);
        match site.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(value) => value,
            inkwell::values::ValueKind::Instruction(_) => {
                unreachable!("the runtime function returns a value")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::ir;

    #[test]
    fn boxes_carry_a_cell_word_only_with_weak_references() {
        let plain = ir("fun main() { let r = Ref.new(1); r.set(2); }");
        assert!(
            plain.contains("@lugha_rt_rc_alloc(i64 8, ptr null)"),
            "{plain}"
        );
        assert!(!plain.contains("lugha_rt_weak"), "{plain}");
        let weak = ir("fun main() { let r = Ref.new(1); let w = Weak.new(r); r.set(2); }");
        assert!(
            weak.contains("@lugha_rt_rc_alloc(i64 16, ptr @lugha_rc_drop_wbox_i64)"),
            "{weak}"
        );
        assert!(weak.contains("call ptr @lugha_rt_weak_new("), "{weak}");
    }

    #[test]
    fn a_weak_link_doesnt_make_a_box_cycle_prone() {
        let src = "struct Node { value: i64, parent: Option<Weak<Node>> }
            fun main() {
                let a = Ref.new(Node { value: 1, parent: None });
                let b = Ref.new(Node { value: 2, parent: Some(Weak.new(a)) });
                a.set(b.get());
            }";
        assert!(!ir(src).contains("lugha_rt_rc_cell_alloc"));
    }
}
