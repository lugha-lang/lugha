//! The Lugha skill (`skill/`): every page is well formed, every example in
//! it compiles and prints what the page says, and `lughac skill` reads it.

#[path = "common/cli.rs"]
mod cli;

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use cli::{TempDir, lughac, text};
use lugha::skill::{NAME, PAGES, field, short_name};

/// A fenced block: its info string (after the backticks), its text, and
/// the line it starts on.
struct Block {
    info: String,
    code: String,
    line: usize,
}

fn blocks(page: &str) -> Vec<Block> {
    let mut found = Vec::new();
    let mut open: Option<Block> = None;
    for (i, line) in page.lines().enumerate() {
        match (&mut open, line.strip_prefix("```")) {
            (None, Some(info)) => {
                open = Some(Block {
                    info: info.trim().to_string(),
                    code: String::new(),
                    line: i + 1,
                });
            }
            (Some(_), Some("")) => found.extend(open.take()),
            (Some(block), _) => {
                block.code.push_str(line);
                block.code.push('\n');
            }
            (None, None) => {}
        }
    }
    assert!(open.is_none(), "a block is never closed");
    found
}

#[test]
fn the_shipped_pages_are_exactly_the_skill_folder() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("skill");
    let mut on_disk = BTreeSet::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the skill folder is readable") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(&root).expect("inside the skill");
                on_disk.insert(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let shipped: BTreeSet<String> = PAGES.iter().map(|(p, _)| (*p).to_string()).collect();
    assert_eq!(on_disk, shipped);
    for (path, page) in PAGES {
        let file = std::fs::read_to_string(root.join(path)).expect("readable");
        assert_eq!(file, page, "{path} differs from what is built in");
    }
}

#[test]
fn the_overview_follows_the_agent_skills_format() {
    let (_, overview) = PAGES[0];
    assert_eq!(field(overview, "name"), Some(NAME));
    let description = field(overview, "description").expect("a description");
    assert!(!description.is_empty() && description.len() <= 1024);
    assert!(!description.contains('<'), "no XML tags in the description");
    assert!(
        overview.lines().count() <= 500,
        "SKILL.md is over 500 lines"
    );
    // Every page is linked from the overview, one level deep.
    for (path, _) in &PAGES[1..] {
        assert!(
            overview.contains(&format!("({path})")),
            "SKILL.md doesn't link {path}"
        );
    }
}

#[test]
fn concept_pages_have_one_shape_and_earlier_prerequisites() {
    let mut seen: Vec<&str> = Vec::new();
    for (path, page) in PAGES.iter().filter(|(p, _)| p.starts_with("concepts/")) {
        for key in ["title", "summary", "prerequisites"] {
            assert!(field(page, key).is_some(), "{path} has no {key}");
        }
        let list = field(page, "prerequisites").expect("checked");
        let inner = list
            .strip_prefix('[')
            .and_then(|l| l.strip_suffix(']'))
            .unwrap_or_else(|| panic!("{path}: prerequisites are a [list]"));
        for name in inner.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            assert!(
                seen.contains(&name),
                "{path}: `{name}` isn't an earlier concept"
            );
        }
        for heading in [
            "## The idea",
            "## Example",
            "## Rules",
            "## Mistakes",
            "## Exercises",
        ] {
            assert!(
                page.contains(&format!("\n{heading}\n")),
                "{path} has no `{heading}`"
            );
        }
        seen.push(short_name(path));
    }
    assert!(seen.len() >= 5);
}

#[test]
fn pages_never_point_at_the_spec_or_history() {
    for (path, page) in PAGES {
        let lower = page.to_lowercase();
        for word in ["specification", "milestone", "prp-", "lughac spec"] {
            assert!(!lower.contains(word), "{path} mentions `{word}`");
        }
    }
}

/// The files a `lugha file NAME` block gave, waiting for the next example.
type Files = Vec<(String, String)>;

/// A fresh folder holding `main.la` and `files`, at their paths.
fn folder(code: &str, files: &Files) -> TempDir {
    let dir = TempDir::with(&[("main.la", code.as_bytes())]);
    for (name, text) in files {
        let path = dir.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("temp dir is writable");
        }
        std::fs::write(path, text).expect("temp dir is writable");
    }
    dir
}

/// Builds and runs `code` with `files`; its exit code, stdout and stderr.
fn run(code: &str, files: &Files) -> (Option<i32>, String, String) {
    let dir = folder(code, files);
    let built = lughac(&dir.0, &["build", "main.la", "-o", "main"]);
    if !built.status.success() {
        return (built.status.code(), String::new(), text(&built.stderr));
    }
    let out = Command::new(dir.0.join("main"))
        .current_dir(&dir.0)
        .env("LUGHA_WORKERS", "1")
        .output()
        .expect("the example runs");
    (out.status.code(), text(&out.stdout), text(&out.stderr))
}

