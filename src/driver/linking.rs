//! Turning a checked program into what runs: an executable, or with
//! `--shared` a shared library and its C header, through codegen and the
//! target's C compiler.

use std::path::{Path, PathBuf};

use super::pipeline::{
    Failure, Front, TempDir, checked, codegen_failure, internal, sources, temp_error, units,
};
use crate::codegen::{self, Cpu, Instruments, OptLevel, Target};
use crate::diagnostic::Diagnostic;
use crate::link;
use crate::modules::Module;

/// Compiles and links `front` at `opt` for `cpu` on `target` into the executable `exe`,
/// with what its `[link]` tables add: `pkg-config` runs first, so its
/// failures come before codegen's work.
pub(super) fn build(
    front: &Front,
    code: (OptLevel, Cpu, Target, Instruments),
    exe: &Path,
) -> Result<(), Failure> {
    build_with(front, code, exe, None)
}

/// [`build`] for `--shared`: the shared library `out`, which C calls
/// through `front`'s exports, and their C header beside it, named after
/// the source file `file`.
pub(super) fn build_shared(
    front: &Front,
    code: (OptLevel, Cpu, Target, Instruments),
    (file, out): (&Path, &Path),
    jni: Option<(&str, super::jni::Lang)>,
) -> Result<(), Failure> {
    let mut symbols: Vec<String> = front
        .modules
        .iter()
        .filter_map(|m| m.program.as_ref())
        .flat_map(|p| &p.items)
        .filter_map(|item| match item {
            crate::ast::Item::Fun(f) => f
                .export
                .as_ref()
                .map(|e| e.symbol.as_ref().unwrap_or(&f.name).name.clone()),
            _ => None,
        })
        .collect();
    // `--jni`: the glue Kotlin or Java calls through, and their class.
    let lib = out.file_stem().unwrap_or(out.as_os_str()).to_string_lossy();
    let lib = lib.strip_prefix("lib").unwrap_or(&lib).to_string();
    let door = match jni {
        Some((class, lang)) => Some(
            super::jni::door(&front.modules, (class, lang, &lib))
                .map_err(|errors| Failure::Program(front.source.clone(), errors))?,
        ),
        None => None,
    };
    if let Some(door) = &door {
        symbols.extend(door.symbols.iter().cloned());
    }
    build_with(
        front,
        code,
        out,
        Some((symbols, door.as_ref().map(|d| d.glue.clone()))),
    )?;
    if let Some(door) = door {
        let (path, text) = door.source;
        let path = out.with_file_name("").join(path);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, text));
        written.map_err(|e| {
            internal(
                &front.source,
                format!("cannot write {}: {e}", path.display()),
            )
        })?;
    }
    let stem = file
        .file_stem()
        .unwrap_or(file.as_os_str())
        .to_string_lossy();
    let header = out.with_file_name(format!("{stem}.h"));
    let text = super::c_header::header(&stem, &front.modules);
    std::fs::write(&header, text).map_err(|e| {
        internal(
            &front.source,
            format!("cannot write {}: {e}", header.display()),
        )
    })
}

/// [`build`], or with `shared` (the C names the library shows, and any C
/// glue to compile into it) a shared library.
fn build_with(
    front: &Front,
    (opt, cpu, target, costs): (OptLevel, Cpu, Target, Instruments),
    exe: &Path,
    shared: Option<(Vec<String>, Option<String>)>,
) -> Result<(), Failure> {
    // `debug(x)` is for development: an optimized build is a release. Only
    // the main file's package's calls are its to remove; a dependency's
    // print nothing there.
    if opt == OptLevel::O2 {
        let own = |at: usize| {
            front.modules.iter().any(|m| {
                (m.base..=m.base + m.text.len()).contains(&at)
                    && m.package == front.modules[0].package
            })
        };
        let found: Vec<Diagnostic> = checked(front)
            .debug_calls
            .iter()
            .filter(|span| own(span.start))
            .map(|&span| debug_in_release(span))
            .collect();
        if !found.is_empty() {
            return Err(Failure::Program(front.source.clone(), found));
        }
    }
    let tls = std_import(&front.modules, "tls", &["tls", "http"]);
    let crypto = std_import(&front.modules, "crypto", &["crypto"]);
    let openssl = link::foreign_refusals(
        (target.triple(), target.uses_system_cc()),
        &front.links,
        (tls, crypto),
        &|name| std::env::var_os(name),
    )
    .map_err(|refused| Failure::Program(front.source.clone(), refused))?;
    let cc = link::compiler::Compiler::for_target(target.triple(), target.uses_system_cc())
        .map_err(|e| internal(&front.source, e.to_string()))?;
    let mut plan = link::Plan::new(&front.links, &mut link::pkg_config::run)
        .map_err(|e| Failure::Program(front.source.clone(), vec![e.diagnostic()]))?;
    (plan.tls, plan.crypto, plan.openssl) = (tls, crypto, openssl);
    let temp = TempDir::new().map_err(|e| internal(&front.source, temp_error(e)))?;
    if let Some((symbols, glue)) = shared {
        if let Some(glue) = glue {
            let file = temp.path.join("jni.c");
            std::fs::write(&file, glue)
                .map_err(|e| internal(&front.source, format!("cannot write the JNI glue: {e}")))?;
            plan.c_files.push(link::inputs::CFile {
                file,
                flags: Vec::new(),
                span: crate::span::Span::new(0, 0),
            });
        }
        plan.shared = Some(symbols);
    }
    let object = temp.path.join("prog.o");
    codegen::emit_object_modules(
        &units(&front.modules),
        checked(front),
        &sources(front),
        opt,
        (cpu, target, costs),
        &object,
    )
    .map_err(|e| codegen_failure(&front.source, e))?;
    let system = link::System::of(target.triple());
    link::link_with(&[&object], &plan, (&temp.path, cc, system), exe).map_err(|e| {
        match e.diagnostic() {
            Some(d) => Failure::Program(front.source.clone(), vec![d]),
            None => internal(&front.source, e.to_string()),
        }
    })
}

