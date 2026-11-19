//! The program's source text, owned by the lowerer: which module an
//! offset falls in, the text between two offsets, and line and column.

use super::{ModuleSource, SourceInfo};

/// One module's source, owned so the lowerer has no extra lifetime.
pub(super) struct OwnedFile {
    pub name: String,
    base: usize,
    text: String,
}

/// Every module's source, in offset order.
pub(super) struct OwnedSource {
    pub files: Vec<OwnedFile>,
}

impl OwnedSource {
    /// A copy of every module's source.
    pub(super) fn new(sources: &[ModuleSource<'_>]) -> OwnedSource {
        OwnedSource {
            files: sources
                .iter()
                .map(|s| OwnedFile {
                    name: s.name.to_string(),
                    base: s.base,
                    text: s.text.to_string(),
                })
                .collect(),
        }
    }

    /// The file holding `offset`, and `offset` within it.
    pub(super) fn file_at(&self, offset: usize) -> (&OwnedFile, usize) {
        let file = self
            .files
            .iter()
            .rev()
            .find(|f| f.base <= offset)
            .unwrap_or(&self.files[0]);
        (file, offset - file.base.min(offset))
    }

    /// The source text from `start` to `end`, as written.
    pub(super) fn written(&self, start: usize, end: usize) -> &str {
        let (file, local) = self.file_at(start);
        let end = (local + end.saturating_sub(start)).min(file.text.len());
        file.text.get(local.min(end)..end).unwrap_or("")
    }

    pub(super) fn line_col(&self, offset: usize) -> (usize, usize) {
        let (file, local) = self.file_at(offset);
        SourceInfo {
            name: &file.name,
            text: &file.text,
        }
        .line_col(local)
    }
}
