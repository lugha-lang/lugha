//! I/O in codegen: `read_line`,
//! `read_file`, `write_file`, `append_file`, `eprint`, `eprintln` and
//! `exit` through the runtime. A runtime function that can fail returns a
//! flag and writes its value or error through pointers; the flag picks
//! between the two `Option`/`Result` values built from them.

use inkwell::IntPredicate;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue, ValueKind};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::value::{Value, llvm_type};
use crate::ast::Expr;
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// One of the I/O intrinsics; the checker checked its arguments.
    pub(super) fn io_intrinsic(
        &mut self,
        call: &Expr,
        name: &str,
        args: &[Expr],
    ) -> Result<Value<'ctx>, CodegenError> {
        match name {
            "read_line" => {
                let line = self.out_slot(&Type::String);
                let ok = self.call_flag("lugha_rt_read_line", &[Type::String], &[line.into()]);
                let text = self.load_slot(line, &Type::String);
                let option = self.ty(call);
                let some = self.construct_values(&option, "Some", vec![text]);
                let none = self.construct_values(&option, "None", Vec::new());
                Ok(Value::Val(self.choose(ok, some, none)))
            }
            "flush" => {
                let flush = self.runtime("lugha_rt_flush", &[], None);
                self.builder.build_call(flush, &[], "").expect(POSITIONED);
                Ok(Value::Void)
            }
            "read_stdin_bytes" => Ok(Value::Val(self.call_runtime_value(
                "lugha_rt_read_stdin_bytes",
                &[],
                &[],
            ))),
            "eprint" | "eprintln" => {
                let text = match args.first() {
                    Some(arg) => {
                        let ty = self.ty(arg);
                        let value = self.get(arg, &ty)?;
                        self.text(value, &ty)
                    }
                    None => self.string_literal("").into(),
                };
                let newline = self
                    .context
                    .i32_type()
                    .const_int(u64::from(name == "eprintln"), false);
                let eprint = self.runtime("lugha_rt_eprint", &[Type::String, Type::I32], None);
                self.builder
                    .build_call(eprint, &[text.into(), newline.into()], "")
                    .expect(POSITIONED);
                self.rc_drop_string(text);
                Ok(Value::Void)
            }
            _ => {
                let code = self.get(&args[0], &Type::I32)?;
                let exit = self.runtime("lugha_rt_exit", &[Type::I32], None);
                self.builder
                    .build_call(exit, &[code.into()], "")
                    .expect(POSITIONED);
                // `exit` never returns.
                self.builder.build_unreachable().expect(POSITIONED);
                self.start_dead_block("after.exit");
                Ok(Value::Never)
            }
        }
    }

    /// A stack slot for a runtime out-parameter, cleared first. Any type:
    /// a tuple of pointers is a struct C fills in field by field.
    pub(super) fn out_slot(&mut self, ty: &Type) -> PointerValue<'ctx> {
        let slot = self.entry_alloca(ty, "out");
        let zero: BasicValueEnum = llvm_type(self.context, ty).const_zero();
        self.builder.build_store(slot, zero).expect(POSITIONED);
        slot
    }

    pub(super) fn load_slot(
        &mut self,
        slot: PointerValue<'ctx>,
        ty: &Type,
    ) -> BasicValueEnum<'ctx> {
        let llvm = llvm_type(self.context, ty);
        self.builder
            .build_load(llvm, slot, "value")
            .expect(POSITIONED)
    }

    /// Calls runtime function `name`, which returns a success flag.
    pub(super) fn call_flag(
        &mut self,
        name: &str,
        params: &[Type],
        args: &[BasicValueEnum<'ctx>],
    ) -> IntValue<'ctx> {
        let function = self.runtime(name, params, Some(Type::I32));
        let args: Vec<_> = args.iter().map(|a| (*a).into()).collect();
        let call = self
            .builder
            .build_call(function, &args, "ok")
            .expect(POSITIONED);
        let ValueKind::Basic(flag) = call.try_as_basic_value() else {
            unreachable!("`{name}` returns a flag")
        };
        let zero = self.context.i32_type().const_zero();
        self.builder
            .build_int_compare(IntPredicate::NE, flag.into_int_value(), zero, "succeeded")
            .expect(POSITIONED)
    }

    /// `Ok` with the value in `value`'s slot, or `Err` with the message, by `ok`.
    pub(super) fn result(
        &mut self,
        call: &Expr,
        ok: IntValue<'ctx>,
        (value, ty): (PointerValue<'ctx>, &Type),
        error: PointerValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let result = self.ty(call);
        let value = self.load_slot(value, ty);
        let message = self.load_slot(error, &Type::String);
        let good = self.construct_values(&result, "Ok", vec![value]);
        let bad = self.construct_values(&result, "Err", vec![message]);
        self.choose(ok, good, bad)
    }

    pub(super) fn choose(
        &mut self,
        flag: IntValue<'ctx>,
        yes: BasicValueEnum<'ctx>,
        no: BasicValueEnum<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        self.builder
            .build_select(flag, yes, no, "chosen")
            .expect(POSITIONED)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn narrow_indexes_widen_by_signedness() {
        let ir = crate::codegen::test_util::ir(
            "fun main() { let xs = [1, 2]; let a: u8 = 1; let b: i32 = 0; let n = xs[a] + xs[b]; }",
        );
        assert!(ir.contains("zext i8"), "{ir}");
        assert!(ir.contains("sext i32"), "{ir}");
    }
}
