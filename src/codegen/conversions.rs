//! The conversion methods in codegen. `truncate` and `saturate` from a
//! float are `as`'s old conversions; `saturate` between integers clamps
//! through `i128`; `round`, `floor` and `ceil` round a float the named way,
//! then panic if the result doesn't fit; `try_into` checks the value
//! survives the trip and builds `Some` or `None`.

use inkwell::values::{BasicValueEnum, FloatValue, IntValue, ValueKind};
use inkwell::{FloatPredicate, IntPredicate};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::value::{Value, int_type, is_signed};
use crate::ast::Expr;
use crate::check::{Type, int_bits};

/// The panic when a rounded value doesn't fit its target.
const OUT_OF_RANGE: &str = "conversion out of range";

impl<'ctx> Lowerer<'ctx> {
    /// `base.method<U>()` for a conversion method; `call` is the whole call,
    /// whose type is `U` (`Option<U>` for `try_into`).
    pub(super) fn conversion(
        &mut self,
        call: &Expr,
        base: &Expr,
        method: &str,
        at: usize,
    ) -> Result<Value<'ctx>, CodegenError> {
        let from = self.ty(base);
        let result = self.ty(call);
        let value = self.get(base, &from)?;
        let to = match (&result, method) {
            (Type::Option(inner), "try_into") => (**inner).clone(),
            _ => result.clone(),
        };
        let out = match method {
            "try_into" => self.try_into(value, &from, &to, &result),
            "saturate" => self.saturate_to(value, &from, &to),
            "round" | "floor" | "ceil" => self.rounded(value, (&from, &to), method, at),
            // `truncate`: the low bits, as `as` kept them.
            _ => self.convert(value, &from, &to),
        };
        Ok(Value::Val(out))
    }

    /// `Some(value as to)` if it converts exactly, else `None`.
    fn try_into(
        &mut self,
        value: BasicValueEnum<'ctx>,
        from: &Type,
        to: &Type,
        option: &Type,
    ) -> BasicValueEnum<'ctx> {
        let fits = self.fits_exactly(value, from, to);
        let converted = match to {
            // A valid code point is its own `char`.
            Type::Char => self.convert(value, from, &Type::U32),
            _ => self.convert(value, from, to),
        };
        let some = self.construct_values(option, "Some", vec![converted]);
        let none = self.construct_values(option, "None", Vec::new());
        self.builder
            .build_select(fits, some, none, "try_into")
            .expect(POSITIONED)
    }

    /// True if `value` of type `from` has an exact `to`.
    fn fits_exactly(
        &mut self,
        value: BasicValueEnum<'ctx>,
        from: &Type,
        to: &Type,
    ) -> IntValue<'ctx> {
        let b = &self.builder;
        let yes = self.context.bool_type().const_int(1, false);
        if from == to {
            return yes;
        }
        match (from.is_float(), to.is_float()) {
            _ if *to == Type::Char => self.is_scalar(value.into_int_value(), from),
            // Whole, and inside the range.
            (true, false) => {
                let x = value.into_float_value();
                let whole = self.float_intrinsic("llvm.trunc", x);
                let same = b
                    .build_float_compare(FloatPredicate::OEQ, x, whole, "whole")
                    .expect(POSITIONED);
                let inside = self.float_in_range(x, to);
                self.builder
                    .build_and(same, inside, "fits")
                    .expect(POSITIONED)
            }
            (true, true) => {
                // Narrowing keeps the value, or it was NaN.
                let x = value.into_float_value();
                let there = self.convert(value, from, to);
                let back = self.convert(there, to, from).into_float_value();
                let same = b
                    .build_float_compare(FloatPredicate::OEQ, back, x, "same")
                    .expect(POSITIONED);
                let nan = b
                    .build_float_compare(FloatPredicate::UNO, x, x, "nan")
                    .expect(POSITIONED);
                b.build_or(same, nan, "fits").expect(POSITIONED)
            }
            // The float holds it if converting back gives it again and it
            // didn't round up past the integer's range.
            (false, true) => {
                let x = value.into_int_value();
                let there = self.convert(value, from, to);
                let back = self.convert(there, to, from).into_int_value();
                let same = b
                    .build_int_compare(IntPredicate::EQ, back, x, "same")
                    .expect(POSITIONED);
                let inside = self.float_in_range(there.into_float_value(), from);
                self.builder
                    .build_and(same, inside, "fits")
                    .expect(POSITIONED)
            }
            (false, false) => self.int_fits(value.into_int_value(), from, to),
        }
    }

    /// True if integer `x` of type `from` is a value of integer type `to`.
    fn int_fits(&mut self, x: IntValue<'ctx>, from: &Type, to: &Type) -> IntValue<'ctx> {
        let there = self.convert(x.into(), from, to);
        let back = self.convert(there, to, from).into_int_value();
        let b = &self.builder;
        let same = b
            .build_int_compare(IntPredicate::EQ, back, x, "same")
            .expect(POSITIONED);
        // Same bits, different meaning: a negative value never fits an
        // unsigned type, nor a value past the signed range a signed one.
        let negative = |v: IntValue<'ctx>, name| {
            b.build_int_compare(IntPredicate::SLT, v, v.get_type().const_zero(), name)
                .expect(POSITIONED)
        };
        let flipped = match (is_signed(from), is_signed(to)) {
            (true, false) => Some(negative(x, "negative")),
            (false, true) => Some(negative(there.into_int_value(), "too_big")),
            _ => None,
        };
        match flipped {
            Some(flipped) => {
                let kept = b.build_not(flipped, "kept").expect(POSITIONED);
                b.build_and(same, kept, "fits").expect(POSITIONED)
            }
            None => same,
        }
    }

    /// True if float `x` lies in integer type `to`'s range, before
    /// truncation: from its minimum up to, not including, one past its
    /// maximum. NaN never does.
    fn float_in_range(&self, x: FloatValue<'ctx>, to: &Type) -> IntValue<'ctx> {
        let (bits, signed) = int_bits(to).expect("an integer target");
        let (low, high) = match signed {
            true => (-(2f64.powi(bits as i32 - 1)), 2f64.powi(bits as i32 - 1)),
            false => (0.0, 2f64.powi(bits as i32)),
        };
        let ft = x.get_type();
        let b = &self.builder;
        let above = b
            .build_float_compare(FloatPredicate::OGE, x, ft.const_float(low), "above")
            .expect(POSITIONED);
        let below = b
            .build_float_compare(FloatPredicate::OLT, x, ft.const_float(high), "below")
            .expect(POSITIONED);
        b.build_and(above, below, "in_range").expect(POSITIONED)
    }

    /// The nearest value of `to` in its range.
    fn saturate_to(
        &mut self,
        value: BasicValueEnum<'ctx>,
        from: &Type,
        to: &Type,
    ) -> BasicValueEnum<'ctx> {
        match (from.is_float(), to.is_float()) {
            (false, false) => self.clamp_int(value.into_int_value(), from, to),
            (true, true) if from == &Type::F64 && to == &Type::F32 => {
                let x = value.into_float_value();
                let max = x.get_type().const_float(f64::from(f32::MAX));
                let min = x.get_type().const_float(f64::from(f32::MIN));
                let b = &self.builder;
                let infinite = x.get_type().const_float(f64::INFINITY);
                let finite = b
                    .build_float_compare(FloatPredicate::ONE, self.fabs(x), infinite, "finite")
                    .expect(POSITIONED);
                let over = b
                    .build_float_compare(FloatPredicate::OGT, x, max, "over")
                    .expect(POSITIONED);
                let under = b
                    .build_float_compare(FloatPredicate::OLT, x, min, "under")
                    .expect(POSITIONED);
                let over = b.build_and(over, finite, "over").expect(POSITIONED);
                let under = b.build_and(under, finite, "under").expect(POSITIONED);
                let x = b
                    .build_select(over, max, x, "clamp")
                    .expect(POSITIONED)
                    .into_float_value();
                let x = b.build_select(under, min, x, "clamp").expect(POSITIONED);
                self.convert(x, from, to)
            }
            // Float to integer is `as`'s: toward zero, clamped, NaN as 0.
            _ => self.convert(value, from, to),
        }
    }

    /// Integer `x` clamped into integer type `to`, compared as `i128`,
    /// which holds every value of both.
    fn clamp_int(&mut self, x: IntValue<'ctx>, from: &Type, to: &Type) -> BasicValueEnum<'ctx> {
        let wide = self.context.i128_type();
        let b = &self.builder;
        let x = if is_signed(from) {
            b.build_int_s_extend(x, wide, "wide")
        } else {
            b.build_int_z_extend(x, wide, "wide")
        }
        .expect(POSITIONED);
        let (bits, signed) = int_bits(to).expect("an integer target");
        let (low, high): (i128, i128) = match signed {
            true => (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1),
            false => (0, (1i128 << bits) - 1),
        };
        let constant = |n: i128| wide.const_int_arbitrary_precision(&[n as u64, (n >> 64) as u64]);
        let (low, high) = (constant(low), constant(high));
        let over = b
            .build_int_compare(IntPredicate::SGT, x, high, "over")
            .expect(POSITIONED);
        let x = b
            .build_select(over, high, x, "clamp")
            .expect(POSITIONED)
            .into_int_value();
        let under = b
            .build_int_compare(IntPredicate::SLT, x, low, "under")
            .expect(POSITIONED);
        let x = b
            .build_select(under, low, x, "clamp")
            .expect(POSITIONED)
            .into_int_value();
        b.build_int_truncate(x, int_type(self.context, to), "saturated")
            .expect(POSITIONED)
            .into()
    }

    /// `round`, `floor` or `ceil` to `to`: the float rounded the named way
    /// (halves away from zero for `round`), then converted; a result `to`
    /// can't hold panics at `at`.
    fn rounded(
        &mut self,
        value: BasicValueEnum<'ctx>,
        (from, to): (&Type, &Type),
        method: &str,
        at: usize,
    ) -> BasicValueEnum<'ctx> {
        match (from.is_float(), to.is_float()) {
            (true, false) => {
                let x = self.float_intrinsic(&format!("llvm.{method}"), value.into_float_value());
                let inside = self.float_in_range(x, to);
                let outside = self.builder.build_not(inside, "outside").expect(POSITIONED);
                self.panic_if(outside, OUT_OF_RANGE, at);
                self.convert(x.into(), from, to)
            }
            (true, true) => {
                let x = match method {
                    "round" => value.into_float_value(),
                    _ => self.float_intrinsic(&format!("llvm.{method}"), value.into_float_value()),
                };
                let y = self.convert(x.into(), from, to).into_float_value();
                // Finite in, infinite out: past the target's range.
                let infinite = y.get_type().const_float(f64::INFINITY);
                let b = &self.builder;
                let became = b
                    .build_float_compare(FloatPredicate::OEQ, self.fabs(y), infinite, "inf")
                    .expect(POSITIONED);
                let was = b
                    .build_float_compare(
                        FloatPredicate::OEQ,
                        self.fabs(x),
                        x.get_type().const_float(f64::INFINITY),
                        "was_inf",
                    )
                    .expect(POSITIONED);
                let was_not = b.build_not(was, "finite").expect(POSITIONED);
                let overflow = b.build_and(became, was_not, "overflow").expect(POSITIONED);
                self.panic_if(overflow, OUT_OF_RANGE, at);
                y.into()
            }
            // An integer to the nearest float.
            (false, true) => self.convert(value, from, to),
            // An integer to an integer: exact, or out of range.
            (false, false) => {
                let fits = self.int_fits(value.into_int_value(), from, to);
                let outside = self.builder.build_not(fits, "outside").expect(POSITIONED);
                self.panic_if(outside, OUT_OF_RANGE, at);
                self.convert(value, from, to)
            }
        }
    }

    /// True if integer `i` of type `ty` is a Unicode scalar value.
    fn is_scalar(&self, i: IntValue<'ctx>, ty: &Type) -> IntValue<'ctx> {
        let i64_type = self.context.i64_type();
        let wide = self.widen_index(i, ty);
        let b = &self.builder;
        let c = |n: u64| i64_type.const_int(n, false);
        // Unsigned compares: a negative `i64` is huge, so it fails both.
        let low = b
            .build_int_compare(IntPredicate::ULT, wide, c(0xD800), "low")
            .expect(POSITIONED);
        let above = b.build_int_sub(wide, c(0xE000), "above").expect(POSITIONED);
        let high = b
            .build_int_compare(IntPredicate::ULT, above, c(0x10_FFFF - 0xE000 + 1), "high")
            .expect(POSITIONED);
        b.build_or(low, high, "scalar").expect(POSITIONED)
    }

    /// `|x|`.
    fn fabs(&self, x: FloatValue<'ctx>) -> FloatValue<'ctx> {
        self.float_intrinsic("llvm.fabs", x)
    }

    /// A one-operand LLVM float intrinsic on `x`.
    fn float_intrinsic(&self, name: &str, x: FloatValue<'ctx>) -> FloatValue<'ctx> {
        let ty = x.get_type();
        let function = inkwell::intrinsics::Intrinsic::find(name)
            .and_then(|i| i.get_declaration(&self.module, &[ty.into()]))
            .expect("LLVM provides the float rounding intrinsics");
        let call = self
            .builder
            .build_call(function, &[x.into()], "round")
            .expect(POSITIONED);
        match call.try_as_basic_value() {
            ValueKind::Basic(result) => result.into_float_value(),
            ValueKind::Instruction(_) => unreachable!("the intrinsic returns a float"),
        }
    }
}
