//! The runtime source each program is linked with, written to a temp file:
//! [`super::RUNTIME_SOURCE`], then [`TLS_SOURCE`] for a program that loads
//! `std.tls` (linking `-lssl -lcrypto`) and
//! [`CRYPTO_SOURCE`] for one that loads `std.crypto` (linking `-lcrypto`).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::LinkError;
use super::compiler::Compiler;
use super::object_cache::Cache;
use super::openssl::Folder;
use crate::span::Span;

/// `runtime/openssl/tls.c`, `openssl/tls_trust.c` (the system's verifier)
/// and `openssl/tls_keys.c` (connections by key):
/// `std.tls` over OpenSSL, only for programs that use it.
pub const TLS_SOURCE: &str = concat!(
    include_str!("../../runtime/openssl/tls.c"),
    include_str!("../../runtime/openssl/tls_trust.c"),
    include_str!("../../runtime/openssl/tls_keys.c")
);

/// Every function codegen calls in [`TLS_SOURCE`].
pub const TLS_SYMBOLS: [&str; 12] = [
    "lugha_rt_tls_client_context",
    "lugha_rt_tls_server_context",
    "lugha_rt_tls_key_context",
    "lugha_rt_tls_free_context",
    "lugha_rt_tls_set_accept",
    "lugha_rt_tls_new",
    "lugha_rt_tls_step",
    "lugha_rt_tls_outgoing",
    "lugha_rt_tls_incoming",
    "lugha_rt_tls_incoming_end",
    "lugha_rt_tls_free",
    "lugha_rt_tls_peer_key",
];

/// `runtime/openssl/crypto.c` and `openssl/crypto_hpke.c` (HPKE): `std.crypto`
/// over OpenSSL, only for programs that use it.
pub const CRYPTO_SOURCE: &str = concat!(
    include_str!("../../runtime/openssl/crypto.c"),
    include_str!("../../runtime/openssl/crypto_hpke.c")
);

/// Every function codegen calls in [`CRYPTO_SOURCE`].
pub const CRYPTO_SYMBOLS: [&str; 15] = [
    "lugha_rt_crypto_sha256",
    "lugha_rt_crypto_sha256_new",
    "lugha_rt_crypto_sha256_update",
    "lugha_rt_crypto_sha256_finish",
    "lugha_rt_crypto_ed25519_public",
    "lugha_rt_crypto_ed25519_sign",
    "lugha_rt_crypto_ed25519_verify",
    "lugha_rt_crypto_equal",
    "lugha_rt_crypto_hkdf",
    "lugha_rt_crypto_seal",
    "lugha_rt_crypto_open",
    "lugha_rt_crypto_x25519_public",
    "lugha_rt_crypto_x25519_shared",
    "lugha_rt_crypto_hpke_seal",
    "lugha_rt_crypto_hpke_open",
];

/// True if `cc`'s complaint is OpenSSL's absence (its headers or
/// libraries) or its age: the runtime's `#error` for one older than 3.2.
pub fn missing_openssl(stderr: &str) -> bool {
    stderr.contains("openssl/")
        || stderr.contains("-lssl")
        || stderr.contains("-lcrypto")
        || stderr.contains("Lugha needs OpenSSL 3.2 or newer")
}

/// The runtime source in a private temp file, removed on drop.
pub(super) struct RuntimeFile(pub PathBuf);

impl RuntimeFile {
    /// Writes the runtime for `system`, with TLS's part when `tls` and
    /// crypto's when `crypto`.
    pub(super) fn write(tls: bool, crypto: bool, system: super::System) -> io::Result<Self> {
        Self::write_in(&std::env::temp_dir(), tls, crypto, system)
    }

    /// [`RuntimeFile::write`] in `folder`.
    fn write_in(folder: &Path, tls: bool, crypto: bool, system: super::System) -> io::Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let pid = std::process::id();
        let mut tries = 0;
        let (path, mut file) = loop {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = folder.join(format!("lughac-rt-{pid}-{n}.c"));
            // create_new: never reuse a file someone planted under that name.
            // One left by an earlier process with this id (Windows reuses ids
            // quickly) is passed over for the next name.
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => break (path, file),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && tries < 1000 => tries += 1,
                Err(e) => return Err(e),
            }
        };
        io::Write::write_all(&mut file, super::runtime_source(system).as_bytes())?;
        if tls {
            io::Write::write_all(&mut file, TLS_SOURCE.as_bytes())?;
        }
        if crypto {
            io::Write::write_all(&mut file, CRYPTO_SOURCE.as_bytes())?;
        }
        Ok(RuntimeFile(path))
    }
}

