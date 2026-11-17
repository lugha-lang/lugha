//! The module: every function, then the C `main` that calls `lugha_fn_main`.

use std::collections::{HashMap, HashSet};

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, ValueKind};

use super::control::Loop;
use super::function::Signature;
use super::runtime::Constants;
use super::scope::Scopes;
use super::{CodegenError, ModuleSource, SourceInfo};
use crate::ast::{ExprId, Item};
use crate::check::{Checked, Enums, InterfaceShape, Iteration, Structs, Type, Unit};
use crate::span::Span;

/// Every builder call below happens after `position_at_end`, so a
/// `BuilderError` can only mean a compiler bug.
pub(super) const POSITIONED: &str = "builder is positioned in a block";

/// Lowering state for one module.
pub(super) struct Lowerer<'ctx> {
    /// The platform: how extern structs cross to C depends on it.
    pub(super) target: super::Target,
    /// Whether the per-call costs of growable stacks and preemption are
    /// simulated (`LUGHA_CALL_COSTS=1`).
    pub(super) costs: super::CallCosts,
    /// The slots the function being lowered releases.
    pub(super) rc_slots: super::rc_slots::RcSlots<'ctx>,
    /// Closure environment drops made so far, for their names.
    pub(super) rc_envs: usize,
    /// `out` arguments of the extern call being lowered.
    pub(super) outs: Vec<super::out_params::OutSlot<'ctx>>,
    pub(super) context: &'ctx Context,
    pub(super) module: Module<'ctx>,
    pub(super) builder: Builder<'ctx>,
    /// The checker's type for every expression, by `ExprId`.
    pub(super) types: Vec<Type>,
    /// Every struct's fields in declaration order.
    pub(super) structs: Structs,
    /// Every enum's variants and payload types in declaration order.
    pub(super) enums: Enums,
    /// Values read from a place for the last time, stored without a copy.
    pub(super) moves: HashSet<ExprId>,
    /// Kept parameters' last-use stores and hand-ons, which move only in
    /// an owning instance; arguments handed over; which parameters each
    /// key keeps (`codegen::owning`).
    pub(super) param_moves: HashSet<ExprId>,
    pub(super) param_hands: HashSet<ExprId>,
    pub(super) hands: HashSet<ExprId>,
    /// Per statement, the `let` locals nothing mentions after it.
    pub(super) dead_after: HashMap<crate::span::Span, Vec<String>>,
    /// The bodies of `match` arms whose bindings borrow (by span).
    pub(super) borrowing_arms: HashSet<crate::span::Span>,
    /// True while an arm whose bindings borrow is binding them.
    pub(super) borrowing: bool,
    /// The landing pads made so far (`codegen::unwind`).
    pub(super) pads: super::unwind::Pads<'ctx>,
    /// A failure lowering a pad's `defer`s, reported when lowering ends.
    pub(super) pad_error: Option<CodegenError>,
    /// While a direct call's arguments are lowered, the fresh ones it will
    /// release once it returns; `None` otherwise.
    pub(super) claims: Option<Vec<super::rc_temps::Claim<'ctx>>>,
    pub(super) keeps: HashMap<String, Vec<bool>>,
    /// Each keeping function's owning instance, by its ordinary one.
    pub(super) owning_instances:
        HashMap<inkwell::values::FunctionValue<'ctx>, super::owning::Owned<'ctx>>,
    /// True while lowering an owning instance's body.
    pub(super) owning: bool,
    /// Every declared function, by key (`geometry.origin`).
    pub(super) functions: HashMap<String, Signature<'ctx>>,
    /// The checker's key for what each call calls.
    pub(super) callees: HashMap<ExprId, String>,
    /// Under `lughac test`, the main file's tests to run, each with
    /// whether it is an integration test; `None` leaves every test out.
    pub(super) tests: Option<Vec<(String, bool)>>,
    /// The checker's type for every written type, by span.
    pub(super) annotations: HashMap<Span, Type>,
    /// Every method and associated function, by `Type.name`.
    pub(super) methods: HashMap<String, super::methods::Method<'ctx>>,
    /// Every concrete type the program uses (`Checked::used`).
    pub(super) used: Vec<Type>,
    /// The type arguments of each generic use (`Checked::instances`).
    pub(super) instances: HashMap<ExprId, Vec<Type>>,
    /// Every generic function and method instance, by `key<args>`.
    pub(super) monos: HashMap<String, super::methods::Method<'ctx>>,
    /// The type parameters of the instance being lowered, and their types.
    pub(super) subst: HashMap<String, Type>,
    /// What each closure literal captures (`Checked::captures`).
    pub(super) captures: HashMap<ExprId, Vec<String>>,
    /// Whether loops and functions count down to preemption (`Checked::uses_tasks`).
    pub(super) uses_tasks: bool,
    /// A spawned call's arguments already evaluated by the spawning thread:
    /// each lowers as a load of the hidden capture named here.
    pub(super) presets: HashMap<ExprId, String>,
    /// Each closure literal's function, once lowered: what an own type calls.
    pub(super) closure_fns: HashMap<ExprId, FunctionValue<'ctx>>,
    /// How externs with callbacks pass their parameters (`Checked::c_params`).
    pub(super) c_params: crate::check::CParams,
    /// C entries made so far, for `lugha_cfn_<n>`.
    pub(super) c_entries: usize,
    /// Variants used as function values (`Checked::variant_values`).
    pub(super) variant_values: HashMap<ExprId, (Type, String)>,
    /// Names reading a constant (`Checked::const_uses`).
    pub(super) const_uses: HashMap<ExprId, String>,
    /// Every include call's bytes (`Checked::includes`).
    pub(super) includes: HashMap<ExprId, crate::check::Included>,
    /// The global of each included `u8[]`, by its bytes' address.
    pub(super) include_globals: HashMap<usize, inkwell::values::PointerValue<'ctx>>,
    /// Every constant's global and type.
    pub(super) const_globals: HashMap<String, (inkwell::values::GlobalValue<'ctx>, Type)>,
    /// `lugha_const_init`, if the program has constants.
    pub(super) const_init: Option<FunctionValue<'ctx>>,
    /// Expressions converted to an interface value (`Checked::conversions`).
    pub(super) conversions: HashMap<ExprId, Type>,
    /// `?` operands whose error converts (`Checked::try_conversions`).
    pub(super) try_conversions: HashMap<ExprId, Type>,
    /// Places written through a type's `index` (`Checked::through_index`).
    pub(super) through_index: std::collections::HashSet<ExprId>,
    /// The chosen calls of brace literals built as collections
    /// (`Checked::collections`).
    pub(super) collections: std::collections::HashSet<ExprId>,
    /// See `Checked::impls`.
    pub(super) impls: crate::check::Impls,
    /// See `Checked::nominal_calls`.
    pub(super) nominal_calls: HashMap<ExprId, (String, Vec<Type>)>,
    /// See `Checked::qualified`.
    pub(super) qualified: std::collections::HashSet<ExprId>,
    /// `impl f64 { … }` blocks' types (`Checked::impl_owners`).
    pub(super) impl_owners: HashMap<crate::span::Span, Type>,
    /// A generic interface's arguments at a call that may reach its
    /// default (`Checked::default_args`).
    pub(super) default_args: HashMap<ExprId, Vec<Type>>,
    /// Every derive block (`Checked::derives`).
    pub(super) derives: crate::check::Derives,
    /// Calls through a derivable interface's bound (`Checked::derived`).
    pub(super) derived: HashMap<ExprId, String>,
    /// Field intrinsic calls (`Checked::derive_calls`).
    pub(super) derive_calls: HashMap<ExprId, String>,
    /// Iterator loops (`Checked::iterations`).
    pub(super) iterations: HashMap<ExprId, Iteration>,
    /// Dispatch tables and conversions between interfaces.
    pub(super) upcasts: super::upcast::Upcasts<'ctx>,
    /// Interface show helpers whose bodies `finish_shows` builds.
    pub(super) shows: Vec<(Type, inkwell::values::FunctionValue<'ctx>)>,
    /// Every interface's methods in order (`Checked::interfaces`).
    pub(super) interfaces: HashMap<String, InterfaceShape>,
    /// Closures emitted so far per enclosing symbol, for `lugha_closure_<f>.<n>`.
    pub(super) closure_counts: HashMap<String, usize>,
    /// The function being emitted, for appending basic blocks.
    pub(super) function: Option<FunctionValue<'ctx>>,
    /// Its return type; `None` for a void function.
    pub(super) ret: Option<Type>,
    /// Local variables in scope.
    pub(super) scopes: Scopes<'ctx>,
    /// `for x of mut xs` variables: each names the element `xs[k]`,
    /// keyed by its placeholder slot (`codegen::places`).
    pub(super) aliases:
        std::collections::HashMap<inkwell::values::PointerValue<'ctx>, super::places::Alias<'ctx>>,
    /// Arrays made unique before the enclosing loops (`codegen::hoist`): the
    /// root variable's slot and the path from it.
    pub(super) owned: Vec<(inkwell::values::PointerValue<'ctx>, crate::ast::Expr)>,
    /// Enclosing loops, innermost last, for `break` and `continue`.
    pub(super) loops: Vec<Loop<'ctx>>,
    /// Labels of `while` and `for` statements, by their body's start,
    /// until the loop is entered (`push_loop`).
    pub(super) loop_labels: std::collections::HashMap<usize, String>,
    /// Per enclosing block, innermost last, the `defer` statements reached
    /// so far (`codegen::control::defer`).
    pub(super) defers: Vec<Vec<super::control::defer::Deferred<'ctx>>>,
    /// String literals and the file name, emitted once each.
    pub(super) constants: Constants<'ctx>,
    /// Every module's source, for panic locations.
    pub(super) source: OwnedSource,
    /// The keys of `@caller_location` functions (`Checked::located`).
    pub(super) located: std::collections::HashSet<String>,
    /// Their declarations, which take the hidden place parameters.
    pub(super) located_functions: std::collections::HashSet<FunctionValue<'ctx>>,
    /// The marked function being lowered, with its place parameters.
    pub(super) location: Option<(FunctionValue<'ctx>, [BasicValueEnum<'ctx>; 3])>,
}

/// One module's source, owned so the lowerer has no extra lifetime.
pub(super) struct OwnedFile {
    pub name: String,
    base: usize,
    text: String,
}

/// Every module's source, in offset order.
pub(super) struct OwnedSource {
    pub files: Vec<OwnedFile>,
}

impl OwnedSource {
    /// The file holding `offset`, and `offset` within it.
    pub(super) fn file_at(&self, offset: usize) -> (&OwnedFile, usize) {
        let file = self
            .files
            .iter()
            .rev()
            .find(|f| f.base <= offset)
            .unwrap_or(&self.files[0]);
        (file, offset - file.base.min(offset))
    }

    /// The source text from `start` to `end`, as written.
    pub(super) fn written(&self, start: usize, end: usize) -> &str {
        let (file, local) = self.file_at(start);
        let end = (local + end.saturating_sub(start)).min(file.text.len());
        file.text.get(local.min(end)..end).unwrap_or("")
    }

    pub(super) fn line_col(&self, offset: usize) -> (usize, usize) {
        let (file, local) = self.file_at(offset);
        SourceInfo {
            name: &file.name,
            text: &file.text,
        }
        .line_col(local)
    }
}

/// Lowers a checked program of modules into one verified LLVM module for `target`.
pub(super) fn lower<'ctx>(
    context: &'ctx Context,
    units: &[Unit],
    checked: &Checked,
    (sources, target, costs): (&[ModuleSource], super::Target, super::CallCosts),
) -> Result<Module<'ctx>, CodegenError> {
    let mut lowerer = Lowerer {
        target,
        costs,
        rc_slots: Default::default(),
        rc_envs: 0,
        context,
        module: context.create_module("lugha"),
        builder: context.create_builder(),
        types: checked.types.clone(),
        structs: checked.structs.clone(),
        enums: checked.enums.clone(),
        moves: checked.moves.clone(),
        param_moves: checked.param_moves.clone(),
        param_hands: checked.param_hands.clone(),
        hands: checked.hands.clone(),
        dead_after: checked.dead_after.clone(),
        borrowing_arms: checked.borrowing_arms.clone(),
        borrowing: false,
        claims: None,
        pads: Default::default(),
        pad_error: None,
        keeps: checked.keeps.clone(),
        owning_instances: HashMap::new(),
        owning: false,
        functions: HashMap::new(),
        callees: checked.callees.clone(),
        annotations: checked.annotations.clone(),
        methods: HashMap::new(),
        used: checked.used.clone(),
        instances: checked.instances.clone(),
        monos: HashMap::new(),
        outs: Vec::new(),
        subst: HashMap::new(),
        captures: checked.captures.clone(),
        uses_tasks: checked.uses_tasks,
        presets: HashMap::new(),
        closure_fns: HashMap::new(),
        c_params: checked.c_params.clone(),
        c_entries: 0,
        variant_values: checked.variant_values.clone(),
        const_uses: checked.const_uses.clone(),
        const_globals: HashMap::new(),
        includes: checked.includes.clone(),
        include_globals: HashMap::new(),
        const_init: None,
        conversions: checked.conversions.clone(),
        interfaces: checked.interfaces.clone(),
        try_conversions: checked.try_conversions.clone(),
        through_index: checked.through_index.clone(),
        collections: checked.collections.clone(),
        impls: checked.impls.clone(),
        nominal_calls: checked.nominal_calls.clone(),
        qualified: checked.qualified.clone(),
        impl_owners: checked.impl_owners.clone(),
        default_args: checked.default_args.clone(),
        derives: checked.derives.clone(),
        derived: checked.derived.clone(),
        derive_calls: checked.derive_calls.clone(),
        iterations: checked.iterations.clone(),
        tests: checked.tests.as_ref().map(|tests| {
            tests
                .iter()
                .map(|t| (t.clone(), checked.integration.contains(t)))
                .collect()
        }),
        upcasts: Default::default(),
        shows: Vec::new(),
        closure_counts: HashMap::new(),
        function: None,
        ret: None,
        scopes: Scopes::default(),
        aliases: std::collections::HashMap::new(),
        owned: Vec::new(),
        loops: Vec::new(),
        loop_labels: std::collections::HashMap::new(),
        defers: Vec::new(),
        constants: Constants::default(),
        source: OwnedSource {
            files: sources
                .iter()
                .map(|s| OwnedFile {
                    name: s.name.to_string(),
                    base: s.base,
                    text: s.text.to_string(),
                })
                .collect(),
        },
        located: checked.located.clone(),
        located_functions: std::collections::HashSet::new(),
        location: None,
    };
    lowerer.declare_enums();
    lowerer.declare_structs();
    for unit in units {
        lowerer.declare_functions(unit)?;
    }
    for unit in units {
        lowerer.declare_methods(unit);
    }
    lowerer.declare_instances(units, &checked.monos);
    lowerer.declare_consts(&checked.consts);
    for unit in units {
        for item in &unit.program.items {
            if let Item::Fun(f) = item
                && f.type_params.is_empty()
                && !super::declare::is_lazy(unit)
                && (f.test.is_none() || lowerer.tests.is_some())
            {
                lowerer.define(unit.path, f)?;
            }
        }
    }
    for unit in units {
        lowerer.define_methods(unit)?;
    }
    lowerer.define_instances(units, &checked.monos)?;
    lowerer.const_init = lowerer.define_const_init(units, &checked.consts)?;
    lowerer.finish_upcasts();
    lowerer.finish_shows();
    match lowerer.tests.clone() {
        Some(tests) => lowerer.c_test_main(&tests),
        None => {
            let user_main = lowerer
                .functions
                .get("main")
                .expect("checked: E0303 requires `main`");
            let (function, params) = (user_main.function, user_main.params.clone());
            lowerer.c_main(function, &params);
        }
    }
    // Struct results through a hidden pointer.
    super::sret::results_through_pointers(context, &lowerer.module);
    // Struct values split into their fields.
    super::split::split_structs(context, &lowerer.module);
    if let Some(e) = lowerer.pad_error.take() {
        return Err(e);
    }
    // Calls with cleanup to run on a panic become invokes of their pads.
    lowerer.invokes();
    // Stack probes; the caller shares slots, before or
    // after optimising (`codegen::frames`).
    super::frames::probe_stacks(context, &lowerer.module);
    lowerer
        .module
        .verify()
        .map_err(|e| CodegenError::Verify(e.to_string()))?;
    Ok(lowerer.module)
}

