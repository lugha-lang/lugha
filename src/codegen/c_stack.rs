//! C on its own stack: once every function is lowered, each call of a C
//! function (declared, external, not one of the runtime's `lugha_` names
//! nor an LLVM intrinsic) is rewritten to run on the C stack. Its arguments
//! go into a frame in the caller's entry block; `lugha_rt_c_call(thunk,
//! frame)` switches to the C stack and runs the call site's thunk,
//! `lugha_c_<n>(frame)`, which makes the original call, with the original
//! call's attributes, and stores its result in the frame; the caller reads
//! it back. A task's stack never holds C's frames, so C code can't run
//! past a task stack's end, which has no guard page. A C function marked
//! [`BLOCKING`] goes through `lugha_rt_c_blocking` instead, which runs the
//! thunk on a helper thread while the task waits. A C entry's call of its
//! body, marked [`CB_RUN`], goes through `lugha_rt_cb_run`, which runs it
//! in place or, on a thread C made, on that thread's C stack. One marked [`PINNED`]
//! first calls `lugha_rt_pin_here()`, keeping its task on this worker.

use inkwell::attributes::AttributeLoc;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::types::{AnyTypeEnum, BasicTypeEnum};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, CallSiteValue, FunctionValue,
    InstructionOpcode, InstructionValue,
};

/// What the thunks are named after, a number following.
const THUNK: &str = "lugha_c_";

/// The attribute on an `extern blocking fun`'s declaration.
pub(super) const BLOCKING: &str = "lugha-blocking";

/// The attribute on a C entry's call of its body: rewritten to run through
/// `lugha_rt_cb_run`, which moves a thread C made onto a stack of its own.
pub(super) const CB_RUN: &str = "lugha-cb-run";

/// [`CB_RUN`]'s value on a `realtime` callback's body: it goes through
/// `lugha_rt_cb_run_realtime`, which never allocates or locks.
pub(super) const REALTIME: &str = "realtime";

/// The attribute on an extern taking or giving a pinned handle: each call
/// first pins the task to its worker (`lugha_rt_pin_here`).
pub(super) const PINNED: &str = "lugha-pinned";

/// Rewrites every call of a C function in `module` to run on the C stack.
pub(super) fn calls_on_c_stack<'ctx>(context: &'ctx Context, module: &Module<'ctx>) {
    let mut sites = Vec::new();
    for function in module.get_functions() {
        if function.count_basic_blocks() == 0 || name(function).starts_with(THUNK) {
            continue;
        }
        for block in function.get_basic_block_iter() {
            for i in block.get_instructions() {
                if i.get_opcode() == InstructionOpcode::Call
                    && let Ok(site) = CallSiteValue::try_from(i)
                    && (site.get_called_fn_value().is_some_and(is_c)
                        || site
                            .get_string_attribute(AttributeLoc::Function, CB_RUN)
                            .is_some())
                {
                    sites.push((function, i));
                }
            }
        }
    }
    let c_call = switch(context, module, "lugha_rt_c_call");
    for (n, (function, call)) in sites.into_iter().enumerate() {
        let callee = CallSiteValue::try_from(call)
            .ok()
            .and_then(|site| site.get_called_fn_value())
            .expect("a direct call of a C function");
        let marked = callee
            .get_string_attribute(AttributeLoc::Function, BLOCKING)
            .is_some();
        let entry_body = CallSiteValue::try_from(call)
            .ok()
            .and_then(|site| site.get_string_attribute(AttributeLoc::Function, CB_RUN));
        let realtime = entry_body
            .as_ref()
            .is_some_and(|a| a.get_string_value().to_bytes() == REALTIME.as_bytes());
        let through = match (entry_body.is_some(), marked) {
            (true, _) if realtime => switch(context, module, "lugha_rt_cb_run_realtime"),
            (true, _) => switch(context, module, "lugha_rt_cb_run"),
            (false, true) => switch(context, module, "lugha_rt_c_blocking"),
            (false, false) => c_call,
        };
        if callee
            .get_string_attribute(AttributeLoc::Function, PINNED)
            .is_some()
        {
            pin_here(context, module, call);
        }
        rewrite(context, module, (function, call), through, n);
    }
}

fn name(function: FunctionValue<'_>) -> String {
    function.get_name().to_string_lossy().into_owned()
}

/// True for a C function: declared here, defined elsewhere, and neither the
/// runtime's (which runs on a task's stack) nor LLVM's.
fn is_c(function: FunctionValue<'_>) -> bool {
    let name = name(function);
    function.count_basic_blocks() == 0
        && function.get_linkage() == Linkage::External
        && !name.starts_with("lugha_")
        && !name.starts_with("llvm.")
}

/// The runtime's `name(thunk, frame)`, `lugha_rt_c_call` or
/// `lugha_rt_c_blocking`, declared once.
fn switch<'ctx>(context: &'ctx Context, module: &Module<'ctx>, name: &str) -> FunctionValue<'ctx> {
    let ptr = context.ptr_type(Default::default());
    module.get_function(name).unwrap_or_else(|| {
        let ty = context
            .void_type()
            .fn_type(&[ptr.into(), ptr.into()], false);
        module.add_function(name, ty, Some(Linkage::External))
    })
}

