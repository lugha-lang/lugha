//! The Lugha skill: readable pages on writing Lugha (`skill/` in the
//! repository), embedded so `lughac skill` works offline and matches this
//! compiler. Exported, they are an Agent Skill folder named `lugha`.

/// The folder name an exported skill must have: its `name`.
pub const NAME: &str = "lugha";

/// Every page, by its path inside the skill, in teaching order: the
/// overview, the concepts, the library reference, the tasks, then the
/// example programs.
pub const PAGES: [(&str, &str); 34] = [
    ("SKILL.md", include_str!("../skill/SKILL.md")),
    (
        "concepts/01-values-and-types.md",
        include_str!("../skill/concepts/01-values-and-types.md"),
    ),
    (
        "concepts/02-functions.md",
        include_str!("../skill/concepts/02-functions.md"),
    ),
    (
        "concepts/03-structs-and-enums.md",
        include_str!("../skill/concepts/03-structs-and-enums.md"),
    ),
    (
        "concepts/04-match-and-patterns.md",
        include_str!("../skill/concepts/04-match-and-patterns.md"),
    ),
    (
        "concepts/05-errors.md",
        include_str!("../skill/concepts/05-errors.md"),
    ),
    (
        "concepts/06-collections.md",
        include_str!("../skill/concepts/06-collections.md"),
    ),
    (
        "concepts/07-interfaces.md",
        include_str!("../skill/concepts/07-interfaces.md"),
    ),
    (
        "concepts/08-generics.md",
        include_str!("../skill/concepts/08-generics.md"),
    ),
    (
        "concepts/09-iterators.md",
        include_str!("../skill/concepts/09-iterators.md"),
    ),
    (
        "concepts/10-capabilities.md",
        include_str!("../skill/concepts/10-capabilities.md"),
    ),
    (
        "concepts/11-tasks-and-scopes.md",
        include_str!("../skill/concepts/11-tasks-and-scopes.md"),
    ),
    (
        "concepts/12-channels-and-actors.md",
        include_str!("../skill/concepts/12-channels-and-actors.md"),
    ),
    (
        "concepts/13-modules-and-packages.md",
        include_str!("../skill/concepts/13-modules-and-packages.md"),
    ),
    (
        "concepts/14-c-interop.md",
        include_str!("../skill/concepts/14-c-interop.md"),
    ),
    ("library/text.md", include_str!("../skill/library/text.md")),
    (
        "library/collections.md",
        include_str!("../skill/library/collections.md"),
    ),
    (
        "library/console-and-files.md",
        include_str!("../skill/library/console-and-files.md"),
    ),
    (
        "library/numbers-time-random.md",
        include_str!("../skill/library/numbers-time-random.md"),
    ),
    ("library/net.md", include_str!("../skill/library/net.md")),
    ("library/http.md", include_str!("../skill/library/http.md")),
    (
        "library/tls-and-crypto.md",
        include_str!("../skill/library/tls-and-crypto.md"),
    ),
    (
        "library/json-and-toml.md",
        include_str!("../skill/library/json-and-toml.md"),
    ),
    (
        "library/env-and-processes.md",
        include_str!("../skill/library/env-and-processes.md"),
    ),
    (
        "tasks/command-line-tool.md",
        include_str!("../skill/tasks/command-line-tool.md"),
    ),
    (
        "tasks/tcp-server.md",
        include_str!("../skill/tasks/tcp-server.md"),
    ),
    (
        "tasks/http-service.md",
        include_str!("../skill/tasks/http-service.md"),
    ),
    (
        "tasks/wrap-a-c-library.md",
        include_str!("../skill/tasks/wrap-a-c-library.md"),
    ),
    (
        "tasks/testing.md",
        include_str!("../skill/tasks/testing.md"),
    ),
    (
        "examples/word-count.la",
        include_str!("../skill/examples/word-count.la"),
    ),
    (
        "examples/parallel-sum.la",
        include_str!("../skill/examples/parallel-sum.la"),
    ),
    (
        "examples/kv-server.la",
        include_str!("../skill/examples/kv-server.la"),
    ),
    (
        "examples/notes-api.la",
        include_str!("../skill/examples/notes-api.la"),
    ),
    (
        "examples/config.la",
        include_str!("../skill/examples/config.la"),
    ),
];

/// A page's name as `lughac skill` shows it: its path without `.md`, or
/// an example's without `.la`.
pub fn page_name(path: &str) -> &str {
    path.strip_suffix(".md")
        .or_else(|| path.strip_suffix(".la"))
        .unwrap_or(path)
}