impl<'ctx> Lowerer<'ctx> {
    /// Emits the C entry point: returns `lugha_fn_main`'s `i32`, or 0 for a
    /// void `main`.
    fn c_main(&mut self, user_main: FunctionValue<'ctx>, params: &[Type]) {
        let i32_type = self.context.i32_type();
        // `main(int argc, char **argv)`: the runtime keeps both for `args()`.
        let ptr = self.context.ptr_type(Default::default());
        let main_type = i32_type.fn_type(&[i32_type.into(), ptr.into()], false);
        let main = self.module.add_function("main", main_type, None);
        self.builder
            .position_at_end(self.context.append_basic_block(main, "entry"));
        let argc = main.get_nth_param(0).expect("main takes argc");
        let argv = main.get_nth_param(1).expect("main takes argv");
        let init = self.runtime("lugha_rt_init", &[Type::I32, Type::String], None);
        let init_args = [argc.into(), argv.into()];
        self.builder
            .build_call(init, &init_args, "")
            .expect(POSITIONED);
        self.call_const_init();
        // What `main`'s parameters ask for, made once the runtime is up.
        let args = self.entry_arguments(params, crate::check::Entry::Main);
        self.lock_down(params);
        let call = self
            .builder
            .build_call(user_main, &args, "result")
            .expect(POSITIONED);
        // A panic in `main` lets go of its arguments too, once unwound.
        self.unwind_entry((main, call), |this| this.main_unwound(params, &args));
        self.root_scope_finish();
        self.rc_after_entry(params, &args);
        let code = match call.try_as_basic_value() {
            ValueKind::Basic(value) => value.into_int_value(),
            ValueKind::Instruction(_) => i32_type.const_zero(),
        };
        self.builder.build_return(Some(&code)).expect(POSITIONED);
    }
}

