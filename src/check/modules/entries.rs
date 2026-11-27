//! The checker's entry points: a program, a program's tests, and a shared
//! library's exports.

use super::{Unit, check_with};
use crate::check::Checked;
use crate::diagnostic::Diagnostic;

/// Type-checks a whole program of modules, the main file first.
///
/// # Errors
///
/// Every diagnostic found (errors and warnings) when there is at least one error.
pub fn check_modules(units: &[Unit]) -> Result<(Checked, Vec<Diagnostic>), Vec<Diagnostic>> {
    check_with(units, None, false)
}

/// [`check_modules`] for `lughac test`: the main file
/// needs no `main`, and [`Checked::tests`] lists its tests.
///
/// # Errors
///
/// As [`check_modules`].
pub fn check_tests(units: &[Unit]) -> Result<(Checked, Vec<Diagnostic>), Vec<Diagnostic>> {
    check_test_roots(units, 1)
}

/// [`check_tests`] for a program of several roots, the first `roots`
/// units: every root's tests are listed, in order.
///
/// # Errors
///
/// As [`check_modules`].
pub fn check_test_roots(
    units: &[Unit],
    roots: usize,
) -> Result<(Checked, Vec<Diagnostic>), Vec<Diagnostic>> {
    check_with(units, Some(roots), false)
}

/// [`check_modules`] for `lughac build --shared`: the main file has no
/// `main` (E0338), and at least one function C calls by name.
///
/// # Errors
///
/// As [`check_modules`].
pub fn check_shared(units: &[Unit]) -> Result<(Checked, Vec<Diagnostic>), Vec<Diagnostic>> {
    check_with(units, None, true)
}
