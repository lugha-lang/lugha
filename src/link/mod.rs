//! Linking — turns object files into an executable with the system C
//! compiler, compiling the runtime in the same step.
//!
//! Runs `cc <objects> <runtime.o> -lm -o <output>` directly, never through a
//! shell. The runtime's files under `runtime/` are embedded, so linking
//! works from any directory with no install layout.
//!
//! The `[link]` tables of `lugha.toml` add C
//! files, compiled first (`c_files`), and folders and libraries after
//! `-lm` (`inputs`), some from `pkg-config` (`pkg_config`).

pub mod c_files;
pub mod compiler;
mod errors;
pub mod inputs;
pub mod ndk;
mod object_cache;
pub mod openssl;
pub mod pkg_config;
pub mod runtime;
mod runtime_parts;
mod runtime_symbols;
mod shared;
pub mod zig;

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

pub use self::errors::{LinkError, foreign_refusals};
pub use self::inputs::{Plan, PlanError};
pub use self::runtime_parts::{
    ANDROID_ONLY, DARWIN_ONLY, LINUX_ONLY, POSIX_ONLY, RUNTIME_PARTS, RUNTIME_SOURCE, System,
    WINDOWS_ONLY, runtime_source,
};
pub use self::runtime_symbols::RUNTIME_SYMBOLS;

/// Links `objects` and the runtime into the executable `output`.
///
/// # Errors
///
/// [`LinkError::CcNotFound`] if there is no C compiler (`cc` or `$LUGHA_CC`), [`LinkError::Io`] if it
/// can't be started or the runtime can't be written to the temp dir,
/// [`LinkError::Failed`] with `cc`'s stderr if it fails.
pub fn link(objects: &[&Path], output: &Path) -> Result<(), LinkError> {
    let cc = compiler::Compiler::from_env()?;
    // lughac itself runs on Linux: a native link is for it.
    link_with(
        objects,
        &Plan::default(),
        (Path::new(""), cc, System::Linux),
        output,
    )
}

/// Links `objects`, the C files of `plan` compiled into `temp`, and the
/// runtime for `system` into `output` with the compiler `cc` (the
/// target's), with `plan`'s folders and libraries.
///
/// # Errors
///
/// As [`link`], and [`LinkError::CFile`] for a C file that doesn't compile.
/// When `plan` isn't empty, a failing link is [`LinkError::Libraries`]
/// rather than [`LinkError::Failed`]: it is most likely the program's.
pub fn link_with(
    objects: &[&Path],
    plan: &Plan,
    (temp, cc, system): (&Path, compiler::Compiler, System),
    output: &Path,
) -> Result<(), LinkError> {
    // Compiled C objects are kept between builds.
    let cache = object_cache::Cache::open(cc);
    let shims = c_files::compile(&plan.c_files, (temp, system != System::Windows), &cache)?;
    let variant = (plan.tls.is_some(), plan.crypto.is_some());
    let openssl = plan
        .tls
        .or(plan.crypto)
        .map(|at| (at, plan.openssl.as_ref()));
    let runtime = runtime::compile(variant, &cache, openssl, (system, plan.shared.is_some()))?;
    let linked: Vec<&Path> = vec![runtime.path.as_path()];
    let shared = match &plan.shared {
        Some(symbols) => shared::args(symbols, (temp, system), output).map_err(LinkError::Io)?,
        None => Vec::new(),
    };
    let result = cache
        .cc()
        .link_command()
        .args(args(objects, &shims, &linked, plan, output))
        .args(shared)
        .output();
    let out = match result {
        Ok(out) => out,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(LinkError::CcNotFound(cache.cc().name()));
        }
        Err(e) => return Err(LinkError::Io(e)),
    };
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    // A TLS or crypto program on a machine without OpenSSL's headers or libraries.
    if let Some(span) = plan.tls.or(plan.crypto)
        && runtime::missing_openssl(&stderr)
    {
        let folder = plan.openssl.as_ref().map(|f| f.path().to_path_buf());
        return Err(LinkError::Tls { span, folder });
    }
    match plan.at {
        Some(span) => Err(LinkError::Libraries { span, stderr }),
        None => Err(LinkError::Failed {
            status: out.status.code(),
            stderr,
        }),
    }
}

/// `cc`'s arguments: objects, C objects, the runtime, `-lm`, the
/// plan's folders and libraries, the output. Libraries go after every
/// object, as a static link needs.
fn args(
    objects: &[&Path],
    shims: &[PathBuf],
    runtime: &[&Path],
    plan: &Plan,
    output: &Path,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = objects.iter().map(|o| o.as_os_str().to_owned()).collect();
    args.extend(shims.iter().map(|o| o.as_os_str().to_owned()));
    args.extend(runtime.iter().map(|o| o.as_os_str().to_owned()));
    args.push(OsString::from("-lm"));
    let (tls, crypto) = (plan.tls.is_some(), plan.crypto.is_some());
    args.extend(openssl::link_args(plan.openssl.as_ref(), tls, crypto));
    args.extend(plan.link_args());
    args.push(OsString::from("-o"));
    args.push(output.as_os_str().to_owned());
    args
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
