//! Buffers in callbacks: a callback parameter `View<T>` is C's
//! `const T *, size_t` and `MutView<T>` is `T *, size_t`, both C's memory
//! itself, read (and written) in place; `T[]`, a `spawned` callback's, is
//! `const T *, size_t` and arrives as a copy, since C's memory may be gone
//! by the time the task runs.

use inkwell::values::BasicValueEnum;

use super::lower::{Lowerer, POSITIONED};
use crate::check::Type;

/// True if callback parameter `ty` is C's pointer and count.
pub(super) fn counted(ty: &Type) -> bool {
    matches!(ty, Type::Array(_) | Type::View(..))
}

/// C's parameters for a callback taking `params` with `data` inserted
/// before parameter `data`: each counted parameter is a pointer and a count.
pub(super) fn c_params(params: &[Type], data: Option<usize>) -> (Vec<Type>, Option<usize>) {
    let (mut out, mut at) = (Vec::new(), None);
    for (i, ty) in params.iter().enumerate() {
        if data == Some(i) {
            at = Some(out.len());
        }
        if counted(ty) {
            out.push(Type::Handle("CPtr".to_string()));
            out.push(Type::U64);
        } else {
            out.push(ty.clone());
        }
    }
    if data == Some(params.len()) {
        at = Some(out.len());
    }
    (out, at)
}

impl<'ctx> Lowerer<'ctx> {
    /// The Lugha value for counted parameter `ty` from C's pointer and
    /// count: a view of C's memory, or a new array holding a copy of it.
    pub(super) fn counted_from_c(
        &mut self,
        ty: &Type,
        (pointer, count): (BasicValueEnum<'ctx>, BasicValueEnum<'ctx>),
    ) -> BasicValueEnum<'ctx> {
        let (pointer, count) = (pointer.into_pointer_value(), count.into_int_value());
        let Type::Array(element) = ty else {
            return self.view_value(pointer, count);
        };
        let array = self.new_array(count, element);
        let size = self.element_size(element);
        let bytes = self
            .builder
            .build_int_mul(
                count,
                self.context.i64_type().const_int(size, false),
                "bytes",
            )
            .expect(POSITIONED);
        let first = self.element_address(array, self.context.i64_type().const_zero(), size);
        self.builder
            .build_memcpy(first, 1, pointer, 1, bytes)
            .expect(POSITIONED);
        array.into()
    }
}
