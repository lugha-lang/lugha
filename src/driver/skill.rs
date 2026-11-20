//! `lughac skill`: the overview, one page, the list of pages, a search over
//! them, or the whole skill written out as a folder.

use std::io::Write;
use std::path::Path;

use super::cli::SkillRequest;
use crate::skill::{NAME, PAGES, find, page_name, search, suggestions, summary};

/// Runs `lughac skill`; returns the exit code: 0; 1 when a search finds
/// nothing (as `grep`); 2 for an unknown page or a folder that can't be
/// written.
pub(super) fn run(request: &SkillRequest, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    // Write errors to `stdout` are kept by the writer `driver::run` passes, which turns them
    // into the exit code; an error writing to `stderr` has nowhere to be reported.
    if request.list {
        let width = PAGES
            .iter()
            .map(|(path, _)| page_name(path).len())
            .max()
            .unwrap_or(0);
        for (path, page) in PAGES {
            let _ = writeln!(stdout, "{:width$}  {}", page_name(path), summary(page));
        }
        return 0;
    }
    if let Some(text) = &request.search {
        let found = search(text);
        for (page, line, content) in &found {
            let _ = writeln!(stdout, "{page}:{line}: {content}");
        }
        return u8::from(found.is_empty());
    }
    if let Some(dir) = &request.export {
        return match export(dir) {
            Ok(folder) => {
                let _ = writeln!(stdout, "wrote {}", folder.display());
                0
            }
            Err(why) => {
                let _ = writeln!(stderr, "error: {why}");
                2
            }
        };
    }
    let name = request.page.as_deref().unwrap_or("SKILL");
    match find(name).as_slice() {
        [(_, page)] => {
            let _ = stdout.write_all(page.as_bytes());
            0
        }
        [] => {
            let _ = writeln!(
                stderr,
                "error: no page `{name}`; did you mean: {}? (`lughac skill --list` lists them)",
                suggestions(name).join(", ")
            );
            2
        }
        several => {
            let full: Vec<&str> = several.iter().map(|(path, _)| page_name(path)).collect();
            let _ = writeln!(
                stderr,
                "error: `{name}` names several pages: {}; give the full name",
                full.join(", ")
            );
            2
        }
    }
}

/// Writes every page into `dir/lugha/`, which must not exist yet; the
/// folder written, or why not.
fn export(dir: &Path) -> Result<std::path::PathBuf, String> {
    let folder = dir.join(NAME);
    if folder.exists() {
        return Err(format!(
            "{} already exists; remove it or export elsewhere",
            folder.display()
        ));
    }
    for (path, page) in PAGES {
        // Paths are the embedded pages' own, relative and without `..`.
        let file = folder.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("can't create {}: {e}", parent.display()))?;
        }
        std::fs::write(&file, page).map_err(|e| format!("can't write {}: {e}", file.display()))?;
    }
    Ok(folder)
}
