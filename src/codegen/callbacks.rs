//! Callbacks into Lugha: C gets plain function
//! pointers with C signatures, and these call the Lugha function value.
//!
//! - A callback without `data` gets `lugha_cfn_<n>`, one per call site. The
//!   checker allows only a function's name or a closure capturing nothing
//!   there (E0436), so the value at a site never changes: it is stored in
//!   the global `lugha_cbv_<n>` before the call, and the entry calls it.
//! - A callback with `data` gets the trampoline `lugha_cb_<signature>`, one
//!   per C signature, and the `data` argument is `lugha_rt_cb_pin(code, env)`:
//!   the trampoline reads the closure back from it, while its scope runs.
//!   A `spawned` one's, `lugha_cbq_<signature>`,
//!   starts a task instead (spawned_callbacks.rs).
//! - A `release` argument is `lugha_rt_cb_release`.

use inkwell::attributes::AttributeLoc;
use inkwell::module::Linkage;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, FunctionType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue, StructValue, ValueKind,
};

use super::CodegenError;
use super::c_calls::{CKind, CSig, has_struct};
use super::declare::abi_attribute;
use super::externs::{converts_from_c, is_nullable_handle};
use super::lower::{Lowerer, POSITIONED};
use super::value::{Value, is_aggregate};
use crate::ast::Expr;
use crate::check::{CParam, Type};

/// The C type of a callback's parameter list with `data` at `data`, as text:
/// `fun(i64, data): i32`, naming its trampoline.
fn c_signature(params: &[Type], ret: &Type, data: Option<usize>) -> String {
    let mut parts: Vec<String> = params.iter().map(ToString::to_string).collect();
    if let Some(at) = data {
        parts.insert(at, "data".into());
    }
    match ret {
        Type::Unit => format!("fun({})", parts.join(", ")),
        ret => format!("fun({}): {ret}", parts.join(", ")),
    }
}

