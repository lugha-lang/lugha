//! The package cache: where downloaded
//! packages live, `<root>/git/<escaped URL>/<commit>`, and how a fresh
//! clone is put there without ever leaving half a package in place.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The cache's root: `$LUGHA_CACHE`, else `$XDG_CACHE_HOME/lugha`, else
/// `$HOME/.cache/lugha`; on Windows `$LUGHA_CACHE`, else
/// `%LOCALAPPDATA%\lugha`, where Windows tools keep caches. Read through
/// `var`. A value that isn't an absolute path is skipped, so a stray
/// relative setting can't scatter packages into whatever folder lughac
/// runs in.
pub fn root(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    root_on(var, cfg!(windows))
}

/// [`root`], on Windows or not.
fn root_on(var: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<PathBuf> {
    let absolute = |name: &str| var(name).map(PathBuf::from).filter(|p| p.is_absolute());
    let own = absolute("LUGHA_CACHE");
    if windows {
        return own.or_else(|| absolute("LOCALAPPDATA").map(|p| p.join("lugha")));
    }
    own.or_else(|| absolute("XDG_CACHE_HOME").map(|p| p.join("lugha")))
        .or_else(|| absolute("HOME").map(|p| p.join(".cache").join("lugha")))
}

/// The user's home folder, read through `var`: `$HOME`, else on Windows
/// `%USERPROFILE%`, which Windows sets where Unix sets `HOME`. Only an
/// absolute path counts.
pub fn home(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    home_on(var, cfg!(windows))
}

/// [`home`], on Windows or not.
fn home_on(var: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<PathBuf> {
    let absolute = |name: &str| var(name).map(PathBuf::from).filter(|p| p.is_absolute());
    absolute("HOME").or_else(|| windows.then(|| absolute("USERPROFILE")).flatten())
}

/// `url` as one folder name: ASCII letters, digits, `.` and `-` are kept,
/// `_` becomes `__`, and every other byte `_XX` (upper-case hex). Distinct
/// URLs give distinct names, and no name holds `/`, NUL or is `..`.
pub fn escape(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for byte in url.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' => out.push(char::from(byte)),
            // A `.` is kept except where it would start the name, so the
            // result can never be `.` or `..`.
            b'.' if !out.is_empty() => out.push('.'),
            b'_' => out.push_str("__"),
            _ => out.push_str(&format!("_{byte:02X}")),
        }
    }
    out
}

