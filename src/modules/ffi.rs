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

/// E0333 for every package declaring C code that no grant covers, added to
/// `loaded`; the packages that may run C, each with the grant that lets it,
/// sorted by name.
pub(super) fn check(packages: &Packages, loaded: &mut Loaded) -> Vec<Holder> {
    let mut covered: HashMap<String, String> = HashMap::new();
    for grant in &packages.grants {
        for package in reach(packages, grant) {
            covered.entry(package).or_insert_with(|| grant.clone());
        }
    }
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

/// E0333 for `needs`, with help naming the program's entry to change: the
/// package's own when the program lists it, else the dependency of the
/// program's that brings it in.
fn ungranted(packages: &Packages, needs: &Needs) -> Diagnostic {
    let direct = packages.listed.get(&None).into_iter().flatten();
    let brings = direct.clone().find(|d| **d == needs.package).or_else(|| {
        direct
            .clone()
            .find(|d| reach(packages, d).contains(&needs.package))
    });
    let name = &needs.package;
    let help = match brings {
        Some(d) if d == name => format!(
            "C code can do anything, so the program must trust it: add `allow = [\"ffi\"]` to `{name}`'s entry in lugha.toml"
        ),
        Some(d) => format!(
            "C code can do anything, so the program must trust it: add `allow = [\"ffi\"]` to `{d}`'s entry in lugha.toml (`{name}` is one of its dependencies)"
        ),
        None => "C code can do anything, so the program must trust it: grant it with `allow = [\"ffi\"]` in lugha.toml".to_string(),
    };
    Diagnostic::error(
        "E0333",
        format!(
            "package `{name}` declares C code ({}) but has no `ffi` permission",
            needs.how
        ),
        needs.at,
    )
    .with_help(help)
}
