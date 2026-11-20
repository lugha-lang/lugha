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
            "read_line" => Ok(Value::Val(self.read_line(call))),
            "flush" => {
                let flush = self.runtime("lugha_rt_flush", &[], None);
                self.builder.build_call(flush, &[], "").expect(POSITIONED);
                Ok(Value::Void)
            }
            "read_stdin_bytes" => {
                let bytes = Type::Array(Box::new(Type::U8));
                let out = self.out_slot(&bytes);
                let error = self.out_slot(&Type::String);
                let ok = self.call_flag(
                    "lugha_rt_read_stdin_bytes",
                    &[Type::String, Type::String],
                    &[out.into(), error.into()],
                );
                Ok(Value::Val(self.result(call, ok, (out, &bytes), error)))
            }
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

    /// `read_line()`: `Ok(Some((number, bytes)))`, `Ok(None)` at the end of
    /// input, or `Err(message)`, by the runtime's 1, 0 or -1.
    fn read_line(&mut self, call: &Expr) -> BasicValueEnum<'ctx> {
        let bytes = Type::Array(Box::new(Type::U8));
        let (number, line) = (self.out_slot(&Type::I64), self.out_slot(&bytes));
        let error = self.out_slot(&Type::String);
        let params = [Type::String, Type::String, Type::String];
        let function = self.runtime("lugha_rt_read_line", &params, Some(Type::I32));
        let state = self
            .builder
            .build_call(
                function,
                &[number.into(), line.into(), error.into()],
                "state",
            )
            .expect(POSITIONED);
        let ValueKind::Basic(state) = state.try_as_basic_value() else {
            unreachable!("`lugha_rt_read_line` returns a state")
        };
        let state = state.into_int_value();
        let zero = self.context.i32_type().const_zero();
        let (got, ok) = (
            self.builder
                .build_int_compare(IntPredicate::SGT, state, zero, "got")
                .expect(POSITIONED),
            self.builder
                .build_int_compare(IntPredicate::SGE, state, zero, "ok")
                .expect(POSITIONED),
        );
        let result = self.ty(call);
        let Type::Enum(_, parts) = &result else {
            unreachable!("checked: `read_line` gives a `Result`")
        };
        let option = parts[0].clone();
        let Type::Option(tuple) = &option else {
            unreachable!("checked: `read_line` gives a `Result<Option<_>, _>`")
        };
        let llvm = llvm_type(self.context, tuple).into_struct_type();
        let mut pair = llvm.get_undef();
        let parts = [
            self.load_slot(number, &Type::I64),
            self.load_slot(line, &bytes),
        ];
        for (i, part) in parts.into_iter().enumerate() {
            let index = u32::try_from(i).expect("two elements");
            pair = self
                .builder
                .build_insert_value(pair, part, index, "line")
                .expect(POSITIONED)
                .into_struct_value();
        }
        let some = self.construct_values(&option, "Some", vec![pair.into()]);
        let none = self.construct_values(&option, "None", Vec::new());
        let maybe = self.choose(got, some, none);
        let message = self.load_slot(error, &Type::String);
        let good = self.construct_values(&result, "Ok", vec![maybe]);
        let bad = self.construct_values(&result, "Err", vec![message]);
        self.choose(ok, good, bad)
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