/// Whether `text` is a full commit id: 40 lower-case hex digits.
pub fn is_commit(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Where the package at `url` and `commit` lives under `root`.
///
/// # Panics
///
/// If `commit` isn't a commit id: callers check it first, so a bad one is a bug.
pub fn folder(root: &Path, url: &str, commit: &str) -> PathBuf {
    assert!(is_commit(commit), "commit ids are checked before use");
    root.join("git").join(escape(url)).join(commit)
}

/// A fresh, empty folder under `<root>/tmp` for a clone to go into.
///
/// # Errors
///
/// If the folders can't be created.
pub fn scratch(root: &Path) -> io::Result<PathBuf> {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let tmp = root.join("tmp");
    std::fs::create_dir_all(&tmp)?;
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = COUNT.fetch_add(1, Ordering::Relaxed);
    let dir = tmp.join(format!("clone-{}-{time}-{n}", std::process::id()));
    // `create_dir` fails if the name is taken, so no two clones share one.
    std::fs::create_dir(&dir)?;
    Ok(dir)
}

/// Moves the finished clone `from` (its `.git` removed) to `to`. If another
/// build put the same commit there first, that copy is kept and `from`
/// removed: both hold the same commit.
///
/// # Errors
///
/// If the files can't be removed, created or renamed.
pub fn install(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::remove_dir_all(from.join(".git"))?;
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if to.exists() {
        return std::fs::remove_dir_all(from);
    }
    std::fs::rename(from, to)
}

#[cfg(test)]
mod tests {
    use super::{escape, folder, home_on, is_commit, root, root_on};
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    /// An absolute path on this machine: `/<name>`, or `C:\\<name>` on
    /// Windows, where a path needs a drive to be absolute.
    fn abs(name: &str) -> String {
        if cfg!(windows) {
            format!("C:\\{name}")
        } else {
            format!("/{name}")
        }
    }

    fn vars(pairs: Vec<(&'static str, String)>) -> impl Fn(&str) -> Option<OsString> {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn on_windows_the_root_is_lugha_cache_else_local_app_data() {
        let (c, l, x) = (abs("c"), abs("l"), abs("x"));
        let all = vec![
            ("LUGHA_CACHE", c.clone()),
            ("LOCALAPPDATA", l.clone()),
            ("XDG_CACHE_HOME", x.clone()),
        ];
        assert_eq!(root_on(vars(all), true), Some(PathBuf::from(&c)));
        let local = vec![("LOCALAPPDATA", l.clone()), ("XDG_CACHE_HOME", x.clone())];
        assert_eq!(
            root_on(vars(local.clone()), true),
            Some(PathBuf::from(&l).join("lugha"))
        );
        // Elsewhere LOCALAPPDATA means nothing.
        assert_eq!(
            root_on(vars(local), false),
            Some(PathBuf::from(&x).join("lugha"))
        );
        assert_eq!(
            root_on(vars(vec![("LOCALAPPDATA", "l".into())]), true),
            None
        );
    }

    #[test]
    fn the_home_is_home_else_userprofile_on_windows() {
        let (h, u) = (abs("h"), abs("u"));
        let both = vec![("HOME", h.clone()), ("USERPROFILE", u.clone())];
        assert_eq!(home_on(vars(both), true), Some(PathBuf::from(&h)));
        let profile = vec![("USERPROFILE", u.clone())];
        assert_eq!(
            home_on(vars(profile.clone()), true),
            Some(PathBuf::from(&u))
        );
        assert_eq!(home_on(vars(profile), false), None);
        assert_eq!(home_on(vars(vec![("HOME", "h".into())]), false), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_root_comes_from_the_first_absolute_variable() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        const ALL: [(&str, &str); 3] = [
            ("LUGHA_CACHE", "/c"),
            ("XDG_CACHE_HOME", "/x"),
            ("HOME", "/h"),
        ];
        assert_eq!(root(env(&ALL)), Some(PathBuf::from("/c")));
        assert_eq!(
            root(env(&[("LUGHA_CACHE", "rel"), ("XDG_CACHE_HOME", "/x")])),
            Some(PathBuf::from("/x/lugha"))
        );
        assert_eq!(
            root(env(&[("HOME", "/h")])),
            Some(PathBuf::from("/h/.cache/lugha"))
        );
        assert_eq!(root(env(&[("HOME", "h")])), None);
    }

    #[test]
    fn escaping_is_injective_and_never_a_path() {
        let names = ["a/b", "a_b", "a_2Fb", "a__b", "https://x.org/geo.git"];
        let escaped: Vec<String> = names.iter().map(|n| escape(n)).collect();
        for (i, a) in escaped.iter().enumerate() {
            assert!(!a.contains(['/', '\0']) && a != ".." && a != ".", "{a}");
            assert!(escaped[i + 1..].iter().all(|b| b != a), "{a} repeats");
        }
        assert_eq!(
            escape("https://x.org/geo.git"),
            "https_3A_2F_2Fx.org_2Fgeo.git"
        );
        assert_eq!(escape(".."), "_2E.");
        assert_eq!(escape("é"), "_C3_A9");
    }

    #[test]
    fn commits_are_forty_lower_case_hex_digits() {
        let c = "0123456789abcdef0123456789abcdef01234567";
        assert!(is_commit(c));
        assert!(!is_commit(&c.to_uppercase()));
        assert!(!is_commit(&c[1..]));
        assert!(!is_commit("../../../../../../../../../../etc/passwd/"));
        assert_eq!(
            folder(Path::new("/c"), "file:///r", c),
            PathBuf::from(format!("/c/git/file_3A_2F_2F_2Fr/{c}"))
        );
    }
}
