//! Building a program's LLVM module into IR text or an object file: the
//! entry points the driver calls, the optimisation level, the CPU level,
//! and the target machine.

use std::path::Path;

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target as LlvmTarget, TargetMachine,
    TargetTriple,
};

use super::{Arch, CodegenError, ModuleSource, SourceInfo, Target, frames, lower};
use crate::ast::Program;
use crate::check::{Checked, Unit};

/// What a build adds for measuring or checking the program, never for
/// shipping it; all off by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Instruments {
    /// Every function also pays the per-call costs of growable stacks and
    /// preemption (a stack check, the scheduling countdown), simulated for
    /// measuring them (`LUGHA_CALL_COSTS=1`).
    pub call_costs: bool,
    /// The owner check can run (`LUGHA_RC_CHECK=1`): writes to an array
    /// see a checked count of one as their own, rather than copying.
    pub rc_check: bool,
}

/// Optimisation level for the LLVM pass pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    /// `default<O0>` — the `lughac` default.
    O0,
    /// `default<O2>`.
    O2,
}

/// The x86-64 CPU level the program's code is built for. Only LLVM's code for the program follows it; the runtime and
/// `[link]` C files keep `cc`'s default, which is the baseline too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cpu {
    /// `x86-64`: every 64-bit Intel and AMD CPU. The default, so a binary
    /// runs on machines other than the one that built it.
    #[default]
    X86_64,
    /// `x86-64-v2`: adds SSE4.2 and `popcnt` (CPUs from about 2009).
    V2,
    /// `x86-64-v3`: adds AVX2 and FMA (about 2015).
    V3,
    /// `x86-64-v4`: adds AVX-512.
    V4,
    /// This machine's own CPU and features; the binary may not run elsewhere.
    Native,
}

/// Lowers a checked `program` to verified LLVM IR text.
///
/// # Errors
///
/// [`CodegenError::Verify`] if the generated IR is invalid.
pub fn emit_ir(
    program: &Program,
    checked: &Checked,
    source: &SourceInfo,
) -> Result<String, CodegenError> {
    emit_ir_modules(
        &[Unit {
            path: &[],
            program,
            imports: &[],
            ffi_denied: None,
            package: None,
            origin: None,
        }],
        checked,
        &[single(source)],
    )
}

/// Lowers a checked program of several modules (the main file first) to
/// verified LLVM IR text.
///
/// # Errors
///
/// As [`emit_ir`].
pub fn emit_ir_modules(
    units: &[Unit],
    checked: &Checked,
    sources: &[ModuleSource],
) -> Result<String, CodegenError> {
    emit_ir_for(units, checked, sources, Target::default())
}

/// [`emit_ir_for`] with `memory`'s way of managing memory.
///
/// # Errors
///
/// As [`emit_ir`].
pub fn emit_ir_with(
    units: &[Unit],
    checked: &Checked,
    sources: &[ModuleSource],
    (target, costs): (Target, Instruments),
) -> Result<String, CodegenError> {
    let context = Context::create();
    let module = lower::lower(&context, units, checked, (sources, target, costs, false))?;
    super::call_costs::insert_stack_checks(&context, &module);
    frames::share_all_slots(&module);
    Ok(module.print_to_string().to_string())
}

/// [`emit_ir_modules`] for `target`: extern structs
/// cross to C by that target's rules.
///
/// # Errors
///
/// As [`emit_ir`].
pub fn emit_ir_for(
    units: &[Unit],
    checked: &Checked,
    sources: &[ModuleSource],
    target: Target,
) -> Result<String, CodegenError> {
    emit_ir_with(units, checked, sources, (target, Instruments::default()))
}

/// A one-file program's source at offset 0.
fn single<'a>(source: &SourceInfo<'a>) -> ModuleSource<'a> {
    ModuleSource {
        name: source.name,
        base: 0,
        text: source.text,
    }
}

/// Lowers a checked `program`, optimises it at `opt` for `cpu` on `target`,
/// and writes a native object file to `path`.
///
/// # Errors
///
/// As [`emit_ir`], plus [`CodegenError::Emit`] if LLVM can't target the host
/// or write the file.
pub fn emit_object(
    program: &Program,
    checked: &Checked,
    source: &SourceInfo,
    opt: OptLevel,
    (cpu, target): (Cpu, Target),
    path: &Path,
) -> Result<(), CodegenError> {
    let units = [Unit {
        path: &[],
        program,
        imports: &[],
        ffi_denied: None,
        package: None,
        origin: None,
    }];
    emit_object_modules(
        &units,
        checked,
        &[single(source)],
        opt,
        (cpu, target, Instruments::default()),
        path,
    )
}

