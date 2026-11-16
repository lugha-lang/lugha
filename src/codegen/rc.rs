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

fn counted_in(ty: &Type, structs: &Structs, enums: &Enums, path: &mut Vec<String>) -> bool {
    match ty {
        Type::String | Type::Array(_) | Type::Ref(_) | Type::Fun(..) | Type::Interface(..) => true,
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

/// What in `checked` the reference-counting prototype doesn't count yet,
/// if anything: tasks, channels and actors, or C callbacks.
pub(super) fn rc_unsupported(checked: &crate::check::Checked) -> Option<&'static str> {
    let concurrent = checked.uses_tasks
        || checked
            .used
            .iter()
            .any(|t| matches!(t, Type::Task(_) | Type::Chan(_) | Type::Actor(_)));
    if concurrent {
        Some("tasks, channels or actors")
    } else if !checked.c_params.is_empty() {
        Some("C callbacks")
    } else {
        None
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
mod tests {
    use crate::codegen::test_util::{ir, ir_rc};

    /// The definition of `lugha_fn_<name>`, up to its closing brace.
    fn body<'a>(ir: &'a str, name: &str) -> &'a str {
        let call = format!("@lugha_fn_{name}(");
        let at = ir
            .match_indices("define ")
            .map(|(at, _)| at)
            .find(|&at| ir[at..].lines().next().is_some_and(|l| l.contains(&call)))
            .unwrap_or_else(|| panic!("{name}: {ir}"));
        let start = &ir[at..];
        &start[..start.find("\n}").expect("a body ends")]
    }

    #[test]
    fn a_copy_adds_a_holder_instead_of_marking_the_array() {
        let ir = ir_rc(
            "fun main(console: Console) { let xs = [\"a\"]; let ys = xs; console.println(xs.len + ys.len); }",
        );
        let main = body(&ir, "main");
        assert!(main.contains("call void @lugha_rc_inc("), "{main}");
        assert!(!main.contains("or i64 %cap"), "{main}");
    }

    #[test]
    fn a_block_end_releases_its_locals() {
        let ir = ir_rc("fun f() { let xs = [1]; }\nfun main() { f(); }");
        assert!(body(&ir, "f").contains("call void @lugha_rc_dec("), "{ir}");
    }

    #[test]
    fn a_returned_local_moves_out_without_a_new_holder() {
        let ir = ir_rc("fun f(): i64[] { let xs = [1]; xs }\nfun main() { let ys = f(); }");
        assert!(!body(&ir, "f").contains("@lugha_rc_inc("), "{ir}");
    }

    #[test]
    fn a_write_clones_an_array_whose_count_is_not_one() {
        let ir = ir_rc("fun main() { let mut xs = [1]; let ys = xs; xs[0] = 2; let n = ys.len; }");
        let main = body(&ir, "main");
        assert!(main.contains("icmp ne i64 %count, 1"), "{main}");
        assert!(main.contains("cow.clone"), "{main}");
    }

    #[test]
    fn objects_carry_their_drop_and_constants_never_die() {
        let ir = ir_rc("fun main() { let xs = [\"hey\"]; }");
        assert!(
            ir.contains(
                "call void @lugha_rt_rc_set_drop(ptr %array, ptr @lugha_rt_rc_drop_pointers)"
            ),
            "{ir}"
        );
        // The count of a literal: the immortal one.
        assert!(
            ir.contains("{ ptr null, i64 -9223372036854775808, i64 3,"),
            "{ir}"
        );
    }

    #[test]
    fn a_recursive_enum_is_counted_through_its_box() {
        let ir = ir_rc("enum L { Nil, Cons(i64, L) }\nfun main() { let l = L.Cons(1, L.Nil); }");
        assert!(ir.contains("@lugha_rc_release_1L("), "{ir}");
        assert!(ir.contains("@lugha_rc_drop_box_1L"), "{ir}");
    }

    #[test]
    fn without_the_flag_nothing_is_counted() {
        let ir = ir(
            "fun main(console: Console) { let xs = [\"a\"]; let ys = xs; console.println(ys.len); }",
        );
        assert!(!ir.contains("lugha_rc_"), "{ir}");
    }
}
