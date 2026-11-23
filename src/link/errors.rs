//! Why linking failed (`LinkError`), and the diagnostics for the failures a
//! program's `[link]` tables cause (E0329–E0332).

use std::path::PathBuf;

use super::PlanError;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// Why linking failed.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    /// The C compiler isn't there: `cc`, or what `$LUGHA_CC` names, as messages name it.
    #[error("C compiler {0} not found; install gcc or clang, or set $LUGHA_CC")]
    CcNotFound(String),
    /// A build for the foreign target named found no `zig` on `PATH`.
    #[error(
        "building for {0} needs zig 0.17.0 or newer ({download}), or set $LUGHA_CC",
        download = super::zig::DOWNLOAD
    )]
    ZigNotFound(String),
    /// The `zig` on `PATH` is older than 0.17.0, or `zig version` printed `found`, which isn't a version.
    #[error(
        "building for {target} needs zig 0.17.0 or newer, but `zig version` says {found:?}; update it ({download}), or set $LUGHA_CC",
        download = super::zig::DOWNLOAD
    )]
    ZigTooOld { found: String, target: String },
    /// `$LUGHA_CC` isn't UTF-8, so its words can't be split.
    #[error("$LUGHA_CC isn't UTF-8")]
    BadCompiler,
    /// `cc` could not be started, or the runtime source could not be written.
    #[error("could not run `cc`: {0}")]
    Io(std::io::Error),
    /// `cc` ran and reported an error.
    #[error("`cc` failed ({}):\n{stderr}", exit_text(*.status))]
    Failed { status: Option<i32>, stderr: String },
    /// A `[link]` C file doesn't compile: E0330 at its `c` line.
    #[error("`{}` doesn't compile:\n{stderr}", file.display())]
    CFile {
        span: Span,
        file: PathBuf,
        stderr: String,
    },
    /// Linking failed with `[link]` input: E0331 at its first line.
    #[error("linking failed:\n{stderr}")]
    Libraries { span: Span, stderr: String },
    /// A TLS or crypto program, and no OpenSSL 3.2 or newer to build it with: E0332 at its import.
    #[error("OpenSSL is missing")]
    Tls { span: Span },
}

impl LinkError {
    /// The diagnostic for a failure the program's `[link]` tables caused
    /// (E0330, E0331); `None` for lughac's own, an internal error.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        match self {
            LinkError::CFile { span, file, stderr } => Some(
                Diagnostic::error(
                    "E0330",
                    format!("the C file `{}` doesn't compile", file.display()),
                    *span,
                )
                .with_help(format!("the C compiler said:\n{}", stderr.trim_end())),
            ),
            LinkError::Libraries { span, stderr } => {
                // The advice follows what the linker said, not a guess.
                let advice = if stderr.contains("multiple definition") {
                    "\na symbol is defined twice: keep it in one C file or library"
                } else if stderr.contains("cannot find -l") {
                    "\ninstall the missing library, or add the folder holding it to `paths`"
                } else if stderr.contains("undefined reference") {
                    "\nadd the library or C file that defines it to `libs` or `c`"
                } else {
                    ""
                };
                Some(
                    Diagnostic::error("E0331", "linking failed", *span)
                        .with_help(format!("the linker said:\n{}{advice}", stderr.trim_end())),
                )
            }
            LinkError::Tls { span } => Some(
                Diagnostic::error(
                    "E0332",
                    "this program needs OpenSSL 3.2 or newer, which isn't installed",
                    *span,
                )
                .with_help(
                    "install OpenSSL's development files, version 3.2 or newer: \
                     `sudo apt install libssl-dev` (Ubuntu 24.04's is 3.0, too old)",
                ),
            ),
            LinkError::CcNotFound(_)
            | LinkError::ZigNotFound(_)
            | LinkError::ZigTooOld { .. }
            | LinkError::BadCompiler
            | LinkError::Io(_)
            | LinkError::Failed { .. } => None,
        }
    }
}

/// E0334 and E0335 for a build for `target` (its triple, and whether it
/// is the machine lughac runs on) with these `links` and the imports
/// bringing in `std.tls` and `std.crypto`: what a foreign build can't
/// link, this machine's OpenSSL and `pkg-config`'s libraries. Empty for a
/// native build.
pub fn foreign_refusals(
    (target, native): (&str, bool),
    links: &[crate::modules::link_table::PackageLink],
    (tls, crypto): (Option<Span>, Option<Span>),
) -> Vec<Diagnostic> {
    if native {
        return Vec::new();
    }
    let ssl = [("std.tls", tls), ("std.crypto", crypto)];
    let pkg_config = super::Plan::pkg_config_line(links);
    ssl.into_iter()
        .filter_map(|(module, at)| at.map(|span| foreign_openssl(module, span, target)))
        .chain(pkg_config.map(|span| foreign_pkg_config(span, target)))
        .collect()
}

/// E0334: a program importing `module` (`std.tls` or `std.crypto`, at
/// `span`) built for the foreign `target`, whose OpenSSL isn't at hand.
fn foreign_openssl(module: &str, span: Span, target: &str) -> Diagnostic {
    Diagnostic::error(
        "E0334",
        format!("`{module}` can't be built for {target}"),
        span,
    )
    .with_help(
        "the C library it needs, OpenSSL, is linked from this machine, which only fits \
         builds for this machine; build without `--target`, or on the target platform",
    )
}

/// E0335: a `[link]` table's `pkg-config` line (at `span`) in a build for
/// the foreign `target`.
fn foreign_pkg_config(span: Span, target: &str) -> Diagnostic {
    Diagnostic::error(
        "E0335",
        format!("`pkg-config` can't be used for {target}"),
        span,
    )
    .with_help(
        "its answers describe this machine's libraries; name the target's libraries \
         with `libs` and `paths`, or build without `--target`",
    )
}

impl PlanError {
    /// E0329 at the `pkg-config` line.
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::error("E0329", "pkg-config failed", self.span).with_help(self.message.clone())
    }
}

fn exit_text(status: Option<i32>) -> String {
    status.map_or_else(
        || "killed by a signal".to_string(),
        |code| format!("exit code {code}"),
    )
}