/// The runtime compiled to an object: the cache's, or
/// one compiled beside its source; the temporary files go on drop.
pub(super) struct RuntimeObject {
    pub path: PathBuf,
    _source: RuntimeFile,
    temp: [PathBuf; 2],
}

impl Drop for RuntimeObject {
    fn drop(&mut self) {
        for file in &self.temp {
            // Best effort: a leftover temp file is harmless and must not hide the result.
            let _ = std::fs::remove_file(file);
        }
    }
}

/// The runtime's object, with TLS's part when `tls` and crypto's when
/// `crypto`, for a shared library when `library`: from `cache` when it has a
/// fresh one, else `cc -c` of the runtime alone. `openssl` is where a TLS or crypto program asked for
/// OpenSSL, for E0332, and the folder of a foreign target's OpenSSL, whose
/// headers are compiled with.
///
/// # Errors
///
/// [`LinkError::Tls`] when the compile fails for OpenSSL's headers,
/// [`LinkError::Failed`] for any other failure, and [`LinkError::CcNotFound`]
/// or [`LinkError::Io`] when `cc` or the source file can't be used.
pub(super) fn compile(
    (tls, crypto): (bool, bool),
    cache: &Cache,
    openssl: Option<(Span, Option<&Folder>)>,
    (system, library): (super::System, bool),
) -> Result<RuntimeObject, LinkError> {
    let source = RuntimeFile::write(tls, crypto, system).map_err(LinkError::Io)?;
    let object = source.0.with_extension("o");
    let temp = [object.clone(), object.with_extension("d")];
    let mut text = super::runtime_source(system).as_bytes().to_vec();
    if tls {
        text.extend_from_slice(TLS_SOURCE.as_bytes());
    }
    if crypto {
        text.extend_from_slice(CRYPTO_SOURCE.as_bytes());
    }
    // Optimized: every program's allocation, strings, maps and channels
    // run through this code.
    // Unwind tables: a panic unwinds through the runtime's own frames.
    let mut flags = vec![std::ffi::OsString::from("-O2"), "-funwind-tables".into()];
    // Position-independent, so a shared library can hold it too (it costs
    // an executable nothing); Windows' code needs no such flag.
    if system != super::System::Windows {
        flags.push("-fPIC".into());
    }
    // A shared library's runtime finds its thread-locals as a library must
    // (os/linux/os.c): part of the flags, so a cached object of its own.
    if library {
        flags.push("-DLUGHA_RT_LIBRARY".into());
    }
    // The folder is part of the flags, so of the cached object's key.
    let folder = openssl.and_then(|(_, folder)| folder);
    flags.extend(folder.map(Folder::include));
    let openssl = openssl.map(|(at, _)| (at, folder.map(|f| f.path().to_path_buf())));
    let compile = |extra: &[std::ffi::OsString]| {
        let all: Vec<std::ffi::OsString> = flags.iter().chain(extra).cloned().collect();
        run(cache.cc(), &source.0, &all, &object, openssl.clone())
    };
    let path = cache.object("runtime", &text, &flags, &object, compile)?;
    Ok(RuntimeObject {
        path,
        _source: source,
        temp,
    })
}

/// `cc -c source flags -o object`, `cc` being the build's compiler.
fn run(
    cc: &Compiler,
    source: &Path,
    extra: &[std::ffi::OsString],
    object: &Path,
    openssl: Option<(Span, Option<PathBuf>)>,
) -> Result<(), LinkError> {
    let result = cc
        .command()
        .arg("-c")
        .arg(source)
        .args(extra)
        .arg("-o")
        .arg(object)
        .output();
    let out = match result {
        Ok(out) => out,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(LinkError::CcNotFound(cc.name()));
        }
        Err(e) => return Err(LinkError::Io(e)),
    };
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    match openssl {
        Some((span, folder)) if missing_openssl(&stderr) => Err(LinkError::Tls { span, folder }),
        _ => Err(LinkError::Failed {
            status: out.status.code(),
            stderr,
        }),
    }
}

