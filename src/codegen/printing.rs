//! Printing values in codegen. A value's text for `print`, `to_string` and
//! `"{x}"` is its `Display`: numbers, `bool`, `char` and strings inline, any
//! other type through its `impl Display`'s `show`. Its debug form, for
//! `x.debug()`, `debug(x)` and panics, is inline for those same types, the
//! prelude's `Debug` blocks or a type's own `impl Debug` for the rest, and
//! a fixed form for what has no structure to show (`<fun>`, `<task>`). Each
//! non-inline type's debug form is one `lugha_debug_<type>` helper.

use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, FunctionValue, ValueKind};

use super::copy::mangle;
use super::instances::instance_name;
use super::lower::{Lowerer, POSITIONED};
use super::methods::{Method, key};
use super::value::llvm_type;
use crate::check::{DEBUG, DISPLAY, Type};

/// True for the types that print without a helper.
fn plain(ty: &Type) -> bool {
    ty.is_numeric() || matches!(ty, Type::Bool | Type::String | Type::Char)
}

/// A type's name as its fixed form shows it: its key's last part.
fn bare(key: &str) -> &str {
    key.rsplit('.').next().unwrap_or(key)
}

impl<'ctx> Lowerer<'ctx> {
    /// `(ty, value)` ready for a print or conversion intrinsic: unchanged
    /// for numbers, `bool` and strings, otherwise its `Display` text.
    pub(super) fn printable(
        &mut self,
        ty: Type,
        value: BasicValueEnum<'ctx>,
    ) -> (Type, BasicValueEnum<'ctx>) {
        if plain(&ty) || matches!(ty, Type::Never | Type::Error) {
            // The text is owned, as made text is.
            if ty == Type::String {
                self.rc_hold_string(value);
            }
            return (ty, value);
        }
        let text = self.text(value, &ty);
        (Type::String, text)
    }

    /// A value's `Display` text: a string is itself. Under reference
    /// counting the text is owned: a string gets one more holder.
    pub(super) fn text(&mut self, value: BasicValueEnum<'ctx>, ty: &Type) -> BasicValueEnum<'ctx> {
        match ty {
            Type::String => {
                self.rc_hold_string(value);
                value
            }
            _ if plain(ty) => self.text_of(ty, value),
            Type::Interface(..) => self
                .dynamic_show(value, ty)
                .expect("checked: a printed interface declares `show`"),
            _ => {
                let show = self
                    .item_method(ty, DISPLAY, "show")
                    .expect("checked: a printed value implements `Display`");
                self.call_item(&show, value, ty)
            }
        }
    }

    /// A value's debug form: strings and chars quoted. Always a new
    /// string or a constant one, so it is owned.
    pub(super) fn part(&mut self, value: BasicValueEnum<'ctx>, ty: &Type) -> BasicValueEnum<'ctx> {
        match ty {
            Type::String => {
                self.call_runtime_value("lugha_rt_str_quote", &[Type::String], &[value])
            }
            Type::Char => self.call_runtime_value("lugha_rt_char_quote", &[Type::Char], &[value]),
            _ if plain(ty) => self.text_of(ty, value),
            Type::Unit => self.string_literal("()").into(),
            Type::Fun(..) => self.string_literal("<fun>").into(),
            _ => {
                let helper = self.debug_function(ty);
                let call = self
                    .builder
                    .build_call(helper, &[value.into()], "text")
                    .expect(POSITIONED);
                match call.try_as_basic_value() {
                    ValueKind::Basic(text) => text,
                    ValueKind::Instruction(_) => unreachable!("debug helpers return a string"),
                }
            }
        }
    }

    /// `a + b` for two owned strings, which it lets go of: under
    /// reference counting both are released once joined.
    pub(super) fn concat_owned(
        &mut self,
        a: BasicValueEnum<'ctx>,
        b: BasicValueEnum<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let joined = self.concat(a, b);
        self.rc_pointer(a.into_pointer_value(), false);
        self.rc_pointer(b.into_pointer_value(), false);

        joined
    }

    /// One more holder of the string `value`.
    pub(super) fn rc_hold_string(&mut self, value: BasicValueEnum<'ctx>) {
        self.rc_pointer(value.into_pointer_value(), true);
    }

    /// Lets go of the owned string `value`.
    pub(super) fn rc_drop_string(&mut self, value: BasicValueEnum<'ctx>) {
        self.rc_pointer(value.into_pointer_value(), false);
    }

    pub(super) fn concat(
        &mut self,
        a: BasicValueEnum<'ctx>,
        b: BasicValueEnum<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        self.call_runtime_value(
            "lugha_rt_str_concat",
            &[Type::String, Type::String],
            &[a, b],
        )
    }

    /// `ty`'s method for item `name` of prelude interface `interface`: its
    /// `impl` block's, or the block covering it, compiled as an instance.
    fn item_method(&self, ty: &Type, interface: &str, name: &str) -> Option<Method<'ctx>> {
        if let Some(t) = self.derives.target(interface, name, ty, |_| false) {
            return self.monos.get(&instance_name(&t.key, &t.args)).cloned();
        }
        let method = self.block_method(ty, interface, &[], name);
        let (owner, args) = match ty {
            Type::Struct(owner, args) | Type::Enum(owner, args) => (owner, args.as_slice()),
            Type::Handle(owner) => (owner, &[][..]),
            _ => return None,
        };
        if method == name {
            return None;
        }
        let k = key(owner, &method);
        match args.is_empty() {
            true => self.methods.get(&k).or_else(|| self.monos.get(&k)).cloned(),
            false => self.monos.get(&instance_name(&k, args)).cloned(),
        }
    }

    /// `m(value)`, the receiver passed through a stack slot.
    fn call_item(
        &mut self,
        m: &Method<'ctx>,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
    ) -> BasicValueEnum<'ctx> {
        let slot = self.entry_alloca(ty, "self");
        self.builder.build_store(slot, value).expect(POSITIONED);
        let call = self
            .builder
            .build_call(m.signature.function, &[slot.into()], "text")
            .expect(POSITIONED);
        match call.try_as_basic_value() {
            ValueKind::Basic(text) => text,
            ValueKind::Instruction(_) => unreachable!("`show` and `debug` return a string"),
        }
    }

