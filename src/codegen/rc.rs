//! Reference counting in codegen: which values hold counted
//! objects, adding and taking away holders, and the functions that do it
//! per type.
//!
//! Strings, arrays, `Ref` boxes, the box an enum's self-referencing payload
//! lives in, a closure's environment and an interface value's box are
//! counted objects. Each carries, in its header, the function that releases
//! what it holds (`runtime/memory/rc.c`), so releasing any of them is the same
//! operation: take one from the count, and free at zero. Structs, tuples,
//! enums, function values and interface values are values holding such
//! pointers: a per-type function retains or releases each one.

use inkwell::AddressSpace;
use inkwell::AtomicOrdering;
use inkwell::IntPredicate;
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::module::Linkage;
use inkwell::values::BasicValue;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

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

fn counted_in(ty: &Type, structs: &Structs, enums: &Enums, path: &mut Vec<String>) -> bool {
    match ty {
        Type::String
        | Type::Array(_)
        | Type::Ref(_)
        | Type::Weak(_)
        | Type::Fun(..)
        | Type::Own(_)
        | Type::Interface(..)
        | Type::Chan(_, _)
        | Type::Task(_)
        | Type::Actor(..) => true,
        Type::Handle(key) => key == crate::check::RESOURCE,
        Type::Struct(..) | Type::Enum(..) | Type::Option(_) => {
            let name = ty.to_string();
            // A type leading back to itself does so through a box on the
            // heap, which is counted.
            if path.contains(&name) {
                return true;
            }
            path.push(name.clone());
            let parts: Vec<Type> = match ty {
                Type::Struct(..) => structs[&name].iter().map(|(_, t)| t.clone()).collect(),
                _ => variants(ty, enums)
                    .into_iter()
                    .flat_map(|(_, p)| p)
                    .collect(),
            };
            let counted = parts.iter().any(|p| counted_in(p, structs, enums, path));
            path.pop();
            counted
        }
        _ => false,
    }
}

/// What a count operation does to the counted objects a value holds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RcOp {
    /// Adds a holder.
    Retain,
    /// Takes a holder away, freeing at zero.
    Release,
    /// Marks shared, with everything reachable: the value crosses to
    /// another task.
    Mark,
}

/// True if a value of type `ty` is itself a pointer to a counted object.
pub(super) fn pointer_counted(ty: &Type) -> bool {
    matches!(
        ty,
        Type::String
            | Type::Array(_)
            | Type::Ref(_)
            | Type::Weak(_)
            | Type::Chan(_, _)
            | Type::Task(_)
            | Type::Actor(..)
    ) || matches!(ty, Type::Handle(k) if k == crate::check::RESOURCE)
}

/// Bit 62 of a count: shared between tasks, its count changed atomically.
pub(super) const SHARED: u64 = 1 << 62;

/// Bits 63 and 61: a count in the owner check's form (`LUGHA_RC_CHECK=1`),
/// negative, so the inline count leaves it to the runtime.
pub(super) const CHECKED: u64 = 0xA << 60;

/// Bits 32–60 of a checked count: the number of the task that made it.
pub(super) const CHECK_OWNER: u64 = 0x1FFF_FFFF << 32;

impl<'ctx> Lowerer<'ctx> {
    /// True if `ty`'s values hold counted objects.
    pub(super) fn counted(&self, ty: &Type) -> bool {
        counted(ty, &self.structs, &self.enums)
    }

    /// Adds a holder to every counted object `value` holds.
    pub(super) fn rc_retain(&mut self, value: BasicValueEnum<'ctx>, ty: &Type) {
        // A struct or enum just loaded, nothing emitted since: retained
        // where it lies, so the whole value needn't be passed (and, at
        // `-O0`, copied onto the stack).
        if matches!(ty, Type::Struct(..) | Type::Enum(..) | Type::Option(_))
            && let Some(load) = inkwell::values::BasicValue::as_instruction_value(&value)
            && load.get_opcode() == inkwell::values::InstructionOpcode::Load
            && self
                .builder
                .get_insert_block()
                .and_then(|b| b.get_last_instruction())
                == Some(load)
            && let Some(inkwell::values::Operand::Value(address)) = load.get_operand(0)
        {
            self.rc_value_at(address.into_pointer_value(), ty, RcOp::Retain);
            return;
        }
        self.rc_value(value, ty, RcOp::Retain);
    }

    /// Takes a holder from every counted object `value` holds, freeing those
    /// it was the last holder of.
    pub(super) fn rc_release(&mut self, value: BasicValueEnum<'ctx>, ty: &Type) {
        self.rc_value(value, ty, RcOp::Release);
    }

    /// Marks every counted object `value` holds shared, with what they
    /// reach: before it crosses to another task.
    pub(super) fn rc_mark(&mut self, value: BasicValueEnum<'ctx>, ty: &Type) {
        self.rc_value(value, ty, RcOp::Mark);
    }

    /// `op` on every counted object `value`, of type `ty`, holds.
    pub(super) fn rc_value(&mut self, value: BasicValueEnum<'ctx>, ty: &Type, op: RcOp) {
        if !self.counted(ty) {
            return;
        }
        match ty {
            _ if pointer_counted(ty) => self.rc_pointer_op(value.into_pointer_value(), op),
            // The environment, or the box: the second field.
            Type::Fun(..) | Type::Own(_) | Type::Interface(..) => {
                let object = self
                    .builder
                    .build_extract_value(value.into_struct_value(), 1, "object")
                    .expect("two fields")
                    .into_pointer_value();
                self.rc_pointer_op(object, op);
            }
            _ => {
                let glue = self.rc_glue(ty, op);
                self.builder
                    .build_call(glue, &[value.into()], "")
                    .expect(POSITIONED);
            }
        }
    }

