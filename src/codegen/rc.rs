//! Reference counting in codegen (`--memory=rc`): which values hold counted
//! objects, adding and taking away holders, and the functions that do it
//! per type.
//!
//! Strings, arrays, `Ref` boxes, the box an enum's self-referencing payload
//! lives in, a closure's environment and an interface value's box are
//! counted objects. Each carries, in its header, the function that releases
//! what it holds (`runtime/rc.c`), so releasing any of them is the same
//! operation: take one from the count, and free at zero. Structs, tuples,
//! enums, function values and interface values are values holding such
//! pointers: a per-type function retains or releases each one.

use inkwell::AddressSpace;
use inkwell::IntPredicate;
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

use super::Memory;
use super::copy::mangle;
use super::enums::variants;
use super::lower::{Lowerer, POSITIONED};
use super::value::llvm_type;
use crate::check::{Enums, Structs, Type};

/// True if a value of type `ty` holds a counted object, directly or in a
/// field or payload. Tasks, channels and actors aren't counted yet.
pub(super) fn counted(ty: &Type, structs: &Structs, enums: &Enums) -> bool {
    counted_in(ty, structs, enums, &mut Vec::new())
}

fn counted_in(ty: &Type, structs: &Structs, enums: &Enums, seen: &mut Vec<String>) -> bool {
    match ty {
        Type::String | Type::Array(_) | Type::Ref(_) | Type::Fun(..) | Type::Interface(..) => true,
        Type::Struct(..) | Type::Enum(..) | Type::Option(_) => {
            let name = ty.to_string();
            // A type that leads back to itself holds what its other parts hold.
            if seen.contains(&name) {
                return false;
            }
            seen.push(name.clone());
            let parts: Vec<Type> = match ty {
                Type::Struct(..) => structs[&name].iter().map(|(_, t)| t.clone()).collect(),
                _ => variants(ty, enums)
                    .into_iter()
                    .flat_map(|(_, p)| p)
                    .collect(),
            };
            parts.iter().any(|p| counted_in(p, structs, enums, seen))
        }
        _ => false,
    }
}

impl<'ctx> Lowerer<'ctx> {
    /// True when building under reference counting.
    pub(super) fn rc(&self) -> bool {
        self.memory == Memory::Rc
    }

    /// True if `ty`'s values hold counted objects.
    pub(super) fn counted(&self, ty: &Type) -> bool {
        counted(ty, &self.structs, &self.enums)
    }

    /// Adds a holder to every counted object `value` holds.
    pub(super) fn rc_retain(&mut self, value: BasicValueEnum<'ctx>, ty: &Type) {
        self.rc_value(value, ty, true);
    }

    /// Takes a holder from every counted object `value` holds, freeing those
    /// it was the last holder of.
    pub(super) fn rc_release(&mut self, value: BasicValueEnum<'ctx>, ty: &Type) {
        self.rc_value(value, ty, false);
    }

    fn rc_value(&mut self, value: BasicValueEnum<'ctx>, ty: &Type, retain: bool) {
        if !self.counted(ty) {
            return;
        }
        match ty {
            Type::String | Type::Array(_) | Type::Ref(_) => {
                self.rc_pointer(value.into_pointer_value(), retain);
            }
            // The environment, or the box: the second field.
            Type::Fun(..) | Type::Interface(..) => {
                let object = self
                    .builder
                    .build_extract_value(value.into_struct_value(), 1, "object")
                    .expect("two fields")
                    .into_pointer_value();
                self.rc_pointer(object, retain);
            }
            _ => {
                let glue = self.rc_glue(ty, retain);
                self.builder
                    .build_call(glue, &[value.into()], "")
                    .expect(POSITIONED);
            }
        }
    }

    /// Adds or takes a holder from the counted object at `object` (none when
    /// it is null), through the inlined `lugha_rc_inc`/`lugha_rc_dec`.
    pub(super) fn rc_pointer(&mut self, object: PointerValue<'ctx>, retain: bool) {
        let function = self.rc_count_function(retain);
        self.builder
            .build_call(function, &[object.into()], "")
            .expect(POSITIONED);
    }

