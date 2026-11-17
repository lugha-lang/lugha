//! Functions: signatures, bodies, `return` and calls.
//!
//! Every signature is declared before any body is lowered, so calls may name
//! functions defined later in the file, including mutual recursion.

use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, ValueKind};

use super::CodegenError;
use super::externs::{converts_from_c, is_nullable_handle};
use super::lower::{Lowerer, POSITIONED};
use super::scope::{Local, Scopes};
use super::value::{Value, is_aggregate};
use crate::ast::{Expr, ExprKind, FunDecl, PassMode};
use crate::check::{CParam, Type, qualify};

/// A declared function.
#[derive(Debug, Clone)]
pub(super) struct Signature<'ctx> {
    pub function: FunctionValue<'ctx>,
    pub params: Vec<Type>,
    /// `None` for a void function.
    pub ret: Option<Type>,
    /// A C function declared with `extern fun`.
    pub is_extern: bool,
    /// A renamed extern whose C function was first declared with another
    /// type: calls use this type, these C parameter types and result.
    pub c_type: Option<CType<'ctx>>,
    /// An extern passing structs by value: its lowered C signature.
    pub c_sig: Option<super::c_calls::CSig<'ctx>>,
}

/// An extern's own C function type, its parameters' Lugha types (`None`
/// for pointers lughac fills in) and result.
pub(super) type CType<'ctx> = (
    inkwell::types::FunctionType<'ctx>,
    Vec<Option<Type>>,
    Option<Type>,
);

impl<'ctx> Lowerer<'ctx> {
    /// Lowers one function body of module `path`. Parameters become locals.
    pub(super) fn define(&mut self, path: &[String], f: &FunDecl) -> Result<(), CodegenError> {
        let signature = self.functions[&qualify(path, &f.name.name)].clone();
        self.define_with(f, &signature, None)
    }

    /// Lowers a function or method body. A method's receiver arrives first,
    /// as a pointer that serves as `self`'s slot: the caller's copy for
    /// `self`, the caller's own place for `mut self`.
    pub(super) fn define_with(
        &mut self,
        f: &FunDecl,
        signature: &Signature<'ctx>,
        receiver: Option<Type>,
    ) -> Result<(), CodegenError> {
        self.define_body(f, signature, receiver.clone())?;
        // A keeping function's owning instance: the same body, its kept
        // parameters its own (`codegen::owning`).
        if let Some(own) = self.owning_of(signature) {
            self.owning = true;
            let defined = self.define_body(f, &own, receiver);
            self.owning = false;
            defined?;
        }
        Ok(())
    }

    /// Lowers one instance of a function or method body.
    fn define_body(
        &mut self,
        f: &FunDecl,
        signature: &Signature<'ctx>,
        receiver: Option<Type>,
    ) -> Result<(), CodegenError> {
        self.function = Some(signature.function);
        let outer = self.rc_enter_function();
        self.enter_located(signature.function);
        self.ret = signature.ret.clone();
        self.scopes = Scopes::default();
        self.loops.clear();
        self.defers.clear();
        self.builder
            .position_at_end(self.context.append_basic_block(signature.function, "entry"));
        self.preempt_point();
        self.stack_check();

        self.scopes.push();
        let first = u32::from(receiver.is_some());
        // An owning instance owns what it keeps:
        // released when it returns unless it moved it on.
        let kept = match (self.owning, self.kept_of(signature.function)) {
            (true, Some(kept)) => kept,
            _ => Vec::new(),
        };
        let owns = |slot: u32| kept.get(slot as usize).copied().unwrap_or(false);
        if let Some(ty) = receiver {
            let arg = signature
                .function
                .get_nth_param(0)
                .expect("methods take their receiver first");
            if owns(0) {
                self.rc_own_param(arg.into_pointer_value(), &ty);
            }
            let local = Local {
                ptr: arg.into_pointer_value(),
                ty,
                borrowed: !owns(0),
            };
            self.scopes.declare("self", local);
        }
        for (index, (param, ty)) in f.params.iter().zip(&signature.params).enumerate() {
            let index = first + u32::try_from(index).expect("parameter count fits in u32");
            let arg = signature
                .function
                .get_nth_param(index)
                .expect("declared with these parameters");
            // A struct arrives as a pointer to the caller's storage, which
            // serves as its slot: parameters are immutable. A `mut` one is
            // the caller's place itself.
            let by_place = matches!(param.mode, Some((crate::ast::PassMode::Mut, _)));
            let ptr = if by_place || is_aggregate(ty) {
                arg.into_pointer_value()
            } else {
                let ptr = self.entry_alloca(ty, &param.name.name);
                self.builder.build_store(ptr, arg).expect(POSITIONED);
                ptr
            };
            if owns(index) && !by_place {
                self.rc_own_param(ptr, ty);
            }
            self.scopes.declare(
                &param.name.name,
                Local {
                    ptr,
                    ty: ty.clone(),
                    borrowed: !owns(index) || by_place,
                },
            );
        }
        let value = self.block_inner(&f.body, true)?;
        self.scopes.pop();

        match (&signature.ret, value) {
            // The checker guarantees control never reaches the end of a body that
            // diverges.
            (_, Value::Never) => {
                self.builder.build_unreachable().expect(POSITIONED);
            }
            (None, _) => {
                self.builder.build_return(None).expect(POSITIONED);
            }
            (Some(_), Value::Val(result)) => {
                self.builder.build_return(Some(&result)).expect(POSITIONED);
            }
            (Some(_), Value::Void) => unreachable!("checked: E0503 rejects missing returns"),
        }
        self.rc_finish_function(signature.function, outer);
        Ok(())
    }