/// E0465: a `debug(x)` call at `span` in an optimized build.
fn debug_in_release(span: crate::span::Span) -> Diagnostic {
    Diagnostic::error("E0465", "`debug` is for development builds", span)
        .with_help("remove the `debug(…)` call, or build without `-O2`")
}

/// The program's import that brings in OpenSSL's `std.<module>`:
/// `None` unless the program loads it; else the first import of the
/// program's own modules that leads there (one of `via`: `std.http` leads
/// to `std.tls`). Only such programs get that runtime part and OpenSSL.
fn std_import(modules: &[Module], module: &str, via: &[&str]) -> Option<crate::span::Span> {
    if !modules.iter().any(|m| m.path == ["std", module]) {
        return None;
    }
    modules
        .iter()
        .filter(|m| m.package.as_deref() != Some("std"))
        .filter_map(|m| m.program.as_ref())
        .flat_map(|p| &p.imports)
        .find(|i| {
            let names: Vec<&str> = i.path.iter().map(|s| s.name.as_str()).collect();
            names.len() >= 2 && names[0] == "std" && via.contains(&names[1])
        })
        .map(|i| i.span)
}

/// Where `build` writes for `target` when there is no `-o`: the file name
/// without its extension, with `.exe` for Windows, which runs only names
/// ending in it.
///
/// A file with no extension gets `.out` elsewhere, so the source is never
/// overwritten.
pub(super) fn default_output(file: &Path, target: Target) -> PathBuf {
    let stem = PathBuf::from(file.file_stem().unwrap_or(file.as_os_str()));
    if target.is_windows() {
        stem.with_extension("exe")
    } else if file.extension().is_none() {
        stem.with_extension("out")
    } else {
        stem
    }
}

/// Where `build --shared` writes for `target` when there is no `-o`: the
/// system's name for a library, `lib<stem>.so`, `lib<stem>.dylib` or
/// `<stem>.dll`.
pub(super) fn shared_output(file: &Path, target: Target) -> PathBuf {
    let stem = file
        .file_stem()
        .unwrap_or(file.as_os_str())
        .to_string_lossy();
    PathBuf::from(if target.is_windows() {
        format!("{stem}.dll")
    } else if target == Target::Aarch64MacOs {
        format!("lib{stem}.dylib")
    } else {
        format!("lib{stem}.so")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_output_never_overwrites_the_source() {
        let linux = Target::X86_64Linux;
        assert_eq!(
            default_output(Path::new("arith.la"), linux),
            PathBuf::from("arith")
        );
        assert_eq!(
            default_output(Path::new("dir/prog.la"), linux),
            PathBuf::from("prog")
        );
        assert_eq!(
            default_output(Path::new("prog"), linux),
            PathBuf::from("prog.out")
        );
    }

    #[test]
    fn a_windows_build_is_named_exe() {
        for windows in [Target::X86_64Windows, Target::Aarch64Windows] {
            assert_eq!(
                default_output(Path::new("dir/app.la"), windows),
                PathBuf::from("app.exe")
            );
            assert_eq!(
                default_output(Path::new("app"), windows),
                PathBuf::from("app.exe")
            );
        }
    }

    #[test]
    fn a_shared_library_has_its_systems_name() {
        assert_eq!(
            shared_output(Path::new("dir/words.la"), Target::X86_64Linux),
            PathBuf::from("libwords.so")
        );
        assert_eq!(
            shared_output(Path::new("words.la"), Target::Aarch64Android),
            PathBuf::from("libwords.so")
        );
        assert_eq!(
            shared_output(Path::new("words.la"), Target::Aarch64MacOs),
            PathBuf::from("libwords.dylib")
        );
        assert_eq!(
            shared_output(Path::new("words.la"), Target::X86_64Windows),
            PathBuf::from("words.dll")
        );
    }
}