    /// `lugha_rc_inc(p)` or `lugha_rc_dec(p)`, defined once and always
    /// inlined: a null check, the count in the header, and a call to the
    /// runtime only to free or for an odd count (a constant, a freed object).
    fn rc_count_function(&mut self, retain: bool) -> FunctionValue<'ctx> {
        let name = if retain {
            "lugha_rc_inc"
        } else {
            "lugha_rc_dec"
        };
        if let Some(function) = self.module.get_function(name) {
            return function;
        }
        let ptr = self.context.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let function = self.module.add_function(
            name,
            self.context.void_type().fn_type(&[ptr.into()], false),
            Some(Linkage::Internal),
        );
        let inline = Attribute::get_named_enum_kind_id("alwaysinline");
        function.add_attribute(
            AttributeLoc::Function,
            self.context.create_enum_attribute(inline, 0),
        );
        let slow = self.runtime("lugha_rt_rc_slow", &[Type::String, Type::I64], None);
        let free = self.runtime("lugha_rt_rc_free", &[Type::String, Type::String], None);
        self.in_function(function, |this| {
            let object = function
                .get_nth_param(0)
                .expect("one parameter")
                .into_pointer_value();
            let (live, done) = (this.append("live"), this.append("done"));
            let null = this
                .builder
                .build_is_null(object, "null")
                .expect(POSITIONED);
            this.builder
                .build_conditional_branch(null, done, live)
                .expect(POSITIONED);
            this.builder.position_at_end(live);
            // The count is the header's second word, just before the object.
            let at = unsafe {
                this.builder.build_gep(
                    i64_type,
                    object,
                    &[i64_type.const_int(u64::MAX, true)],
                    "count.at",
                )
            }
            .expect(POSITIONED);
            let count = this
                .builder
                .build_load(i64_type, at, "count")
                .expect(POSITIONED)
                .into_int_value();
            let one = i64_type.const_int(1, false);
            let (fast, odd) = (this.append("fast"), this.append("odd"));
            // Retaining needs a count of at least one; releasing without
            // freeing, more than one.
            let floor = if retain { 0 } else { 1 };
            let plain = this
                .builder
                .build_int_compare(
                    IntPredicate::SGT,
                    count,
                    i64_type.const_int(floor, false),
                    "plain",
                )
                .expect(POSITIONED);
            this.builder
                .build_conditional_branch(plain, fast, odd)
                .expect(POSITIONED);
            this.builder.position_at_end(fast);
            let next = if retain {
                this.builder.build_int_add(count, one, "next")
            } else {
                this.builder.build_int_sub(count, one, "next")
            }
            .expect(POSITIONED);
            this.builder.build_store(at, next).expect(POSITIONED);
            this.branch(done);
            this.builder.position_at_end(odd);
            if retain {
                this.builder
                    .build_call(slow, &[object.into(), one.into()], "")
                    .expect(POSITIONED);
            } else {
                // The last holder frees it; zero or a negative count is odd.
                let (last, other) = (this.append("last"), this.append("other"));
                let is_last = this
                    .builder
                    .build_int_compare(IntPredicate::EQ, count, one, "is.last")
                    .expect(POSITIONED);
                this.builder
                    .build_conditional_branch(is_last, last, other)
                    .expect(POSITIONED);
                this.builder.position_at_end(last);
                this.builder
                    .build_call(free, &[object.into(), ptr.const_null().into()], "")
                    .expect(POSITIONED);
                this.branch(done);
                this.builder.position_at_end(other);
                let minus = i64_type.const_int(u64::MAX, true);
                this.builder
                    .build_call(slow, &[object.into(), minus.into()], "")
                    .expect(POSITIONED);
            }
            this.branch(done);
            this.builder.position_at_end(done);
            this.builder.build_return(None).expect(POSITIONED);
        });
        function
    }

    /// `lugha_rc_retain_<T>(value)` or `lugha_rc_release_<T>(value)` for a
    /// struct, tuple, enum or `Option` type: each counted part in turn, an
    /// enum's by its tag. Declared before its body, so types leading back to
    /// themselves call themselves.
    fn rc_glue(&mut self, ty: &Type, retain: bool) -> FunctionValue<'ctx> {
        let verb = if retain { "retain" } else { "release" };
        let name = format!("lugha_rc_{verb}_{}", mangle(ty));
        if let Some(function) = self.module.get_function(&name) {
            return function;
        }
        let llvm = llvm_type(self.context, ty);
        let function = self.module.add_function(
            &name,
            self.context.void_type().fn_type(&[llvm.into()], false),
            Some(Linkage::Internal),
        );
        let ty = ty.clone();
        self.in_function(function, |this| {
            let value = function.get_nth_param(0).expect("one parameter");
            match &ty {
                Type::Struct(..) => {
                    let fields = this.structs[&ty.to_string()].clone();
                    for (index, (field, field_ty)) in fields.iter().enumerate() {
                        if !this.counted(field_ty) {
                            continue;
                        }
                        let index = u32::try_from(index).expect("field count fits in u32");
                        let part = this
                            .builder
                            .build_extract_value(value.into_struct_value(), index, field)
                            .expect("field exists");
                        this.rc_value(part, field_ty, retain);
                    }
                }
                _ => this.rc_variants(value, &ty, retain),
            }
            this.builder.build_return(None).expect(POSITIONED);
        });
        function
    }

    /// An enum's counted payloads, by its tag: an indirect payload is a
    /// pointer to a counted box, any other read where it lies.
    fn rc_variants(&mut self, value: BasicValueEnum<'ctx>, ty: &Type, retain: bool) {
        let slot = self.entry_alloca(ty, "variant");
        self.builder.build_store(slot, value).expect(POSITIONED);
        let tag = self.tag(ty, slot);
        let done = self.append("variants.done");
        let mut cases = Vec::new();
        for (index, (_, payloads)) in variants(ty, &self.enums).into_iter().enumerate() {
            if !payloads.iter().any(|p| self.counted(p)) {
                continue;
            }
            let block = self.append("variant");
            let index = u64::try_from(index).expect("variant count fits");
            cases.push((
                self.context.i32_type().const_int(index, false),
                block,
                payloads,
            ));
        }
        let switch_cases: Vec<_> = cases.iter().map(|(v, b, _)| (*v, *b)).collect();
        self.builder
            .build_switch(tag, done, &switch_cases)
            .expect(POSITIONED);
        for (_, block, payloads) in cases {
            self.builder.position_at_end(block);
            for (index, payload) in payloads.iter().enumerate() {
                if !self.counted(payload) {
                    continue;
                }
                let index = u32::try_from(index).expect("payload count fits");
                let address = self.payload_address(ty, slot, &payloads, index);
                if self.indirect(ty, payload) {
                    let ptr = self.context.ptr_type(AddressSpace::default());
                    let boxed = self
                        .builder
                        .build_load(ptr, address, "boxed")
                        .expect(POSITIONED)
                        .into_pointer_value();
                    self.rc_pointer(boxed, retain);
                } else {
                    let part = self.load_element(address, payload);
                    self.rc_value(part, payload, retain);
                }
            }
            self.branch(done);
        }
        self.builder.position_at_end(done);
    }

    /// The function releasing the contents of an object holding one value
    /// of type `contents` at its start (a `Ref` box, an indirect payload's
    /// box, an interface value's box); `None` when nothing in it is counted.
    pub(super) fn rc_box_drop(&mut self, contents: &Type) -> Option<FunctionValue<'ctx>> {
        if !self.counted(contents) {
            return None;
        }
        let name = format!("lugha_rc_drop_box_{}", mangle(contents));
        if let Some(function) = self.module.get_function(&name) {
            return Some(function);
        }
        let ptr = self.context.ptr_type(AddressSpace::default());
        let function = self.module.add_function(
            &name,
            self.context.void_type().fn_type(&[ptr.into()], false),
            Some(Linkage::Internal),
        );
        let contents = contents.clone();
        self.in_function(function, |this| {
            let object = function
                .get_nth_param(0)
                .expect("one parameter")
                .into_pointer_value();
            let value = this.load_element(object, &contents);
            this.rc_release(value, &contents);
            this.builder.build_return(None).expect(POSITIONED);
        });
        Some(function)
    }

    /// The function releasing an array's elements of type `element`: the
    /// runtime's for pointers, one generated per type for values holding
    /// them; `None` when the elements hold nothing counted.
    pub(super) fn rc_array_drop(&mut self, element: &Type) -> Option<FunctionValue<'ctx>> {
        if !self.counted(element) {
            return None;
        }
        if matches!(element, Type::String | Type::Array(_) | Type::Ref(_)) {
            return Some(self.runtime("lugha_rt_rc_drop_pointers", &[Type::String], None));
        }
        let name = format!("lugha_rc_drop_arr_{}", mangle(element));
        if let Some(function) = self.module.get_function(&name) {
            return Some(function);
        }
        let ptr = self.context.ptr_type(AddressSpace::default());
        let function = self.module.add_function(
            &name,
            self.context.void_type().fn_type(&[ptr.into()], false),
            Some(Linkage::Internal),
        );
        let element = element.clone();
        self.in_function(function, |this| {
            let array = function
                .get_nth_param(0)
                .expect("one parameter")
                .into_pointer_value();
            let len = this.length(array);
            let size = this.element_size(&element);
            this.count_loop(len, |this, i| {
                let item = this.load_element(this.element_address(array, i, size), &element);
                this.rc_release(item, &element);
                Ok(())
            })
            .expect("drop loops only lower runtime calls");
            this.builder.build_return(None).expect(POSITIONED);
        });
        Some(function)
    }

    /// Stores `drop` as what releases `object`'s contents when it is freed.
    pub(super) fn rc_set_drop(
        &mut self,
        object: PointerValue<'ctx>,
        drop: Option<FunctionValue<'ctx>>,
    ) {
        let Some(drop) = drop else { return };
        let set = self.runtime("lugha_rt_rc_set_drop", &[Type::String, Type::String], None);
        let address = drop.as_global_value().as_pointer_value();
        self.builder
            .build_call(set, &[object.into(), address.into()], "")
            .expect(POSITIONED);
    }

    /// Runs `body` positioned in `function`'s new entry block, then comes
    /// back to where lowering was.
    fn in_function(&mut self, function: FunctionValue<'ctx>, body: impl FnOnce(&mut Self)) {
        let (saved_block, saved_function) = (self.builder.get_insert_block(), self.function);
        self.function = Some(function);
        self.builder
            .position_at_end(self.context.append_basic_block(function, "entry"));
        body(self);
        self.function = saved_function;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
    }
}
