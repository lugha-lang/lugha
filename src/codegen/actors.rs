//! Actors in codegen. An actor's state is its struct and
//! its methods ordinary methods (`lugha_fn_Counter.add`). Each method a
//! handle calls gets a handler `lugha_actor_Counter.add(state, args,
//! result)`, which the actor's green thread runs: it reads the arguments
//! from a counted block (element layout, one field each), calls the method on the
//! state and stores the result. A call on a handle copies its arguments into
//! such a block and goes through `lugha_rt_actor_call` (waits) or
//! `lugha_rt_actor_cast` (doesn't).

use inkwell::IntPredicate;
use inkwell::module::Linkage;
use inkwell::types::{BasicTypeEnum, StructType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue, ValueKind,
};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::value::{Value, is_aggregate};
use crate::ast::Expr;
use crate::check::Type;

/// How many messages may wait for an actor declared without `mailbox n`.
const DEFAULT_MAILBOX: u64 = 1024;

impl<'ctx> Lowerer<'ctx> {
    /// `s.spawn(Counter { … })` or `s.spawn(Counter.new(…))`: the state,
    /// made by the spawning thread, moved to a counted block and handed to
    /// a new actor, whose loop runs as a task of the scope `scope` (its
    /// record), so the scope's policy meets its failure as any task's.
    pub(super) fn spawn_actor(
        &mut self,
        key: &str,
        operand: &Expr,
        scope: BasicValueEnum<'ctx>,
        scope_error: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let state_type = Type::Struct(key.to_string(), Vec::new());
        let state = self.get(operand, &state_type)?;
        let size = self.element_size(&state_type).max(1);
        // The block holds the state, and the
        // actor the block.
        let drop = {
            self.rc_retain(state, &state_type);
            self.rc_box_drop(&state_type)
        };
        let size = self.context.i64_type().const_int(size, false);
        let block = self.new_object(size, drop, "state");
        self.store_element(block, &state_type, state);
        let name = self.actor_name(key);
        let mailbox = self.mailbox(key);
        let values = [block.into(), name.into(), mailbox.into()];
        let actor = self.call_rt("lugha_rt_actor_new", &values, true);
        // The task's closure is the runtime's loop, its environment the
        // actor: one holder more than the handle.
        let ptr = self.context.ptr_type(Default::default());
        let run = self
            .module
            .get_function("lugha_rt_actor_run")
            .unwrap_or_else(|| {
                let fn_type = self.context.void_type().fn_type(&[ptr.into()], false);
                self.module
                    .add_function("lugha_rt_actor_run", fn_type, None)
            });
        let handle = Type::Actor(key.to_string());
        self.rc_retain(actor, &handle);
        let pair = self.context.struct_type(&[ptr.into(), ptr.into()], false);
        let pair = self
            .builder
            .build_insert_value(
                pair.get_undef(),
                run.as_global_value().as_pointer_value(),
                0,
                "run",
            )
            .expect("a pair has 2 fields");
        let pair = self
            .builder
            .build_insert_value(pair, actor, 1, "run")
            .expect("a pair has 2 fields")
            .into_struct_value();
        let simple = key.rsplit('.').next().unwrap_or(key).to_string();
        let task = self.start_task(scope, scope_error, pair, &Type::Unit, &simple)?;
        self.call_rt("lugha_rt_actor_started", &[task, actor], false);
        // The scope holds the task; the actor's handle is what `spawn` gives.
        self.rc_release(task, &Type::Task(Box::new(Type::Unit)));
        Ok(actor)
    }

