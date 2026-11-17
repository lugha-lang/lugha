//! Function types as interfaces in codegen. A function value stays a
//! `{ code, env }` pair; two bridges join it to the interface machinery:
//! a table slot for `call` on a function value (it loads the pair from the
//! box and calls it), and a struct implementing a function interface turned
//! into a function value (its `call` method is the code, the boxed struct
//! the environment: a method takes its `self` struct by pointer, as code
//! takes its environment).

use inkwell::module::Linkage;
use inkwell::types::{BasicMetadataTypeEnum, BasicType};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue};

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use super::value::{Value, llvm_type};
use crate::ast::{Expr, ExprId};
use crate::check::{FunOrigin, Type, symbol};

impl<'ctx> Lowerer<'ctx> {
    /// `f(args)` where `f` has one function's own type (in an instance):
    /// that function's code, called directly with `f`'s environment; through
    /// the pair only when the closure isn't lowered yet.
    pub(super) fn call_own(
        &mut self,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<Value<'ctx>, CodegenError> {
        let ty = self.ty(callee);
        let Type::Own(own) = &ty else {
            unreachable!("called with an own type")
        };
        let pair = self.get(callee, &ty)?.into_struct_value();
        let values = self.arguments(args, &own.params, false)?;
        let code = match &own.origin {
            FunOrigin::Closure(id, _) => self.closure_fns.get(&ExprId(*id)).copied(),
            FunOrigin::Named(key) => {
                let target = self.functions.get(key).map(|f| f.function);
                target.map(|t| self.wrapper(t))
            }
        };
        let Some(code) = code else {
            return Ok(self.call_pair(pair, (&own.params, &own.ret), values));
        };
        let env = self
            .builder
            .build_extract_value(pair, 1, "env")
            .expect("a pair has 2 fields");
        let mut all: Vec<BasicMetadataValueEnum> = vec![env.into()];
        all.extend(values);
        let site = self
            .builder
            .build_call(code, &all, "call")
            .expect(POSITIONED);
        Ok(match site.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Value::Val(v),
            inkwell::values::ValueKind::Instruction(_) => Value::Void,
        })
    }

    /// The table slot for `call` on a function value of type `ty`:
    /// `(box, args…) -> ret`, calling the pair the box holds. Emitted once.
    pub(super) fn fun_call_slot(&mut self, ty: &Type) -> FunctionValue<'ctx> {
        let Type::Fun(params, ret) = ty else {
            unreachable!("a function value's slot")
        };
        let name = format!(
            "lugha_fn_$prelude/$Fun{}.call<{}>",
            params.len(),
            symbol(&ty.to_string())
        );
        if let Some(existing) = self.module.get_function(&name) {
            return existing;
        }
        let ptr = self.context.ptr_type(Default::default());
        let mut types: Vec<BasicMetadataTypeEnum> = vec![ptr.into()];
        types.extend(params.iter().map(|p| self.param_type(p)));
        let fn_type = match **ret {
            Type::Unit => self.context.void_type().fn_type(&types, false),
            ref r => llvm_type(self.context, r).fn_type(&types, false),
        };
        let slot = self
            .module
            .add_function(&name, fn_type, Some(Linkage::Internal));
        // Build the body elsewhere, then come back to where lowering was.
        let (saved_block, saved_function) = (self.builder.get_insert_block(), self.function);
        self.function = Some(slot);
        self.builder
            .position_at_end(self.context.append_basic_block(slot, "entry"));
        let boxed = slot
            .get_nth_param(0)
            .expect("the box comes first")
            .into_pointer_value();
        let pair = self
            .builder
            .build_load(llvm_type(self.context, ty), boxed, "fun")
            .expect(POSITIONED)
            .into_struct_value();
        let args = slot.get_param_iter().skip(1).map(Into::into).collect();
        match self.call_pair(pair, (params, ret), args) {
            Value::Val(v) => self.builder.build_return(Some(&v)),
            _ => self.builder.build_return(None),
        }
        .expect(POSITIONED);
        self.function = saved_function;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        slot
    }

    /// `value`, of struct or enum type `ty` implementing function type
    /// `to`'s interface, as a function value: its `call`, and its box.
    pub(super) fn fun_from_value(
        &mut self,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
        to: &Type,
    ) -> BasicValueEnum<'ctx> {
        let Type::Fun(params, ret) = to else {
            unreachable!("checked: converted to a function type")
        };
        let mut args = params.clone();
        args.push((**ret).clone());
        let key = format!("$prelude.$Fun{}", params.len());
        let code = self.method_of(ty, &key, &args, "call");
        let boxed = self.new_box(ty, value);
        let ptr = self.context.ptr_type(Default::default());
        let pair = self.context.struct_type(&[ptr.into(), ptr.into()], false);
        let pair = self
            .builder
            .build_insert_value(
                pair.get_undef(),
                code.as_global_value().as_pointer_value(),
                0,
                "code",
            )
            .expect("a pair has 2 fields");
        self.builder
            .build_insert_value(pair, boxed, 1, "env")
            .expect("a pair has 2 fields")
            .into_struct_value()
            .into()
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::ir;

    /// The body of the function whose definition line contains `name`.
    fn body<'a>(ir: &'a str, name: &str) -> &'a str {
        let start = ir
            .lines()
            .find(|l| l.starts_with("define") && l.contains(name))
            .and_then(|l| ir.find(l))
            .expect("defined");
        let rest = &ir[start..];
        &rest[..rest.find("\n}").expect("a closing brace")]
    }

    #[test]
    fn each_function_passed_gets_an_instance_calling_it_directly() {
        let ir = ir("fun twice<F: fun(i64): i64>(f: F, x: i64): i64 = f(f(x));
            fun double(x: i64): i64 = x * 2;
            fun main() { let k = 3; let a = twice(fun(x: i64): i64 = x * k, 1); let b = twice(double, 1); }");
        let closure = body(&ir, "twice<fun(i64): i64 {closure 1 in main}>");
        assert!(
            closure.contains("call i64 @lugha_closure_main.0("),
            "{closure}"
        );
        let named = body(&ir, "twice<fun(i64): i64 {double}>");
        assert!(named.contains("call i64 @lugha_ref_double("), "{named}");
    }

    #[test]
    fn a_function_value_is_still_called_through_its_pair() {
        let ir = ir("fun twice<F: fun(i64): i64>(f: F, x: i64): i64 = f(f(x));
            fun run(g: fun(i64): i64): i64 = twice(g, 1);
            fun main() {}");
        let erased = body(&ir, "twice<fun(i64): i64>\"(");
        assert!(!erased.contains("@lugha_closure_"), "{erased}");
        assert!(erased.contains("call i64 %"), "an indirect call: {erased}");
    }

    #[test]
    fn a_struct_becomes_a_function_value_through_its_call() {
        let ir = ir("struct Scale { k: i64 }
            impl fun(i64): i64 for Scale { fun call(self, x: i64): i64 = x * self.k; }
            fun apply(f: fun(i64): i64): i64 = f(3);
            fun main() { let a = apply(Scale { k: 2 }); }");
        let main = body(&ir, "@lugha_fn_main(");
        assert!(main.contains("Scale.call"), "{main}");
    }
}
