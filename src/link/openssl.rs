//! An OpenSSL built for a foreign target, for programs loading `std.tls`
//! or `std.crypto`: a folder named by `LUGHA_OPENSSL_<TARGET>` holding its
//! headers and static libraries.
//!
//! This machine's OpenSSL fits builds for this machine only; a build for
//! another platform links the one built for it, by path, so a shared
//! library is never picked up and the program runs with nothing installed.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The files a folder must hold, checked in this order.
const NEEDED: [&str; 3] = ["include/openssl/ssl.h", "lib/libssl.a", "lib/libcrypto.a"];

/// An OpenSSL built for a target: a folder holding [`NEEDED`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder(PathBuf);

/// Why the folder a variable names isn't an OpenSSL to build with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bad {
    /// The path isn't absolute: the build's folder would decide it.
    Relative(PathBuf),
    /// Nothing is there, or it isn't a folder.
    NotAFolder(PathBuf),
    /// The folder lacks this file (one of [`NEEDED`]).
    Missing(PathBuf, &'static str),
}

impl Bad {
    /// The folder, and what's wrong with it, for E0336.
    pub fn explain(&self) -> String {
        match self {
            Bad::Relative(p) => format!("`{}` isn't an absolute path", p.display()),
            Bad::NotAFolder(p) => format!("`{}` isn't a folder", p.display()),
            Bad::Missing(p, file) => format!("`{}` has no `{file}`", p.display()),
        }
    }
}

/// The variable naming `target`'s OpenSSL: `LUGHA_OPENSSL_` and the
/// triple in capitals, `-` written `_`.
pub fn variable(target: &str) -> String {
    format!("LUGHA_OPENSSL_{}", target.to_uppercase().replace('-', "_"))
}

/// The OpenSSL `env` names for `target`: `None` when the variable is
/// unset or empty.
///
/// # Errors
///
/// [`Bad`] when it names something other than a folder holding
/// OpenSSL's headers and both static libraries.
pub fn find(target: &str, env: &dyn Fn(&str) -> Option<OsString>) -> Result<Option<Folder>, Bad> {
    let Some(value) = env(&variable(target)).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let root = PathBuf::from(value);
    if !root.is_absolute() {
        return Err(Bad::Relative(root));
    }
    if !root.is_dir() {
        return Err(Bad::NotAFolder(root));
    }
    if let Some(file) = NEEDED.into_iter().find(|f| !root.join(f).is_file()) {
        return Err(Bad::Missing(root, file));
    }
    Ok(Some(Folder(root)))
}

impl Folder {
    /// The folder itself, for messages.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The runtime's compile flag for OpenSSL's headers.
    pub fn include(&self) -> OsString {
        let mut flag = OsString::from("-I");
        flag.push(self.0.join("include"));
        flag
    }
}

/// The link's OpenSSL arguments: none without TLS or crypto; this
/// machine's `-lssl -lcrypto` without a `folder`; its static libraries by
/// path with one. TLS needs both, crypto only `libcrypto`.
pub fn link_args(folder: Option<&Folder>, tls: bool, crypto: bool) -> Vec<OsString> {
    let libs: &[&str] = match (tls, crypto) {
        (true, _) => &["ssl", "crypto"],
        (false, true) => &["crypto"],
        (false, false) => &[],
    };
    libs.iter()
        .map(|lib| match folder {
            Some(f) => f.0.join(format!("lib/lib{lib}.a")).into_os_string(),
            None => OsString::from(format!("-l{lib}")),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh folder holding `files`, removed on drop.
    struct Temp(PathBuf);

    impl Temp {
        fn with(files: &[&str]) -> Temp {
            // Tests run in parallel: each folder gets its own number.
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let i = NEXT.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("lugha-openssl-{}-{i}", std::process::id()));
            for file in files {
                let path = root.join(file);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, b"").unwrap();
            }
            std::fs::create_dir_all(&root).unwrap();
            Temp(root)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            // Best effort: a leftover temp folder is harmless.
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn env_naming(value: &Path) -> impl Fn(&str) -> Option<OsString> {
        let value = value.as_os_str().to_owned();
        move |k| (k == "LUGHA_OPENSSL_AARCH64_APPLE_DARWIN").then(|| value.clone())
    }

    const MAC: &str = "aarch64-apple-darwin";

    #[test]
    fn the_variable_is_the_target_in_capitals() {
        assert_eq!(variable(MAC), "LUGHA_OPENSSL_AARCH64_APPLE_DARWIN");
        assert_eq!(
            variable("x86_64-unknown-linux-musl"),
            "LUGHA_OPENSSL_X86_64_UNKNOWN_LINUX_MUSL"
        );
    }

    #[test]
    fn unset_or_empty_names_nothing() {
        assert_eq!(find(MAC, &|_| None), Ok(None));
        assert_eq!(find(MAC, &|_| Some(OsString::new())), Ok(None));
    }

    #[test]
    fn a_complete_folder_is_found() {
        let t = Temp::with(&NEEDED);
        assert_eq!(find(MAC, &env_naming(&t.0)), Ok(Some(Folder(t.0.clone()))));
    }

    #[test]
    fn a_relative_path_is_refused() {
        let found = find(MAC, &env_naming(Path::new("openssl")));
        assert_eq!(found, Err(Bad::Relative("openssl".into())));
        assert!(
            found
                .unwrap_err()
                .explain()
                .contains("isn't an absolute path")
        );
    }

    #[test]
    fn a_missing_folder_is_refused() {
        let t = Temp::with(&[]);
        let gone = t.0.join("nothing");
        assert_eq!(
            find(MAC, &env_naming(&gone)),
            Err(Bad::NotAFolder(gone.clone()))
        );
    }

    #[test]
    fn each_missing_file_is_named() {
        for (i, file) in NEEDED.iter().enumerate() {
            let others: Vec<&str> = NEEDED
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, f)| *f)
                .collect();
            let t = Temp::with(&others);
            let found = find(MAC, &env_naming(&t.0));
            assert_eq!(found, Err(Bad::Missing(t.0.clone(), file)));
            assert!(found.unwrap_err().explain().contains(file));
        }
    }

    #[test]
    fn a_folder_gives_its_headers_and_static_libraries_by_path() {
        let f = Folder(PathBuf::from("/o"));
        assert_eq!(f.include(), OsString::from("-I/o/include"));
        assert_eq!(
            link_args(Some(&f), true, false),
            ["/o/lib/libssl.a", "/o/lib/libcrypto.a"]
        );
        assert_eq!(
            link_args(Some(&f), true, true),
            ["/o/lib/libssl.a", "/o/lib/libcrypto.a"]
        );
        assert_eq!(link_args(Some(&f), false, true), ["/o/lib/libcrypto.a"]);
        assert!(link_args(Some(&f), false, false).is_empty());
    }

    #[test]
    fn without_a_folder_this_machines_libraries_are_named() {
        assert_eq!(link_args(None, true, false), ["-lssl", "-lcrypto"]);
        assert_eq!(link_args(None, false, true), ["-lcrypto"]);
    }
}
