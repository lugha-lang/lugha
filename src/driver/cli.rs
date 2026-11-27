//! `lughac`'s command line: the commands, their options and the values
//! they take, as clap reads them. `dispatch` (in the module above) acts on them.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::codegen::{Cpu, Instruments, OptLevel, Target};
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
        /// The platform to build for [default: this machine]
        #[arg(long, value_enum)]
        target: Option<TargetArg>,
        #[command(flatten)]
        options: Options,
    },
    /// Build to a temporary file and run it
    Run {
        /// Source file
        file: PathBuf,
        /// The platform to build for, run under qemu for another CPU [default: this machine]
        #[arg(long, value_enum)]
        target: Option<TargetArg>,
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
        /// Diagnostic format
        #[arg(long, value_enum, default_value = "human")]
        diagnostics: Format,
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
        /// Diagnostic format, for compile errors on stderr
        #[arg(long, value_enum, default_value = "human")]
        diagnostics: Format,
        /// The x86-64 level the test executables are built for [default: x86-64]
        #[arg(long, value_enum)]
        cpu: Option<CpuLevel>,
        /// The platform to build for, run under qemu for another CPU [default: this machine]
        #[arg(long, value_enum)]
        target: Option<TargetArg>,
        /// Run plain tests with ready tasks picked at random from SEED (drawn if not given)
        #[arg(long, value_name = "SEED", num_args = 0..=1, require_equals = true)]
        shuffle: Option<Option<u64>>,
    },
    /// Explain an error or warning code, or list every code
    Explain {
        /// The code, such as E0501 [default: list every code]
        code: Option<String>,
    },
    /// Read the Lugha skill: how to write Lugha, for people and AI assistants
    Skill(SkillRequest),
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

/// `lughac skill`'s arguments: a page, or one of the flags, or neither
/// (the overview).
#[derive(Args)]
#[group(multiple = false)]
pub(super) struct SkillRequest {
    /// A page, such as `functions` or `concepts/02-functions` [default: the overview]
    pub(super) page: Option<String>,
    /// List every page with its summary, in teaching order
    #[arg(long)]
    pub(super) list: bool,
    /// Print every line containing TEXT (any case), as page:line: text
    #[arg(long, value_name = "TEXT")]
    pub(super) search: Option<String>,
    /// Write the skill as DIR/lugha/, ready to upload as an Agent Skill
    #[arg(long, value_name = "DIR")]
    pub(super) export: Option<PathBuf>,
}

impl Command {
    pub(super) fn format(&self) -> Format {
        match self {
            Command::Build { options, .. } | Command::Run { options, .. } => options.diagnostics,
            Command::Check { diagnostics, .. }
            | Command::Update { diagnostics, .. }
            | Command::Fmt { diagnostics, .. }
            | Command::Test { diagnostics, .. } => *diagnostics,
            Command::Skill(_) | Command::Init { .. } | Command::Lsp | Command::Explain { .. } => {
                Format::Human
            }
        }
    }

    /// The source file the command reads, as given.
    pub(super) fn file(&self) -> Option<String> {
        match self {
            Command::Build { file, .. }
            | Command::Run { file, .. }
            | Command::Check { file, .. }
            | Command::Update { file, .. } => Some(file.display().to_string()),
            Command::Skill(_)
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
}

/// `$LUGHA_CALL_COSTS`: `1` simulates the per-call costs of growable stacks
/// and preemption, a measuring tool for the benchmarks. `$LUGHA_RC_CHECK`:
/// `1` builds for the owner check, which the program then runs with the
/// same variable set.
pub(super) fn instruments_from_env() -> Instruments {
    let set = |name: &str| std::env::var(name).as_deref() == Ok("1");
    Instruments {
        call_costs: set("LUGHA_CALL_COSTS"),
        rc_check: set("LUGHA_RC_CHECK"),
    }
}

impl Command {
    /// Whether this command names an ARM64 target and an x86-64 `--cpu`
    /// level: a usage error.
    pub(super) fn arm_with_cpu(&self) -> bool {
        let arm = |t: &Option<TargetArg>| {
            matches!(
                t,
                Some(TargetArg::Aarch64 | TargetArg::Aarch64Musl | TargetArg::Aarch64MacOs)
            )
        };
        match self {
            Command::Build {
                target, options, ..
            }
            | Command::Run {
                target, options, ..
            } => arm(target) && options.cpu.is_some(),
            Command::Test { target, cpu, .. } => arm(target) && cpu.is_some(),
            _ => false,
        }
    }
}

/// `--target`'s values.
#[derive(Clone, Copy, ValueEnum)]
pub(super) enum TargetArg {
    #[value(name = "x86_64-unknown-linux-gnu")]
    X86_64,
    #[value(name = "aarch64-unknown-linux-gnu")]
    Aarch64,
    #[value(name = "x86_64-unknown-linux-musl")]
    X86_64Musl,
    #[value(name = "aarch64-unknown-linux-musl")]
    Aarch64Musl,
    #[value(name = "aarch64-apple-darwin")]
    Aarch64MacOs,
    #[value(name = "x86_64-pc-windows-gnu")]
    X86_64Windows,
    #[value(name = "aarch64-pc-windows-gnu")]
    Aarch64Windows,
}

impl From<TargetArg> for Target {
    fn from(target: TargetArg) -> Target {
        match target {
            TargetArg::X86_64 => Target::X86_64Linux,
            TargetArg::Aarch64 => Target::Aarch64Linux,
            TargetArg::X86_64Musl => Target::X86_64LinuxMusl,
            TargetArg::Aarch64Musl => Target::Aarch64LinuxMusl,
            TargetArg::Aarch64MacOs => Target::Aarch64MacOs,
            TargetArg::X86_64Windows => Target::X86_64Windows,
            TargetArg::Aarch64Windows => Target::Aarch64Windows,
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
