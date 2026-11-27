//! Android's NDK as the C compiler for Android
//! targets: a build for Android compiles the runtime and links with the
//! NDK's `clang --target=<arch>-linux-android29`, against Android's own C
//! library (bionic), since only a program linked against it runs from
//! every Android shell and app. The NDK is the folder `$ANDROID_NDK_HOME`
//! names, else `$ANDROID_NDK_ROOT`, as Android's own tools and Rust's
//! `cargo-ndk` find it; nothing else is searched. Its `clang` is run from
//! an argument list, never through a shell.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::LinkError;

/// The oldest Android programs run on, as its API level: Android 10, the
/// first whose C library has thread-locals of its own (the runtime reads
/// its worker and task through them on every switch).
pub const API: u32 = 29;

/// Where the NDK's releases are, for messages.
pub const DOWNLOAD: &str = "https://developer.android.com/ndk/downloads";

/// The variables naming the NDK, in the order they are read.
pub const VARIABLES: [&str; 2] = ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"];

/// The NDK's folder: the first of [`VARIABLES`] that `env` gives a value
/// that isn't empty; `None` when neither does.
pub fn root(env: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    VARIABLES
        .iter()
        .filter_map(|name| env(name))
        .find(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The NDK's prebuilt tools for the machine lughac runs on: the NDK ships
/// them for x86-64 Linux and Windows and for macOS (one folder for both of
/// a Mac's CPUs).
fn host() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else if cfg!(windows) {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    }
}

/// The NDK's `clang` under the NDK folder `root`.
pub fn clang(root: &Path) -> PathBuf {
    root.join("toolchains/llvm/prebuilt")
        .join(host())
        .join("bin")
        .join(if cfg!(windows) { "clang.exe" } else { "clang" })
}

/// The arguments before lughac's own for `triple` (an Android target's):
/// the target with its API level, which picks bionic's headers and
/// libraries for it.
pub fn args(triple: &str) -> Vec<OsString> {
    vec![OsString::from(format!("--target={triple}{API}"))]
}

/// The arguments the NDK's link takes beyond `cc`'s: every segment
/// aligned to 16 KB, so the program loads on devices with 16 KB pages
/// (Android 15 and newer may have them) as on those with 4 KB.
pub fn link_args() -> Vec<OsString> {
    vec![OsString::from("-Wl,-z,max-page-size=16384")]
}

/// The NDK's `clang` for a build for `target`, found through `env`, and
/// the first line `clang --version` printed (the object cache's key).
///
/// # Errors
///
/// [`LinkError::NdkNotFound`] when no variable names the NDK, or its
/// folder has no `clang`; [`LinkError::Io`] when `clang` can't start.
pub fn find(
    target: &str,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Result<(PathBuf, String), LinkError> {
    let not_found = || LinkError::NdkNotFound(target.to_string());
    let clang = clang(&root(env).ok_or_else(not_found)?);
    if !clang.is_file() {
        return Err(not_found());
    }
    let out = Command::new(&clang)
        .arg("--version")
        .output()
        .map_err(LinkError::Io)?;
    let text = String::from_utf8_lossy(&out.stdout);
    let version = text.lines().next().unwrap_or("").trim().to_string();
    Ok((clang, version))
}

#[cfg(test)]
mod tests {
    use super::{API, args, clang, find, link_args, root};
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn android_ndk_home_comes_before_android_ndk_root() {
        let both = env(&[("ANDROID_NDK_HOME", "/a"), ("ANDROID_NDK_ROOT", "/b")]);
        assert_eq!(root(&both), Some(PathBuf::from("/a")));
        let root_only = env(&[("ANDROID_NDK_ROOT", "/b")]);
        assert_eq!(root(&root_only), Some(PathBuf::from("/b")));
        let empty_home = env(&[("ANDROID_NDK_HOME", ""), ("ANDROID_NDK_ROOT", "/b")]);
        assert_eq!(root(&empty_home), Some(PathBuf::from("/b")));
        assert_eq!(root(&env(&[])), None);
    }

    #[test]
    fn clang_is_under_the_hosts_prebuilt_tools() {
        let path = clang(Path::new("/ndk"));
        assert!(
            path.starts_with("/ndk/toolchains/llvm/prebuilt"),
            "{path:?}"
        );
        assert!(path.ends_with(if cfg!(windows) {
            "bin/clang.exe"
        } else {
            "bin/clang"
        }));
    }

    #[test]
    fn the_target_carries_the_api_level_and_the_link_aligns_to_16_kb() {
        let target = format!("--target=aarch64-linux-android{API}");
        assert_eq!(args("aarch64-linux-android"), [target.as_str()]);
        assert_eq!(link_args(), ["-Wl,-z,max-page-size=16384"]);
    }

    #[test]
    fn no_ndk_and_an_ndk_without_clang_name_both_variables() {
        let none = find("aarch64-linux-android", &env(&[])).unwrap_err();
        let empty = std::env::temp_dir();
        let path = empty.to_str().expect("a UTF-8 temp folder");
        let without = find("aarch64-linux-android", &env(&[("ANDROID_NDK_HOME", path)]));
        for error in [none, without.unwrap_err()] {
            let text = error.to_string();
            assert!(text.contains("aarch64-linux-android"), "{text}");
            assert!(text.contains("$ANDROID_NDK_HOME"), "{text}");
            assert!(text.contains("$ANDROID_NDK_ROOT"), "{text}");
            assert!(text.contains("$LUGHA_CC"), "{text}");
        }
    }
}