    /// `lugha_debug_<mangled>(T) -> string`, generated once per type. Declared
    /// before its body, so a recursive type's helper can call itself; an
    /// interface's body comes at the end (`codegen::show_iface`).
    fn debug_function(&mut self, ty: &Type) -> FunctionValue<'ctx> {
        let name = format!("lugha_debug_{}", mangle(ty));
        if let Some(function) = self.module.get_function(&name) {
            return function;
        }
        let ptr = self.context.ptr_type(Default::default());
        let llvm = llvm_type(self.context, ty);
        let function = self.module.add_function(
            &name,
            ptr.fn_type(&[llvm.into()], false),
            Some(Linkage::Internal),
        );
        if let Type::Interface(..) = ty {
            self.shows.push((ty.clone(), function));
            return function;
        }
        let (saved_block, saved_function) = (self.builder.get_insert_block(), self.function);
        self.function = Some(function);
        self.builder
            .position_at_end(self.context.append_basic_block(function, "entry"));
        let value = function.get_nth_param(0).expect("one parameter");
        let text = match (self.item_method(ty, DEBUG, "debug"), ty) {
            (Some(m), _) => self.call_item(&m, value, ty),
            (None, Type::Ref(inner)) => self.show_box(value.into_pointer_value(), inner),
            // A handle shows its type, never its address.
            (None, Type::Handle(key)) => self.string_literal(&format!("<{}>", bare(key))).into(),
            (None, Type::Task(_)) => self.string_literal("<task>").into(),
            (None, Type::Chan(_)) => self.string_literal("<chan>").into(),
            (None, Type::Actor(key)) => self
                .string_literal(&format!("<actor {}>", bare(key)))
                .into(),
            // Only C knows a union's live field.
            (None, _) => self.string_literal("<union>").into(),
        };
        self.builder.build_return(Some(&text)).expect(POSITIONED);
        self.function = saved_function;
        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        function
    }

    /// `debug(x)`: `[file:line] x = <debug form>` on stderr, and `x` back.
    pub(super) fn debug_print(
        &mut self,
        call: &crate::ast::Expr,
        arg: &crate::ast::Expr,
        ty: &Type,
        value: BasicValueEnum<'ctx>,
    ) {
        let (file, _) = self.source.file_at(call.span.start);
        let file = file.name.clone();
        let (line, _) = self.source.line_col(call.span.start);
        let written = self
            .source
            .written(arg.span.start, arg.span.end)
            .to_string();
        let lead: BasicValueEnum = self
            .string_literal(&format!("[{file}:{line}] {written} = "))
            .into();
        let form = self.part(value, ty);
        let text = self.concat_owned(lead, form);
        let newline = self.context.i32_type().const_int(1, false);
        let eprint = self.runtime("lugha_rt_eprint", &[Type::String, Type::I32], None);
        self.builder
            .build_call(eprint, &[text.into(), newline.into()], "")
            .expect(POSITIONED);
        self.rc_drop_string(text);
    }

    /// `x.show()` on interface value `value`, through its table: `None`
    /// when the interface doesn't declare `show`.
    fn dynamic_show(
        &mut self,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
    ) -> Option<BasicValueEnum<'ctx>> {
        let Type::Interface(key, targs) = ty else {
            return None;
        };
        let methods = self.interfaces[key].methods_at(targs);
        let index = methods.iter().position(|m| m.name == "show")?;
        let (table, boxed) = self.parts(value.into_struct_value());
        let slot =
            u32::try_from(index).expect("method count fits in u32") + super::interfaces::SLOTS;
        let code = self.slot(table, slot);
        let ptr = self.context.ptr_type(Default::default());
        let site = self
            .builder
            .build_indirect_call(
                ptr.fn_type(&[ptr.into()], false),
                code,
                &[boxed.into()],
                "shown",
            )
            .expect(POSITIONED);
        match site.try_as_basic_value() {
            ValueKind::Basic(text) => Some(text),
            ValueKind::Instruction(_) => unreachable!("`show` returns a string"),
        }
    }
}
