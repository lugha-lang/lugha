//! Reference counting at the sites values come from and go to: whether an
//! expression's value is fresh (owned, so a temporary releases it) or read
//! from somewhere (borrowed), and how a store takes its value over.

use inkwell::values::{BasicValueEnum, PointerValue};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::value::{Value, llvm_type};
use crate::ast::{Expr, ExprId, ExprKind};
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// After `expr` is lowered to `value`: a fresh value of a counted type
    /// goes into a temporary of the statement.
    pub(super) fn rc_settle(&mut self, expr: &Expr, value: Value<'ctx>) -> Value<'ctx> {
        let Value::Val(v) = value else {
            return value;
        };
        let ty = self.ty(expr);
        if self.counted(&ty) && self.fresh(expr) {
            self.rc_temporary(Some(expr.id), v, &ty);
        }
        value
    }

    /// True if `expr`'s value is new, held by no place: a call's result, a
    /// literal, a conversion's box. A read of a local, field, element or
    /// constant is borrowed, as is what `if`, `match` and blocks give,
    /// which the values inside them settle.
    fn fresh(&self, expr: &Expr) -> bool {
        self.conversions.contains_key(&expr.id) || self.fresh_unconverted(expr)
    }

    /// `fresh`, for `expr`'s value before any conversion.
    pub(super) fn fresh_unconverted(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Name(name) if name == "None" && self.scopes.find(name).is_none() => true,
            ExprKind::Name(_) | ExprKind::Field(..)
                if self.variant_values.contains_key(&expr.id)
                    || self.callees.contains_key(&expr.id) =>
            {
                true
            }
            ExprKind::Field(base, _) if self.ty(base) == Type::Unit => true,
            ExprKind::Index(base, ..) if self.is_map(base) || self.indexes_by_method(base) => true,
            // A brace literal built as a collection: its call settles itself.
            ExprKind::StructLit(lit) if self.chosen_collection(lit).is_some() => false,
            ExprKind::Name(_)
            | ExprKind::Field(..)
            | ExprKind::Index(..)
            | ExprKind::Str(_)
            | ExprKind::Try(_)
            | ExprKind::If { .. }
            | ExprKind::IfLet(_)
            | ExprKind::Block(_)
            | ExprKind::Loop { .. }
            | ExprKind::Match(..) => false,
            // `debug(x)` gives `x` back.
            ExprKind::Call(callee, _)
                if matches!(&callee.kind, ExprKind::Name(n) if n == "debug")
                    && !self.callees.contains_key(&expr.id) =>
            {
                false
            }
            _ => true,
        }
    }

    /// Lowers `expr` for a store of type `ty` under reference counting: the
    /// store owns the result, taking over a fresh value's temporary, moving
    /// an owned local on its last use, or else adding a holder.
    pub(super) fn rc_value_for_store(
        &mut self,
        expr: &Expr,
        ty: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        self.rc_forget_last();
        let value = self.get(expr, ty)?;
        if !self.counted(ty) || self.rc_take(expr.id, ty) {
            return Ok(value);
        }
        if self.moved(expr.id) && self.rc_move_local(expr, ty) {
            return Ok(value);
        }
        self.rc_retain(value, ty);
        Ok(value)
    }

    /// An argument for a parameter an owning instance keeps, under
    /// reference counting: the callee owns it, so the caller hands over a
    /// fresh value or a local it is done with (`handed`), or else adds a
    /// holder.
    pub(super) fn rc_handed(
        &mut self,
        arg: &Expr,
        ty: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        self.rc_forget_last();
        let value = self.get(arg, ty)?;
        if !self.counted(ty) || self.rc_take(arg.id, ty) {
            return Ok(value);
        }
        if self.handed(arg.id) && self.rc_move_local(arg, ty) {
            return Ok(value);
        }
        self.rc_retain(value, ty);
        Ok(value)
    }

    /// A `return` value or function tail: owned by the caller. A plain owned
    /// local moves out (unless a `defer` may still read it).
    pub(super) fn rc_value_for_return(
        &mut self,
        expr: &Expr,
        ty: &Type,
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        self.rc_forget_last();
        let value = self.get(expr, ty)?;
        if !self.counted(ty) || self.rc_take(expr.id, ty) {
            return Ok(value);
        }
        if !self.defers_pending(0) && self.rc_move_local(expr, ty) {
            return Ok(value);
        }
        self.rc_retain(value, ty);
        Ok(value)
    }

    /// Empties the slot of the owned local `expr` names, which hands its
    /// value over; false if `expr` isn't one.
    fn rc_move_local(&mut self, expr: &Expr, ty: &Type) -> bool {
        let ExprKind::Name(name) = &expr.kind else {
            return false;
        };
        if self.const_uses.contains_key(&expr.id) || self.callees.contains_key(&expr.id) {
            return false;
        }
        match self.scopes.find(name) {
            Some(local) if !local.borrowed && self.counted(&local.ty) => {
                self.builder
                    .build_store(local.ptr, llvm_type(self.context, ty).const_zero())
                    .expect(POSITIONED);
                true
            }
            _ => false,
        }
    }

    /// A branch's value, for its expression `body`, into `result`, the
    /// slot owning the branch's value: a fresh value's temporary is taken
    /// over, any other value gets a holder.
    pub(super) fn rc_branch_value(
        &mut self,
        body: &Expr,
        value: BasicValueEnum<'ctx>,
        (result, ty): (PointerValue<'ctx>, &Type),
    ) {
        if self.counted(ty) && !self.rc_take(value_source(body), ty) {
            self.rc_retain(value, ty);
        }
        self.builder.build_store(result, value).expect(POSITIONED);
    }
}

/// The expression whose value `expr` gives: a block's tail, through nested
/// blocks; the temporary a fresh value went into is recorded under it.
pub(super) fn value_source(expr: &Expr) -> ExprId {
    match &expr.kind {
        ExprKind::Block(block) => match &block.tail {
            Some(tail) => value_source(tail),
            None => expr.id,
        },
        _ => expr.id,
    }
}