    /// `return [value];` — leaves the builder in a fresh dead block.
    pub(super) fn return_stmt(&mut self, value: Option<&Expr>) -> Result<(), CodegenError> {
        match (self.ret.clone(), value) {
            (Some(ty), Some(expr)) => {
                let result = self.value_for_return(expr, &ty)?;
                // Computed, then every block's defers run.
                let result = self.hold(result, &ty, 0);
                self.run_defers(0)?;
                self.builder.build_return(Some(&result)).expect(POSITIONED);
            }
            _ => {
                self.run_defers(0)?;
                self.builder.build_return(None).expect(POSITIONED);
            }
        }
        self.start_dead_block("after.return");
        Ok(())
    }

    /// The pointer C receives for a Lugha string: its first data byte, 8 bytes
    /// past the length header, NUL-terminated.
    pub(super) fn c_string(&self, string: BasicValueEnum<'ctx>) -> BasicValueEnum<'ctx> {
        // The header layout `{ i64 len, [0 x i8] bytes }`; field 1 is the data, 8 bytes in.
        let i64_type = self.context.i64_type().into();
        let bytes = self.context.i8_type().array_type(0).into();
        let header = self.context.struct_type(&[i64_type, bytes], false);
        let data = self
            .builder
            .build_struct_gep(header, string.into_pointer_value(), 1, "cstr");
        data.expect("field 1 of a two-field struct exists").into()
    }

    /// A call to a declared function; arguments run left to right.
    pub(super) fn call(&mut self, call: &Expr, args: &[Expr]) -> Result<Value<'ctx>, CodegenError> {
        let ExprKind::Call(callee, _) = &call.kind else {
            unreachable!("called with a call expression")
        };
        // The checker resolved the callee: `f` here or `m.f`.
        // A generic callee calls its instance at this call's arguments.
        let key = self.callees.get(&call.id).cloned();
        // An extern taking callbacks passes its arguments by its plan.
        let plan = key.as_ref().and_then(|k| self.c_params.get(k).cloned());
        let signature = key.and_then(|key| match self.prelude_target(call, &key) {
            Some(instance) => Some(instance.signature),
            None => self.functions.get(&key).cloned(),
        });
        let Some(signature) = signature else {
            // Not a declared function, so one of the intrinsics (checked: E0301 otherwise).
            let ExprKind::Name(name) = &callee.kind else {
                unreachable!("checked: E0406 rejects non-name callees")
            };
            // A file's contents, read by the checker.
            if self.includes.contains_key(&call.id) {
                return Ok(Value::Val(self.included(call).into()));
            }
            // Written in Lugha in the prelude.
            if let Some(helper) = crate::check::prelude_helper(name) {
                return self.prelude_intrinsic(helper, args);
            }
            if ["args", "parse_f64"].contains(&name.as_str()) {
                return self.input_intrinsic(name, args);
            }
            const IO: [&str; 9] = [
                "read_line",
                "eprint",
                "eprintln",
                "exit",
                "read_stdin_bytes",
                "string_from_utf8",
                "c_read",
                "c_write",
                "flush",
            ];
            if IO.contains(&name.as_str()) {
                return self.io_intrinsic(call, name, args);
            }
            return self.intrinsic(call, name, args);
        };
        // Structs by value go as C passes them.
        if let Some(sig) = signature.c_sig.clone() {
            let plan = plan.unwrap_or_else(|| vec![CParam::Value; args.len()]);
            let raw = self.callback_arguments(args, &signature.params, &plan)?;
            let code = signature.function.as_global_value().as_pointer_value();
            let called = self.c_sig_call(&sig, code, raw);
            // `out` arguments take what C wrote.
            self.write_outs();
            return Ok(match (called, &signature.ret) {
                (Some(result), Some(ret)) if converts_from_c(ret) => {
                    Value::Val(self.value_from_c(result, ret))
                }
                (Some(result), _) => Value::Val(result),
                (None, _) => Value::Void,
            });
        }
        let mut claims = Vec::new();
        let values = match &plan {
            Some(plan) => self
                .callback_arguments(args, &signature.params, plan)?
                .into_iter()
                .map(Into::into)
                .collect(),
            // A keeping function's owning instance.
            None if let Some(own) = self.owning_instances.get(&signature.function).cloned() => {
                let values = self.owned_arguments(args, &signature.params, &own.kept)?;
                let values = self.with_place(own.function, values, call.span.start);
                let site = self
                    .builder
                    .build_call(own.function, &values, "call")
                    .expect(POSITIONED);
                return Ok(match site.try_as_basic_value() {
                    ValueKind::Basic(result) => Value::Val(result),
                    ValueKind::Instruction(_) => Value::Void,
                });
            }
            // Fresh arguments of a Lugha function are let go of as soon as
            // it returns.
            None if !signature.is_extern => {
                // A call among the arguments claims its own.
                let outer = self.claims.replace(Vec::new());
                let values = self.arguments(args, &signature.params, false);
                claims = std::mem::replace(&mut self.claims, outer).unwrap_or_default();
                values?
            }
            None => self.arguments(args, &signature.params, signature.is_extern)?,
        };
        // A `@caller_location` function takes this call's place.
        let values = self.with_place(signature.function, values, call.span.start);
        let site = match &signature.c_type {
            // A renamed extern with its own type for a shared C function.
            Some((fn_type, abi, ret)) => {
                let code = signature.function.as_global_value().as_pointer_value();
                let site = self
                    .builder
                    .build_indirect_call(*fn_type, code, &values, "call")
                    .expect(POSITIONED);
                self.call_site_abi(site, abi, ret.as_ref());
                site
            }
            None => self
                .builder
                .build_call(signature.function, &values, "call")
                .expect(POSITIONED),
        };
        let instruction = match site.try_as_basic_value() {
            ValueKind::Basic(v) => inkwell::values::BasicValue::as_instruction_value(&v)
                .expect("a call is an instruction"),
            ValueKind::Instruction(i) => i,
        };
        self.rc_release_claims(instruction, claims);
        self.write_outs();
        Ok(match (site.try_as_basic_value(), &signature.ret) {
            (ValueKind::Basic(result), Some(ret))
                if signature.is_extern && converts_from_c(ret) =>
            {
                Value::Val(self.value_from_c(result, ret))
            }
            (ValueKind::Basic(result), _) => Value::Val(result),
            (ValueKind::Instruction(_), _) => Value::Void,
        })
    }

