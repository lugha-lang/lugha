//! Functions C calls by name, `extern fun name(…)
//! { … }`. Each gets a C entry under its C name, with C's calling rules (as
//! a callback's, `codegen::callbacks`), and a body the entry runs through
//! `lugha_rt_export_run`: as a task while C's thread waits, so the body may
//! wait. The body converts C's arguments, calls the Lugha function through
//! its function value (the one calling convention every internal path
//! uses), lets go of what it copied from C, and gives C its result, a
//! string as a `malloc`ed copy. A panic isn't caught here: it ends the
//! task, and with it the process. Lugha calls the function as any other.

use inkwell::attributes::AttributeLoc;
use inkwell::module::Linkage;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, ValueKind};

use super::CodegenError;
use super::externs::converts_from_c;
use super::lower::{Lowerer, POSITIONED};
use super::value::{Value, is_aggregate};
use crate::ast::Item;
use crate::check::{Type, Unit, qualify};

impl<'ctx> Lowerer<'ctx> {
    /// A C entry for every exported function in `units`.
    pub(super) fn c_exports(&mut self, units: &[Unit]) -> Result<(), CodegenError> {
        for unit in units {
            if super::declare::is_lazy(unit) {
                continue;
            }
            for item in &unit.program.items {
                let Item::Fun(f) = item else { continue };
                let Some(export) = &f.export else { continue };
                let key = qualify(unit.path, &f.name.name);
                let symbol = export.symbol.as_ref().unwrap_or(&f.name).name.clone();
                let sig = &self.functions[&key];
                let (target, params) = (sig.function, sig.params.clone());
                let ret = sig.ret.clone().unwrap_or(Type::Unit);
                self.export_entry(&symbol, target, &params, &ret);
            }
        }
        Ok(())
    }

    /// The C function `symbol`, calling Lugha's `target`.
    fn export_entry(
        &mut self,
        symbol: &str,
        target: FunctionValue<'ctx>,
        params: &[Type],
        ret: &Type,
    ) {
        let fn_type = self.c_function_type(params, ret, None);
        let entry = self
            .module
            .add_function(symbol, fn_type, Some(Linkage::External));
        let sig = self.entry_sig(params, ret, None);
        match &sig {
            Some(sig) => self.c_sig_attributes(sig, |at, a| entry.add_attribute(at, a)),
            None => self.c_abi_entry(entry, params, ret, None),
        }
        let (saved_block, saved_function) = (self.builder.get_insert_block(), self.function);
        // The entry hands its arguments to the body through the runtime's
        // door, rewritten by the C-stack pass into a thunk and a frame.
        let body = self.module.add_function(
            &format!("lugha_export.{symbol}"),
            entry.get_type(),
            Some(Linkage::Internal),
        );
        // Both keep C's rules through the passes after lowering.
        let c_entry = self
            .context
            .create_string_attribute(super::c_stack::C_ENTRY, "");
        entry.add_attribute(AttributeLoc::Function, c_entry);
        body.add_attribute(AttributeLoc::Function, c_entry);
        self.builder
            .position_at_end(self.context.append_basic_block(entry, "entry"));
        let args: Vec<BasicMetadataValueEnum> = entry.get_param_iter().map(Into::into).collect();
        let call = self
            .builder
            .build_call(body, &args, "body")
            .expect(POSITIONED);
        let mark = self
            .context
            .create_string_attribute(super::c_stack::CB_RUN, super::c_stack::EXPORT);
        call.add_attribute(AttributeLoc::Function, mark);
        match call.try_as_basic_value() {
            ValueKind::Basic(v) => self.builder.build_return(Some(&v)),
            ValueKind::Instruction(_) => self.builder.build_return(None),
        }
        .expect(POSITIONED);
        self.function = Some(body);
        self.builder
            .position_at_end(self.context.append_basic_block(body, "entry"));
        // A shared library's constants, made once, by its first call.
        if let Some(init) = self.const_init {
            let ptr: BasicValueEnum = init.as_global_value().as_pointer_value().into();
            self.rt("lugha_rt_export_init", &[Type::String], None, &[ptr]);
        }
        let (incoming, sret): (Vec<BasicValueEnum>, _) = match &sig {
            Some(sig) => self.c_sig_params(sig, body),
            None => (body.get_param_iter().collect(), None),
        };
        let mut values: Vec<BasicMetadataValueEnum> = Vec::with_capacity(params.len());
        let mut copied = Vec::new();
        for (ty, value) in params.iter().zip(incoming) {
            let value = if converts_from_c(ty) {
                let lugha = self.value_from_c(value, ty);
                copied.push((lugha, ty.clone()));
                lugha
            } else {
                value
            };
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
        let wrapper = self.wrapper(target);
        let ptr = self.context.ptr_type(Default::default());
        let pair = self.context.struct_type(&[ptr.into(), ptr.into()], false);
        let code = wrapper.as_global_value().as_pointer_value();
        let pair = pair.const_named_struct(&[code.into(), ptr.const_null().into()]);
        let result = self.call_pair(pair, (params, ret), values);
        // What was copied from C is this call's, as any fresh argument.
        for (value, ty) in copied {
            self.rc_release(value, &ty);
        }
        let result = match result {
            Value::Val(v) if *ret == Type::String => {
                let copy = self.rt(
                    "lugha_rt_export_string",
                    &[Type::String],
                    Some(Type::String),
                    &[v],
                );
                self.rc_release(v, ret);
                copy
            }
            Value::Val(v) => Some(v),
            Value::Void | Value::Never => None,
        };
        self.c_entry_return(sig.as_ref(), sret, ret, result);
        self.function = saved_function;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
    }
}