    /// The constant `lugha_actor_<key>.$mailbox`: how many messages may
    /// wait for the actor (`mailbox n`, or the default).
    fn mailbox(&self, key: &str) -> inkwell::values::IntValue<'ctx> {
        let name = format!("lugha_actor_{}.$mailbox", crate::check::symbol(key));
        let i64_type = self.context.i64_type();
        self.module
            .get_global(&name)
            .and_then(|g| g.get_initializer())
            .map(|v| v.into_int_value())
            .unwrap_or_else(|| i64_type.const_int(DEFAULT_MAILBOX, false))
    }

    /// Records actor `key`'s mailbox size, `mailbox n` or the default, for
    /// the places that start it.
    pub(super) fn declare_mailbox(&mut self, key: &str, mailbox: Option<&Expr>) {
        let size = mailbox
            .and_then(crate::check::mailbox_size)
            .unwrap_or(DEFAULT_MAILBOX);
        let name = format!("lugha_actor_{}.$mailbox", crate::check::symbol(key));
        let i64_type = self.context.i64_type();
        let global = self.module.add_global(i64_type, None, &name);
        global.set_linkage(Linkage::Internal);
        global.set_constant(true);
        global.set_initializer(&i64_type.const_int(size, false));
    }

    /// A method call on a handle: a call or a cast, or one of the built-in
    /// `stop` and `is`.
    pub(super) fn actor_method(
        &mut self,
        call: &Expr,
        base: &Expr,
        name: &str,
        args: &[Expr],
    ) -> Result<Value<'ctx>, CodegenError> {
        let ty = self.ty(base);
        let handle = self.get(base, &ty)?;
        let at = self.location_of(call.span.start);
        match name {
            "stop" => {
                let values = [handle, at[0], at[1], at[2]];
                self.call_rt("lugha_rt_actor_stop", &values, false);
                return Ok(Value::Void);
            }
            "is" => {
                let other = self.get(&args[0], &ty)?.into_pointer_value();
                let same = self
                    .builder
                    .build_int_compare(IntPredicate::EQ, handle.into_pointer_value(), other, "same")
                    .expect(POSITIONED);
                return Ok(Value::Val(same.into()));
            }
            _ => {}
        }
        let key = self
            .callees
            .get(&call.id)
            .cloned()
            .expect("the checker named the method a message calls");
        let method = self.methods[&key].clone();
        let params = method.signature.params.clone();
        let ret = method.signature.ret.clone();
        let layout = self.args_layout(&params);
        let block = match layout {
            Some(layout) => {
                let size = layout.size_of().expect("argument blocks are sized");
                // The block holds the arguments.
                let drop = self.rc_env_drop(layout, &params, true);
                let block = self.new_object(size, drop, "args");
                for (i, (arg, ty)) in (0u32..).zip(args.iter().zip(&params)) {
                    let value = self.value_for_store(arg, ty)?;
                    let at = self
                        .builder
                        .build_struct_gep(layout, block, i, "arg")
                        .expect("a field per argument");
                    self.store_element(at, ty, value);
                }
                // It crosses to the actor: marked shared.
                self.rc_pointer_op(block, super::rc::RcOp::Mark);

                block
            }
            None => self.context.ptr_type(Default::default()).const_null(),
        };
        let handler = self.handler(&key, &method.signature.function, &params, ret.as_ref());
        let handler = handler.as_global_value().as_pointer_value();
        match ret {
            Some(ret) => {
                let size = self
                    .context
                    .i64_type()
                    .const_int(self.element_size(&ret).max(1), false);
                let values = [
                    handle,
                    handler.into(),
                    block.into(),
                    size.into(),
                    at[0],
                    at[1],
                    at[2],
                ];
                let result = self.call_rt("lugha_rt_actor_call", &values, true);
                let value = self.load_element(result.into_pointer_value(), &ret);
                // The result is the caller's now,
                // crossed back from the actor; its block is freed.
                self.rc_mark(value, &ret);
                self.call_rt("lugha_rt_rc_block_done", &[result], false);

                Ok(Value::Val(value))
            }
            None => {
                let values = [handle, handler.into(), block.into(), at[0], at[1], at[2]];
                self.call_rt("lugha_rt_actor_cast", &values, false);
                Ok(Value::Void)
            }
        }
    }

    /// `lugha_actor_<method>(state, args, result)`, made once per method.
    fn handler(
        &mut self,
        key: &str,
        method: &FunctionValue<'ctx>,
        params: &[Type],
        ret: Option<&Type>,
    ) -> FunctionValue<'ctx> {
        // Named like the method's own symbol: the owner's path, then `.name`.
        let (owner, name) = key.rsplit_once('.').expect("a method key is `Type.name`");
        let symbol = format!("lugha_actor_{}.{name}", crate::check::symbol(owner));
        if let Some(existing) = self.module.get_function(&symbol) {
            return existing;
        }
        let ptr = self.context.ptr_type(Default::default());
        let fn_type = self
            .context
            .void_type()
            .fn_type(&[ptr.into(), ptr.into(), ptr.into()], false);
        let handler = self
            .module
            .add_function(&symbol, fn_type, Some(Linkage::Internal));
        let (saved_block, saved_function) = (self.builder.get_insert_block(), self.function);
        self.function = Some(handler);
        self.builder
            .position_at_end(self.context.append_basic_block(handler, "entry"));
        let param = |i: u32| {
            handler
                .get_nth_param(i)
                .expect("declared with three parameters")
                .into_pointer_value()
        };
        let mut values: Vec<BasicMetadataValueEnum<'ctx>> = vec![param(0).into()];
        if let Some(layout) = self.args_layout(params) {
            for (i, ty) in (0u32..).zip(params) {
                let at = self
                    .builder
                    .build_struct_gep(layout, param(1), i, "arg")
                    .expect("a field per argument");
                let value = self.load_element(at, ty);
                values.push(if is_aggregate(ty) {
                    let slot = self.entry_alloca(ty, "arg");
                    self.builder.build_store(slot, value).expect(POSITIONED);
                    slot.into()
                } else {
                    value.into()
                });
            }
        }
        let site = self
            .builder
            .build_call(*method, &values, "result")
            .expect(POSITIONED);
        if let (Some(ret), ValueKind::Basic(result)) = (ret, site.try_as_basic_value()) {
            self.store_element(param(2), ret, result);
        }
        self.builder.build_return(None).expect(POSITIONED);
        self.function = saved_function;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        handler
    }

    /// The argument block's layout: each parameter in element layout.
    fn args_layout(&self, params: &[Type]) -> Option<StructType<'ctx>> {
        if params.is_empty() {
            return None;
        }
        let fields: Vec<BasicTypeEnum> = params.iter().map(|p| self.storage_type(p)).collect();
        Some(self.context.struct_type(&fields, false))
    }

    /// The actor's name as a C string, for its panics and printing.
    fn actor_name(&mut self, key: &str) -> PointerValue<'ctx> {
        let name = key.rsplit('.').next().unwrap_or(key);
        self.builder
            .build_global_string_ptr(name, "actor.name")
            .expect(POSITIONED)
            .as_pointer_value()
    }

    /// `file, line, col` of source offset `at`, for the runtime's panics.
    fn location_of(&mut self, at: usize) -> [BasicValueEnum<'ctx>; 3] {
        let file = self.file_name(at);
        let (line, col) = self.source.line_col(at);
        let i64_type = self.context.i64_type();
        [
            file.into(),
            i64_type.const_int(line as u64, false).into(),
            i64_type.const_int(col as u64, false).into(),
        ]
    }

    /// Calls runtime function `name` with `values`, declared from them; its
    /// pointer result when `returns`.
    fn call_rt(
        &mut self,
        name: &str,
        values: &[BasicValueEnum<'ctx>],
        returns: bool,
    ) -> BasicValueEnum<'ctx> {
        let ptr = self.context.ptr_type(Default::default());
        let function = self.module.get_function(name).unwrap_or_else(|| {
            let params: Vec<_> = values.iter().map(|v| v.get_type().into()).collect();
            let fn_type = if returns {
                ptr.fn_type(&params, false)
            } else {
                self.context.void_type().fn_type(&params, false)
            };
            self.module.add_function(name, fn_type, None)
        });
        let args: Vec<_> = values.iter().map(|v| (*v).into()).collect();
        let site = self
            .builder
            .build_call(function, &args, "rt")
            .expect(POSITIONED);
        match site.try_as_basic_value() {
            ValueKind::Basic(value) => value,
            // A void call's value is never used.
            ValueKind::Instruction(_) => ptr.const_null().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::ir;

    const COUNTER: &str = "actor Counter {
        count: i64,
        fun add(mut self, n: i64) { self.count += n; }
        fun get(self): i64 = self.count;
    }\n";

    #[test]
    fn calls_wait_and_casts_do_not() {
        let ir = ir(&format!(
            "{COUNTER}fun main(root: Scope) {{ let c = root.spawn(Counter {{ count: 0 }}); c.add(1); let n = c.get(); }}"
        ));
        assert!(ir.contains("call ptr @lugha_rt_actor_new("), "{ir}");
        assert!(ir.contains("@lugha_rt_actor_run"), "{ir}");
        assert!(ir.contains("call void @lugha_rt_actor_cast(ptr %"), "{ir}");
        assert!(ir.contains("@lugha_actor_Counter.add"), "{ir}");
        assert!(ir.contains("call ptr @lugha_rt_actor_call(ptr %"), "{ir}");
        assert!(
            ir.contains("define internal void @lugha_actor_Counter.get(ptr"),
            "{ir}"
        );
    }

    #[test]
    fn a_handler_calls_the_method_on_the_state() {
        let ir = ir(&format!(
            "{COUNTER}fun main(root: Scope) {{ let c = root.spawn(Counter {{ count: 0 }}); c.add(1); }}"
        ));
        let handler = &ir[ir
            .find("define internal void @lugha_actor_Counter.add")
            .unwrap()..];
        let handler = &handler[..handler.find("\n}\n").unwrap()];
        assert!(
            handler.contains("call void @lugha_fn_Counter.add(ptr %0, i64"),
            "{handler}"
        );
    }
}
