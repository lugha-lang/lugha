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
use crate::span::Span;

/// `runtime/openssl/tls.c` and `openssl/tls_keys.c` (connections by key):
/// `std.tls` over OpenSSL, only for programs that use it.
pub const TLS_SOURCE: &str = concat!(
    include_str!("../../runtime/openssl/tls.c"),
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
    /// Writes the runtime, with TLS's part when `tls` and crypto's when `crypto`.
    pub(super) fn write(tls: bool, crypto: bool) -> io::Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("lughac-rt-{}-{n}.c", std::process::id()));
        // create_new: fail rather than reuse a file someone planted under that name.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        io::Write::write_all(&mut file, super::RUNTIME_SOURCE.as_bytes())?;
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
/// `crypto`: from `cache` when it has a fresh one, else `cc -c` of the
/// runtime alone. `openssl` is where a TLS or crypto program asked for
/// OpenSSL, for E0332.
///
/// # Errors
///
/// [`LinkError::Tls`] when the compile fails for OpenSSL's headers,
/// [`LinkError::Failed`] for any other failure, and [`LinkError::CcNotFound`]
/// or [`LinkError::Io`] when `cc` or the source file can't be used.
pub(super) fn compile(
    (tls, crypto): (bool, bool),
    cache: &Cache,
    openssl: Option<Span>,
) -> Result<RuntimeObject, LinkError> {
    let source = RuntimeFile::write(tls, crypto).map_err(LinkError::Io)?;
    let object = source.0.with_extension("o");
    let temp = [object.clone(), object.with_extension("d")];
    let mut text = super::RUNTIME_SOURCE.as_bytes().to_vec();
    if tls {
        text.extend_from_slice(TLS_SOURCE.as_bytes());
    }
    if crypto {
        text.extend_from_slice(CRYPTO_SOURCE.as_bytes());
    }
    // Optimized: every program's allocation, strings, maps and channels
    // run through this code.
    // Unwind tables: a panic unwinds through the runtime's own frames.
    let flags = vec![std::ffi::OsString::from("-O2"), "-funwind-tables".into()];
    let compile = |extra: &[std::ffi::OsString]| {
        let all: Vec<std::ffi::OsString> = flags.iter().chain(extra).cloned().collect();
        run(cache.cc(), &source.0, &all, &object, openssl)
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
    openssl: Option<Span>,
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
        Some(span) if missing_openssl(&stderr) => Err(LinkError::Tls { span }),
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
            let file = RuntimeFile::write(tls, crypto).unwrap();
            std::fs::read_to_string(&file.0).unwrap()
        };
        assert!(!read(false, false).contains("openssl"));
        let tls = read(true, false);
        assert!(tls.contains("#include <openssl/ssl.h>") && !tls.contains("openssl/hpke.h"));
        let crypto = read(false, true);
        assert!(crypto.contains("#include <openssl/hpke.h>") && !crypto.contains("openssl/ssl.h"));
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
        }
        .diagnostic();
        assert_eq!(d.map(|d| d.code), Some("E0332"));
    }
}