    /// C's extensions on a call's arguments and result, for a call that
    /// doesn't go through a declaration carrying them.
    fn call_site_abi(
        &self,
        site: inkwell::values::CallSiteValue<'ctx>,
        params: &[Option<Type>],
        ret: Option<&Type>,
    ) {
        use super::declare::abi_attribute;
        use inkwell::attributes::AttributeLoc;
        for (index, ty) in params.iter().enumerate() {
            if let Some(attribute) = ty.as_ref().and_then(|ty| abi_attribute(self.context, ty)) {
                let index = u32::try_from(index).expect("parameter count fits in u32");
                site.add_attribute(AttributeLoc::Param(index), attribute);
            }
        }
        if let Some(attribute) = ret.and_then(|ty| abi_attribute(self.context, ty)) {
            site.add_attribute(AttributeLoc::Return, attribute);
        }
    }

    /// Lowers call arguments left to right: aggregates are passed as
    /// a pointer to a caller-side copy, strings to C as their bytes.
    pub(super) fn arguments(
        &mut self,
        args: &[Expr],
        params: &[Type],
        is_extern: bool,
    ) -> Result<Vec<BasicMetadataValueEnum<'ctx>>, CodegenError> {
        let mut values: Vec<BasicMetadataValueEnum> = Vec::with_capacity(args.len());
        for (arg, ty) in args.iter().zip(params) {
            // `mut place`: the place's address, which the callee changes.
            if !is_extern && let ExprKind::Pass(PassMode::Mut, place) = &arg.kind {
                values.push(self.mut_place(place)?.into());
                continue;
            }
            let value = self.get(arg, ty)?;
            let mut claim = match self.claims {
                Some(_) => self.rc_claim(arg.id, value, ty),
                None => None,
            };
            let value = if is_extern && *ty == Type::String {
                self.c_string(value)
            } else if is_extern && is_nullable_handle(ty) {
                self.nullable_to_c(value, ty)
            } else if is_aggregate(ty) {
                // Passed as a pointer to caller storage, never copied.
                let slot = self.entry_alloca(ty, "arg");
                self.builder.build_store(slot, value).expect(POSITIONED);
                if let Some(claim) = claim.as_mut() {
                    claim.at = Some(slot);
                }
                slot.into()
            } else {
                value
            };
            if let (Some(claims), Some(claim)) = (self.claims.as_mut(), claim) {
                claims.push(claim);
            }
            values.push(value.into());
        }
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::ir;

    #[test]
    fn functions_are_prefixed_and_typed_by_their_signature() {
        let ir = ir(
            "fun half(x: u8): f64 = x as f64 / 2.0;\nfun main(): i32 { half(9).saturate<i32>() }",
        );
        assert!(ir.contains("define double @lugha_fn_half(i8"), "{ir}");
        assert!(ir.contains("call double @lugha_fn_half(i8"), "{ir}");
    }

    #[test]
    fn diverging_bodies_end_unreachable() {
        let ir = ir("fun f(): i64 { return 1; }\nfun main() {}");
        let f = &ir[ir.find("define i64 @lugha_fn_f").unwrap()..];
        assert!(f[..f.find("\n}").unwrap()].contains("unreachable"), "{f}");
    }
}