    /// Adds or takes a holder from the counted object at `object` (none when
    /// it is null), through the inlined `lugha_rc_inc`/`lugha_rc_dec`.
    pub(super) fn rc_pointer(&mut self, object: PointerValue<'ctx>, retain: bool) {
        let op = if retain { RcOp::Retain } else { RcOp::Release };
        self.rc_pointer_op(object, op);
    }

    /// `op` on the counted object at `object` (none when it is null).
    pub(super) fn rc_pointer_op(&mut self, object: PointerValue<'ctx>, op: RcOp) {
        let function = match op {
            RcOp::Mark => self.runtime("lugha_rt_rc_mark", &[Type::String], None),
            _ => self.rc_count_function(op == RcOp::Retain),
        };
        self.builder
            .build_call(function, &[object.into()], "")
            .expect(POSITIONED);
    }

    /// The count word at `at`, read as an `unordered` atomic: the same
    /// instruction as a plain load, never torn and never undefined when
    /// another thread changes a shared count beside it, and still free to
    /// be hoisted out of a loop (a `monotonic` load isn't, which cost
    /// fannkuch 5%). The fast paths only write unshared counts, which no
    /// other thread touches; a shared count's bit never clears.
    pub(super) fn load_count(&self, at: PointerValue<'ctx>) -> inkwell::values::IntValue<'ctx> {
        let i64_type = self.context.i64_type();
        let count = self
            .builder
            .build_load(i64_type, at, "count")
            .expect(POSITIONED);
        let load = count
            .as_instruction_value()
            .expect("a load is an instruction");
        load.set_alignment(8).expect("a load has an alignment");
        load.set_atomic_ordering(AtomicOrdering::Unordered)
            .expect("a load can be atomic");
        count.into_int_value()
    }

    /// Stores `count` at `at` as an `unordered` atomic.
    fn store_count(&self, at: PointerValue<'ctx>, count: inkwell::values::IntValue<'ctx>) {
        let store = self.builder.build_store(at, count).expect(POSITIONED);
        store.set_alignment(8).expect("a store has an alignment");
        store
            .set_atomic_ordering(AtomicOrdering::Unordered)
            .expect("a store can be atomic");
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
        let free = self.runtime("lugha_rt_rc_free", &[Type::String], None);
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
            let count = this.load_count(at);
            let one = i64_type.const_int(1, false);
            let (fast, odd) = (this.append("fast"), this.append("odd"));
            // A plain count is from 1 to below the shared bit: retaining
            // needs one, releasing without freeing more than one. One
            // unsigned compare each: `count - floor < SHARED - floor`.
            let floor = if retain { 1 } else { 2 };
            let floor = i64_type.const_int(floor, false);
            let above = this
                .builder
                .build_int_sub(count, floor, "above")
                .expect(POSITIONED);
            let limit = this
                .builder
                .build_int_sub(i64_type.const_int(SHARED, false), floor, "limit")
                .expect(POSITIONED);
            let plain = this
                .builder
                .build_int_compare(IntPredicate::ULT, above, limit, "plain")
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
            this.store_count(at, next);
            this.branch(done);
            this.builder.position_at_end(odd);
            if retain {
                this.builder
                    .build_call(slow, &[object.into(), one.into()], "")
                    .expect(POSITIONED);
            } else {
                // The last holder frees it; a shared, constant or freed
                // count is the runtime's.
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
                    .build_call(free, &[object.into()], "")
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

    /// `lugha_rc_retain_<T>(value)`, `lugha_rc_release_<T>` or
    /// `lugha_rc_mark_<T>` for a struct, tuple, enum or `Option` type: each
    /// counted part in turn, an enum's by its tag. Declared before its body,
    /// so types leading back to themselves call themselves.
    fn rc_glue(&mut self, ty: &Type, op: RcOp) -> FunctionValue<'ctx> {
        let verb = match op {
            RcOp::Retain => "retain",
            RcOp::Release => "release",
            RcOp::Mark => "mark",
        };
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
                        this.rc_value(part, field_ty, op);
                    }
                }
                _ => this.rc_variants(value, &ty, op),
            }
            this.builder.build_return(None).expect(POSITIONED);
        });
        function
    }

    /// An enum's counted payloads, by its tag, through a slot holding it.
    fn rc_variants(&mut self, value: BasicValueEnum<'ctx>, ty: &Type, op: RcOp) {
        let slot = self.entry_alloca(ty, "variant");
        self.builder.build_store(slot, value).expect(POSITIONED);
        self.rc_variants_at(slot, ty, op);
    }

    /// Runs `body` positioned in `function`'s new entry block, with a
    /// builder of its own: lowering's builder keeps its exact place, which
    /// may be before a block's terminator.
    pub(super) fn in_function(
        &mut self,
        function: FunctionValue<'ctx>,
        body: impl FnOnce(&mut Self),
    ) {
        let saved_function = self.function;
        let saved_builder = std::mem::replace(&mut self.builder, self.context.create_builder());
        self.function = Some(function);
        self.builder
            .position_at_end(self.context.append_basic_block(function, "entry"));
        body(self);
        self.function = saved_function;
        self.builder = saved_builder;
    }
}

#[cfg(test)]
#[path = "rc_tests.rs"]
mod tests;
