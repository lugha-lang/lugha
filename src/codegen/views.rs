//! Views of C's memory: `{ elements, count }`, C's pointer and how many
//! `T`s it has. Nothing is counted or freed: the memory is C's. Indexing is
//! bounds-checked against the count; writes go straight to C.

use inkwell::IntPredicate;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::value::{Value, llvm_type};
use crate::ast::{AssignOp, BinOp, Expr};
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// The view `{ pointer, count }`.
    pub(super) fn view_value(
        &self,
        pointer: PointerValue<'ctx>,
        count: IntValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let ty = Type::View(false, Box::new(Type::U8));
        let llvm = llvm_type(self.context, &ty).into_struct_type();
        let b = &self.builder;
        let view = b
            .build_insert_value(llvm.get_undef(), pointer, 0, "view")
            .expect("a view has 2 fields");
        b.build_insert_value(view, count, 1, "view")
            .expect("a view has 2 fields")
            .into_struct_value()
            .into()
    }

    /// A view's pointer and count.
    pub(super) fn view_parts(
        &self,
        view: BasicValueEnum<'ctx>,
    ) -> (PointerValue<'ctx>, IntValue<'ctx>) {
        let view = view.into_struct_value();
        let b = &self.builder;
        let pointer = b
            .build_extract_value(view, 0, "elements")
            .expect("a view has 2 fields");
        let count = b
            .build_extract_value(view, 1, "count")
            .expect("a view has 2 fields");
        (pointer.into_pointer_value(), count.into_int_value())
    }

    /// The pointer and count of view `expr`, of type `ty`.
    pub(super) fn view_of(
        &mut self,
        expr: &Expr,
        ty: &Type,
    ) -> Result<(PointerValue<'ctx>, IntValue<'ctx>), CodegenError> {
        let view = self.get(expr, ty)?;
        Ok(self.view_parts(view))
    }

    /// The address of element `i` of the view at `pointer`, unchecked.
    pub(super) fn view_address(
        &self,
        pointer: PointerValue<'ctx>,
        i: IntValue<'ctx>,
        element: &Type,
    ) -> PointerValue<'ctx> {
        self.address_after(pointer, 0, i, self.element_size(element))
    }

    /// The bounds-checked address of `base[index]` for a view `base`.
    pub(super) fn view_element(
        &mut self,
        base: &Expr,
        at: usize,
        index: &Expr,
    ) -> Result<(PointerValue<'ctx>, Type), CodegenError> {
        let ty = self.ty(base);
        let Type::View(_, element) = &ty else {
            unreachable!("checked: a view")
        };
        let (pointer, count) = self.view_of(base, &ty)?;
        let index_type = self.ty(index);
        let i = self.get(index, &index_type)?.into_int_value();
        let i = self.widen_index(i, &index_type);
        self.bounds_check(count, i, at);
        Ok((self.view_address(pointer, i, element), (**element).clone()))
    }

    /// `v[i] op= value` through a `MutView`: the element in C's memory.
    pub(super) fn view_assign(
        &mut self,
        op: (AssignOp, Option<BinOp>),
        (at, open): (usize, usize),
        (base, index): (&Expr, &Expr),
        value: &Expr,
    ) -> Result<(), CodegenError> {
        let (address, element) = self.view_element(base, open, index)?;
        let new = match op.1 {
            None => self.get(value, &element)?,
            Some(bin) => {
                let current = self.load_element(address, &element);
                let rhs = self.get(value, &element)?;
                self.arithmetic(bin, element.clone(), current, rhs, at)
            }
        };
        self.store_element(address, &element, new);
        Ok(())
    }

    /// `v.slice(from, to)`, `v.to_array()` and `m.copy_from(xs)`.
    pub(super) fn view_method(
        &mut self,
        base: &Expr,
        name: &str,
        args: &[Expr],
        at: usize,
    ) -> Result<Value<'ctx>, CodegenError> {
        let ty = self.ty(base);
        let Type::View(_, element) = &ty else {
            unreachable!("checked: a view")
        };
        let (pointer, count) = self.view_of(base, &ty)?;
        let i64_type = self.context.i64_type();
        Ok(Value::Val(match name {
            "slice" => {
                let from = self.get(&args[0], &Type::I64)?.into_int_value();
                let to = self.get(&args[1], &Type::I64)?.into_int_value();
                // `0 <= from <= to <= count`; unsigned compares catch negatives.
                let backwards = self
                    .builder
                    .build_int_compare(IntPredicate::UGT, from, to, "backwards")
                    .expect(POSITIONED);
                let past = self
                    .builder
                    .build_int_compare(IntPredicate::UGT, to, count, "past")
                    .expect(POSITIONED);
                let bad = self
                    .builder
                    .build_or(backwards, past, "bad")
                    .expect(POSITIONED);
                self.panic_if(bad, "slice out of bounds of a view", at);
                let start = self.view_address(pointer, from, element);
                let len = self
                    .builder
                    .build_int_sub(to, from, "len")
                    .expect(POSITIONED);
                self.view_value(start, len)
            }
            "to_array" => {
                let array = self.new_array(count, element);
                let first = self.element_address(array, i64_type.const_zero(), 0);
                let bytes = self.view_bytes(count, element);
                self.builder
                    .build_memcpy(first, 1, pointer, 1, bytes)
                    .expect(POSITIONED);
                array.into()
            }
            "copy_from" => {
                let array_type = Type::Array(element.clone());
                let array = self.get(&args[0], &array_type)?.into_pointer_value();
                let len = self.length(array);
                let differ = self
                    .builder
                    .build_int_compare(IntPredicate::NE, len, count, "differ")
                    .expect(POSITIONED);
                self.panic_if(differ, "copy_from: the array's length isn't the view's", at);
                let first = self.element_address(array, i64_type.const_zero(), 0);
                let bytes = self.view_bytes(count, element);
                // C's memory may overlap anything the program can reach.
                self.builder
                    .build_memmove(pointer, 1, first, 1, bytes)
                    .expect(POSITIONED);
                return Ok(Value::Void);
            }
            _ => unreachable!("checked: a view's methods"),
        }))
    }

    /// `p.view(n, f)`: `f` called with a view of `n` `T`s at `p`. A null `p`
    /// gives an empty view only; one not aligned for `T` panics.
    pub(super) fn cptr_view(
        &mut self,
        base: &Expr,
        args: &[Expr],
        at: usize,
    ) -> Result<Value<'ctx>, CodegenError> {
        let fun_type = self.ty(&args[1]);
        let Type::Fun(params, ret) = &fun_type else {
            unreachable!("checked: a function of one view")
        };
        let Type::View(_, element) = &params[0] else {
            unreachable!("checked: a function of one view")
        };
        let cptr = Type::Handle("CPtr".to_string());
        let pointer = self.get(base, &cptr)?.into_pointer_value();
        let count = self.get(&args[0], &Type::U64)?.into_int_value();
        let f = self.get(&args[1], &fun_type)?;
        let i64_type = self.context.i64_type();
        let b = &self.builder;
        let address = b
            .build_ptr_to_int(pointer, i64_type, "address")
            .expect(POSITIONED);
        let null = b
            .build_int_compare(IntPredicate::EQ, address, i64_type.const_zero(), "null")
            .expect(POSITIONED);
        let some = b
            .build_int_compare(IntPredicate::NE, count, i64_type.const_zero(), "some")
            .expect(POSITIONED);
        let empty_null = b.build_and(null, some, "null.some").expect(POSITIONED);
        self.panic_if(empty_null, "NULL CPtr given to view", at);
        let align = super::structs::layout(element, &self.structs, &self.enums).1;
        let rem = self
            .builder
            .build_int_unsigned_rem(address, i64_type.const_int(align, false), "rem")
            .expect(POSITIONED);
        let misaligned = self
            .builder
            .build_int_compare(IntPredicate::NE, rem, i64_type.const_zero(), "misaligned")
            .expect(POSITIONED);
        self.panic_if(misaligned, "a CPtr not aligned for the view's elements", at);
        let view = self.view_value(pointer, count);
        Ok(self.call_pair(f.into_struct_value(), (params, ret), vec![view.into()]))
    }

    /// `count` elements of `element`, in bytes.
    fn view_bytes(&self, count: IntValue<'ctx>, element: &Type) -> IntValue<'ctx> {
        let size = self
            .context
            .i64_type()
            .const_int(self.element_size(element), false);
        self.builder
            .build_int_mul(count, size, "bytes")
            .expect(POSITIONED)
    }
}
