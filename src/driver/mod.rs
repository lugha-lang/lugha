//! Driver — the `lughac` command line.
//!
//! Parses arguments, runs the pipeline for `build`, `run` and `check`, prints
//! the bundled spec for `spec`, starts a project for `init` and serves the
//! language server for `lsp`, prints
//! diagnostics (human or JSON) and chooses the exit code: 0 success, 1 the
//! program has errors, 2 bad usage or an internal error.

mod build;
mod cli;
mod explain;
mod fmt;
mod guard;
mod init;
mod json;
mod lsp;
mod pipeline;
mod render;
mod source;
mod stdout;
mod test;
mod test_build;
mod test_json;
mod update;

use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

use clap::Parser;

use self::cli::{Cli, Command, Emit, Format, TargetArg, offline_mode};
use self::pipeline::Failure;
use self::render::Report;
use self::source::Source;
use crate::codegen::Target;
use crate::diagnostic::Diagnostic;

/// Runs `lughac` with the process's arguments and standard streams.
pub fn main() -> ExitCode {
    guard::install_hook();
    let args: Vec<OsString> = std::env::args_os().collect();
    // Where to report a crash: the format and file the command asked for.
    let (format, file) = match Cli::try_parse_from(&args) {
        Ok(cli) => (cli.command.format(), cli.command.file()),
        Err(_) => (Format::Human, None),
    };
    let code = guard::guarded(format, file.as_deref(), &mut std::io::stderr(), || {
        run(args, &mut std::io::stdout(), &mut std::io::stderr())
    });
    ExitCode::from(code)
}

/// Runs `lughac` with `args` (including the program name), returning the exit code.
///
/// `lughac run` lets the compiled program inherit the process's real
/// standard streams; everything lughac itself prints goes to `stdout` and
/// `stderr`. A command that succeeds but can't write its output to `stdout` exits 2, so
/// `lughac build --emit=ir > out.ll` on a full disk isn't taken for a success; a reader that
/// stops early (`lughac spec | head`) is not a failure.
pub fn run<I, T>(args: I, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut watched = stdout::Watched::new(stdout);
    let code = dispatch(args, &mut watched, stderr);
    match watched.finish() {
        Some(error) if code == 0 => {
            // Nowhere better to report it; if stderr fails too, the exit code still says so.
            let _ = writeln!(stderr, "error: cannot write the output: {error}");
            2
        }
        _ => code,
    }
}

/// Parses `args` and runs the command they name, returning its exit code.
fn dispatch<I, T>(args: I, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            // `--help` and `--version` go to stdout with exit 0; usage errors to stderr with exit 2.
            // A failed write to stdout is kept by `run`.
            let out: &mut dyn Write = if error.use_stderr() { stderr } else { stdout };
            let _ = write!(out, "{}", error.render());
            return u8::try_from(error.exit_code()).unwrap_or(2);
        }
    };
    // `--cpu` names x86-64 levels only: a usage error, as clap's are.
    if let Command::Build {
        target: Some(TargetArg::Aarch64),
        options,
        ..
    } = &cli.command
        && options.cpu.is_some()
    {
        let message = "`--cpu` takes x86-64 levels; aarch64 builds for the baseline ARMv8-A";
        let error = <Cli as clap::CommandFactory>::command()
            .error(clap::error::ErrorKind::ArgumentConflict, message);
        // Nowhere better to report it; if stderr fails too, the exit code still says so.
        let _ = write!(stderr, "{}", error.render());
        return 2;
    }
    let format = cli.command.format();
    let mut out = Output {
        stdout,
        stderr,
        format,
    };
    let result = match &cli.command {
        Command::Build {
            file,
            output,
            emit,
            target,
            options,
        } => {
            let target = target.map_or(Target::default(), Into::into);
            let code = (options.opt, options.cpu(), target);
            build::build(
                &mut out,
                file,
                output.as_deref(),
                *emit,
                (code, options.mode()),
            )
        }
        Command::Run {
            file,
            options,
            args,
        } => pipeline::front(file, options.mode()).and_then(|front| {
            out.diagnostics(&front.source, &front.warnings);
            pipeline::run(
                &front,
                (options.opt, options.cpu(), Target::default()),
                args,
            )
        }),
        Command::Check { file, offline, .. } => pipeline::front_check(file, offline_mode(*offline))
            .map(|front| {
                out.diagnostics(&front.source, &front.warnings);
                0
            }),
        Command::Update { file, .. } => update::update(file).map(|(source, warnings)| {
            out.diagnostics(&source, &warnings);
            0
        }),
        Command::Fmt { paths, check } => Ok(fmt::fmt(paths, *check, &mut out)),
        Command::Test {
            paths,
            filter,
            exact,
            integration,
            timeout,
            list,
            json,
            cpu,
        } => Ok(test::test(
            &test::Request {
                paths,
                filters: filter,
                exact: *exact,
                integration: *integration,
                timeout: std::time::Duration::from_secs(*timeout),
                list: *list,
                json: *json,
                cpu: (*cpu).into(),
            },
            &mut out,
        )),
        Command::Explain { code } => Ok(explain::run(code.as_deref(), out.stdout, out.stderr)),
        Command::Spec => Ok(spec(out.stdout, out.stderr)),
        Command::Lsp => Ok(lsp::serve(out.stderr)),
        Command::Init { folder, lib, name } => Ok(init::init(
            &init::Request {
                folder: folder.as_deref(),
                lib: *lib,
                name: name.as_deref(),
            },
            out.stdout,
            out.stderr,
        )),
    };
    match result {
        Ok(code) => code,
        Err(Failure::Program(source, diagnostics)) => {
            out.diagnostics(&source, &diagnostics);
            1
        }
        Err(Failure::Internal(source, report)) => {
            out.report(&source, &report);
            2
        }
    }
}

/// `lughac spec`: the embedded spec, byte for byte.
fn spec(stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    match stdout
        .write_all(crate::SPEC.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => 0,
        // A reader that stops early (`lughac spec | head`) is not a failure.
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => 0,
        Err(error) => {
            // Nowhere better to report it; if stderr fails too, the exit code still says so.
            let _ = writeln!(stderr, "error: cannot write the spec: {error}");
            2
        }
    }
}

/// Where lughac's own output goes. Write errors are ignored here: `stdout::Watched` keeps
/// the one that matters, and `run` turns it into the exit code.
struct Output<'a> {
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
    format: Format,
}

impl Output<'_> {
    fn print(&mut self, text: &str) {
        let _ = self.stdout.write_all(text.as_bytes());
    }

    /// One of lughac's own errors, not tied to a source.
    fn error(&mut self, message: &str) {
        let _ = writeln!(self.stderr, "error: {message}");
    }

    fn diagnostics(&mut self, source: &Source, diagnostics: &[Diagnostic]) {
        for diagnostic in diagnostics {
            self.report(source, &Report::from(diagnostic));
        }
        // People read the human format; tools read JSON, which has the codes.
        if matches!(self.format, Format::Human)
            && let Some(hint) = explain::hint(diagnostics)
        {
            let _ = self.stderr.write_all(hint.as_bytes());
        }
    }

    fn report(&mut self, source: &Source, report: &Report) {
        let text = match self.format {
            Format::Human => render::human(report, source),
            Format::Json => json::line(report, source),
        };
        let _ = self.stderr.write_all(text.as_bytes());
    }
}