impl Drop for RuntimeFile {
    fn drop(&mut self) {
        // Best effort: a leftover temp file is harmless and must not hide the result.
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CRYPTO_SOURCE, CRYPTO_SYMBOLS, RuntimeFile, TLS_SOURCE, TLS_SYMBOLS, missing_openssl,
    };

    #[test]
    fn the_tls_part_defines_its_symbols_and_only_tls_programs_get_it() {
        for name in TLS_SYMBOLS {
            assert!(
                TLS_SOURCE.contains(&format!("{name}(")),
                "tls.c lacks {name}"
            );
        }
        assert!(!super::super::RUNTIME_SOURCE.contains("openssl"));
        for name in CRYPTO_SYMBOLS {
            assert!(
                CRYPTO_SOURCE.contains(&format!("{name}(")),
                "crypto.c lacks {name}"
            );
        }
        let read = |tls, crypto| {
            let file = RuntimeFile::write(tls, crypto, super::super::System::Linux).unwrap();
            std::fs::read_to_string(&file.0).unwrap()
        };
        assert!(!read(false, false).contains("openssl"));
        let tls = read(true, false);
        assert!(tls.contains("#include <openssl/ssl.h>") && !tls.contains("openssl/hpke.h"));
        // The system's verifier is joined with TLS, and only a Mac asks it.
        assert!(tls.contains("tls_trust_verify") && tls.contains("#define OS_TLS_TRUST 0"));
        let crypto = read(false, true);
        assert!(crypto.contains("#include <openssl/hpke.h>") && !crypto.contains("openssl/ssl.h"));
    }

    #[test]
    fn the_runtime_file_passes_over_names_already_taken() {
        let folder = std::env::temp_dir().join(format!("lughac-rt-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let system = super::super::System::Linux;
        let first = RuntimeFile::write_in(&folder, false, false, system).unwrap();
        let name = first.0.file_stem().unwrap().to_string_lossy().into_owned();
        let (prefix, n) = name.rsplit_once('-').unwrap();
        let n: usize = n.parse().unwrap();
        // The next few names, as a leftover from an earlier process with
        // this id would have taken them.
        for next in n + 1..n + 4 {
            std::fs::write(folder.join(format!("{prefix}-{next}.c")), "").unwrap();
        }
        let fresh = RuntimeFile::write_in(&folder, false, false, system).unwrap();
        assert!(!std::fs::read_to_string(&fresh.0).unwrap().is_empty());
        drop((first, fresh));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn openssl_missing_is_recognised() {
        assert!(missing_openssl(
            "fatal error: openssl/ssl.h: No such file or directory"
        ));
        assert!(missing_openssl(
            "/usr/bin/ld: cannot find -lssl: No such file"
        ));
        assert!(missing_openssl(
            "lugha_rt.c:4100:2: error: #error \"Lugha needs OpenSSL 3.2 or newer\""
        ));
        assert!(!missing_openssl("undefined reference to `foo'"));
    }

    #[test]
    fn only_a_tls_plan_links_openssl() {
        use std::path::Path;
        let mut plan = super::super::Plan::default();
        let has_ssl = |plan: &super::super::Plan| {
            super::super::args(&[], &[], &[Path::new("rt.o")], plan, Path::new("out"))
                .iter()
                .any(|a| a == "-lssl")
        };
        assert!(!has_ssl(&plan));
        plan.crypto = Some(crate::span::Span::new(0, 1));
        let args = super::super::args(&[], &[], &[Path::new("rt.o")], &plan, Path::new("out"));
        assert!(args.iter().any(|a| a == "-lcrypto") && !has_ssl(&plan));
        plan.tls = Some(crate::span::Span::new(0, 1));
        assert!(has_ssl(&plan));
        let d = super::super::LinkError::Tls {
            span: crate::span::Span::new(0, 1),
            folder: None,
        }
        .diagnostic();
        assert_eq!(d.map(|d| d.code), Some("E0332"));
        // A foreign target's OpenSSL: the help names its folder.
        let d = super::super::LinkError::Tls {
            span: crate::span::Span::new(0, 1),
            folder: Some(std::path::PathBuf::from("/o")),
        }
        .diagnostic()
        .unwrap();
        assert_eq!(d.code, "E0332");
        assert!(format!("{d:?}").contains("`/o` is older than 3.2"), "{d:?}");
    }
}
