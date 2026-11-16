//! `lughac`'s command line: the commands, their options and the values
//! they take, as clap reads them. `dispatch` (in the module above) acts on them.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::codegen::{Cpu, Memory, OptLevel, Target};
use crate::modules::fetch::Mode;

#[derive(Parser)]
#[command(name = "lughac", version, about = "The Lugha compiler")]
pub(super) struct Cli {
    #[command(subcommand)]
    pub(super) command: Command,
}

#[derive(Subcommand)]
pub(super) enum Command {
    /// Compile and link an executable
    Build {
        /// Source file
        file: PathBuf,
        /// Output path [default: the file name without its extension]
        #[arg(short = 'o', value_name = "OUT")]
        output: Option<PathBuf>,
        /// Print one intermediate stage to stdout and stop
        #[arg(long, value_enum)]
        emit: Option<Emit>,
        /// The platform to build for [default: x86_64-unknown-linux-gnu]
        #[arg(long, value_enum)]
        target: Option<TargetArg>,
        #[command(flatten)]
        options: Options,
    },
    /// Build to a temporary file and run it
    Run {
        /// Source file
        file: PathBuf,
        #[command(flatten)]
        options: Options,
        /// Arguments for the program, after `--` (read with `args()`)
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Lex, parse and type-check only, reporting errors
    Check {
        /// Source file
        file: PathBuf,
        /// Diagnostic format
        #[arg(long, value_enum, default_value = "human")]
        diagnostics: Format,
        /// Run no `git`: every git package must be locked and cached
        #[arg(long)]
        offline: bool,
    },
    /// Fetch every git package's newest matching tag and rewrite `lugha.lock`
    Update {
        /// Source file
        file: PathBuf,
        /// Diagnostic format
        #[arg(long, value_enum, default_value = "human")]
        diagnostics: Format,
    },
    /// Format .la files in place in the one canonical style
    Fmt {
        /// Files or folders (every .la file under them) [default: the current folder]
        paths: Vec<PathBuf>,
        /// Write nothing; list the files that would change and exit 1 if any
        #[arg(long)]
        check: bool,
    },
    /// Run the `@test` functions in .la files, each in its own process
    Test {
        /// Files or folders (every .la file under them) [default: the current folder]
        paths: Vec<PathBuf>,
        /// Run only the tests whose `module.name` contains TEXT (repeatable)
        #[arg(long, value_name = "TEXT")]
        filter: Vec<String>,
        /// Each --filter must equal a whole test name
        #[arg(long)]
        exact: bool,
        /// Also run the `@test(integration)` tests, given the real world
        #[arg(long)]
        integration: bool,
        /// Fail a test that runs longer than SECS seconds
        #[arg(long, value_name = "SECS", default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..))]
        timeout: u64,
        /// List the tests and where they are; build and run nothing
        #[arg(long)]
        list: bool,
        /// One JSON object per line, for tools such as editors
        #[arg(long)]
        json: bool,
        /// The x86-64 level the test executables are built for
        #[arg(long, value_enum, default_value = "x86-64")]
        cpu: CpuLevel,
    },
    /// Explain an error or warning code, or list every code
    Explain {
        /// The code, such as E0501 [default: list every code]
        code: Option<String>,
    },
    /// Print the LLM-ready spec bundled with this compiler version
    Spec,
    /// Serve the Language Server Protocol on stdin/stdout, for editors
    Lsp,
    /// Start a program (or with --lib a library) in a folder; never overwrites
    Init {
        /// The folder [default: the current one]
        folder: Option<PathBuf>,
        /// Make a library: src/<name>.la and examples/hello.la
        #[arg(long)]
        lib: bool,
        /// The package name [default: the folder's name]
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
}

impl Command {
    pub(super) fn format(&self) -> Format {
        match self {
            Command::Build { options, .. } | Command::Run { options, .. } => options.diagnostics,
            Command::Check { diagnostics, .. } | Command::Update { diagnostics, .. } => {
                *diagnostics
            }
            Command::Spec
            | Command::Init { .. }
            | Command::Lsp
            | Command::Fmt { .. }
            | Command::Test { .. }
            | Command::Explain { .. } => Format::Human,
        }
    }

    /// The source file the command reads, as given.
    pub(super) fn file(&self) -> Option<String> {
        match self {
            Command::Build { file, .. }
            | Command::Run { file, .. }
            | Command::Check { file, .. }
            | Command::Update { file, .. } => Some(file.display().to_string()),
            Command::Spec
            | Command::Init { .. }
            | Command::Lsp
            | Command::Fmt { .. }
            | Command::Test { .. }
            | Command::Explain { .. } => None,
        }
    }
}

#[derive(Args)]
pub(super) struct Options {
    /// Diagnostic format
    #[arg(long, value_enum, default_value = "human")]
    pub(super) diagnostics: Format,
    /// Optimisation level: 0 or 2
    #[arg(short = 'O', value_name = "LEVEL", default_value = "0", value_parser = opt_level)]
    pub(super) opt: OptLevel,
    /// The x86-64 level the executable is built for; `native` runs only on CPUs like this one
    #[arg(long, value_enum)]
    pub(super) cpu: Option<CpuLevel>,
    /// Run no `git`: every git package must be locked and cached
    #[arg(long)]
    pub(super) offline: bool,
    /// How memory is managed: `gc`, the collector, or `rc`, the reference-counting prototype [default: `$LUGHA_MEMORY`, else gc]
    #[arg(long, value_enum)]
    pub(super) memory: Option<MemoryArg>,
}

impl Options {
    pub(super) fn mode(&self) -> Mode {
        offline_mode(self.offline)
    }
}

/// How a build may fetch git packages: never with `--offline`.
pub(super) fn offline_mode(offline: bool) -> Mode {
    Mode {
        offline,
        update: false,
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum Format {
    Human,
    Json,
}

/// `--cpu`'s values: a fixed list, since LLVM only
/// warns about a CPU name it doesn't know and builds generic code.
#[derive(Clone, Copy, ValueEnum)]
pub(super) enum CpuLevel {
    #[value(name = "x86-64")]
    X86_64,
    #[value(name = "x86-64-v2")]
    V2,
    #[value(name = "x86-64-v3")]
    V3,
    #[value(name = "x86-64-v4")]
    V4,
    Native,
}

impl Options {
    /// `--cpu`, or the baseline when it isn't given.
    pub(super) fn cpu(&self) -> Cpu {
        self.cpu.map_or(Cpu::default(), Into::into)
    }

    /// `--memory`, else `$LUGHA_MEMORY`, else the collector.
    pub(super) fn memory(&self) -> Memory {
        match self.memory {
            Some(MemoryArg::Gc) => Memory::Gc,
            Some(MemoryArg::Rc) => Memory::Rc,
            None => memory_from_env(),
        }
    }
}

/// `--memory`'s values.
#[derive(Clone, Copy, ValueEnum)]
pub(super) enum MemoryArg {
    Gc,
    Rc,
}

/// `$LUGHA_MEMORY`: `rc` chooses reference counting; anything else, or
/// nothing, the collector. `lughac test` reads only this.
pub(super) fn memory_from_env() -> Memory {
    match std::env::var("LUGHA_MEMORY").as_deref() {
        Ok("rc") => Memory::Rc,
        _ => Memory::Gc,
    }
}

/// `--target`'s values.
#[derive(Clone, Copy, ValueEnum)]
pub(super) enum TargetArg {
    #[value(name = "x86_64-unknown-linux-gnu")]
    X86_64,
    #[value(name = "aarch64-unknown-linux-gnu")]
    Aarch64,
}

impl From<TargetArg> for Target {
    fn from(target: TargetArg) -> Target {
        match target {
            TargetArg::X86_64 => Target::X86_64Linux,
            TargetArg::Aarch64 => Target::Aarch64Linux,
        }
    }
}

impl From<CpuLevel> for Cpu {
    fn from(level: CpuLevel) -> Cpu {
        match level {
            CpuLevel::X86_64 => Cpu::X86_64,
            CpuLevel::V2 => Cpu::V2,
            CpuLevel::V3 => Cpu::V3,
            CpuLevel::V4 => Cpu::V4,
            CpuLevel::Native => Cpu::Native,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum Emit {
    Tokens,
    Ast,
    Ir,
}

pub(super) fn opt_level(level: &str) -> Result<OptLevel, String> {
    match level {
        "0" => Ok(OptLevel::O0),
        "2" => Ok(OptLevel::O2),
        _ => Err("expected 0 or 2".to_string()),
    }
}
