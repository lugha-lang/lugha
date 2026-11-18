//! The stack check at every function's entry, and the per-call cost of
//! preemption simulated for measuring (`LUGHA_CALL_COSTS=1`): the
//! countdown to the next scheduling point at entries and loop back-edges in
//! every program, with tasks or without, where a spent countdown only
//! yields when another task is ready.
//!
//! A task's stack has no guard pages: its entry compares the stack pointer
//! with the running task's `lugha_rt_stack_limit` (0 for code on a stack
//! the runtime didn't make), and below it `lugha_rt_stack_low` panics with
//! `stack overflow`, which unwinds.

use inkwell::IntPredicate;
use inkwell::context::Context;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::{Linkage, Module};
use inkwell::values::{FunctionValue, ValueKind};

use crate::check::Type;

use super::lower::{Lowerer, POSITIONED};

/// The attribute marking a Lugha function whose entry checks the stack.
const STACK_CHECK: &str = "lugha-stack-check";

/// The stack check at the entry of every marked function `module` still
/// defines, once optimised: the stack pointer (`llvm.stacksave`, needing
/// no frame pointer) against `lugha_rt_stack_limit`, with the call of
/// `lugha_rt_stack_low` on the cold side. The check takes a new first
/// block, and the function's slots (its first block's `alloca`s) move into
/// it, so they stay its fixed slots.
pub(super) fn insert_stack_checks<'ctx>(context: &'ctx Context, module: &Module<'ctx>) {
    let marked: Vec<FunctionValue<'ctx>> = module
        .get_functions()
        .filter(|f| f.count_basic_blocks() > 0)
        .filter(|f| {
            f.get_string_attribute(inkwell::attributes::AttributeLoc::Function, STACK_CHECK)
                .is_some()
        })
        .collect();
    if marked.is_empty() {
        return;
    }
    let i64_type = context.i64_type();
    let ptr = context.ptr_type(Default::default());
    let limit = module
        .get_global("lugha_rt_stack_limit")
        .unwrap_or_else(|| {
            let global = module.add_global(i64_type, None, "lugha_rt_stack_limit");
            global.set_linkage(Linkage::External);
            global
        });
    let low = module
        .get_function("lugha_rt_stack_low")
        .unwrap_or_else(|| {
            let ty = context.void_type().fn_type(&[], false);
            module.add_function("lugha_rt_stack_low", ty, Some(Linkage::External))
        });
    let stack_save = Intrinsic::find("llvm.stacksave")
        .and_then(|i| i.get_declaration(module, &[ptr.into()]))
        .expect("LLVM has llvm.stacksave");
    let builder = context.create_builder();
    for function in marked {
        let body = function
            .get_first_basic_block()
            .expect("a defined function");
        let check = context.prepend_basic_block(body, "stack.check");
        let cold = context.insert_basic_block_after(check, "stack.low");
        builder.position_at_end(check);
        // The slots move ahead of the check, staying in the first block.
        let slots: Vec<_> = body
            .get_instructions()
            .take_while(|i| i.get_opcode() == inkwell::values::InstructionOpcode::Alloca)
            .collect();
        for slot in slots {
            // Re-inserting takes the name again, or it would be lost.
            let name = slot.get_name().map(|n| n.to_string_lossy().into_owned());
            slot.remove_from_basic_block();
            builder.insert_instruction(&slot, name.as_deref());
        }
        let sp = builder.build_call(stack_save, &[], "sp").expect(POSITIONED);
        let ValueKind::Basic(sp) = sp.try_as_basic_value() else {
            unreachable!("llvm.stacksave returns a pointer")
        };
        let at = builder
            .build_ptr_to_int(sp.into_pointer_value(), i64_type, "sp.at")
            .expect(POSITIONED);
        let floor = builder
            .build_load(i64_type, limit.as_pointer_value(), "stack.limit")
            .expect(POSITIONED)
            .into_int_value();
        let below = builder
            .build_int_compare(IntPredicate::ULT, at, floor, "stack.below")
            .expect(POSITIONED);
        builder
            .build_conditional_branch(below, cold, body)
            .expect(POSITIONED);
        builder.position_at_end(cold);
        builder.build_call(low, &[], "").expect(POSITIONED);
        builder.build_unconditional_branch(body).expect(POSITIONED);
    }
}

impl<'ctx> Lowerer<'ctx> {
    /// The C `main`, `main(argc, argv)`, which runs the program as the
    /// `main` task on a pooled stack through `lugha_rt_main`; and the
    /// function it runs, `lugha_entry` (of `main_type`), for the caller to
    /// fill with the program's start.
    pub(super) fn c_entry(
        &mut self,
        main_type: inkwell::types::FunctionType<'ctx>,
    ) -> FunctionValue<'ctx> {
        let entry = self
            .module
            .add_function("lugha_entry", main_type, Some(Linkage::Internal));
        let main = self.module.add_function("main", main_type, None);
        let run = self.runtime(
            "lugha_rt_main",
            &[Type::I32, Type::String, Type::String],
            Some(Type::I32),
        );
        let builder = self.context.create_builder();
        builder.position_at_end(self.context.append_basic_block(main, "entry"));
        let argc = main.get_nth_param(0).expect("main takes argc");
        let argv = main.get_nth_param(1).expect("main takes argv");
        let entry_at = entry.as_global_value().as_pointer_value();
        let code = builder
            .build_call(run, &[argc.into(), argv.into(), entry_at.into()], "code")
            .expect(POSITIONED)
            .try_as_basic_value();
        let ValueKind::Basic(code) = code else {
            unreachable!("lugha_rt_main returns the exit code")
        };
        builder.build_return(Some(&code)).expect(POSITIONED);
        entry
    }

    /// True when the per-call costs are simulated.
    pub(super) fn call_costs(&self) -> bool {
        self.instruments.call_costs
    }

    /// Marks `function` for the stack check its entry gets once the module
    /// is optimised (`insert_stack_checks`): after inlining, so a function
    /// inlined into a loop doesn't check on every turn.
    pub(super) fn mark_stack_check(&self, function: FunctionValue<'ctx>) {
        let mark = self.context.create_string_attribute(STACK_CHECK, "");
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, mark);
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::{ir_calls, ir_rc};

    const FIB: &str = "fun fib(n: i64): i64 = if n < 2 { n } else { fib(n - 1) + fib(n - 2) };\n\
                       fun main() { let x = fib(3); }";

    #[test]
    fn every_entry_checks_the_stack() {
        for ir in [ir_rc(FIB), ir_calls(FIB)] {
            let at = ir.find("define i64 @lugha_fn_fib(").expect("defined");
            let fib = &ir[at..at + ir[at..].find("\n}").expect("a body ends")];
            assert!(fib.contains("@llvm.stacksave"), "{fib}");
            assert!(fib.contains("call void @lugha_rt_stack_low()"), "{fib}");
            assert!(!fib.contains("@llvm.frameaddress"), "{fib}");
        }
    }

    #[test]
    fn only_the_simulation_counts_down_without_tasks() {
        let fib_at = |ir: &str| {
            let at = ir.find("define i64 @lugha_fn_fib(").expect("defined");
            ir[at..at + ir[at..].find("\n}").expect("a body ends")].to_string()
        };
        assert!(fib_at(&ir_calls(FIB)).contains("@lugha_rt_task_budget"));
        assert!(!ir_rc(FIB).contains("lugha_rt_task_budget"));
    }
}
