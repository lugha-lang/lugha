//! Count operations on a value where it lies, reading only what is
//! counted: releasing a slot this way never loads the whole value, which
//! for a large struct or enum would take a stack copy of its own at `-O0`
//! for every release.

use inkwell::AddressSpace;
use inkwell::module::Linkage;
use inkwell::values::{FunctionValue, PointerValue};

use super::copy::mangle;
use super::enums::variants;
use super::lower::{Lowerer, POSITIONED};
use super::rc::{RcOp, pointer_counted};
use super::value::llvm_type;
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// `op` on what the value of type `ty` at `address` (laid out as its
    /// type, not as an element) holds.
    pub(super) fn rc_value_at(&mut self, address: PointerValue<'ctx>, ty: &Type, op: RcOp) {
        if !self.counted(ty) {
            return;
        }
        let ptr = self.context.ptr_type(AddressSpace::default());
        match ty {
            _ if pointer_counted(ty) => {
                let object = self
                    .builder
                    .build_load(ptr, address, "held")
                    .expect(POSITIONED);
                self.rc_pointer_op(object.into_pointer_value(), op);
            }
            // The environment, or the box: the second field.
            Type::Fun(..) | Type::Interface(..) => {
                let pair = self.context.struct_type(&[ptr.into(), ptr.into()], false);
                let at = self
                    .builder
                    .build_struct_gep(pair, address, 1, "object.at")
                    .expect("two fields");
                let object = self
                    .builder
                    .build_load(ptr, at, "object")
                    .expect(POSITIONED);
                self.rc_pointer_op(object.into_pointer_value(), op);
            }
            Type::Struct(..) | Type::Enum(..) | Type::Option(_) => {
                let glue = self.rc_glue_at(ty, op);
                self.builder
                    .build_call(glue, &[address.into()], "")
                    .expect(POSITIONED);
            }
            _ => {
                let value = self
                    .builder
                    .build_load(llvm_type(self.context, ty), address, "held")
                    .expect(POSITIONED);
                self.rc_value(value, ty, op);
            }
        }
    }

    /// `lugha_rc_<op>_at_<T>(address)`: `rc_value_at` for a struct, enum
    /// or `Option` type, one function per type and operation.
    fn rc_glue_at(&mut self, ty: &Type, op: RcOp) -> FunctionValue<'ctx> {
        let verb = match op {
            RcOp::Retain => "retain",
            RcOp::Release => "release",
            RcOp::Mark => "mark",
        };
        let name = format!("lugha_rc_{verb}_at_{}", mangle(ty));
        if let Some(function) = self.module.get_function(&name) {
            return function;
        }
        let ptr = self.context.ptr_type(AddressSpace::default());
        let function = self.module.add_function(
            &name,
            self.context.void_type().fn_type(&[ptr.into()], false),
            Some(Linkage::Internal),
        );
        let ty = ty.clone();
        self.in_function(function, |this| {
            let address = function
                .get_nth_param(0)
                .expect("one parameter")
                .into_pointer_value();
            if let Type::Struct(..) = &ty {
                let layout = llvm_type(this.context, &ty).into_struct_type();
                let fields = this.structs[&ty.to_string()].clone();
                for (index, (field, field_ty)) in fields.iter().enumerate() {
                    if !this.counted(field_ty) {
                        continue;
                    }
                    let index = u32::try_from(index).expect("field count fits in u32");
                    let at = this
                        .builder
                        .build_struct_gep(layout, address, index, field)
                        .expect("field exists");
                    this.rc_value_at(at, field_ty, op);
                }
            } else {
                this.rc_variants_at(address, &ty, op);
            }
            this.builder.build_return(None).expect(POSITIONED);
        });
        function
    }

    /// An enum's counted payloads, by its tag, read from `slot`: an
    /// indirect payload is a pointer to a counted box, a counted pointer is
    /// read alone, anything else read where it lies.
    pub(super) fn rc_variants_at(&mut self, slot: PointerValue<'ctx>, ty: &Type, op: RcOp) {
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
        let ptr = self.context.ptr_type(AddressSpace::default());
        for (_, block, payloads) in cases {
            self.builder.position_at_end(block);
            for (index, payload) in payloads.iter().enumerate() {
                if !self.counted(payload) {
                    continue;
                }
                let index = u32::try_from(index).expect("payload count fits");
                let address = self.payload_address(ty, slot, &payloads, index);
                if self.indirect(ty, payload) || pointer_counted(payload) {
                    let object = self
                        .builder
                        .build_load(ptr, address, "boxed")
                        .expect(POSITIONED)
                        .into_pointer_value();
                    self.rc_pointer_op(object, op);
                } else {
                    let part = self.load_element(address, payload);
                    self.rc_value(part, payload, op);
                }
            }
            self.branch(done);
        }
        self.builder.position_at_end(done);
    }
}
