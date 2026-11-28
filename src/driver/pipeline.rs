//! The compile pipeline behind each command: read → lex → parse → codegen → link.
//!
//! Every stage stops the pipeline on error. Failures come back as
//! `Failure::Program` (exit 1) or `Failure::Internal` (exit 2), each with the
//! source needed to render them.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::render::Report;
use super::source::Source;
use super::update;

pub(super) use super::linking::{build, build_shared, default_output, shared_output};
use crate::check::{self, Checked};
use crate::codegen::{self, CodegenError, Cpu, Instruments, OptLevel, Target};
use crate::diagnostic::{Diagnostic, Severity};
use crate::modules::fetch::Mode;
use crate::modules::link_table::PackageLink;
use crate::modules::{self, Loaded, Module};

/// Why a command stopped.
pub(super) enum Failure {
    /// Errors in the user's program — exit 1.
    Program(Source, Vec<Diagnostic>),
    /// An unreadable file or a compiler limitation or bug — exit 2.
    /// The report is boxed: it is large and this path is rare.
    Internal(Source, Box<Report>),
}

/// A program whose modules all loaded and parsed, with any warnings.
pub(super) struct Front {
    pub source: Source,
    /// Every module, the main file first.
    pub modules: Vec<Module>,
    pub warnings: Vec<Diagnostic>,
    /// The checker's output; `None` for the parse-only front end (`--emit=ast`).
    pub checked: Option<Checked>,
    /// The `[link]` tables of the program's packages.
    pub links: Vec<PackageLink>,
}

/// Loads `path` and every module it imports, fetching git packages as
/// `mode` allows and writing a changed `lugha.lock`. A file that
/// can't be read is internal; mistakes in the program come back as diagnostics.
pub(super) fn loaded(path: &Path, mode: Mode) -> Result<(Source, Loaded), Failure> {
    let mut read = |file: &Path| std::fs::read(file);
    loaded_with(path, mode, &mut read, (true, false, false))
}

/// `loaded` reading files through `read`, writing `lugha.lock` only if
/// `write_lock`, and keeping modules' recovered programs if `partial`: the
/// language server reads open documents' unsaved text, never writes, and
/// checks text with syntax errors.
/// With `as_module`, a file in its package's `source` folder is loaded as
/// that package's module.
pub(super) fn loaded_with(
    path: &Path,
    mode: Mode,
    read: &mut modules::Read<'_>,
    flags: (bool, bool, bool),
) -> Result<(Source, Loaded), Failure> {
    loaded_all(&[path.to_path_buf()], mode, read, flags)
}

/// [`loaded_with`] for several roots in one folder, loaded as one program,
/// such as a package folder's test files.
fn loaded_all(
    paths: &[PathBuf],
    mode: Mode,
    read: &mut modules::Read<'_>,
    (write_lock, partial, as_module): (bool, bool, bool),
) -> Result<(Source, Loaded), Failure> {
    let path = paths.first().expect("a load has a root").as_path();
    let name = path.display().to_string();
    let env = modules::layout::LoadEnv {
        partial,
        as_module,
        ..update::env(mode)
    };
    match modules::load_all(paths, read, env) {
        Ok(loaded) => {
            let source = Source::of(&name, &loaded);
            if let (Some(text), true) = (&loaded.lock, write_lock) {
                update::write_lock(&loaded.lock_folder, text, &source)?;
            }
            Ok((source, loaded))
        }
        Err(error) => {
            let report = Report::internal(error.to_string(), None);
            Err(Failure::Internal(
                Source::single(name, ""),
                Box::new(report),
            ))
        }
    }
}

/// Loads, lexes and parses every module — everything `--emit=ast` needs.
pub(super) fn parsed(path: &Path, mode: Mode) -> Result<Front, Failure> {
    parse_loaded(loaded(path, mode)?)
}

fn parse_loaded((source, loaded): (Source, Loaded)) -> Result<Front, Failure> {
    if loaded
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err(Failure::Program(source, loaded.diagnostics));
    }
    Ok(Front {
        source,
        modules: loaded.modules,
        warnings: loaded.diagnostics,
        checked: None,
        links: loaded.links,
    })
}

/// Reads, lexes, parses and type-checks `path`.
pub(super) fn front(path: &Path, mode: Mode) -> Result<Front, Failure> {
    check_parsed(parsed(path, mode)?, Checking::Program)
}

/// [`front`] for `build --shared`: no `main`, and the functions C calls.
pub(super) fn front_shared(path: &Path, mode: Mode) -> Result<Front, Failure> {
    check_parsed(parsed(path, mode)?, Checking::Shared)
}

/// [`parsed`] for `lughac test` and `lughac check`: a file in its package's
/// `source` folder is that package's module.
fn parsed_module(path: &Path, mode: Mode) -> Result<Front, Failure> {
    let mut read = |file: &Path| std::fs::read(file);
    parse_loaded(loaded_with(path, mode, &mut read, (true, false, true))?)
}

