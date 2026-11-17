//! Copy-on-write arrays.
//!
//! A copy site doesn't copy an array: it adds a holder. Before anything
//! writes into an array, `own` checks its count; an array with other
//! holders (or a constant, which no write may touch) is cloned one level
//! deep, its elements each getting one more holder, and the clone is
//! stored back into the slot that held it, which lets go of the original.
//! Programs behave exactly as with eager copies.

use inkwell::values::PointerValue;

use super::heap::element_of;
use super::lower::{Lowerer, POSITIONED};
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// The array in `slot` (an array of type `ty`), made unique first: if it
    /// is shared, a one-level clone is stored back into `slot`.
    pub(super) fn own(&mut self, slot: PointerValue<'ctx>, ty: &Type) -> PointerValue<'ctx> {
        self.own_labelled(slot, ty, "cow")
    }

    /// `own`, its blocks named `<label>.clone` and `<label>.done`.
    pub(super) fn own_labelled(
        &mut self,
        slot: PointerValue<'ctx>,
        ty: &Type,
        label: &str,
    ) -> PointerValue<'ctx> {
        let ptr = self.context.ptr_type(Default::default());
        let array = self
            .builder
            .build_load(ptr, slot, "array")
            .expect(POSITIONED)
            .into_pointer_value();
        let shared = self.rc_shared(array);
        let from = self.builder.get_insert_block().expect(POSITIONED);
        let (clone, done) = (
            self.append(&format!("{label}.clone")),
            self.append(&format!("{label}.done")),
        );
        let branch = self
            .builder
            .build_conditional_branch(shared, clone, done)
            .expect(POSITIONED);
        // Writes to unique arrays are the rule: mark the clone path cold, so
        // the check costs a load and a predicted branch in hot loops.
        let i32_type = self.context.i32_type();
        let weights = self.context.metadata_node(&[
            self.context.metadata_string("branch_weights").into(),
            i32_type.const_int(1, false).into(),
            i32_type.const_int(2000, false).into(),
        ]);
        let kind = self.context.get_kind_id("prof");
        branch
            .set_metadata(weights, kind)
            .expect("a branch takes `prof` metadata");
        self.builder.position_at_end(clone);
        let copy = self.clone_array(array, ty);
        self.builder.build_store(slot, copy).expect(POSITIONED);
        // The slot holds the clone now, not the array.
        self.rc_pointer(array, false);
        let cloned = self.builder.get_insert_block().expect(POSITIONED);
        self.branch(done);
        self.builder.position_at_end(done);
        let phi = self.builder.build_phi(ptr, "owned").expect(POSITIONED);
        phi.add_incoming(&[(&array, from), (&copy, cloned)]);
        phi.as_basic_value().into_pointer_value()
    }

    /// A new array with `array`'s elements: plain elements by one `memcpy`,
    /// elements holding counted values each with one more holder.
    fn clone_array(&mut self, array: PointerValue<'ctx>, ty: &Type) -> PointerValue<'ctx> {
        let element = element_of(ty);
        if element.is_deep(&self.structs) || self.counted(&element) {
            self.clone_deep(array, ty)
        } else {
            self.clone_flat(array, &element)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::ir;

    #[test]
    fn a_copy_holds_instead_of_copying() {
        let ir = ir("fun main() { let xs = [1, 2]; let ys = xs; let n = xs[0] + ys[0]; }");
        let main = &ir[ir.find("define void @lugha_fn_main").unwrap()..];
        let main = &main[..main.find("\n}").unwrap()];
        assert!(main.contains("call void @lugha_rc_inc("), "{main}");
        assert!(!main.contains("llvm.memcpy"), "{main}");
    }

    #[test]
    fn a_write_owns_its_array_first() {
        let ir = ir("fun main() { let mut xs = [1, 2]; let ys = xs; xs[0] = 5; let n = ys[0]; }");
        let main = &ir[ir.find("define void @lugha_fn_main").unwrap()..];
        let main = &main[..main.find("\n}").unwrap()];
        let check = main.find("cow.clone").expect("an own check");
        let store = main.rfind("store i64 5").expect("the element store");
        assert!(check < store, "{main}");
    }
}
