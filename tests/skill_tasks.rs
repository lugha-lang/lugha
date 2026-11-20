//! The Lugha skill's task pages and example programs: each task page has
//! its shape and names pages that exist; each example builds, runs with no
//! input, and prints exactly the `// Output:` block it ends with.

#[path = "common/cli.rs"]
mod cli;

use std::process::{Command, Stdio};

use cli::{TempDir, lughac, text};
use lugha::skill::{PAGES, field, short_name};

#[test]
fn task_pages_have_one_shape_and_name_pages_that_exist() {
    let shorts: Vec<&str> = PAGES.iter().map(|(p, _)| short_name(p)).collect();
    let tasks: Vec<_> = PAGES
        .iter()
        .filter(|(p, _)| p.starts_with("tasks/"))
        .collect();
    assert!(tasks.len() >= 5, "only {} task pages", tasks.len());
    for (path, page) in tasks {
        for key in ["title", "summary", "uses"] {
            assert!(field(page, key).is_some(), "{path} has no {key}");
        }
        let uses = field(page, "uses").expect("checked");
        let list = uses.trim_start_matches('[').trim_end_matches(']');
        for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            assert!(
                shorts.contains(&name),
                "{path} uses `{name}`, which is no page"
            );
        }
        for heading in [
            "## Goal",
            "## Steps",
            "## The whole program",
            "## Going further",
        ] {
            assert!(
                page.contains(&format!("\n{heading}\n")),
                "{path} has no `{heading}`"
            );
        }
    }
}

/// The text of an example's `// Output:` block: each line after it with
/// its `// ` (or a bare `//`) taken off.
fn expected_output(program: &str) -> Option<String> {
    let at = program.find("\n// Output:\n")?;
    let mut out = String::new();
    for line in program[at + "\n// Output:\n".len()..].lines() {
        let text = line
            .strip_prefix("// ")
            .or_else(|| line.strip_prefix("//"))
            .unwrap_or_else(|| panic!("a line after `// Output:` isn't a comment: {line}"));
        out.push_str(text);
        out.push('\n');
    }
    Some(out)
}

#[test]
fn every_example_prints_its_output_block() {
    let examples: Vec<_> = PAGES
        .iter()
        .filter(|(p, _)| p.starts_with("examples/"))
        .collect();
    assert!(examples.len() >= 5, "only {} examples", examples.len());
    let mut failures = Vec::new();
    for (path, program) in examples {
        assert!(
            program.starts_with("// "),
            "{path} doesn't start with what it does"
        );
        let Some(want) = expected_output(program) else {
            failures.push(format!("{path}: no `// Output:` block"));
            continue;
        };
        let dir = TempDir::with(&[("main.la", program.as_bytes())]);
        let built = lughac(&dir.0, &["build", "main.la", "-o", "main"]);
        if !built.status.success() {
            failures.push(format!("{path}: {}", text(&built.stderr)));
            continue;
        }
        let out = Command::new(dir.0.join("main"))
            .current_dir(&dir.0)
            .stdin(Stdio::null())
            .env("LUGHA_WORKERS", "1")
            .output()
            .expect("the example runs");
        if out.status.code() != Some(0) || text(&out.stdout) != want {
            failures.push(format!(
                "{path}: exit {:?}, stdout:\n{}want:\n{want}{}",
                out.status.code(),
                text(&out.stdout),
                text(&out.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

#[test]
fn the_output_block_reads_comment_lines() {
    let program = "// x\nfun main() {}\n\n// Output:\n// a b\n//\n// c\n";
    assert_eq!(expected_output(program).as_deref(), Some("a b\n\nc\n"));
    assert_eq!(expected_output("fun main() {}\n"), None);
}