/// [`front`] for `lughac test`: no `main` needed, and the
/// checker lists the file's tests.
pub(super) fn front_tests(path: &Path, mode: Mode) -> Result<Front, Failure> {
    check_parsed(parsed_module(path, mode)?, Checking::Tests(1))
}

/// [`front_tests`] for several test files of one package folder, checked
/// and listed as one program.
pub(super) fn front_test_roots(paths: &[PathBuf], mode: Mode) -> Result<Front, Failure> {
    let mut read = |file: &Path| std::fs::read(file);
    let loaded = loaded_all(paths, mode, &mut read, (true, false, true))?;
    check_parsed(parse_loaded(loaded)?, Checking::Tests(paths.len()))
}

/// [`front`] for `lughac check`: a file without `fun main` is checked as a
/// library, as `lughac test` checks it; one with it is a main file, as
/// `build` and `run` take it, even inside its package's `source` folder.
pub(super) fn front_check(path: &Path, mode: Mode) -> Result<Front, Failure> {
    // An unreadable file is reported by the load below.
    if std::fs::read(path).is_ok_and(|text| modules::declares_main(&text)) {
        return front(path, mode);
    }
    check_parsed(parsed_module(path, mode)?, Checking::Tests(1))
}

/// A program checked for the language server: what the checker found,
/// errors or not, with the modules and files it came from.
pub(super) struct PartialFront {
    pub source: Source,
    /// Every module, the main file first and the prelude last.
    pub modules: Vec<Module>,
    /// The loader's diagnostics: its warnings, and the syntax errors of a
    /// program checked partially; the checker's are in `partial`.
    pub warnings: Vec<Diagnostic>,
    pub partial: check::Partial,
}

/// Reads through `read` (never writing `lugha.lock`), lexes, parses and
/// checks `path` for the language server, keeping the checker's findings
/// even when the program has errors. `as_module`
/// loads it as a module, so other modules may import it.
pub(super) fn front_partial(
    path: &Path,
    mode: Mode,
    read: &mut modules::Read<'_>,
    as_module: bool,
) -> Result<PartialFront, Failure> {
    let (source, loaded) = loaded_with(path, mode, read, (false, true, as_module))?;
    // A module that didn't lex has no program: nothing to check.
    if loaded.modules.iter().any(|m| m.program.is_none()) {
        return Err(Failure::Program(source, loaded.diagnostics));
    }
    // Past every file loaded, manifests too: the loader's diagnostics stay
    // in the files they name.
    let end = loaded
        .manifests
        .iter()
        .map(|m| (m.base, &m.text))
        .chain(loaded.modules.iter().map(|m| (m.base, &m.text)))
        .map(|(base, text)| base + text.len() + 1)
        .max()
        .unwrap_or(0);
    let mut front = Front {
        source,
        modules: loaded.modules,
        warnings: loaded.diagnostics,
        checked: None,
        links: loaded.links,
    };
    let mut prelude_errors = Vec::new();
    modules::with_prelude_at(&mut front.modules, &mut prelude_errors, end);
    let prelude = front.modules.last().expect("the prelude was just added");
    front.source.add("<prelude>", prelude.base, &prelude.text);
    let start = prelude.base;
    if !prelude_errors.is_empty() {
        return Err(prelude_failure(&front.source, &prelude_errors));
    }
    let partial = check::check_partial(&units(&front.modules));
    // A mistake inside the prelude is the compiler's, not the program's.
    let in_prelude = partial
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.span.start >= start);
    if in_prelude {
        return Err(prelude_failure(&front.source, &partial.diagnostics));
    }
    Ok(PartialFront {
        source: front.source,
        modules: front.modules,
        warnings: front.warnings,
        partial,
    })
}

/// How a parsed program is checked: as a program, as tests (listing those
/// of that many first modules), or as a shared library.
enum Checking {
    Program,
    Tests(usize),
    Shared,
}

/// Checks a parsed program `how` it is to be built.
fn check_parsed(mut front: Front, how: Checking) -> Result<Front, Failure> {
    // The prelude's array helpers are checked and compiled with every program.
    let mut prelude_errors = Vec::new();
    // Past every file loaded, manifests too: a link diagnostic points
    // into a `lugha.toml` after the checker ran, and must not land in the prelude.
    let end = front
        .source
        .files
        .iter()
        .map(|f| f.base + f.text.len() + 1)
        .max()
        .unwrap_or(0);
    modules::with_prelude_at(&mut front.modules, &mut prelude_errors, end);
    let prelude = front.modules.last().expect("the prelude was just added");
    front.source.add("<prelude>", prelude.base, &prelude.text);
    let start = prelude.base;
    let units = units(&front.modules);
    let result = match how {
        Checking::Tests(roots) => check::check_test_roots(&units, roots),
        Checking::Shared => check::check_shared(&units),
        Checking::Program => check::check_modules(&units),
    };
    match result {
        Ok((checked, warnings)) if prelude_errors.is_empty() => {
            front.warnings.extend(warnings);
            front.checked = Some(checked);
            Ok(front)
        }
        Ok(_) => Err(prelude_failure(&front.source, &prelude_errors)),
        // A mistake inside the prelude is the compiler's, not the program's.
        Err(errors) => match errors.iter().find(|d| d.span.start >= start) {
            Some(_) => Err(prelude_failure(&front.source, &errors)),
            None => Err(Failure::Program(front.source, errors)),
        },
    }
}

