//! The per-call costs of growable stacks and preemption, simulated for
//! measuring only (`LUGHA_CALL_COSTS=1`): a stack
//! check at every function's entry, and the countdown to the next
//! scheduling point at entries and loop back-edges, in every program, with
//! tasks or without.
//! Neither ever changes what a program does: the stack limit is zero, and
//! a spent countdown only yields when another task is ready.

use inkwell::IntPredicate;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::Linkage;

use super::lower::{Lowerer, POSITIONED};

impl<'ctx> Lowerer<'ctx> {
    /// True when the per-call costs are simulated.
    pub(super) fn call_costs(&self) -> bool {
        self.costs.0
    }

    /// At a function's entry, when the costs are simulated: this frame's
    /// address against the stack's limit, with a call on the cold side.
    pub(super) fn stack_check(&mut self) {
        if !self.call_costs() {
            return;
        }
        let i64_type = self.context.i64_type();
        let ptr = self.context.ptr_type(Default::default());
        let limit = match self.module.get_global("lugha_rt_stack_limit") {
            Some(global) => global,
            None => {
                let global = self
                    .module
                    .add_global(i64_type, None, "lugha_rt_stack_limit");
                global.set_linkage(Linkage::External);
                global
            }
        };
        let frame_address = Intrinsic::find("llvm.frameaddress")
            .and_then(|i| i.get_declaration(&self.module, &[ptr.into()]))
            .expect("LLVM has llvm.frameaddress");
        let site = self
            .builder
            .build_call(
                frame_address,
                &[self.context.i32_type().const_zero().into()],
                "frame",
            )
            .expect(POSITIONED);
        let frame = match site.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            inkwell::values::ValueKind::Instruction(_) => {
                unreachable!("llvm.frameaddress returns a pointer")
            }
        };
        let at = self
            .builder
            .build_ptr_to_int(frame, i64_type, "frame.at")
            .expect(POSITIONED);
        let floor = self
            .builder
            .build_load(i64_type, limit.as_pointer_value(), "stack.limit")
            .expect(POSITIONED)
            .into_int_value();
        let low = self
            .builder
            .build_int_compare(IntPredicate::ULT, at, floor, "stack.low")
            .expect(POSITIONED);
        let (grow, go_on) = (self.append("stack.grow"), self.append("stack.ok"));
        self.builder
            .build_conditional_branch(low, grow, go_on)
            .expect(POSITIONED);
        self.builder.position_at_end(grow);
        let function = self.runtime("lugha_rt_stack_low", &[], None);
        self.builder
            .build_call(function, &[], "")
            .expect(POSITIONED);
        self.branch(go_on);
        self.builder.position_at_end(go_on);
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::{ir_calls, ir_rc};

    const FIB: &str = "fun fib(n: i64): i64 = if n < 2 { n } else { fib(n - 1) + fib(n - 2) };\n\
                       fun main() { let x = fib(3); }";

    #[test]
    fn simulated_costs_check_the_stack_and_count_down_at_entry() {
        let ir = ir_calls(FIB);
        let at = ir.find("define i64 @lugha_fn_fib(").expect("defined");
        let fib = &ir[at..at + ir[at..].find("\n}").expect("a body ends")];
        assert!(fib.contains("@llvm.frameaddress"), "{fib}");
        assert!(fib.contains("call void @lugha_rt_stack_low()"), "{fib}");
        assert!(fib.contains("@lugha_rt_task_budget"), "{fib}");
    }

    #[test]
    fn without_the_simulation_a_call_costs_nothing_more() {
        let ir = ir_rc(FIB);
        assert!(!ir.contains("lugha_rt_stack_low"), "{ir}");
        assert!(!ir.contains("lugha_rt_task_budget"), "{ir}");
    }
}