/// A page's short name: its last part, without a leading `NN-`.
pub fn short_name(path: &str) -> &str {
    let last = page_name(path).rsplit('/').next().unwrap_or(path);
    match last.split_once('-') {
        Some((n, rest)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => rest,
        _ => last,
    }
}

/// The pages called `name`: the one with that full name
/// (`concepts/02-functions`), else every page with that short one
/// (`functions`), in any case. More than one means `name` is ambiguous.
pub fn find(name: &str) -> Vec<(&'static str, &'static str)> {
    let name = name
        .trim_end_matches(".md")
        .trim_end_matches(".la")
        .to_ascii_lowercase();
    let full: Vec<_> = PAGES
        .iter()
        .filter(|(path, _)| page_name(path).eq_ignore_ascii_case(&name))
        .copied()
        .collect();
    if !full.is_empty() {
        return full;
    }
    PAGES
        .iter()
        .filter(|(path, _)| short_name(path) == name)
        .copied()
        .collect()
}

/// The value of `key` in a page's frontmatter (the lines between the
/// first two `---`), if it has one.
pub fn field<'a>(page: &'a str, key: &str) -> Option<&'a str> {
    let body = page.strip_prefix("---\n")?;
    let end = body.find("\n---\n")?;
    body[..end].lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        (k.trim() == key).then(|| v.trim())
    })
}

/// A page's one-line summary: `summary`, or the overview's `description`.
pub fn summary(page: &str) -> &str {
    field(page, "summary")
        .or_else(|| field(page, "description"))
        // An example program: its first comment line.
        .or_else(|| page.lines().next()?.strip_prefix("// "))
        .unwrap_or("")
}

/// Every line containing `text`, in any case, as (page name, line number
/// from 1, line), in page order.
pub fn search(text: &str) -> Vec<(&'static str, usize, &'static str)> {
    let needle = text.to_lowercase();
    let mut found = Vec::new();
    for (path, page) in PAGES {
        for (i, line) in page.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                found.push((page_name(path), i + 1, line));
            }
        }
    }
    found
}

/// The names closest to `name`, for a page that doesn't exist: those
/// sharing its first three letters, else every short name.
pub fn suggestions(name: &str) -> Vec<&'static str> {
    let start: String = name.to_ascii_lowercase().chars().take(3).collect();
    let close: Vec<&str> = PAGES
        .iter()
        .map(|(path, _)| short_name(path))
        .filter(|short| !start.is_empty() && short.starts_with(&start))
        .collect();
    if close.is_empty() {
        PAGES.iter().map(|(path, _)| short_name(path)).collect()
    } else {
        close
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_found_by_full_or_short_name() {
        let paths = |name: &str| find(name).iter().map(|(p, _)| *p).collect::<Vec<_>>();
        assert_eq!(paths("functions"), ["concepts/02-functions.md"]);
        assert_eq!(paths("concepts/02-functions"), ["concepts/02-functions.md"]);
        assert_eq!(paths("Functions.md"), ["concepts/02-functions.md"]);
        assert_eq!(paths("SKILL"), ["SKILL.md"]);
        assert!(paths("nothing").is_empty());
        // A short name two pages share finds both; a full name finds one.
        assert_eq!(
            paths("collections"),
            ["concepts/06-collections.md", "library/collections.md"]
        );
        assert_eq!(paths("library/collections"), ["library/collections.md"]);
    }

    #[test]
    fn names_drop_the_extension_and_the_number() {
        assert_eq!(
            page_name("concepts/02-functions.md"),
            "concepts/02-functions"
        );
        assert_eq!(short_name("concepts/02-functions.md"), "functions");
        assert_eq!(short_name("tasks/tcp-server.md"), "tcp-server");
        assert_eq!(short_name("SKILL.md"), "SKILL");
        assert_eq!(page_name("examples/word-count.la"), "examples/word-count");
        assert_eq!(short_name("examples/word-count.la"), "word-count");
    }

    #[test]
    fn frontmatter_fields_are_read() {
        let page = "---\ntitle: A\nsummary: One line.\n---\n# A\n";
        assert_eq!(field(page, "title"), Some("A"));
        assert_eq!(summary(page), "One line.");
        assert_eq!(field("# no frontmatter\n", "title"), None);
        assert_eq!(summary("# none\n"), "");
        assert_eq!(
            summary("// Counts words.\nfun main() {}\n"),
            "Counts words."
        );
    }

    #[test]
    fn search_gives_page_line_and_text_in_any_case() {
        let found = search("LET MUT");
        assert!(!found.is_empty());
        assert!(
            found
                .iter()
                .all(|(_, _, l)| l.to_lowercase().contains("let mut"))
        );
        assert!(search("no such text anywhere 12345").is_empty());
    }

    #[test]
    fn unknown_names_get_close_suggestions() {
        assert_eq!(suggestions("funcs"), ["functions"]);
        assert_eq!(suggestions("zzz").len(), PAGES.len());
    }
}