impl Lowerer<'_> {
    /// Fills every constant, after the runtime starts and before any test or
    /// `main` runs.
    pub(super) fn call_const_init(&mut self) {
        if let Some(init) = self.const_init {
            self.builder.build_call(init, &[], "").expect(POSITIONED);
        }
    }
}

impl<'ctx> Lowerer<'ctx> {
    /// Limits the process to the capability kinds in `params`, once they
    /// are made and before any of the program's code runs.
    pub(super) fn lock_down(&mut self, params: &[Type]) {
        let kinds = crate::check::lockdown_kinds(params);
        let function = self.runtime("lugha_rt_lockdown", &[Type::I64], None);
        let value = self.context.i64_type().const_int(kinds as u64, false);
        self.builder
            .build_call(function, &[value.into()], "")
            .expect(POSITIONED);
    }

    /// One value per parameter in `params`, in order: the arguments array
    /// for `string[]`, and for each capability the one `entry` is given
    /// (a test's stand-ins, or the real ones).
    pub(super) fn entry_arguments(
        &mut self,
        params: &[Type],
        entry: crate::check::Entry,
    ) -> Vec<BasicMetadataValueEnum<'ctx>> {
        let mut values = Vec::with_capacity(params.len());
        for param in params {
            let value = match param {
                Type::Array(_) => self.call_runtime_value("lugha_rt_args", &[], &[]),
                Type::Interface(key, _) | Type::Struct(key, _) => {
                    let make = crate::check::maker(key, entry)
                        .expect("checked: an entry takes only capabilities it can be given");
                    let function = self
                        .monos
                        .get(make)
                        .or_else(|| self.methods.get(make))
                        .unwrap_or_else(|| panic!("checked: `{make}` is listed"))
                        .signature
                        .function;
                    let site = self
                        .builder
                        .build_call(function, &[], "capability")
                        .expect(POSITIONED);
                    match site.try_as_basic_value() {
                        ValueKind::Basic(value) => value,
                        ValueKind::Instruction(_) => unreachable!("it returns the capability"),
                    }
                }
                _ => unreachable!("checked: `main` takes only what the runtime supplies"),
            };
            values.push(self.entry_passed(param, value));
        }
        values
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::ir;

    #[test]
    fn main_returns_a_real_i32() {
        let ir = ir("fun main(): i32 { 2 + 3 * 4 }");
        assert!(ir.contains("define i32 @lugha_fn_main()"), "{ir}");
        assert!(ir.contains("define i32 @main(i32 %0, ptr %1)"), "{ir}");
        assert!(ir.contains("call i32 @lugha_fn_main()"), "{ir}");
        assert!(!ir.contains("trunc"), "no truncation in the C main: {ir}");
    }

    #[test]
    fn void_main_exits_with_zero() {
        let ir = ir("fun main() { }");
        assert!(ir.contains("define void @lugha_fn_main()"), "{ir}");
        assert!(ir.contains("ret i32 0"), "{ir}");
    }
}
