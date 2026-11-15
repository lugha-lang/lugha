//! `@caller_location` in codegen. A marked function or method takes three
//! hidden trailing parameters, the place it was called from: the file
//! name (a C string), the line and the column. A call passes the call's
//! own place as constants, or, inside a marked function, the place that
//! function was given, so a chain of marked calls reports the first
//! unmarked caller. `panic(…)` inside a marked function reports it.

use inkwell::types::BasicMetadataTypeEnum;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue};

use super::lower::{Lowerer, POSITIONED};
use crate::check::Type;

impl<'ctx> Lowerer<'ctx> {
    /// The hidden parameters' types, after a marked function's own.
    pub(super) fn location_types(&self) -> [BasicMetadataTypeEnum<'ctx>; 3] {
        let ptr = self.context.ptr_type(Default::default());
        let i64_type = self.context.i64_type();
        [ptr.into(), i64_type.into(), i64_type.into()]
    }

    /// `types` with the hidden parameters added when `key` is marked.
    pub(super) fn located_types(
        &self,
        key: &str,
        mut types: Vec<BasicMetadataTypeEnum<'ctx>>,
    ) -> Vec<BasicMetadataTypeEnum<'ctx>> {
        if self.located.contains(key) {
            types.extend(self.location_types());
        }
        types
    }

    /// Records that `function`, declared for `key`, takes the hidden
    /// parameters when `key` is marked.
    pub(super) fn mark_located(&mut self, key: &str, function: FunctionValue<'ctx>) {
        if self.located.contains(key) {
            self.located_functions.insert(function);
        }
    }

    /// On entering `function`'s body: its hidden parameters, when it has
    /// them, become the place its calls and `panic` report.
    pub(super) fn enter_located(&mut self, function: FunctionValue<'ctx>) {
        if !self.located_functions.contains(&function) {
            return;
        }
        let count = function.count_params();
        let param = |n: u32| {
            function
                .get_nth_param(n)
                .expect("a marked function's place")
        };
        let place = [param(count - 3), param(count - 2), param(count - 1)];
        self.location = Some((function, place));
    }

    /// The place a call at source offset `at` passes to a marked callee:
    /// the current function's own place when it is marked, else `at`'s.
    fn place(&mut self, at: usize) -> [BasicValueEnum<'ctx>; 3] {
        if let Some((function, place)) = self.location
            && Some(function) == self.function
        {
            return place;
        }
        let file = self.file_name(at);
        let (line, col) = self.source.line_col(at);
        let i64_type = self.context.i64_type();
        [
            file.into(),
            i64_type.const_int(line as u64, false).into(),
            i64_type.const_int(col as u64, false).into(),
        ]
    }

    /// `values` with the place added when `function` is marked.
    pub(super) fn with_place(
        &mut self,
        function: FunctionValue<'ctx>,
        mut values: Vec<BasicMetadataValueEnum<'ctx>>,
        at: usize,
    ) -> Vec<BasicMetadataValueEnum<'ctx>> {
        if self.located_functions.contains(&function) {
            values.extend(self.place(at).map(BasicMetadataValueEnum::from));
        }
        values
    }

    /// `panic(message)` at source offset `at`, reporting the caller's place
    /// inside a marked function.
    pub(super) fn panic_located(&mut self, message: BasicValueEnum<'ctx>, at: usize) {
        let [file, line, col] = self.place(at);
        let panic = self.runtime(
            "lugha_rt_panic",
            &[Type::String, Type::String, Type::I64, Type::I64],
            None,
        );
        let args = [message.into(), file.into(), line.into(), col.into()];
        self.builder.build_call(panic, &args, "").expect(POSITIONED);
        self.builder.build_unreachable().expect(POSITIONED);
        self.start_dead_block("after.panic");
    }
}
