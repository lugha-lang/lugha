//! The Lugha skill's library pages: their shape, and that each names
//! every public item of the std modules it covers and every prelude method
//! of the blocks it covers, so nothing new goes undocumented.

use std::path::Path;

use lugha::skill::{PAGES, field};

#[test]
fn library_pages_have_one_shape_and_long_pages_a_contents_list() {
    for (path, page) in PAGES.iter().filter(|(p, _)| p.starts_with("library/")) {
        for key in ["title", "summary"] {
            assert!(field(page, key).is_some(), "{path} has no {key}");
        }
        for heading in ["## Overview", "## Reference", "## Examples"] {
            assert!(
                page.contains(&format!("\n{heading}\n")),
                "{path} has no `{heading}`"
            );
        }
    }
    for (path, page) in PAGES {
        if page.lines().count() > 300 {
            let top: String = page.lines().take(15).collect::<Vec<_>>().join("\n");
            assert!(
                top.contains("Contents:"),
                "{path} is long and has no contents list"
            );
        }
    }
}

/// Whether `name` appears in `page` as a whole word.
fn names(page: &str, name: &str) -> bool {
    page.match_indices(name).any(|(at, _)| {
        let word = |c: char| c.is_alphanumeric() || c == '_';
        let before = page[..at].chars().next_back().is_none_or(|c| !word(c));
        let after = page[at + name.len()..]
            .chars()
            .next()
            .is_none_or(|c| !word(c));
        before && after
    })
}

/// The page every public item of a std module is documented on: those its
/// `modules` frontmatter lists.
#[test]
fn library_pages_name_every_public_item_of_their_std_modules() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut missing = Vec::new();
    for (path, page) in PAGES.iter().filter(|(p, _)| p.starts_with("library/")) {
        let Some(list) = field(page, "modules") else {
            continue;
        };
        let list = list.trim_start_matches('[').trim_end_matches(']');
        for module in list.split(',').map(str::trim) {
            let source = std::fs::read_to_string(root.join(format!("src/std/{module}.la")))
                .unwrap_or_else(|_| panic!("{path}: no std module `{module}`"));
            for line in source.lines() {
                let Some(rest) = line.trim_start().strip_prefix("public ") else {
                    continue;
                };
                let rest = ["fun ", "struct ", "enum ", "interface ", "const "]
                    .iter()
                    .find_map(|k| rest.strip_prefix(k))
                    .unwrap_or(rest);
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() && !names(page, &name) {
                    missing.push(format!("{path}: std.{module}'s `{name}`"));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "not documented:\n{}",
        missing.join("\n")
    );
}

/// The prelude's methods, by file and the block they are in, and the page
/// that documents them.
const PRELUDE_PAGES: [(&str, &str, &str); 27] = [
    ("core.la", "impl string", "library/text.md"),
    ("text.la", "impl string", "library/text.md"),
    ("text.la", "impl char", "library/text.md"),
    ("iter.la", "impl string", "library/text.md"),
    ("core.la", "impl string[]", "library/text.md"),
    ("core.la", "impl<T> T[]", "library/collections.md"),
    ("iter.la", "impl<T> T[]", "library/collections.md"),
    ("ops.la", "impl<T: Ord> T[]", "library/collections.md"),
    ("core.la", "impl<T> Option<T>", "library/collections.md"),
    (
        "core.la",
        "impl<T, E> Result<T, E>",
        "library/collections.md",
    ),
    ("maps.la", "impl<T> Option<T>", "library/collections.md"),
    (
        "maps.la",
        "impl<T, E> Result<T, E>",
        "library/collections.md",
    ),
    (
        "maps.la",
        "impl<K: Key, V> Map<K, V>",
        "library/collections.md",
    ),
    ("maps.la", "impl<T: Key> Set<T>", "library/collections.md"),
    (
        "capabilities.la",
        "interface Console: Sendable",
        "library/console-and-files.md",
    ),
    (
        "capabilities.la",
        "interface Fs: Sendable",
        "library/console-and-files.md",
    ),
    (
        "capabilities.la",
        "interface Dir: Sendable",
        "library/console-and-files.md",
    ),
    (
        "capabilities.la",
        "interface File: Sendable",
        "library/console-and-files.md",
    ),
    (
        "capabilities.la",
        "interface Clock: Sendable",
        "library/numbers-time-random.md",
    ),
    (
        "capabilities.la",
        "interface Entropy: Sendable",
        "library/numbers-time-random.md",
    ),
    ("net.la", "interface Net: Sendable", "library/net.md"),
    ("net.la", "interface Listener: Sendable", "library/net.md"),
    ("net.la", "interface Conn: Sendable", "library/net.md"),
    ("net.la", "interface Udp: Sendable", "library/net.md"),
    ("narrow.la", "impl OnlyNet", "library/net.md"),
    (
        "capabilities.la",
        "interface Env: Sendable",
        "library/env-and-processes.md",
    ),
    (
        "capabilities.la",
        "interface Proc: Sendable",
        "library/env-and-processes.md",
    ),
];

#[test]
fn library_pages_name_every_prelude_method_they_cover() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/prelude");
    let mut missing = Vec::new();
    for (file, header, page_path) in PRELUDE_PAGES {
        let page = PAGES
            .iter()
            .find(|(p, _)| *p == page_path)
            .map(|(_, text)| *text)
            .unwrap_or_else(|| panic!("no page {page_path}"));
        let source = std::fs::read_to_string(root.join(file)).expect("a prelude file");
        // The block's methods, with whether each is marked `public`.
        let mut methods: Vec<(String, bool)> = Vec::new();
        let mut inside = false;
        let mut found = false;
        for line in source.lines() {
            if line.starts_with(header) && line.trim_end().ends_with('{') {
                (inside, found) = (true, true);
            } else if line.starts_with('}') {
                inside = false;
            } else if inside {
                let (rest, public) = match line.strip_prefix("    public fun ") {
                    Some(rest) => (rest, true),
                    None => match line.strip_prefix("    fun ") {
                        Some(rest) => (rest, false),
                        None => continue,
                    },
                };
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                methods.push((name, public));
            }
        }
        // A block marking some methods `public` keeps the rest to the
        // prelude: only the public ones reach programs.
        let marked = methods.iter().any(|(_, public)| *public);
        for (name, public) in methods {
            if !name.is_empty() && (public || !marked) && !names(page, &name) {
                missing.push(format!("{page_path}: `{name}` ({file}, {header})"));
            }
        }
        assert!(found, "{file} has no block `{header}`");
    }
    assert!(
        missing.is_empty(),
        "not documented:\n{}",
        missing.join("\n")
    );
}
