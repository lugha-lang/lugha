//! Codegen — lowers a checked program to LLVM IR and native object files.
//!
//! Every type comes from the checker's table; codegen never infers one.
//! Does not link — see `crate::link`.

mod actors;
mod arith;
mod array;
mod assertions;
mod bits;
mod c_abi;
mod c_abi_arm;
mod c_arrays;
mod c_calls;
mod c_stack;
mod call_costs;
mod callbacks;
mod cast;
mod channels;
mod closures;
mod concat;
mod conditional;
mod construct;
mod consts;
mod control;
mod conversions;
mod copy;
mod cow;
mod declare;
mod defaults;
mod derive;
mod derive_pairs;
mod dispatch;
mod emit;
mod enums;
mod equality;
mod expr;
mod externs;
mod float_ops;
mod format;
mod frames;
mod fun_interfaces;
mod fun_values;
mod function;
mod growth;
mod hashing;
mod heap;
mod hoist;
mod hoist_scan;
mod impls;
mod includes;
mod input;
mod instances;
mod int_checked;
mod int_methods;
mod interfaces;
mod invoke;
mod io;
mod iterators;
mod located;
mod lower;
mod maps;
mod matching;
mod methods;
mod net;
mod operators;
mod or_patterns;
mod ordering;
mod os;
mod out_params;
mod owning;
mod patterns;
mod places;
mod printing;
mod rc;
mod rc_at;
mod rc_objects;
mod rc_sites;
mod rc_slots;
mod rc_temps;
mod rebuild;
mod refs;
mod results;
mod runtime;
mod scope;
mod scope_failures;
mod select;
mod shields;
mod show_iface;
mod source;
mod spawned_callbacks;
mod split;
mod split_leaves;
mod split_params;
mod sret;
mod stmt;
mod structs;
mod swap;
mod targets;
mod task_handles;
mod task_scopes;
mod tasks;
mod test_main;
mod through_index;
mod tuples;
mod unwind;
mod upcast;
mod value;
mod views;

pub use self::emit::{
    Cpu, Instruments, OptLevel, emit_ir, emit_ir_for, emit_ir_modules, emit_ir_with, emit_object,
    emit_object_modules,
};
pub use self::targets::Target;

/// The source file being compiled, for panic locations.
#[derive(Debug, Clone, Copy)]
pub struct SourceInfo<'a> {
    /// The path as given on the command line.
    pub name: &'a str,
    /// The file's text.
    pub text: &'a str,
}

/// One module's source for panic locations: its name, where it starts in
/// the program's one offset space, and its text.
#[derive(Debug, Clone, Copy)]
pub struct ModuleSource<'a> {
    /// The name diagnostics use for the file.
    pub name: &'a str,
    /// The offset of its first byte.
    pub base: usize,
    /// The file's text.
    pub text: &'a str,
}

impl SourceInfo<'_> {
    /// 1-based line and byte column of `offset`.
    pub(crate) fn line_col(&self, offset: usize) -> (usize, usize) {
        let before = &self.text.as_bytes()[..offset.min(self.text.len())];
        let line_start = before
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        (
            before.iter().filter(|&&b| b == b'\n').count() + 1,
            before.len() - line_start + 1,
        )
    }
}