impl<'ctx> Lowerer<'ctx> {
    /// The LLVM type C uses for a value of type `ty` in an extern or callback.
    fn c_value_type(&self, ty: &Type) -> BasicMetadataTypeEnum<'ctx> {
        self.c_result_type(ty).into()
    }

    /// The parameter types C sees for an extern with plan `plan` and
    /// Lugha parameters `params` (the written ones, callbacks as `Fun`).
    pub(super) fn c_param_types(
        &self,
        plan: &[CParam],
        params: &[Type],
    ) -> Vec<(BasicMetadataTypeEnum<'ctx>, Option<Type>)> {
        let ptr = self.context.ptr_type(Default::default());
        let mut written = params.iter();
        plan.iter()
            .map(|c| match c {
                CParam::Value => {
                    let ty = written
                        .next()
                        .expect("checked: a value per written parameter");
                    (self.param_type(ty), Some(ty.clone()))
                }
                CParam::Callback { .. } | CParam::Pointer(_) | CParam::Elements { .. } => {
                    written.next();
                    (ptr.into(), None)
                }
                CParam::Data | CParam::Release => (ptr.into(), None),
            })
            .collect()
    }

    /// The arguments of a call to an extern with plan `plan`: the written
    /// ones left to right, then what lughac adds for `data` and
    /// `release`.
    pub(super) fn callback_arguments(
        &mut self,
        args: &[Expr],
        params: &[Type],
        plan: &[CParam],
    ) -> Result<Vec<BasicValueEnum<'ctx>>, CodegenError> {
        let mut written = Vec::with_capacity(args.len());
        // The written parameters' kinds: `out` ones pass a slot.
        let mut kinds = plan
            .iter()
            .filter(|c| !matches!(c, CParam::Data | CParam::Release));
        for (arg, ty) in args.iter().zip(params) {
            match kinds.next() {
                Some(CParam::Pointer(mode)) => {
                    written.push(self.pointer_argument(arg, ty, *mode)?);
                    continue;
                }
                Some(CParam::Elements { mutable }) => {
                    written.push(self.elements_argument(arg, ty, *mutable)?);
                    continue;
                }
                _ => {}
            }
            let value = self.get(arg, ty)?;
            written.push(match ty {
                Type::String => self.c_string(value),
                ty if is_nullable_handle(ty) => self.nullable_to_c(value, ty),
                _ => value,
            });
        }
        let mut written = written.into_iter();
        let (mut pinned, mut values) = (None, Vec::new());
        // `data` may come before its callback, so it is filled in afterwards.
        let mut data_at = None;
        for c in plan {
            let value: BasicValueEnum = match c {
                CParam::Value | CParam::Pointer(_) | CParam::Elements { .. } => {
                    written.next().expect("checked arity")
                }
                CParam::Callback {
                    params,
                    ret,
                    data,
                    spawned,
                    realtime,
                } => {
                    let pair = written.next().expect("checked arity").into_struct_value();
                    // A thread record set aside, so C's thread's first call
                    // of a real-time callback allocates nothing.
                    if *realtime {
                        let reserve = self.runtime("lugha_rt_cb_reserve", &[], None);
                        self.builder.build_call(reserve, &[], "").expect(POSITIONED);
                    }
                    let run = (*spawned, *realtime);
                    match data {
                        None => self.static_entry(pair, params, ret, *realtime).into(),
                        Some(at) => {
                            pinned = Some(self.pin(pair));
                            self.trampoline(params, ret, *at, run).into()
                        }
                    }
                }
                CParam::Data => {
                    data_at = Some(values.len());
                    self.context
                        .ptr_type(Default::default())
                        .const_null()
                        .into()
                }
                CParam::Release => {
                    let release = self.runtime("lugha_rt_cb_release", &[Type::String], None);
                    release.as_global_value().as_pointer_value().into()
                }
            };
            values.push(value);
        }
        if let (Some(at), Some(pinned)) = (data_at, pinned) {
            values[at] = pinned.into();
        }
        Ok(values)
    }

    /// `lugha_rt_cb_pin(code, env)`: the closure, kept alive for C.
    fn pin(&mut self, pair: StructValue<'ctx>) -> PointerValue<'ctx> {
        let code = self
            .builder
            .build_extract_value(pair, 0, "code")
            .expect("a pair has 2 fields");
        let env = self
            .builder
            .build_extract_value(pair, 1, "env")
            .expect("a pair has 2 fields");
        let pin = self.runtime(
            "lugha_rt_cb_pin",
            &[Type::String, Type::String],
            Some(Type::String),
        );
        let call = self
            .builder
            .build_call(pin, &[code.into(), env.into()], "pinned")
            .expect(POSITIONED);
        match call.try_as_basic_value() {
            ValueKind::Basic(pinned) => pinned.into_pointer_value(),
            ValueKind::Instruction(_) => unreachable!("lugha_rt_cb_pin returns a pointer"),
        }
    }

    /// The C function type of a callback, `data` included at `data`.
    fn c_function_type(
        &self,
        params: &[Type],
        ret: &Type,
        data: Option<usize>,
    ) -> FunctionType<'ctx> {
        // Counted arrays are two C parameters.
        let (params, data) = &super::c_arrays::c_params(params, data);
        let (params, data) = (params.as_slice(), *data);
        if let Some(sig) = self.entry_sig(params, ret, data) {
            return sig.fn_type;
        }
        let mut types: Vec<BasicMetadataTypeEnum> =
            params.iter().map(|p| self.c_value_type(p)).collect();
        if let Some(at) = data {
            types.insert(at, self.context.ptr_type(Default::default()).into());
        }
        match ret {
            Type::Unit => self.context.void_type().fn_type(&types, false),
            ret => self.c_result_type(ret).fn_type(&types, false),
        }
    }

    /// `lugha_cfn_<n>` for a callback without `data` at this call site:
    /// `pair` is stored in `lugha_cbv_<n>`, which the entry calls.
    fn static_entry(
        &mut self,
        pair: StructValue<'ctx>,
        params: &[Type],
        ret: &Type,
        realtime: bool,
    ) -> PointerValue<'ctx> {
        let n = self.c_entries;
        self.c_entries += 1;
        let pair_type = pair.get_type();
        let global = self
            .module
            .add_global(pair_type, None, &format!("lugha_cbv_{n}"));
        global.set_linkage(Linkage::Internal);
        global.set_initializer(&pair_type.const_zero());
        let slot = global.as_pointer_value();
        self.builder.build_store(slot, pair).expect(POSITIONED);
        let fn_type = self.c_function_type(params, ret, None);
        let entry =
            self.module
                .add_function(&format!("lugha_cfn_{n}"), fn_type, Some(Linkage::Internal));
        self.c_body(entry, params, ret, (None, false, realtime), |this, _| {
            this.builder
                .build_load(pair_type, slot, "pair")
                .expect(POSITIONED)
                .into_struct_value()
        });
        entry.as_global_value().as_pointer_value()
    }

    /// `lugha_cb_<signature>`: calls the closure pinned in its `data`
    /// parameter, at index `data`; `lugha_cbq_…` when `spawned`, starting a
    /// task that calls it. Emitted once per signature.
    fn trampoline(
        &mut self,
        params: &[Type],
        ret: &Type,
        data: usize,
        (spawned, realtime): (bool, bool),
    ) -> PointerValue<'ctx> {
        // A real-time one enters through its own runtime door
        // (`lugha_rt_cb_run_realtime`), so it has its own trampoline.
        let kind = match (spawned, realtime) {
            (true, _) => "cbq",
            (false, true) => "cbr",
            (false, false) => "cb",
        };
        let name = format!("lugha_{kind}_{}", c_signature(params, ret, Some(data)));
        if let Some(existing) = self.module.get_function(&name) {
            return existing.as_global_value().as_pointer_value();
        }
        let fn_type = self.c_function_type(params, ret, Some(data));
        let entry = self
            .module
            .add_function(&name, fn_type, Some(Linkage::Internal));
        // The closure comes from the pin, its captures held for the call.
        self.c_body(
            entry,
            params,
            ret,
            (Some(data), spawned, realtime),
            |_, _| unreachable!("a trampoline holds its closure through its pin"),
        );
        entry.as_global_value().as_pointer_value()
    }

    /// The body of a C entry `entry`: get the function value (with `pair`,
    /// or from the pin in the `data` parameter at `data`), convert C's
    /// arguments (all but `data`), call it, convert its result; when
    /// `spawned`, start a task to call it instead. A panic stops here.
    fn c_body(
        &mut self,
        entry: FunctionValue<'ctx>,
        params: &[Type],
        ret: &Type,
        (data, spawned, realtime): (Option<usize>, bool, bool),
        pair: impl FnOnce(&mut Self, Option<PointerValue<'ctx>>) -> StructValue<'ctx>,
    ) {
        // Counted arrays are two C parameters each.
        let (c_params, c_data) = super::c_arrays::c_params(params, data);
        // Structs by value arrive as C passes them.
        let sig = self.entry_sig(&c_params, ret, c_data);
        match &sig {
            Some(sig) => self.c_sig_attributes(sig, |at, a| entry.add_attribute(at, a)),
            None => self.c_abi_entry(entry, &c_params, ret, c_data),
        }
        let (saved_block, saved_function) = (self.builder.get_insert_block(), self.function);
        // C calls `entry`, a shell running the body through
        // `lugha_rt_cb_run`: on a thread C made, the body runs on that
        // thread's C stack, aligned as Lugha code needs.
        let name = entry.get_name().to_string_lossy().into_owned();
        let body = self.module.add_function(
            &format!("{name}.body"),
            entry.get_type(),
            Some(Linkage::Internal),
        );
        self.builder
            .position_at_end(self.context.append_basic_block(entry, "entry"));
        let args: Vec<BasicMetadataValueEnum> = entry.get_param_iter().map(Into::into).collect();
        let call = self
            .builder
            .build_call(body, &args, "body")
            .expect(POSITIONED);
        // A real-time callback's body enters through its own door.
        let door = if realtime {
            super::c_stack::REALTIME
        } else {
            ""
        };
        let mark = self
            .context
            .create_string_attribute(super::c_stack::CB_RUN, door);
        call.add_attribute(AttributeLoc::Function, mark);
        match call.try_as_basic_value() {
            ValueKind::Basic(v) => self.builder.build_return(Some(&v)),
            ValueKind::Instruction(_) => self.builder.build_return(None),
        }
        .expect(POSITIONED);
        let entry = body;
        self.function = Some(entry);
        self.builder
            .position_at_end(self.context.append_basic_block(entry, "entry"));
        // The closure's captures, held while it runs: let go of on a panic.
        let held = self.entry_alloca(&Type::String, "held.env");
        let null = self.context.ptr_type(Default::default()).const_null();
        self.builder.build_store(held, null).expect(POSITIONED);
        // Never on a helper thread; inside a direct callback, no
        // cancellation point unwinds.
        let enter = self.runtime("lugha_rt_cb_enter", &[], None);
        self.builder.build_call(enter, &[], "").expect(POSITIONED);
        let (all, sret): (Vec<BasicValueEnum>, _) = match &sig {
            Some(sig) => self.c_sig_params(sig, entry),
            None => (entry.get_param_iter().collect(), None),
        };
        let pinned = c_data.map(|at| all[at].into_pointer_value());
        let pair = match pinned {
            Some(pinned) => {
                // Once its scope's body has ended, nothing runs.
                let (pair, live) = self.hold_callback(pinned, held);
                let (dead, go) = (self.append("cb.stopped"), self.append("cb.live"));
                self.builder
                    .build_conditional_branch(live, go, dead)
                    .expect(POSITIONED);
                self.builder.position_at_end(dead);
                self.rt("lugha_rt_cb_leave", &[], None, &[]);
                self.c_entry_return(sig.as_ref(), sret, ret, None);
                self.builder.position_at_end(go);
                pair
            }
            None => pair(self, None),
        };
        let mut incoming = all
            .into_iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != c_data)
            .map(|(_, p)| p);
        let mut values: Vec<BasicMetadataValueEnum> = Vec::with_capacity(params.len());
        let mut plain = Vec::with_capacity(params.len());
        for ty in params {
            let value = incoming.next().expect("C passes every parameter");
            let value = if super::c_arrays::counted(ty) {
                let count = incoming.next().expect("a counted parameter has its count");
                self.counted_from_c(ty, (value, count))
            } else if converts_from_c(ty) {
                self.value_from_c(value, ty)
            } else {
                value
            };
            plain.push(value);
            let value = if is_aggregate(ty) {
                // Function values take aggregates as a pointer to the caller's copy.
                let slot = self.entry_alloca(ty, "arg");
                self.builder.build_store(slot, value).expect(POSITIONED);
                slot.into()
            } else {
                value
            };
            values.push(value.into());
        }
        let leave = |this: &mut Self| {
            this.rt("lugha_rt_cb_leave", &[], None, &[]);
        };
        if let (true, Some(pinned)) = (spawned, pinned) {
            // The block takes the held captures over.
            let pins = params.iter().any(|t| self.is_pinned(t));
            self.queue_call(pinned, pair, params, plain, pins);
            self.builder.build_store(held, null).expect(POSITIONED);
            leave(self);
            self.c_entry_return(sig.as_ref(), sret, ret, None);
        } else {
            let result = self.call_pair(pair, (params, ret), values);
            if pinned.is_some() {
                let env = self
                    .builder
                    .build_load(null.get_type(), held, "held")
                    .expect(POSITIONED);
                self.rt("lugha_rt_rc_release", &[Type::String], None, &[env]);
            }
            leave(self);
            let result = match result {
                Value::Val(v) => Some(v),
                Value::Void | Value::Never => None,
            };
            self.c_entry_return(sig.as_ref(), sret, ret, result);
        }
        let ret = ret.clone();
        self.unwind_stops_here(|this| {
            let env = this
                .builder
                .build_load(null.get_type(), held, "held")
                .expect(POSITIONED);
            this.rt("lugha_rt_rc_release", &[Type::String], None, &[env]);
            let pinned: BasicValueEnum = pinned.unwrap_or(null).into();
            this.rt("lugha_rt_cb_caught", &[Type::String], None, &[pinned]);
            let sig = this.entry_sig(&c_params, &ret, c_data);
            let sret = Self::entry_sret(sig.as_ref(), entry);
            this.c_entry_return(sig.as_ref(), sret, &ret, None);
        });
        self.function = saved_function;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
    }

    /// The lowered signature of a C entry taking `params` (with the `data`
    /// pointer at `data`) and returning `ret`, when it passes structs.
    fn entry_sig(&self, params: &[Type], ret: &Type, data: Option<usize>) -> Option<CSig<'ctx>> {
        let mut kinds: Vec<CKind> = params.iter().cloned().map(CKind::Value).collect();
        if let Some(at) = data {
            kinds.insert(at, CKind::Pointer);
        }
        has_struct(&kinds, ret).then(|| self.c_sig(&kinds, ret))
    }

    /// C's extensions on an entry's `bool`, `u8`, `u16`, `i8` and `i16`.
    fn c_abi_entry(
        &self,
        entry: FunctionValue<'ctx>,
        params: &[Type],
        ret: &Type,
        data: Option<usize>,
    ) {
        // C's parameter list is `params` with the `data` pointer inserted.
        let mut c_params: Vec<Option<&Type>> = params.iter().map(Some).collect();
        if let Some(at) = data {
            c_params.insert(at, None);
        }
        for (index, ty) in c_params.into_iter().enumerate() {
            if let Some(attribute) = ty.and_then(|ty| abi_attribute(self.context, ty)) {
                let index = u32::try_from(index).expect("parameter count fits in u32");
                entry.add_attribute(AttributeLoc::Param(index), attribute);
            }
        }
        if let Some(attribute) = abi_attribute(self.context, ret) {
            entry.add_attribute(AttributeLoc::Return, attribute);
        }
    }
}
