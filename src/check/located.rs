//! `@caller_location`: a marked function's or method's panics report the
//! place of the call that reached it. The marks are gathered here, by key,
//! for codegen, which gives every marked one three hidden trailing
//! parameters (file, line, column). A mark where no call site is at hand
//! is E0210: an interface's block or a derive block (called through tables
//! and bounds), an actor's message, `main` and a test.

use super::Checker;
use crate::ast::{FunDecl, Item, Program};
use crate::diagnostic::Diagnostic;

/// E0210: `@caller_location` on `what`, which no call site reaches.
fn misplaced(f: &FunDecl, what: &str) -> Option<Diagnostic> {
    let span = f.caller_location?;
    let message = format!("`@caller_location` can't mark {what}");
    let help = "only a function or a type's own method, called where its caller is known, can report its caller's place";
    Some(Diagnostic::error("E0210", message, span).with_help(help))
}

impl Checker {
    /// Pass 2, after methods and blocks: every marked function and method,
    /// E0210 for a mark where it can't apply.
    pub(super) fn collect_located(&mut self, program: &Program) {
        for item in &program.items {
            match item {
                Item::Fun(f) if f.caller_location.is_some() => {
                    let what = match () {
                        _ if f.name.name == "main" && self.module.is_empty() => Some("`main`"),
                        _ if f.test.is_some() => Some("a test"),
                        _ => None,
                    };
                    match what.and_then(|w| misplaced(f, w)) {
                        Some(d) => self.report(d),
                        None => {
                            let key = self.key(&f.name.name);
                            self.located.insert(key);
                        }
                    }
                }
                Item::Impl(i) => {
                    let what = match () {
                        _ if i.interface.is_some() => Some("an interface's item"),
                        _ if i.actor => Some("an actor's message"),
                        _ => None,
                    };
                    for f in i.methods.iter().filter(|f| f.caller_location.is_some()) {
                        if let Some(d) = what.and_then(|w| misplaced(f, w)) {
                            self.report(d);
                            continue;
                        }
                        // The method's key, found by its declaration.
                        let key = self
                            .methods
                            .values()
                            .flat_map(|ms| ms.values())
                            .find(|m| m.span == f.name.span)
                            .map(|m| m.key.clone());
                        self.located.extend(key);
                    }
                }
                Item::Derive(d) => {
                    for f in &d.methods {
                        if let Some(d) = misplaced(f, "a derive block's item") {
                            self.report(d);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// E0406 unless function `key` can be a value: a marked one takes its
    /// caller's place, which a value has none of.
    pub(super) fn located_value(&mut self, key: &str, name: &str, at: crate::span::Span) -> bool {
        if !self.located.contains(key) {
            return true;
        }
        let help = "a `@caller_location` function can't be a value: wrap the call in a closure";
        self.report(super::errors::not_value(name, at, help));
        false
    }
}
