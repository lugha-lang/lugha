//! Open documents, the reader that prefers their unsaved text to the disk,
//! and the `file:` URIs that name them.

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The documents the editor has open, by normalized path.
#[derive(Default, Clone)]
pub(super) struct Documents {
    open: HashMap<PathBuf, String>,
}

impl Documents {
    /// Records `path`'s text, opened or changed.
    pub(super) fn set(&mut self, path: &Path, text: String) {
        self.open.insert(normalize(path), text);
    }

    /// Forgets `path`: the disk is its text again.
    pub(super) fn close(&mut self, path: &Path) {
        self.open.remove(&normalize(path));
    }

    /// The open text of `path`, if the editor has it open.
    pub(super) fn text(&self, path: &Path) -> Option<&str> {
        self.open.get(&normalize(path)).map(String::as_str)
    }

    /// Every open document's path.
    pub(super) fn paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self.open.keys().cloned().collect();
        paths.sort();
        paths
    }

    /// Reads `file` as the module loader wants it: the open text, else the disk.
    pub(super) fn read(&self, file: &Path) -> io::Result<Vec<u8>> {
        match self.text(file) {
            Some(text) => Ok(text.as_bytes().to_vec()),
            None => std::fs::read(file),
        }
    }
}

/// Resolves `.` and `..` without touching the disk, so `src/../geo.la` from
/// the loader and `geo.la` from the editor are one document.
pub(super) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// The path of a `file:` URI (`file:///a%20b/x.la` → `/a b/x.la`, and on
/// Windows `file:///c%3A/a/x.la` → `C:\a\x.la`); `None` for any other
/// scheme, a remote host or bad percent-encoding.
pub(super) fn uri_to_path(uri: &str) -> Option<PathBuf> {
    uri_to_path_on(uri, cfg!(windows))
}

/// [`uri_to_path`], on Windows or not.
fn uri_to_path_on(uri: &str, windows: bool) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // `file:///x` has an empty host; `file://localhost/x` names this machine.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let mut text = String::from_utf8(out).ok()?;
    if windows {
        // `/c:/a/x.la`: the drive, upper-cased as Windows' own paths spell
        // it, so the same file compares equal however the editor wrote it.
        let drive = text.as_bytes().get(1).filter(|d| d.is_ascii_alphabetic());
        if text.as_bytes().get(2) != Some(&b':') || drive.is_none() {
            return None;
        }
        text = format!("{}{}", text[1..2].to_ascii_uppercase(), &text[2..]).replace('/', "\\");
    }
    // A `..` in a URI would name a file outside what the editor showed.
    Some(normalize(&PathBuf::from(text)))
}

/// The `file:` URI of an absolute `path`, percent-encoding all but the
/// characters VS Code itself leaves as they are; on Windows in VS Code's
/// own form, `C:\a\x.la` → `file:///c%3A/a/x.la`.
pub(super) fn path_to_uri(path: &Path) -> String {
    path_to_uri_on(&path.to_string_lossy(), cfg!(windows))
}

/// [`path_to_uri`] of the path spelled `text`, on Windows or not.
fn path_to_uri_on(text: &str, windows: bool) -> String {
    let mut uri = String::from("file://");
    let text = if windows && text.as_bytes().get(1) == Some(&b':') {
        format!("/{}{}", text[..1].to_ascii_lowercase(), &text[1..]).replace('\\', "/")
    } else {
        text.to_string()
    };
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip_through_paths() {
        let path = Path::new("/home/me/my proj/é.la");
        let uri = path_to_uri(path);
        assert_eq!(uri, "file:///home/me/my%20proj/%C3%A9.la");
        assert_eq!(uri_to_path(&uri).unwrap(), path);
        assert_eq!(
            uri_to_path("file://localhost/x.la").unwrap(),
            Path::new("/x.la")
        );
        assert_eq!(
            uri_to_path("file:///a/../b.la").unwrap(),
            Path::new("/b.la")
        );
    }

    #[test]
    fn windows_uris_name_a_drive_as_vs_code_writes_them() {
        let uri = path_to_uri_on("C:\\Users\\me\\my proj\\x.la", true);
        assert_eq!(uri, "file:///c%3A/Users/me/my%20proj/x.la");
        for written in [uri.as_str(), "file:///C:/Users/me/my%20proj/x.la"] {
            let path = uri_to_path_on(written, true).unwrap();
            assert_eq!(path.to_string_lossy(), "C:\\Users\\me\\my proj\\x.la");
        }
        assert_eq!(uri_to_path_on("file:///home/x.la", true), None);
        assert_eq!(
            uri_to_path_on("file:///c%3A/x.la", false).unwrap(),
            Path::new("/c:/x.la")
        );
    }

    #[test]
    fn other_uris_have_no_path() {
        assert_eq!(uri_to_path("untitled:Untitled-1"), None);
        assert_eq!(uri_to_path("file://server/share/x.la"), None);
        assert_eq!(uri_to_path("file:///bad%zz.la"), None);
        assert_eq!(uri_to_path("file:///cut%4"), None);
    }

    #[test]
    fn open_text_wins_over_the_disk() {
        let dir = std::env::temp_dir().join(format!("lugha-docs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x.la");
        std::fs::write(&file, "disk").unwrap();
        let mut docs = Documents::default();
        assert_eq!(docs.read(&file).unwrap(), b"disk");
        docs.set(&dir.join("sub/../x.la"), "open".to_string());
        assert_eq!(docs.read(&file).unwrap(), b"open");
        assert_eq!(docs.paths(), vec![file.clone()]);
        docs.close(&file);
        assert_eq!(docs.read(&file).unwrap(), b"disk");
        assert!(docs.read(&dir.join("missing.la")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
