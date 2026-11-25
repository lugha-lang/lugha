//! Why linking failed (`LinkError`), and the diagnostics for the failures a
//! program's `[link]` tables cause (E0329–E0336).

use std::path::PathBuf;

use super::PlanError;
use super::openssl::Folder;
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
    /// `folder` is the foreign target's OpenSSL, `None` for this machine's.
    #[error("OpenSSL is missing")]
    Tls { span: Span, folder: Option<PathBuf> },
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
            LinkError::Tls { span, folder: None } => Some(
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
            LinkError::Tls {
                span,
                folder: Some(folder),
            } => Some(
                Diagnostic::error(
                    "E0332",
                    "this program needs OpenSSL 3.2 or newer, which isn't installed",
                    *span,
                )
                .with_help(format!(
                    "the OpenSSL in `{}` is older than 3.2, or its headers don't compile",
                    folder.display()
                )),
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

/// What a build for `target` (its triple, and whether it is the machine
/// lughac runs on) with these `links` and the imports bringing in
/// `std.tls` and `std.crypto` can't link: E0334 for OpenSSL when `env`
/// names none built for it, E0336 when what it names isn't one, E0335 for
/// `pkg-config`. Otherwise the OpenSSL to link: `None` for a native build
/// (this machine's) or one without TLS or crypto.
///
/// # Errors
///
/// The diagnostics, when there are any.
pub fn foreign_refusals(
    (target, native): (&str, bool),
    links: &[crate::modules::link_table::PackageLink],
    (tls, crypto): (Option<Span>, Option<Span>),
    env: &dyn Fn(&str) -> Option<std::ffi::OsString>,
) -> Result<Option<Folder>, Vec<Diagnostic>> {
    if native {
        return Ok(None);
    }
    let mut refused = Vec::new();
    let mut folder = None;
    if let Some(first) = tls.or(crypto) {
        match super::openssl::find(target, env) {
            Ok(Some(found)) => folder = Some(found),
            Ok(None) => {
                let ssl = [("std.tls", tls), ("std.crypto", crypto)];
                refused.extend(ssl.into_iter().filter_map(|(module, at)| {
                    at.map(|span| foreign_openssl(module, span, target))
                }));
            }
            Err(bad) => refused.push(bad_openssl(&bad, first, target)),
        }
    }
    let pkg_config = super::Plan::pkg_config_line(links);
    refused.extend(pkg_config.map(|span| foreign_pkg_config(span, target)));
    match refused.is_empty() {
        true => Ok(folder),
        false => Err(refused),
    }
}

/// E0334: a program importing `module` (`std.tls` or `std.crypto`, at
/// `span`) built for the foreign `target`, with no OpenSSL named for it.
fn foreign_openssl(module: &str, span: Span, target: &str) -> Diagnostic {
    Diagnostic::error(
        "E0334",
        format!("`{module}` can't be built for {target}"),
        span,
    )
    .with_help(format!(
        "it needs OpenSSL 3.2 or newer built for {target}, and this machine's only fits \
         builds for this machine; set `{}` to a folder holding one (`include/`, \
         `lib/libssl.a` and `lib/libcrypto.a`), or build without `--target`",
        super::openssl::variable(target)
    ))
}

/// E0336: the variable for `target`'s OpenSSL names something `bad`, for
/// the import at `span`.
fn bad_openssl(bad: &super::openssl::Bad, span: Span, target: &str) -> Diagnostic {
    let variable = super::openssl::variable(target);
    Diagnostic::error(
        "E0336",
        format!("`{variable}` doesn't name an OpenSSL for {target}"),
        span,
    )
    .with_help(format!(
        "{}; it must be an absolute path to a folder holding `include/openssl/ssl.h`, \
         `lib/libssl.a` and `lib/libcrypto.a`, built for {target}",
        bad.explain()
    ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    const MAC: &str = "aarch64-apple-darwin";
    fn at() -> Span {
        Span::new(3, 9)
    }

    fn codes(found: &Result<Option<Folder>, Vec<Diagnostic>>) -> Vec<&'static str> {
        match found {
            Ok(_) => Vec::new(),
            Err(ds) => ds.iter().map(|d| d.code).collect(),
        }
    }

    #[test]
    fn a_native_build_never_reads_the_variable() {
        let env = |_: &str| -> Option<OsString> { panic!("read") };
        assert_eq!(
            foreign_refusals(("x", true), &[], (Some(at()), Some(at())), &env),
            Ok(None)
        );
    }

    #[test]
    fn a_program_without_openssl_never_reads_the_variable() {
        let env = |_: &str| -> Option<OsString> { panic!("read") };
        assert_eq!(
            foreign_refusals((MAC, false), &[], (None, None), &env),
            Ok(None)
        );
    }

    #[test]
    fn no_folder_is_e0334_per_import_naming_the_variable() {
        let found = foreign_refusals((MAC, false), &[], (Some(at()), Some(at())), &|_| None);
        assert_eq!(codes(&found), ["E0334", "E0334"]);
        let text = format!("{:?}", found.unwrap_err()[0]);
        assert!(
            text.contains("LUGHA_OPENSSL_AARCH64_APPLE_DARWIN"),
            "{text}"
        );
    }

    #[test]
    fn a_bad_folder_is_e0336_once() {
        let env = |_: &str| Some(OsString::from("relative"));
        let found = foreign_refusals((MAC, false), &[], (Some(at()), Some(at())), &env);
        assert_eq!(codes(&found), ["E0336"]);
        let text = format!("{:?}", found.unwrap_err()[0]);
        assert!(text.contains("`relative` isn't an absolute path"), "{text}");
    }
}
