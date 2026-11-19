//! Callbacks registered against a scope, the C entries' side: a queued
//! callback's entry copies C's arguments and the closure into a block and
//! has the runtime start a task in the registering scope
//! (`lugha_rt_cb_queue`), which runs `lugha_cbq_<signature>`; an inline
//! one holds its closure's captures for the call (`lugha_rt_cb_hold`),
//! and a panic in it stops at the entry (`lugha_rt_cb_caught`), which
//! gives C a zero value. An `independent(f)` scope's handler gets a
//! callback task's panic through a function made for its error type.

use inkwell::module::Linkage;
use inkwell::types::{BasicTypeEnum, StructType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue,
    StructValue,
};
use inkwell::{AddressSpace, IntPredicate};

use super::c_calls::CSig;
use super::externs::is_nullable_handle;
use super::lower::{Lowerer, POSITIONED};
use super::scope_failures::failure_types;
use super::value::{is_aggregate, llvm_type};
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// Returns from a C entry the way C expects: `result` (the zero value
    /// of `ret` when `None` and `ret` isn't `()`), through `sig` when the
    /// entry passes structs.
    pub(super) fn c_entry_return(
        &mut self,
        sig: Option<&CSig<'ctx>>,
        sret: Option<PointerValue<'ctx>>,
        ret: &Type,
        result: Option<BasicValueEnum<'ctx>>,
    ) {
        let result = match (result, ret) {
            (Some(v), _) => Some(v),
            (None, Type::Unit | Type::Never) => None,
            (None, ty) => Some(llvm_type(self.context, ty).const_zero()),
        };
        let result = match result {
            Some(v) if is_nullable_handle(ret) => Some(self.nullable_to_c(v, ret)),
            other => other,
        };
        match sig {
            Some(sig) => self.c_sig_return(sig, sret, result),
            None => {
                match result {
                    Some(v) => self.builder.build_return(Some(&v)),
                    None => self.builder.build_return(None),
                }
                .expect(POSITIONED);
            }
        }
    }

    /// The sret pointer of `entry`, if `sig` returns through memory.
    pub(super) fn entry_sret(
        sig: Option<&CSig<'ctx>>,
        entry: FunctionValue<'ctx>,
    ) -> Option<PointerValue<'ctx>> {
        match sig {
            Some(sig) if sig.returns_in_memory() => {
                entry.get_nth_param(0).map(|p| p.into_pointer_value())
            }
            _ => None,
        }
    }

    /// `lugha_rt_cb_hold(pinned, slot)`: the closure in `pinned`, its
    /// captures held in `slot`, and whether its scope still runs.
    pub(super) fn hold_callback(
        &mut self,
        pinned: PointerValue<'ctx>,
        slot: PointerValue<'ctx>,
    ) -> (StructValue<'ctx>, IntValue<'ctx>) {
        let ptr = self.context.ptr_type(AddressSpace::default());
        let live = self
            .rt(
                "lugha_rt_cb_hold",
                &[Type::String, Type::String],
                Some(Type::I32),
                &[pinned.into(), slot.into()],
            )
            .expect("returns whether it runs")
            .into_int_value();
        let live = self
            .builder
            .build_int_compare(IntPredicate::NE, live, live.get_type().const_zero(), "live")
            .expect(POSITIONED);
        // The pin's first word is the closure's code, which never changes.
        let code = self
            .builder
            .build_load(ptr, pinned, "code")
            .expect(POSITIONED);
        let env = self.builder.build_load(ptr, slot, "env").expect(POSITIONED);
        let shape = self.context.struct_type(&[ptr.into(), ptr.into()], false);
        let pair = self
            .builder
            .build_insert_value(shape.get_undef(), code, 0, "code")
            .expect("a pair has 2 fields");
        let pair = self
            .builder
            .build_insert_value(pair, env, 1, "env")
            .expect("a pair has 2 fields")
            .into_struct_value();
        (pair, live)
    }

    /// A queued call's block: the closure, then each of C's arguments.
    fn queued_layout(&self, params: &[Type]) -> (StructType<'ctx>, Vec<Type>) {
        let mut types = vec![Type::Fun(params.to_vec(), Box::new(Type::Unit))];
        types.extend(params.iter().cloned());
        let fields: Vec<BasicTypeEnum> = types.iter().map(|t| llvm_type(self.context, t)).collect();
        (self.context.struct_type(&fields, false), types)
    }

    /// Starts the task for a queued callback's call: `pair` (its captures
    /// held) and `args` (C's arguments, converted) go into a block the
    /// task owns. `pins`: an argument is a pinned handle.
    pub(super) fn queue_call(
        &mut self,
        pinned: PointerValue<'ctx>,
        pair: StructValue<'ctx>,
        params: &[Type],
        args: Vec<BasicValueEnum<'ctx>>,
        pins: bool,
    ) {
        let (layout, types) = self.queued_layout(params);
        let drop = self.rc_env_drop(layout, &types, false);
        let size = layout.size_of().expect("a block is sized");
        let block = self.new_object(size, drop, "callback.call");
        let fields = std::iter::once(pair.as_basic_value_enum()).chain(args);
        for (i, value) in fields.enumerate() {
            let index = u32::try_from(i).expect("argument count fits in u32");
            let at = self
                .builder
                .build_struct_gep(layout, block, index, "field")
                .expect("a field per argument");
            self.builder.build_store(at, value).expect(POSITIONED);
        }
        // The task runs it, wherever: its counts are shared.
        self.rt("lugha_rt_rc_mark", &[Type::String], None, &[block.into()]);
        let entry = self.queued_task_entry(params);
        let pins = self.context.i32_type().const_int(u64::from(pins), false);
        self.rt(
            "lugha_rt_cb_queue",
            &[Type::String, Type::String, Type::String, Type::I32],
            None,
            &[
                pinned.into(),
                entry.as_global_value().as_pointer_value().into(),
                block.into(),
                pins.into(),
            ],
        );
    }

    /// `lugha_cbq_<signature>(code, env, result)`: a queued call's task,
    /// `env` its block. A panic stops here: the task's scope's policy
    /// decides (`lugha_rt_cb_task_panicked`). Emitted once per signature.
    fn queued_task_entry(&mut self, params: &[Type]) -> FunctionValue<'ctx> {
        let names: Vec<String> = params.iter().map(ToString::to_string).collect();
        let name = format!("lugha_cbq_fun({})", names.join(", "));
        if let Some(existing) = self.module.get_function(&name) {
            return existing;
        }
        let ptr = self.context.ptr_type(AddressSpace::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr.into(), ptr.into(), ptr.into()], false);
        let entry = self
            .module
            .add_function(&name, fn_type, Some(Linkage::Internal));
        let (saved_block, saved_function) = (self.builder.get_insert_block(), self.function);
        self.function = Some(entry);
        self.builder
            .position_at_end(self.context.append_basic_block(entry, "entry"));
        let block = entry
            .get_nth_param(1)
            .expect("declared with three parameters")
            .into_pointer_value();
        let (layout, types) = self.queued_layout(params);
        let mut fields = Vec::new();
        for (i, ty) in types.iter().enumerate() {
            let index = u32::try_from(i).expect("argument count fits in u32");
            let at = self
                .builder
                .build_struct_gep(layout, block, index, "field")
                .expect("a field per argument");
            fields.push(
                self.builder
                    .build_load(llvm_type(self.context, ty), at, "arg")
                    .expect(POSITIONED),
            );
        }
        let pair = fields[0].into_struct_value();
        let mut values: Vec<BasicMetadataValueEnum> = Vec::new();
        for (ty, value) in params.iter().zip(&fields[1..]) {
            if is_aggregate(ty) {
                // Function values take aggregates as a pointer to a copy.
                let slot = self.entry_alloca(ty, "arg");
                self.builder.build_store(slot, *value).expect(POSITIONED);
                values.push(slot.into());
            } else {
                values.push((*value).into());
            }
        }
        self.call_pair(pair, (params, &Type::Unit), values);
        let call = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_last_instruction())
            .expect("the call was just built");
        self.builder.build_return(None).expect(POSITIONED);
        self.catch_call(call, |this| {
            let text = this.entry_alloca(&Type::String, "panic.text");
            let how = this
                .rt(
                    "lugha_rt_task_caught",
                    &[Type::String],
                    Some(Type::I32),
                    &[text.into()],
                )
                .expect("returns how it stopped")
                .into_int_value();
            let panicked = this
                .builder
                .build_int_compare(
                    IntPredicate::EQ,
                    how,
                    this.context.i32_type().const_int(2, false),
                    "panicked",
                )
                .expect(POSITIONED);
            let (failed, done) = (this.append("cb.panicked"), this.append("cb.stopped"));
            this.builder
                .build_conditional_branch(panicked, failed, done)
                .expect(POSITIONED);
            this.builder.position_at_end(failed);
            let message = this
                .builder
                .build_load(ptr, text, "message")
                .expect(POSITIONED);
            this.rt(
                "lugha_rt_cb_task_panicked",
                &[Type::String],
                None,
                &[message],
            );
            this.branch(done);
            this.builder.position_at_end(done);
            this.builder.build_return(None).expect(POSITIONED);
        });
        self.function = saved_function;
        if let Some(b) = saved_block {
            self.builder.position_at_end(b);
        }
        entry
    }

    /// `lugha_scope_panic.<n>(text, task, code, env)`: for a scope of
    /// `failure` (`Failure<E>`) with `independent(f)`, calls the handler
    /// `{ code, env }` with a callback task's panic as a `Failure<E>`,
    /// taking `text` and `task` over.
    pub(super) fn scope_panic_handler(&mut self, failure: &Type) -> PointerValue<'ctx> {
        let Type::Struct(_, args) = failure else {
            unreachable!("checked: a scope fails with a `Failure`")
        };
        let (failure_type, kind_type) = failure_types(&args[0]);
        let count = self
            .closure_counts
            .entry("scope panic".to_string())
            .or_insert(0);
        let name = format!("lugha_scope_panic.{count}");
        *count += 1;
        let ptr = self.context.ptr_type(AddressSpace::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr.into(), ptr.into(), ptr.into(), ptr.into()], false);
        let handler = self
            .module
            .add_function(&name, fn_type, Some(Linkage::Internal));
        let (saved_block, saved_function) = (self.builder.get_insert_block(), self.function);
        self.function = Some(handler);
        self.builder
            .position_at_end(self.context.append_basic_block(handler, "entry"));
        let param = |i: u32| handler.get_nth_param(i).expect("four parameters");
        let kind = self.construct_values(&kind_type, "Panic", vec![param(0)]);
        let failure = self.failure_struct(&failure_type, param(1).into_pointer_value(), kind);
        let shape = self.context.struct_type(&[ptr.into(), ptr.into()], false);
        let pair = self
            .builder
            .build_insert_value(shape.get_undef(), param(2), 0, "code")
            .expect("a pair has 2 fields");
        let pair = self
            .builder
            .build_insert_value(pair, param(3), 1, "env")
            .expect("a pair has 2 fields")
            .into_struct_value();
        let at = self.entry_alloca(&failure_type, "failure");
        self.builder.build_store(at, failure).expect(POSITIONED);
        self.call_pair(
            pair,
            (std::slice::from_ref(&failure_type), &Type::Unit),
            vec![at.into()],
        );
        self.rc_release(failure, &failure_type);
        self.builder.build_return(None).expect(POSITIONED);
        self.function = saved_function;
        if let Some(b) = saved_block {
            self.builder.position_at_end(b);
        }
        handler.as_global_value().as_pointer_value()
    }
}