/// `lugha_rt_pin_here()` just before `call`.
fn pin_here<'ctx>(context: &'ctx Context, module: &Module<'ctx>, call: InstructionValue<'ctx>) {
    let pin = module.get_function("lugha_rt_pin_here").unwrap_or_else(|| {
        let ty = context.void_type().fn_type(&[], false);
        module.add_function("lugha_rt_pin_here", ty, Some(Linkage::External))
    });
    let builder = context.create_builder();
    builder.position_before(&call);
    builder.build_call(pin, &[], "").expect("positioned");
}

/// Turns `call`, in `function`, into a frame and a call of `c_call` with
/// a new thunk, the `n`th.
fn rewrite<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    (function, call): (FunctionValue<'ctx>, InstructionValue<'ctx>),
    c_call: FunctionValue<'ctx>,
    n: usize,
) {
    let site = CallSiteValue::try_from(call).expect("a call");
    let callee = site.get_called_fn_value().expect("a direct call");
    let count = call.get_num_operands() - 1;
    let args: Vec<BasicValueEnum<'ctx>> = (0..count)
        .map(|i| {
            call.get_operand(i)
                .and_then(|o| o.value())
                .expect("a call's argument is a value")
        })
        .collect();
    let ret: Option<BasicTypeEnum<'ctx>> = match call.get_type() {
        AnyTypeEnum::VoidType(_) => None,
        other => {
            Some(BasicTypeEnum::try_from(other).expect("a C function returns a value or nothing"))
        }
    };
    let mut fields: Vec<BasicTypeEnum<'ctx>> = args.iter().map(|a| a.get_type()).collect();
    fields.extend(ret);
    let frame_type = context.struct_type(&fields, false);

    // The thunk: the original call, from the frame, on the C stack.
    let ptr = context.ptr_type(Default::default());
    let thunk = module.add_function(
        &format!("{THUNK}{n}"),
        context.void_type().fn_type(&[ptr.into()], false),
        Some(Linkage::Internal),
    );
    let builder = context.create_builder();
    builder.position_at_end(context.append_basic_block(thunk, "entry"));
    let frame = thunk
        .get_nth_param(0)
        .expect("a thunk takes its frame")
        .into_pointer_value();
    let field = |b: &inkwell::builder::Builder<'ctx>, at: u32| {
        b.build_struct_gep(frame_type, frame, at, "field")
            .expect("a field per argument")
    };
    let mut values: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
    for (at, ty) in (0u32..).zip(&fields[..args.len()]) {
        let value = builder
            .build_load(*ty, field(&builder, at), "arg")
            .expect("positioned");
        values.push(value.into());
    }
    let inner = builder
        .build_call(callee, &values, "c")
        .expect("positioned");
    copy_attributes(site, inner, args.len());
    if ret.is_some() {
        let at = u32::try_from(args.len()).expect("arguments fit in u32");
        let inkwell::values::ValueKind::Basic(result) = inner.try_as_basic_value() else {
            unreachable!("the call returns a value")
        };
        builder
            .build_store(field(&builder, at), result)
            .expect("positioned");
    }
    builder.build_return(None).expect("positioned");

    // The caller: the frame, the switch, the result.
    let entry = function.get_first_basic_block().expect("an entry block");
    let slot_builder = context.create_builder();
    match entry.get_first_instruction() {
        Some(first) => slot_builder.position_before(&first),
        None => slot_builder.position_at_end(entry),
    }
    let slot = slot_builder
        .build_alloca(frame_type, "c.frame")
        .expect("positioned");
    builder.position_before(&call);
    for (at, value) in (0u32..).zip(&args) {
        let to = builder
            .build_struct_gep(frame_type, slot, at, "field")
            .expect("a field per argument");
        builder.build_store(to, *value).expect("positioned");
    }
    let thunk_at = thunk.as_global_value().as_pointer_value();
    builder
        .build_call(c_call, &[thunk_at.into(), slot.into()], "")
        .expect("positioned");
    if let Some(ty) = ret {
        let at = u32::try_from(args.len()).expect("arguments fit in u32");
        let from = builder
            .build_struct_gep(frame_type, slot, at, "result")
            .expect("the result's field");
        let result = builder
            .build_load(ty, from, "c.result")
            .expect("positioned");
        let result = result
            .as_instruction_value()
            .expect("a load is an instruction");
        call.replace_all_uses_with(&result);
    }
    call.erase_from_basic_block();
}

/// The original call's attributes (C's extensions, `byval`, `sret`) on
/// the thunk's call of the same function.
fn copy_attributes<'ctx>(from: CallSiteValue<'ctx>, to: CallSiteValue<'ctx>, args: usize) {
    let mut places = vec![AttributeLoc::Function, AttributeLoc::Return];
    places.extend((0..args).map(|i| AttributeLoc::Param(u32::try_from(i).expect("fits"))));
    for place in places {
        for attribute in from.attributes(place) {
            to.add_attribute(place, attribute);
        }
    }
}
