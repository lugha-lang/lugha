//! `mut` parameters: `fun bump(mut c: Counter)` changes its caller's
//! place, written `bump(mut c)`. The argument must be written `mut` exactly
//! where the parameter is (E0443), be a mutable place (E0501, E0502), and
//! be no other argument of the call, nor overlap one (E0508).

use super::assign::{PlaceUse, root_name};
use super::expr::Expect;
use super::generics::{Generic, subst};
use super::{Checker, Type, errors};
use crate::ast::{Expr, ExprKind, PassMode};
use crate::diagnostic::Diagnostic;

/// The arguments written `mut`, with their places, settled after the
/// call's type arguments are known.
pub(super) type MutArgs<'a> = Vec<(usize, &'a Expr, &'a Expr)>;

impl Checker {
    /// The inputs that infer a call whose parameters `muts` marks (empty:
    /// none): a `mut` argument infers through its place. E0443 for a `mut`
    /// written where the parameter isn't, or missing where it is.
    pub(super) fn mut_inputs<'a>(
        &mut self,
        muts: &[bool],
        args: &'a [Expr],
        params: &[Type],
    ) -> (Vec<(&'a Expr, Expect)>, MutArgs<'a>) {
        let mut inputs = Vec::new();
        let mut passed = Vec::new();
        for (i, (arg, ty)) in args.iter().zip(params).enumerate() {
            let wanted = muts.get(i).copied().unwrap_or(false);
            match (&arg.kind, wanted) {
                (ExprKind::Pass(PassMode::Mut, place), true) => {
                    inputs.push((&**place, Expect::of(ty.clone())));
                    passed.push((i, arg, &**place));
                }
                (ExprKind::Pass(_, inner), _) => {
                    self.report(super::out_params::misplaced(arg.span));
                    self.expr(inner, None);
                }
                (_, true) => {
                    let message = "this parameter is `mut`: the call changes the place you pass";
                    let help =
                        "write `mut x` for it, with `x` a `let mut` variable or a part of one";
                    self.report(Diagnostic::error("E0443", message, arg.span).with_help(help));
                    inputs.push((arg, Expect::of(ty.clone())));
                }
                (_, false) => inputs.push((arg, Expect::of(ty.clone()))),
            }
        }
        (inputs, passed)
    }

    /// After inference, each `mut` argument of a call with arguments
    /// `args`, at its parameter's type (`params` in `generics`, at
    /// `targs`): the place must be mutable and of exactly that type, and no
    /// other argument may be it or overlap it.
    pub(super) fn settle_mut(
        &mut self,
        passed: &MutArgs,
        args: &[Expr],
        (params, generics, targs): (&[Type], &[Generic], &[Type]),
    ) {
        for (i, arg, place) in passed {
            let want = subst(&params[*i], generics, targs);
            let found = self.types[place.id.0 as usize]
                .clone()
                .unwrap_or(Type::Error);
            // The function writes the place, so it can't be converted.
            if self.conversions.remove(&place.id).is_some() {
                self.report(errors::mismatch(
                    want.clone(),
                    found.clone(),
                    place.span,
                    None,
                ));
            }
            self.record(arg, found);
            if !self.check_place_for(place, PlaceUse::MutArg) {
                continue;
            }
            // Two `mut` arguments are one problem, reported from the first.
            let others: Vec<(&Expr, bool)> = args
                .iter()
                .enumerate()
                .filter(|(j, a)| j != i && !(j < i && matches!(a.kind, ExprKind::Pass(..))))
                .map(|(_, a)| (unwrap_pass(a), matches!(a.kind, ExprKind::Pass(..))))
                .collect();
            self.no_alias_mut(place, &others);
        }
    }

    /// E0508 unless no argument of `others` (each with whether it is
    /// written `mut`) is `place` or overlaps it: another `mut` argument with
    /// the same root, or a value of that root passed by pointer. A number
    /// or string read out of it is a fresh value.
    fn no_alias_mut(&mut self, place: &Expr, others: &[(&Expr, bool)]) {
        let Some(root) = root_name(place) else { return };
        for (other, is_mut) in others {
            if root_name(other) != Some(root) {
                continue;
            }
            if *is_mut || self.by_pointer(other) {
                self.report(errors::mut_arg_alias(root, place.span, other.span));
                return;
            }
        }
    }
}

impl Checker {
    /// E0508 for a `mut` argument rooted where the receiver `base` of a
    /// `mut self` call is: the method would change one place two ways.
    pub(super) fn mut_receiver_alias(&mut self, base: &Expr, args: &[Expr]) {
        let Some(root) = root_name(base) else { return };
        for arg in args {
            if let ExprKind::Pass(PassMode::Mut, place) = &arg.kind
                && root_name(place) == Some(root)
            {
                self.report(errors::mut_self_alias(root, place.span, base.span));
                return;
            }
        }
    }
}

/// The place of a `mut place` argument, or the argument itself.
pub(super) fn unwrap_pass(e: &Expr) -> &Expr {
    match &e.kind {
        ExprKind::Pass(_, inner) => inner,
        _ => e,
    }
}

/// Which of `params` are `mut`, empty when none is.
pub(super) fn muts_of(params: &[crate::ast::Param]) -> Vec<bool> {
    let muts: Vec<bool> = params
        .iter()
        .map(|p| matches!(p.mode, Some((crate::ast::PassMode::Mut, _))))
        .collect();
    if muts.contains(&true) {
        muts
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{errors, ok};

    const BUMP: &str = "struct P { x: i64, xs: i64[] }\n\
        fun bump(mut n: i64) { n += 1; }\n\
        fun grow(mut p: P) { p.xs.push(p.x); }\n\
        fun two(mut a: P, b: P) {}\n";

    fn codes(body: &str) -> Vec<&'static str> {
        errors(&format!("{BUMP}fun main() {{ {body} }}"))
            .into_iter()
            .map(|(c, _)| c)
            .collect()
    }

    #[test]
    fn a_mut_argument_is_a_mutable_place_of_the_parameters_type() {
        ok(&format!(
            "{BUMP}fun main() {{ let mut p = P {{ x: 1, xs: [] }}; bump(mut p.x); grow(mut p); \
             let mut ps = [p]; bump(mut ps[0].x); let q = p; two(mut p, q); }}"
        ));
        assert_eq!(codes("let n = 1; bump(mut n);"), ["E0501"]);
        assert_eq!(codes("let mut f = 1.5; bump(mut f);"), ["E0403"]);
    }

    #[test]
    fn mut_is_written_at_both_ends_or_neither() {
        assert_eq!(codes("let mut n = 1; bump(n);"), ["E0443"]);
        assert_eq!(
            codes("let mut p = P { x: 1, xs: [] }; let q = p; two(mut p, mut q);"),
            ["E0443"]
        );
    }

    #[test]
    fn a_place_is_one_argument_only() {
        let p = "let mut p = P { x: 1, xs: [] };";
        assert_eq!(codes(&format!("{p} two(mut p, p);")), ["E0508"]);
        // A number read out of it is a fresh value.
        ok(&format!(
            "{BUMP}fun main() {{ {p} two(mut p, P {{ x: p.x, xs: [] }}); }}"
        ));
    }

    #[test]
    fn a_mut_parameter_is_assignable_in_its_body() {
        ok("fun reset(mut n: i64) { n = 0; }\nfun main() {}");
        assert_eq!(
            errors("fun reset(n: i64) { n = 0; }\nfun main() {}"),
            [("E0501", "n")]
        );
    }
}
