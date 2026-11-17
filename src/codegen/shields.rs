//! Shields in codegen. `shield within ms { body }` is the block the parser
//! made of it (`$os.shield_enter(ms)`, a `defer` of `$os.shield_exit()`,
//! the body), lowered with a landing pad of its own, as a scope's body is:
//! an unwind out of the block stops at its edge, and goes on just past it
//! (`lugha_rt_shield_end`). That is how a shield entered while its task
//! already unwinds (in a `defer`) is cut off at its deadline: the cut-off
//! is a second unwind, which only the shield's own pad may stop, giving
//! the first one back. So inside a pad, a shield's calls get pads too.

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::task_scopes::Catch;
use super::value::{Value, llvm_type};
use crate::ast::{Block, Expr};
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// `shield within ms { body }`: the body's value; an unwind out of it
    /// goes on past the shield, once the runtime has seen it stopped.
    pub(super) fn shield_expr(
        &mut self,
        expr: &Expr,
        block: &Block,
    ) -> Result<Value<'ctx>, CodegenError> {
        let ty = self.ty(expr);
        // A shield in a pad's `defer` still needs its calls' pads.
        let was = std::mem::replace(&mut self.pads.lowering, false);
        if was {
            // The pad's own blocks are skipped when calls are given pads;
            // a block of its own isn't, until the pad is done.
            let own = self.append("shield");
            self.branch(own);
            self.builder.position_at_end(own);
        }
        let here = self.current_block();
        let start = (here, here.get_last_instruction());
        // A statement around the shield may start here too: its record
        // must not take in the blocks below, which its calls use.
        let outer = self.unwind_record(start);
        self.unwind_begins(start);
        let caught = self.append("shield.caught");
        let end = self.append("shield.end");
        self.pads.catches.push(Catch {
            rec: None,
            caught,
            defers: self.defers.len(),
            statements: self.rc_statement_depth(),
            start,
        });
        let body = self.block(block);
        let mut finished = None;
        if let Ok(value) = &body {
            match value {
                Value::Never => {
                    self.builder.build_unreachable().expect(POSITIONED);
                }
                value => {
                    finished = Some((*value, self.current_block()));
                    self.branch(end);
                }
            }
            // The body's calls still without a pad (its tail's) stop here too.
            self.unwind_statement(start);
        }
        self.pads.catches.pop();
        self.pads.lowering = was;
        if outer.is_some() {
            self.unwind_restore(start, outer);
        }
        body?;

        self.builder.position_at_end(caught);
        // Only a `()` shield goes on after a cut-off while unwinding.
        if ty != Type::Unit {
            self.rt("lugha_rt_shield_lost", &[], None, &[]);
        }
        self.branch(end);
        self.builder.position_at_end(end);
        let value = match finished {
            Some((Value::Val(v), from)) => {
                let phi = self
                    .builder
                    .build_phi(llvm_type(self.context, &ty), "shield")
                    .expect(POSITIONED);
                // The value from a stopped unwind is never used: past the
                // shield, that unwind goes on (or the program ended).
                let none = llvm_type(self.context, &ty).const_zero();
                phi.add_incoming(&[(&v, from), (&none, caught)]);
                Value::Val(phi.as_basic_value())
            }
            Some((value, _)) => value,
            None => Value::Never,
        };
        self.rt("lugha_rt_shield_end", &[], None, &[]);
        if matches!(value, Value::Never) {
            self.builder.build_unreachable().expect(POSITIONED);
            self.start_dead_block("after.shield");
        }
        Ok(value)
    }
}