/// [`emit_object`] for a program of several modules, the main file first.
///
/// # Errors
///
/// As [`emit_object`].
pub fn emit_object_modules(
    units: &[Unit],
    checked: &Checked,
    sources: &[ModuleSource],
    opt: OptLevel,
    (cpu, target, costs): (Cpu, Target, Instruments),
    path: &Path,
) -> Result<(), CodegenError> {
    let context = Context::create();
    let release = opt == OptLevel::O2;
    let module = lower::lower(&context, units, checked, (sources, target, costs, release))?;
    let machine = target_machine(opt, cpu, target)?;
    module.set_triple(&machine.get_triple());
    module.set_data_layout(&machine.get_target_data().get_data_layout());
    let pipeline = match opt {
        OptLevel::O0 => "default<O0>",
        OptLevel::O2 => "default<O2>",
    };
    let emit_error = |e: inkwell::support::LLVMString| CodegenError::Emit(e.to_string());
    // Slots are shared before -O0's passes but after -O2's: the optimiser
    // works far harder on merged slots (kivuli's build: 33 s → 50 s), and
    // removes many slots itself.
    if opt == OptLevel::O0 {
        frames::share_all_slots(&module);
    }
    module
        .run_passes(pipeline, &machine, PassBuilderOptions::create())
        .map_err(emit_error)?;
    // Each function still standing checks the stack at its entry.
    super::call_costs::insert_stack_checks(&context, &module);
    if opt == OptLevel::O2 {
        frames::share_all_slots(&module);
    }
    machine
        .write_to_file(&module, FileType::Object, path)
        .map_err(emit_error)
}

/// A target machine for `target` at `cpu`'s level (x86-64; ARM builds for
/// its baseline, ARMv8-A), producing position-independent code because
/// Ubuntu's `cc` links PIE executables by default.
fn target_machine(opt: OptLevel, cpu: Cpu, target: Target) -> Result<TargetMachine, CodegenError> {
    let config = InitializationConfig::default();
    match target.arch() {
        Arch::X86_64 => LlvmTarget::initialize_x86(&config),
        Arch::Aarch64 => LlvmTarget::initialize_aarch64(&config),
    }
    let triple = TargetTriple::create(target.llvm_triple());
    let llvm = LlvmTarget::from_triple(&triple).map_err(|e| CodegenError::Emit(e.to_string()))?;
    let level = match opt {
        OptLevel::O0 => OptimizationLevel::None,
        OptLevel::O2 => OptimizationLevel::Default,
    };
    // A level is a CPU name whose features LLVM knows; only `native` adds
    // the host's own.
    let (cpu, features) = match (target, cpu) {
        // The driver refuses `--cpu` for ARM, so only the default reaches here.
        (t, _) if t.arch() == Arch::Aarch64 => ("generic".to_string(), String::new()),
        (_, Cpu::X86_64) => ("x86-64".to_string(), String::new()),
        (_, Cpu::V2) => ("x86-64-v2".to_string(), String::new()),
        (_, Cpu::V3) => ("x86-64-v3".to_string(), String::new()),
        (_, Cpu::V4) => ("x86-64-v4".to_string(), String::new()),
        (_, Cpu::Native) => (
            TargetMachine::get_host_cpu_name().to_string(),
            TargetMachine::get_host_cpu_features().to_string(),
        ),
    };
    let machine = llvm
        .create_target_machine(
            &triple,
            &cpu,
            &features,
            level,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| CodegenError::Emit(format!("LLVM has no target machine for {triple}")))?;
    // Codegen's scalar sizes (`split_leaves::byte_size`) assume 8-byte pointers.
    assert_eq!(
        machine.get_target_data().get_pointer_byte_size(None),
        8,
        "every target lughac knows is LP64"
    );
    Ok(machine)
}

#[cfg(test)]
mod tests {
    use super::{Cpu, OptLevel, Target, TargetMachine, target_machine};

    #[test]
    fn each_level_is_its_llvm_cpu_with_no_extra_features() {
        for (cpu, name) in [
            (Cpu::default(), "x86-64"),
            (Cpu::V2, "x86-64-v2"),
            (Cpu::V3, "x86-64-v3"),
            (Cpu::V4, "x86-64-v4"),
        ] {
            let machine =
                target_machine(OptLevel::O2, cpu, Target::default()).expect("x86-64 is a target");
            assert_eq!(machine.get_cpu().to_string(), name);
            assert_eq!(machine.get_feature_string().to_string_lossy(), "");
        }
    }

    #[test]
    fn native_is_the_hosts_cpu_and_features() {
        let machine = target_machine(OptLevel::O0, Cpu::Native, Target::default())
            .expect("the host is a target");
        assert_eq!(
            machine.get_cpu().to_string(),
            TargetMachine::get_host_cpu_name().to_string()
        );
        assert_eq!(
            machine.get_feature_string().to_string_lossy(),
            TargetMachine::get_host_cpu_features().to_string()
        );
    }

    #[test]
    fn each_target_has_its_triple_and_arm_builds_for_generic() {
        assert_eq!(Target::default(), Target::X86_64Linux);
        let all = [
            Target::X86_64Linux,
            Target::Aarch64Linux,
            Target::X86_64LinuxMusl,
            Target::Aarch64LinuxMusl,
        ];
        for target in all {
            let machine = target_machine(OptLevel::O2, Cpu::default(), target).expect("a target");
            assert_eq!(
                machine.get_triple().as_str().to_string_lossy(),
                target.triple()
            );
        }
        for arm in [Target::Aarch64Linux, Target::Aarch64LinuxMusl] {
            let arm = target_machine(OptLevel::O0, Cpu::default(), arm).unwrap();
            assert_eq!(arm.get_cpu().to_string(), "generic");
        }
    }
}
