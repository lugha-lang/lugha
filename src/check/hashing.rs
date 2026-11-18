//! The `$hash` built-ins the prelude's `Hasher` uses: `$hash.key(i)`, word
//! `i` (a literal 0 or 1) of the process's secret key, and `$hash.chunk(s, i)`, a
//! `u64` of string `s`'s bytes from `i`, eight or fewer. Everything else
//! hashes in Lugha. Only bundled sources can name them.

use super::expr::Expect;
use super::float_ops::builtin_op;
use super::{Checker, Type};
use crate::ast::{Expr, ExprKind};
use crate::diagnostic::Diagnostic;

/// The `$hash` built-in `callee` names, `key` for `$hash.key`.
pub fn hash_op(callee: &Expr) -> Option<&str> {
    builtin_op(callee, "$hash")
}

impl Checker {
    /// `$hash.key(i)`: an `i64`; a `u64`. `$hash.chunk(s, i)`: a `string`
    /// and an `i64`; a `u64`.
    pub(super) fn hash_call(
        &mut self,
        call: &Expr,
        (callee, op): (&Expr, &str),
        args: &[Expr],
    ) -> Type {
        self.record(callee, Type::Unit);
        if let ExprKind::Field(base, _) = &callee.kind {
            self.record(base, Type::Unit);
        }
        let params = match (op, args.len()) {
            ("key", 1) => vec![Type::I64],
            ("chunk", 2) => vec![Type::String, Type::I64],
            _ => {
                // A compiler bug: only the prelude writes `$hash`.
                let d = Diagnostic::error(
                    "E0415",
                    format!("no `$hash` built-in `{op}` with these arguments"),
                    call.span,
                );
                self.report(d);
                return self.unchecked_args(args);
            }
        };
        for (arg, ty) in args.iter().zip(params) {
            self.value(arg, Some(&Expect::of(ty)));
        }
        Type::U64
    }
}
