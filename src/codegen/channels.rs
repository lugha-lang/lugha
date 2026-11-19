//! Channels in codegen. Each end of a channel, `Sender<T>` and
//! `Receiver<T>`, is a pointer to the runtime's one channel; values cross
//! it as `T` in element layout, copied here (a copy site) and moved byte
//! for byte by the runtime. `select` is `codegen::select`.

use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue, ValueKind};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::value::{Value, llvm_type};
use crate::ast::Expr;
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// `Chan<T>.new(n)` or `Chan<T>.unbounded()`, `call` being the whole
    /// call: one channel, both ends of the pair pointing at it.
    pub(super) fn chan_new(
        &mut self,
        call: &Expr,
        args: &[Expr],
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let pair = self.ty(call);
        let Some([Type::Chan(writer, element), receiver]) = crate::check::tuple_elements(&pair)
        else {
            unreachable!("checked: `Chan<T>.new` makes a `(Sender<T>, Receiver<T>)`")
        };
        let ring = *writer == crate::check::End::Ring;
        let (element, receiver) = ((**element).clone(), receiver.clone());
        let i64_type = self.context.i64_type();
        let size = i64_type.const_int(self.element_size(&element).max(1), false);
        let (function, mut values) = match args.first() {
            Some(cap) if ring => (
                "lugha_rt_ring_new",
                vec![self.get(cap, &Type::I64)?, size.into()],
            ),
            Some(cap) => (
                "lugha_rt_chan_new",
                vec![self.get(cap, &Type::I64)?, size.into()],
            ),
            None => ("lugha_rt_chan_unbounded", vec![size.into()]),
        };
        values.extend(self.location(call.span.start));
        let chan = self.call_runtime(function, &values, true);
        // The channel releases what it buffers.
        let ptr = self.context.ptr_type(Default::default());
        let visit = match self.rc_box_drop(&element) {
            Some(f) => f.as_global_value().as_pointer_value(),
            None => ptr.const_null(),
        };
        self.call_runtime("lugha_rt_rc_chan_setup", &[chan, visit.into()], false);
        // Two ends, two holders of the one channel.
        self.rc_retain(chan, &receiver);
        let shell = llvm_type(self.context, &pair)
            .into_struct_type()
            .get_undef();
        let pair = self
            .builder
            .build_insert_value(shell, chan, 0, "ends")
            .expect("a pair has 2 fields");
        let pair = self
            .builder
            .build_insert_value(pair, chan, 1, "ends")
            .expect("a pair has 2 fields");
        Ok(pair.into_struct_value().into())
    }

    /// A method of a channel's end: `send`, `try_send`, `close`, `recv`,
    /// `try_recv` or `is`.
    pub(super) fn chan_method(
        &mut self,
        call: &Expr,
        base: &Expr,
        name: &str,
        args: &[Expr],
    ) -> Result<Value<'ctx>, CodegenError> {
        let ty = self.ty(base);
        let Type::Chan(_, element) = &ty else {
            unreachable!("checked: a channel method has a channel's end as its receiver")
        };
        let chan = self.get(base, &ty)?;
        Ok(match name {
            "send" => {
                let data = self.element_value(&args[0], element)?;
                let mut values = vec![chan, data.into()];
                values.extend(self.location(call.span.start));
                self.call_runtime("lugha_rt_chan_send", &values, false);
                Value::Void
            }
            "try_send" => {
                let data = self.element_value(&args[0], element)?;
                // A ring's writing end never locks, allocates or waits.
                let function = match &ty {
                    Type::Chan(crate::check::End::Ring, _) => "lugha_rt_ring_try_send",
                    _ => "lugha_rt_chan_try_send",
                };
                let how = self.call_runtime(function, &[chan, data.into()], true);
                // Not sent: the value is still the slot's, and comes back.
                let tried = self.ty(call);
                let value = self.load_element(data, element);
                let full = self.construct_values(&tried, "Full", vec![value]);
                let closed = self.construct_values(&tried, "Closed", vec![value]);
                let sent = self.construct_values(&tried, "Sent", Vec::new());
                Value::Val(self.three_way(how, sent, full, closed))
            }
            "recv" | "next" => Value::Val(self.chan_receive(chan, element)),
            "try_recv" => {
                let data = self.element_slot(element);
                let how = self.call_runtime("lugha_rt_chan_try_recv", &[chan, data.into()], true);
                let tried = self.ty(call);
                let value = self.load_element(data, element);
                let ready = self.construct_values(&tried, "Ready", vec![value]);
                let empty = self.construct_values(&tried, "Empty", Vec::new());
                let closed = self.construct_values(&tried, "Closed", Vec::new());
                Value::Val(self.three_way(how, empty, ready, closed))
            }
            "close" => {
                let mut values = vec![chan];
                values.extend(self.location(call.span.start));
                self.call_runtime("lugha_rt_chan_close", &values, false);
                Value::Void
            }
            "is" => {
                let other = self.get(&args[0], &ty)?;
                let same = self
                    .builder
                    .build_int_compare(
                        IntPredicate::EQ,
                        chan.into_pointer_value(),
                        other.into_pointer_value(),
                        "same",
                    )
                    .expect(POSITIONED);
                Value::Val(same.into())
            }
            _ => unreachable!("checked: a channel's ends have only these methods"),
        })
    }

    /// `recv` on the channel `chan` carrying `element`: `Some` value, or
    /// `None` once it is closed and empty.
    pub(super) fn chan_receive(
        &mut self,
        chan: BasicValueEnum<'ctx>,
        element: &Type,
    ) -> BasicValueEnum<'ctx> {
        let data = self.element_slot(element);
        let flag = self.call_runtime("lugha_rt_chan_recv", &[chan, data.into()], true);
        let got = self.nonzero(flag);
        self.received(got, data, element)
    }

    /// The runtime's answer `how` (0, 1 or 2) as the value of that rank.
    fn three_way(
        &mut self,
        how: BasicValueEnum<'ctx>,
        zero: BasicValueEnum<'ctx>,
        one: BasicValueEnum<'ctx>,
        two: BasicValueEnum<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let how = how.into_int_value();
        let is = |this: &mut Self, n: u64| {
            this.builder
                .build_int_compare(
                    IntPredicate::EQ,
                    how,
                    how.get_type().const_int(n, false),
                    "is",
                )
                .expect(POSITIONED)
        };
        let is_one = is(self, 1);
        let is_zero = is(self, 0);
        let rest = self.choose(is_one, one, two);
        self.choose(is_zero, zero, rest)
    }

    /// `Some(value at data)` when `got`, else `None`.
    pub(super) fn received(
        &mut self,
        got: IntValue<'ctx>,
        data: PointerValue<'ctx>,
        element: &Type,
    ) -> BasicValueEnum<'ctx> {
        let option = Type::Option(Box::new(element.clone()));
        let value = self.load_element(data, element);
        let some = self.construct_values(&option, "Some", vec![value]);
        let none = self.construct_values(&option, "None", Vec::new());
        self.choose(got, some, none)
    }

    /// A slot holding a copy of `value` in element layout, to send.
    pub(super) fn element_value(
        &mut self,
        value: &Expr,
        element: &Type,
    ) -> Result<PointerValue<'ctx>, CodegenError> {
        let value = self.value_for_store(value, element)?;
        // It crosses to another task: marked shared.
        self.rc_mark(value, element);

        let slot = self.element_slot(element);
        self.store_element(slot, element, value);
        Ok(slot)
    }

    /// A stack slot for one `element` in element layout.
    pub(super) fn element_slot(&self, element: &Type) -> PointerValue<'ctx> {
        self.raw_alloca(self.storage_type(element), "elem")
    }

    /// An alloca of `ty` at the top of the entry block.
    pub(super) fn raw_alloca(&self, ty: BasicTypeEnum<'ctx>, name: &str) -> PointerValue<'ctx> {
        let function = self.function.expect("inside a function");
        let entry = function
            .get_first_basic_block()
            .expect("functions start with an entry block");
        let builder = self.context.create_builder();
        match entry.get_first_instruction() {
            Some(first) => builder.position_before(&first),
            None => builder.position_at_end(entry),
        }
        builder.build_alloca(ty, name).expect(POSITIONED)
    }

    pub(super) fn nonzero(&self, flag: BasicValueEnum<'ctx>) -> IntValue<'ctx> {
        let flag = flag.into_int_value();
        self.builder
            .build_int_compare(IntPredicate::NE, flag, flag.get_type().const_zero(), "got")
            .expect(POSITIONED)
    }

    /// `file, line, col` of source offset `at`, for the runtime's panics.
    pub(super) fn location(&mut self, at: usize) -> [BasicValueEnum<'ctx>; 3] {
        let file = self.file_name(at);
        let (line, col) = self.source.line_col(at);
        let i64_type = self.context.i64_type();
        [
            file.into(),
            i64_type.const_int(line as u64, false).into(),
            i64_type.const_int(col as u64, false).into(),
        ]
    }

    /// Calls the runtime function `name` with `values`, declared from them;
    /// its result when `returns` (a pointer for `chan_new`, else an
    /// integer of the C function's width).
    pub(super) fn call_runtime(
        &mut self,
        name: &str,
        values: &[BasicValueEnum<'ctx>],
        returns: bool,
    ) -> BasicValueEnum<'ctx> {
        let function = match self.module.get_function(name) {
            Some(function) => function,
            None => {
                let params: Vec<_> = values.iter().map(|v| v.get_type().into()).collect();
                let fn_type = match (returns, name) {
                    (false, _) => self.context.void_type().fn_type(&params, false),
                    (
                        true,
                        "lugha_rt_chan_new" | "lugha_rt_chan_unbounded" | "lugha_rt_ring_new",
                    ) => self
                        .context
                        .ptr_type(Default::default())
                        .fn_type(&params, false),
                    (true, "lugha_rt_task_select") => {
                        self.context.i64_type().fn_type(&params, false)
                    }
                    (true, _) => self.context.i32_type().fn_type(&params, false),
                };
                self.module.add_function(name, fn_type, None)
            }
        };
        let args: Vec<_> = values.iter().map(|v| (*v).into()).collect();
        let site = self
            .builder
            .build_call(function, &args, "rt")
            .expect(POSITIONED);
        match site.try_as_basic_value() {
            ValueKind::Basic(value) => value,
            // A void call's value is never used.
            ValueKind::Instruction(_) => self.context.i32_type().const_zero().into(),
        }
    }
}
