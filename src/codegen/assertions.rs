//! `skip(reason)`: the runtime ends the test as skipped. The assertions
//! are prelude functions marked `@caller_location`.

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::value::Value;
use crate::ast::Expr;
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// `skip(reason)`; the checker made the argument fit.
    pub(super) fn assertion(
        &mut self,
        call: &Expr,
        name: &str,
        args: &[Expr],
    ) -> Result<Value<'ctx>, CodegenError> {
        let at = call.span.start;
        // `skip(reason)`: the runtime ends the test as skipped.
        if name == "skip" {
            let reason = self.get(&args[0], &Type::String)?;
            let file = self.file_name(at);
            let (line, col) = self.source.line_col(at);
            let i64_type = self.context.i64_type();
            let skip = self.runtime(
                "lugha_rt_test_skip",
                &[Type::String, Type::String, Type::I64, Type::I64],
                None,
            );
            let args = [
                reason.into(),
                file.into(),
                i64_type.const_int(line as u64, false).into(),
                i64_type.const_int(col as u64, false).into(),
            ];
            self.builder.build_call(skip, &args, "").expect(POSITIONED);
            self.builder.build_unreachable().expect(POSITIONED);
            // The call never returns: what follows goes in a dead block, as after `panic`.
            self.start_dead_block("after.skip");
            return Ok(Value::Never);
        }
        unreachable!("checked: `skip` is the one assertion intrinsic")
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::ir;

    #[test]
    fn a_failed_assertion_panics_with_both_values() {
        let ir = ir("fun main() { assert(1 < 2); assert_eq(1, 2); assert_ne(\"a\", \"b\"); }");
        assert!(ir.contains("assertion failed"), "{ir}");
        assert!(ir.contains("assert_eq failed: left "), "{ir}");
        assert!(ir.contains("assert_ne failed: both "), "{ir}");
        assert!(ir.contains("call void @lugha_rt_panic"), "{ir}");
    }
}
