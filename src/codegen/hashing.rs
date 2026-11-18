//! The `$hash` built-ins in codegen, inline for speed: `$hash.key(i)`
//! loads word `i` of the runtime's `lugha_rt_hash_key`, the process's
//! secret key, and `$hash.chunk(s, i)` reads up to eight of string `s`'s
//! bytes from `i` as a little-endian `u64`, zero beyond the string's end.
//! The prelude's `Hasher` builds SipHash-1-3 on them.

use inkwell::module::Linkage;
use inkwell::values::BasicValueEnum;

use super::CodegenError;
use super::lower::{Lowerer, POSITIONED};
use crate::ast::Expr;
use crate::check::Type;

/// The runtime's two-word key, filled as it starts.
const KEY: &str = "lugha_rt_hash_key";

impl<'ctx> Lowerer<'ctx> {
    /// `$hash.key(i)` or `$hash.chunk(s, i)`, as a `u64`.
    pub(super) fn hash_call(
        &mut self,
        op: &str,
        args: &[Expr],
    ) -> Result<BasicValueEnum<'ctx>, CodegenError> {
        let i64_type = self.context.i64_type();
        if op == "key" {
            // The prelude asks for word 0 or 1, written as a literal.
            let index = match args[0].kind {
                crate::ast::ExprKind::Int(1) => 1,
                _ => 0,
            };
            // Two `u64`s, laid out as C's `uint64_t[2]`.
            let words = self
                .context
                .struct_type(&[i64_type.into(), i64_type.into()], false);
            let key = match self.module.get_global(KEY) {
                Some(global) => global,
                None => {
                    let global = self.module.add_global(words, None, KEY);
                    global.set_linkage(Linkage::External);
                    global
                }
            };
            let at = self
                .builder
                .build_struct_gep(words, key.as_pointer_value(), index, "key")
                .expect("the key has two words");
            return Ok(self
                .builder
                .build_load(i64_type, at, "key")
                .expect(POSITIONED));
        }
        let s = self.get(&args[0], &Type::String)?.into_pointer_value();
        let from = self.get(&args[1], &Type::I64)?.into_int_value();
        let left = self
            .builder
            .build_int_sub(self.length(s), from, "left")
            .expect(POSITIONED);
        let eight = i64_type.const_int(8, false);
        let whole = self
            .builder
            .build_int_compare(inkwell::IntPredicate::SGE, left, eight, "whole")
            .expect(POSITIONED);
        let source = self.byte_address(s, from);
        let word = self.entry_alloca(&Type::U64, "chunk");
        let (full, tail, done) = (
            self.append("chunk.full"),
            self.append("chunk.tail"),
            self.append("chunk.done"),
        );
        self.builder
            .build_conditional_branch(whole, full, tail)
            .expect(POSITIONED);
        // Eight bytes or more left: one unaligned load.
        self.builder.position_at_end(full);
        let load = self
            .builder
            .build_load(i64_type, source, "chunk")
            .expect(POSITIONED);
        load.into_int_value()
            .as_instruction()
            .expect("a load is an instruction")
            .set_alignment(1)
            .expect("an alignment of 1 is valid");
        self.builder.build_store(word, load).expect(POSITIONED);
        self.branch(done);
        // Fewer: copied into a zeroed word.
        self.builder.position_at_end(tail);
        self.builder
            .build_store(word, i64_type.const_zero())
            .expect(POSITIONED);
        self.builder
            .build_memcpy(word, 1, source, 1, left)
            .expect(POSITIONED);
        self.branch(done);
        self.builder.position_at_end(done);
        Ok(self
            .builder
            .build_load(i64_type, word, "chunk")
            .expect(POSITIONED))
    }
}