/// What is wrong with one block, if anything:
/// - ```` ```lugha ```` checks; followed by ```` ```output ```` it runs and
///   prints exactly that (exit 0, or N for ```` ```lugha exit N ````);
/// - ```` ```lugha error E0xxx ```` fails with that code;
/// - ```` ```lugha file NAME ```` is written as `NAME` beside the next
///   example's `main.la` (kept in `files` until then);
/// - `output`, `sh`, `text` and `toml` blocks are shown, not checked.
fn wrong(block: &Block, next: Option<&Block>, files: &mut Files) -> Option<String> {
    let words: Vec<&str> = block.info.split_whitespace().collect();
    if let ["lugha", "file", name] = words.as_slice() {
        files.push(((*name).to_string(), block.code.clone()));
        return None;
    }
    if !matches!(words.first(), Some(&"lugha")) {
        return match words.as_slice() {
            ["output"] | ["sh"] | ["text"] | ["toml"] => None,
            _ => Some(format!("unknown block info `{}`", block.info)),
        };
    }
    // This example takes the files given before it.
    let files = std::mem::take(files);
    let check = |code: &str| {
        let dir = folder(code, &files);
        lughac(&dir.0, &["check", "main.la"])
    };
    match words.as_slice() {
        ["lugha", "error", code] => {
            let out = check(&block.code);
            let shown = format!("error[{code}]");
            (out.status.code() != Some(1) || !text(&out.stderr).contains(&shown))
                .then(|| format!("should fail with {code}: {}", text(&out.stderr)))
        }
        ["lugha", rest @ ..] => {
            let exit = match rest {
                [] => 0,
                ["exit", n] => n.parse().ok()?,
                _ => return Some(format!("unknown block info `{}`", block.info)),
            };
            match next.filter(|b| b.info == "output") {
                Some(output) => {
                    let (code, stdout, stderr) = run(&block.code, &files);
                    (code != Some(exit) || stdout != output.code).then(|| {
                        format!(
                            "exit {code:?} (want {exit}), stdout:\n{stdout}want:\n{}{stderr}",
                            output.code
                        )
                    })
                }
                None => {
                    let out = check(&block.code);
                    (!out.status.success()).then(|| text(&out.stderr))
                }
            }
        }
        _ => Some(format!("unknown block info `{}`", block.info)),
    }
}

#[test]
fn every_example_compiles_and_prints_what_the_page_says() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for (path, page) in PAGES {
        let all = blocks(page);
        let mut files = Files::new();
        for (i, block) in all.iter().enumerate() {
            if let Some(why) = wrong(block, all.get(i + 1), &mut files) {
                failures.push(format!("{path}:{}: {why}", block.line));
            }
            checked += 1;
        }
        if !files.is_empty() {
            failures.push(format!(
                "{path}: a `lugha file` block has no example after it"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
    assert!(checked >= 20, "only {checked} blocks");
}

#[test]
fn skill_prints_the_overview_or_a_page() {
    let dir = TempDir::with(&[]);
    let out = lughac(&dir.0, &["skill"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), PAGES[0].1);
    for name in ["functions", "concepts/02-functions"] {
        let out = lughac(&dir.0, &["skill", name]);
        assert_eq!(out.status.code(), Some(0), "{name}");
        assert_eq!(text(&out.stdout), PAGES[2].1, "{name}");
    }
    let out = lughac(&dir.0, &["skill", "funcs"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("did you mean: functions?"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn skill_lists_and_searches() {
    let dir = TempDir::with(&[]);
    let out = lughac(&dir.0, &["skill", "--list"]);
    assert_eq!(out.status.code(), Some(0));
    let listed = text(&out.stdout);
    let names: Vec<&str> = listed
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(names[0], "SKILL");
    assert_eq!(names[1], "concepts/01-values-and-types");
    assert_eq!(names.len(), PAGES.len());
    let out = lughac(&dir.0, &["skill", "--search", "LET MUT"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out.stdout).contains("concepts/01-values-and-types:"));
    let out = lughac(&dir.0, &["skill", "--search", "no such words 98765"]);
    assert_eq!((out.status.code(), out.stdout.is_empty()), (Some(1), true));
    let both = lughac(&dir.0, &["skill", "functions", "--list"]);
    assert_eq!(both.status.code(), Some(2));
}

#[test]
fn skill_exports_a_folder_once() {
    let dir = TempDir::with(&[]);
    let out = lughac(&dir.0, &["skill", "--export", "out"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    for (path, page) in PAGES {
        let file = std::fs::read_to_string(dir.0.join("out").join(NAME).join(path));
        assert_eq!(file.expect("exported").as_str(), page);
    }
    let again = lughac(&dir.0, &["skill", "--export", "out"]);
    assert_eq!(again.status.code(), Some(2));
    assert!(text(&again.stderr).contains("already exists"));
}