/// An internal error for a diagnostic inside the prelude: a compiler bug.
fn prelude_failure(source: &Source, errors: &[Diagnostic]) -> Failure {
    let first = errors.first().map_or("an error", |d| d.message.as_str());
    internal(source, format!("the prelude doesn't check: {first}"))
}

/// The program's LLVM IR for `target`, for `--emit=ir`.
pub(super) fn ir(front: &Front, (target, costs): (Target, Instruments)) -> Result<String, Failure> {
    let units = units(&front.modules);
    codegen::emit_ir_with(&units, checked(front), &sources(front), (target, costs))
        .map_err(|e| codegen_failure(&front.source, e))
}

/// Builds `front` into a temp dir and runs it with inherited standard
/// streams through `runner`, passing `args` to the program.
///
/// Returns the program's exit code, or 128 + N if signal N killed it, as a shell reports it.
pub(super) fn run(
    front: &Front,
    code: (OptLevel, Cpu, Target, Instruments),
    runner: &super::emulate::Runner,
    args: &[String],
) -> Result<u8, Failure> {
    let temp = TempDir::new().map_err(|e| internal(&front.source, temp_error(e)))?;
    let exe = temp.path.join("prog");
    build(front, code, &exe)?;
    let status = runner.command(&exe).args(args).status().map_err(|e| {
        internal(
            &front.source,
            format!("cannot run the compiled program: {e}"),
        )
    })?;
    Ok(exit_code(status))
}

/// A finished program's exit code as a shell reports it: its own, or
/// 128 + N if signal N killed it.
#[cfg(unix)]
fn exit_code(status: std::process::ExitStatus) -> u8 {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        // Exit codes are already 0–255 on Unix; truncation is a no-op.
        (Some(code), _) => code as u8,
        (None, Some(signal)) => 128u8.wrapping_add(signal as u8),
        (None, None) => 1,
    }
}

/// A finished program's exit code: Windows has no signals, and its codes
/// are 32 bits wide, of which the low 8 are kept, as a shell's `$?` does.
#[cfg(not(unix))]
fn exit_code(status: std::process::ExitStatus) -> u8 {
    status.code().map_or(1, |code| code as u8)
}

/// The source name and text codegen needs for panic locations.
/// Every module's file, for panic locations in any of them.
pub(super) fn sources(front: &Front) -> Vec<codegen::ModuleSource<'_>> {
    front
        .source
        .files
        .iter()
        .map(|f| codegen::ModuleSource {
            name: &f.name,
            base: f.base,
            text: &f.text,
        })
        .collect()
}

/// The modules as the checker and codegen take them, the main file first.
pub(super) fn units(modules: &[Module]) -> Vec<check::Unit<'_>> {
    modules
        .iter()
        .map(|m| check::Unit {
            path: &m.path,
            program: m.program.as_ref().expect("every module parsed"),
            imports: &m.imports,
            ffi_denied: m.ffi_denied.as_deref(),
            package: m.package.as_deref(),
            origin: check::Origin::of(m),
        })
        .collect()
}

/// The type table of a front end that ran the checker.
pub(super) fn checked(front: &Front) -> &Checked {
    front
        .checked
        .as_ref()
        .expect("codegen runs only after `front()` type-checked")
}

pub(super) fn codegen_failure(source: &Source, error: CodegenError) -> Failure {
    // Codegen errors are compiler or LLVM failures, with no source location.
    Failure::Internal(
        source.clone(),
        Box::new(Report::internal(error.to_string(), None)),
    )
}

pub(super) fn internal(source: &Source, message: String) -> Failure {
    Failure::Internal(source.clone(), Box::new(Report::internal(message, None)))
}

pub(super) fn temp_error(error: io::Error) -> String {
    format!("cannot create a temporary directory: {error}")
}

/// A private temporary directory, removed on drop.
pub(super) struct TempDir {
    pub(super) path: PathBuf,
}

impl TempDir {
    pub(super) fn new() -> io::Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("lughac-{}-{n}", std::process::id()));
        // `create_dir`, not `create_dir_all`: fail rather than reuse a directory someone planted.
        std::fs::create_dir(&path)?;
        Ok(TempDir { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir is harmless and must not hide the real result.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
