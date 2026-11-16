//! Counted objects under reference counting: the visit each one carries
//! (what releases or marks its contents), the header constants
//! carry, and whether an array has other holders.

use inkwell::AddressSpace;
use inkwell::IntPredicate;
use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

use super::copy::mangle;
use super::lower::{Lowerer, POSITIONED};
use super::rc::{RcOp, pointer_counted};
use super::value::llvm_type;
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// A visit function `name(object, op)`: `body` lowers what it does with
    /// the object's contents for one operation, release or mark, and runs
    /// once for each.
    fn rc_visit_function(
        &mut self,
        name: &str,
        body: impl Fn(&mut Self, PointerValue<'ctx>, RcOp),
    ) -> FunctionValue<'ctx> {
        let ptr = self.context.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        let function = self.module.add_function(
            name,
            self.context
                .void_type()
                .fn_type(&[ptr.into(), i64_type.into()], false),
            Some(Linkage::Internal),
        );
        self.in_function(function, |this| {
            let object = function
                .get_nth_param(0)
                .expect("two parameters")
                .into_pointer_value();
            let op = function
                .get_nth_param(1)
                .expect("two parameters")
                .into_int_value();
            let (mark, release) = (this.append("mark"), this.append("release"));
            // The runtime's `RC_MARK` is 1.
            let marking = this
                .builder
                .build_int_compare(
                    IntPredicate::EQ,
                    op,
                    i64_type.const_int(1, false),
                    "marking",
                )
                .expect(POSITIONED);
            this.builder
                .build_conditional_branch(marking, mark, release)
                .expect(POSITIONED);
            for (block, op) in [(mark, RcOp::Mark), (release, RcOp::Release)] {
                this.builder.position_at_end(block);
                body(this, object, op);
                this.builder.build_return(None).expect(POSITIONED);
            }
        });
        function
    }

    /// The visit of an object holding one value of type `contents` at its
    /// start (a `Ref` box, an indirect payload's box, an interface value's
    /// box); `None` when nothing in it is counted.
    pub(super) fn rc_box_drop(&mut self, contents: &Type) -> Option<FunctionValue<'ctx>> {
        if !self.counted(contents) {
            return None;
        }
        let name = format!("lugha_rc_drop_box_{}", mangle(contents));
        if let Some(function) = self.module.get_function(&name) {
            return Some(function);
        }
        let contents = contents.clone();
        Some(self.rc_visit_function(&name, |this, object, op| {
            let value = this.load_element(object, &contents);
            this.rc_value(value, &contents, op);
        }))
    }

    /// The visit of an array of `element`s: the runtime's for pointers, one
    /// generated per type for values holding them; `None` when the elements
    /// hold nothing counted.
    pub(super) fn rc_array_drop(&mut self, element: &Type) -> Option<FunctionValue<'ctx>> {
        if !self.counted(element) {
            return None;
        }
        if pointer_counted(element) {
            return Some(self.runtime(
                "lugha_rt_rc_drop_pointers",
                &[Type::String, Type::I64],
                None,
            ));
        }
        let name = format!("lugha_rc_drop_arr_{}", mangle(element));
        if let Some(function) = self.module.get_function(&name) {
            return Some(function);
        }
        let element = element.clone();
        Some(self.rc_visit_function(&name, |this, array, op| {
            let len = this.length(array);
            let size = this.element_size(&element);
            this.count_loop(len, |this, i| {
                let item = this.load_element(this.element_address(array, i, size), &element);
                this.rc_value(item, &element, op);
                Ok(())
            })
            .expect("visit loops only lower runtime calls");
        }))
    }

    /// The visit of a closure environment's captures, laid out as
    /// `layout`; `None` when none of them is counted.
    pub(super) fn rc_env_drop(
        &mut self,
        layout: inkwell::types::StructType<'ctx>,
        types: &[Type],
    ) -> Option<FunctionValue<'ctx>> {
        if !types.iter().any(|t| self.counted(t)) {
            return None;
        }
        self.rc_envs += 1;
        let name = format!("lugha_rc_drop_env.{}", self.rc_envs);
        let types = types.to_vec();
        Some(self.rc_visit_function(&name, |this, env, op| {
            for (i, ty) in types.iter().enumerate() {
                if !this.counted(ty) {
                    continue;
                }
                let index = u32::try_from(i).expect("capture count fits in u32");
                let at = this
                    .builder
                    .build_struct_gep(layout, env, index, "capture")
                    .expect("a field per capture");
                let value = this
                    .builder
                    .build_load(llvm_type(this.context, ty), at, "capture")
                    .expect(POSITIONED);
                this.rc_value(value, ty, op);
            }
        }))
    }

    /// A constant object's fields, after the header reference counting
    /// puts before every object: no drop, and a count increments and
    /// decrements leave alone, so it is never freed.
    pub(super) fn immortal_header(
        &self,
        fields: Vec<BasicValueEnum<'ctx>>,
    ) -> Vec<BasicValueEnum<'ctx>> {
        if !self.rc() {
            return fields;
        }
        let ptr = self.context.ptr_type(AddressSpace::default());
        let immortal = self.context.i64_type().const_int(i64::MIN as u64, false);
        [ptr.const_null().into(), immortal.into()]
            .into_iter()
            .chain(fields)
            .collect()
    }

    /// The object in `global`, past any header `immortal_header` added.
    pub(super) fn past_header(
        &self,
        global: inkwell::values::GlobalValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let start = global.as_pointer_value();
        if !self.rc() {
            return start;
        }
        let i8_type = self.context.i8_type();
        let header = self.context.i64_type().const_int(16, false);
        // Sixteen bytes in: the header's two words.
        unsafe { start.const_gep(i8_type, &[header]) }
    }

    /// True if `array` has another holder (or is a constant), so a write
    /// must clone it first: its count isn't one.
    pub(super) fn rc_shared(&self, array: PointerValue<'ctx>) -> inkwell::values::IntValue<'ctx> {
        let i64_type = self.context.i64_type();
        let at = unsafe {
            self.builder.build_gep(
                i64_type,
                array,
                &[i64_type.const_int(u64::MAX, true)],
                "count.at",
            )
        }
        .expect(POSITIONED);
        let count = self
            .builder
            .build_load(i64_type, at, "count")
            .expect(POSITIONED)
            .into_int_value();
        self.builder
            .build_int_compare(
                IntPredicate::NE,
                count,
                i64_type.const_int(1, false),
                "is_shared",
            )
            .expect(POSITIONED)
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

    /// The interface value `pair`, read from the place at `address`, with
    /// a box of its own: a box with other holders is copied (through its
    /// table's copy slot), the copy stored back in the place, and the
    /// place's hold on the old box released.
    pub(super) fn rc_own_box(
        &mut self,
        pair: inkwell::values::StructValue<'ctx>,
        (address, ty, element): (PointerValue<'ctx>, &Type, bool),
    ) -> inkwell::values::StructValue<'ctx> {
        let (table, boxed) = self.parts(pair);
        let shared = self.rc_shared(boxed);
        let from = self.builder.get_insert_block().expect(POSITIONED);
        let (copy_block, done) = (self.append("box.copy"), self.append("box.owned"));
        self.builder
            .build_conditional_branch(shared, copy_block, done)
            .expect(POSITIONED);
        self.builder.position_at_end(copy_block);
        let copy = self.slot(table, 1);
        let ptr = self.context.ptr_type(AddressSpace::default());
        let site = self
            .builder
            .build_indirect_call(
                ptr.fn_type(&[ptr.into()], false),
                copy,
                &[boxed.into()],
                "box",
            )
            .expect(POSITIONED);
        let fresh = match site.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            inkwell::values::ValueKind::Instruction(_) => {
                unreachable!("a box copy returns the box")
            }
        };
        let owned = self.pair(table, fresh);
        if element {
            self.store_element(address, ty, owned.into());
        } else {
            self.builder.build_store(address, owned).expect(POSITIONED);
        }
        self.rc_pointer(boxed, false);
        let copied = self.builder.get_insert_block().expect(POSITIONED);
        self.branch(done);
        self.builder.position_at_end(done);
        let phi = self
            .builder
            .build_phi(pair.get_type(), "receiver")
            .expect(POSITIONED);
        phi.add_incoming(&[(&pair, from), (&owned, copied)]);
        phi.as_basic_value().into_struct_value()
    }
}