/// Why codegen could not produce output. None of these are the user's
/// mistake, so the driver reports them with exit code 2.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodegenError {
    /// The generated module is invalid — a compiler bug.
    #[error("generated LLVM IR failed verification (compiler bug): {0}")]
    Verify(String),
    /// LLVM could not set up the target, run passes, or write the object file.
    #[error("could not emit object code: {0}")]
    Emit(String),
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::emit_ir;
    use crate::{check, lexer, parser};

    /// The IR for `src`, which must lex, parse, type-check and lower, with
    /// the prelude as every program has it (`==` on a struct comes from
    /// it).
    /// Calls that unwind to a landing pad (`invoke`) read as plain calls:
    /// tests check what is called, `ir_raw` how.
    pub fn ir(src: &str) -> String {
        calls(ir_in(src, super::Instruments::default()))
    }

    /// `ir` (counting is how every program is lowered).
    pub fn ir_rc(src: &str) -> String {
        ir(src)
    }

    /// `ir`, with the simulated per-call costs.
    pub fn ir_calls(src: &str) -> String {
        calls(ir_in(
            src,
            super::Instruments {
                call_costs: true,
                ..Default::default()
            },
        ))
    }

    /// `ir`, built for the owner check.
    pub fn ir_rc_check(src: &str) -> String {
        calls(ir_in(
            src,
            super::Instruments {
                rc_check: true,
                ..Default::default()
            },
        ))
    }

    /// The IR for `src` exactly as lowered, invokes and all.
    pub fn ir_raw(src: &str) -> String {
        ir_in(src, super::Instruments::default())
    }

    /// `ir` with every `invoke` written as a `call`.
    fn calls(ir: String) -> String {
        ir.replace(" invoke ", " call ")
            .replace("= invoke ", "= call ")
    }

    fn ir_in(src: &str, costs: super::Instruments) -> String {
        let (tokens, _) = lexer::lex(src).expect("test source lexes");
        let (mut program, _) = parser::parse(&tokens).expect("test source parses");
        crate::modules::impls::name_impls(&mut program, "");
        let main = crate::modules::Module {
            path: Vec::new(),
            file: "test.la".into(),
            base: 0,
            text: src.to_string(),
            program: Some(program),
            package: None,
            folder: None,
            imports: Vec::new(),
            ffi_denied: None,
        };
        let mut modules = vec![main];
        let mut errors = Vec::new();
        crate::modules::with_prelude(&mut modules, &mut errors);
        assert!(errors.is_empty(), "the prelude parses: {errors:?}");
        let units: Vec<check::Unit> = modules
            .iter()
            .map(|m| check::Unit {
                path: &m.path,
                program: m.program.as_ref().expect("parsed"),
                imports: &m.imports,
                ffi_denied: m.ffi_denied.as_deref(),
                package: m.package.as_deref(),
                origin: check::Origin::of(m),
            })
            .collect();
        let (checked, _) =
            check::check_modules(&units).unwrap_or_else(|e| panic!("{src:?}: {e:?}"));
        let names: Vec<String> = modules
            .iter()
            .map(|m| m.file.to_string_lossy().into_owned())
            .collect();
        let sources: Vec<super::ModuleSource> = modules
            .iter()
            .zip(&names)
            .map(|(m, name)| super::ModuleSource {
                name,
                base: m.base,
                text: &m.text,
            })
            .collect();
        super::emit_ir_with(
            &units,
            &checked,
            &sources,
            (super::Target::default(), costs),
        )
        .unwrap_or_else(|e| panic!("{src:?}: {e}"))
    }

    /// The IR for `src` lowered for `target`.
    pub fn ir_for(src: &str, target: super::Target) -> String {
        let (tokens, _) = lexer::lex(src).expect("test source lexes");
        let (program, _) = parser::parse(&tokens).expect("test source parses");
        let (checked, _) = check::check(&program).unwrap_or_else(|e| panic!("{src:?}: {e:?}"));
        let unit = check::Unit {
            path: &[],
            program: &program,
            imports: &[],
            ffi_denied: None,
            package: None,
            origin: None,
        };
        let source = super::ModuleSource {
            name: "test.la",
            base: 0,
            text: src,
        };
        super::emit_ir_for(&[unit], &checked, &[source], target)
            .unwrap_or_else(|e| panic!("{src:?}: {e}"))
    }

    /// The IR of `src` built for `lughac test`.
    pub fn test_ir(src: &str) -> String {
        let (tokens, _) = lexer::lex(src).expect("test source lexes");
        let (program, _) = parser::parse(&tokens).expect("test source parses");
        let unit = check::Unit {
            path: &[],
            program: &program,
            imports: &[],
            ffi_denied: None,
            package: None,
            origin: None,
        };
        let (checked, _) = check::check_tests(&[unit]).unwrap_or_else(|e| panic!("{src:?}: {e:?}"));
        let source = super::SourceInfo {
            name: "test.la",
            text: src,
        };
        emit_ir(&program, &checked, &source).unwrap_or_else(|e| panic!("{src:?}: {e}"))
    }
}
