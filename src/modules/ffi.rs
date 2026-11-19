//! The `ffi` permission: which packages declare C code (an `extern` or a
//! `[link]` table), which of them the program's grants cover, and E0333 for
//! the rest. A grant covers its package and every package reachable from it
//! through its manifests' entries. The program's own code and the bundled
//! prelude and std never need one.

use std::collections::{HashMap, VecDeque};

use super::Loaded;
use super::packages::{Packages, STD};
use crate::ast::Item;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// A package that may run C code, and the grant that lets it: the granted
/// package's name (its own, when granted directly).
pub type Holder = (String, String);

/// A package that declares C code: its name, where it first does, and how.
struct Needs {
    package: String,
    at: Span,
    how: String,
}

/// Every package declaring C code, once each, in load order: its first
/// `extern` (or, failing that, the entry that lists a package with a
/// `[link]` table).
fn needing(packages: &Packages, loaded: &Loaded) -> Vec<Needs> {
    let mut found: Vec<Needs> = Vec::new();
    let foreign = |p: &Option<String>| match p {
        Some(name) => name != STD && packages.own.as_ref() != Some(name),
        None => false,
    };
    for module in loaded.modules.iter().filter(|m| foreign(&m.package)) {
        let name = module.package.clone().expect("foreign: a package");
        if found.iter().any(|n| n.package == name) {
            continue;
        }
        let Some(program) = &module.program else {
            continue;
        };
        let first = program.items.iter().find_map(|item| match item {
            Item::Extern(e) => Some((e.span, format!("`extern fun {}`", e.name.name))),
            Item::ExternType(t) => Some((t.span, format!("`extern type {}`", t.name.name))),
            _ => None,
        });
        if let Some((at, how)) = first {
            found.push(Needs {
                package: name,
                at,
                how,
            });
        }
    }
    for link in loaded.links.iter().filter(|l| foreign(&l.package)) {
        let name = link.package.clone().expect("foreign: a package");
        if found.iter().any(|n| n.package == name) {
            continue;
        }
        if let Some(at) = packages.listed_at.get(&name) {
            found.push(Needs {
                package: name,
                at: *at,
                how: "a `[link]` table".to_string(),
            });
        }
    }
    found
}

/// The packages reachable from `start` through the manifests' entries,
/// `start` included.
fn reach(packages: &Packages, start: &str) -> Vec<String> {
    let mut seen = vec![start.to_string()];
    let mut queue = VecDeque::from([start.to_string()]);
    while let Some(next) = queue.pop_front() {
        for listed in packages.listed.get(&Some(next)).into_iter().flatten() {
            if !seen.contains(listed) {
                seen.push(listed.clone());
                queue.push_back(listed.clone());
            }
        }
    }
    seen
}

/// Every package a grant covers, with the grant that does.
fn covered(packages: &Packages) -> HashMap<String, String> {
    let mut covered: HashMap<String, String> = HashMap::new();
    for grant in &packages.grants {
        for package in reach(packages, grant) {
            covered.entry(package).or_insert_with(|| grant.clone());
        }
    }
    covered
}

/// Every package among `of` (each module's package, `None` for the root)
/// other than the program's own and std that no grant covers, with E0333's
/// help for it: such a package may not read C's memory either, which only
/// the checker can see it do (`p.view`).
pub(super) fn denied<'a>(
    packages: &Packages,
    of: impl IntoIterator<Item = &'a Option<String>>,
) -> HashMap<String, String> {
    let covered = covered(packages);
    let mut denied = HashMap::new();
    for name in of.into_iter().flatten() {
        if name == STD || packages.own.as_ref() == Some(name) || covered.contains_key(name) {
            continue;
        }
        denied
            .entry(name.clone())
            .or_insert_with(|| grant_help(packages, name));
    }
    denied
}

/// E0333 for every package declaring C code that no grant covers, added to
/// `loaded`; the packages that may run C, each with the grant that lets it,
/// sorted by name.
pub(super) fn check(packages: &Packages, loaded: &mut Loaded) -> Vec<Holder> {
    let covered = covered(packages);
    let mut holders = Vec::new();
    for needs in needing(packages, loaded) {
        if let Some(through) = covered.get(&needs.package) {
            holders.push((needs.package, through.clone()));
            continue;
        }
        loaded.diagnostics.push(ungranted(packages, &needs));
    }
    holders.sort();
    holders
}

/// E0333 for `needs`, with help naming the program's entry to change.
fn ungranted(packages: &Packages, needs: &Needs) -> Diagnostic {
    Diagnostic::error(
        "E0333",
        format!(
            "package `{}` declares C code ({}) but has no `ffi` permission",
            needs.package, needs.how
        ),
        needs.at,
    )
    .with_help(grant_help(packages, &needs.package))
}

/// E0333's help for package `name`: the program's entry to change, the
/// package's own when the program lists it, else the dependency of the
/// program's that brings it in.
fn grant_help(packages: &Packages, name: &str) -> String {
    let direct = packages.listed.get(&None).into_iter().flatten();
    let brings = direct.clone().find(|d| *d == name).or_else(|| {
        direct
            .clone()
            .find(|d| reach(packages, d).iter().any(|r| r == name))
    });
    match brings {
        Some(d) if d == name => format!(
            "C code can do anything, so the program must trust it: add `allow = [\"ffi\"]` to `{name}`'s entry in lugha.toml"
        ),
        Some(d) => format!(
            "C code can do anything, so the program must trust it: add `allow = [\"ffi\"]` to `{d}`'s entry in lugha.toml (`{name}` is one of its dependencies)"
        ),
        None => "C code can do anything, so the program must trust it: grant it with `allow = [\"ffi\"]` in lugha.toml".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::fetch::Fetcher;
    use std::path::PathBuf;

    /// The program lists `outer` (granted, depending on `report`) and
    /// `plain` (not granted), and is package `app` itself.
    fn packages() -> Packages {
        let mut p = Packages::new(PathBuf::new(), PathBuf::new(), Fetcher::none());
        p.listed
            .insert(None, vec!["outer".to_string(), "plain".to_string()]);
        p.listed
            .insert(Some("outer".to_string()), vec!["report".to_string()]);
        p.grants = vec!["outer".to_string()];
        p.own = Some("app".to_string());
        p
    }

    #[test]
    fn only_packages_no_grant_covers_are_denied_with_their_entry() {
        let of = [
            None,
            Some("app"),
            Some("std"),
            Some("outer"),
            Some("report"),
            Some("plain"),
            Some("plain"),
        ]
        .map(|p| p.map(str::to_string));
        let denied = denied(&packages(), &of);
        assert_eq!(denied.len(), 1, "{denied:?}");
        assert!(
            denied["plain"].contains("to `plain`'s entry in lugha.toml"),
            "{denied:?}"
        );
    }
}
