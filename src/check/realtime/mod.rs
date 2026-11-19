//! `realtime fun(…)` callbacks: the function passed for one, and every Lugha
//! function it calls, may not allocate, wait, or call what can't be seen
//! when compiling (a function value, an interface value's method). It
//! runs on the checked program, once every body is typed, following named
//! calls through the prelude and std; anything it can't follow counts
//! against it, as Clang's `nonblocking` counts an unmarked callee. E0482.

use std::collections::{HashMap, HashSet};

use super::{CParam, Checker, End, Type, Unit, qualify};
use crate::ast::{
    AssignOp, BinOp, Block, Expr, ExprKind, ExternDecl, ForIter, FunDecl, Item, Stmt, StmtKind,
};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

mod walk;

use walk::{Child, each_in_block, expr_children, stmt_children};

/// What a construct does that a `realtime` callback can't.
#[derive(Clone, Copy)]
enum Breaks {
    Allocates,
    Waits,
    Unseen,
}

impl Breaks {
    fn verb(self) -> &'static str {
        match self {
            Breaks::Allocates => "allocate",
            Breaks::Waits => "wait",
            Breaks::Unseen => "call what can't be checked",
        }
    }
}

/// Built-in methods of arrays and strings that only read.
const READS: [&str; 7] = [
    "len",
    "is_empty",
    "get",
    "first",
    "last",
    "contains",
    "char_count",
];

struct Pass<'a> {
    checker: &'a Checker,
    funs: HashMap<String, &'a FunDecl>,
    externs: HashMap<String, &'a ExternDecl>,
    /// Functions already walked from this callback.
    seen: HashSet<String>,
    /// The calls followed to reach the body being walked: who, and where.
    chain: Vec<(String, Span)>,
    found: Vec<Diagnostic>,
    reported: HashSet<(usize, usize)>,
}

/// Every `realtime` callback in `units`, checked: E0482 for each thing one
/// does that it can't.
pub(super) fn check(units: &[Unit], checker: &Checker) -> Vec<Diagnostic> {
    let mut pass = Pass {
        checker,
        funs: HashMap::new(),
        externs: HashMap::new(),
        seen: HashSet::new(),
        chain: Vec::new(),
        found: Vec::new(),
        reported: HashSet::new(),
    };
    for unit in units {
        for item in &unit.program.items {
            match item {
                Item::Fun(f) => {
                    pass.funs.insert(qualify(unit.path, &f.name.name), f);
                }
                Item::Extern(e) => {
                    pass.externs.insert(qualify(unit.path, &e.name.name), e);
                }
                Item::Impl(i) => {
                    let owner = match checker.impl_owners.get(&i.span) {
                        Some(Type::Struct(k, _) | Type::Enum(k, _)) => k.clone(),
                        Some(other) => other.to_string(),
                        None => qualify(unit.path, &i.ty.name),
                    };
                    for f in &i.methods {
                        pass.funs.insert(format!("{owner}.{}", f.name.name), f);
                    }
                }
                _ => {}
            }
        }
    }
    let mut roots = Vec::new();
    for f in pass.funs.values() {
        sites(&f.body, checker, &mut roots);
    }
    for root in roots {
        pass.seen.clear();
        pass.chain.clear();
        pass.root(root);
    }
    pass.found
}

/// The arguments passed for `realtime` callbacks in `block`.
fn sites<'a>(block: &'a Block, checker: &Checker, roots: &mut Vec<&'a Expr>) {
    each_in_block(block, &mut |e| {
        if let ExprKind::Call(_, args) = &e.kind
            && let Some(key) = checker.callees.get(&e.id)
            && let Some(plan) = checker.c_params.get(key)
        {
            let written = plan
                .iter()
                .filter(|c| matches!(c, CParam::Value | CParam::Callback { .. }));
            for (c, arg) in written.zip(args) {
                if matches!(c, CParam::Callback { realtime: true, .. }) {
                    roots.push(arg);
                }
            }
        }
        true
    });
}

impl<'a> Pass<'a> {
    fn ty(&self, e: &Expr) -> Option<&'a Type> {
        self.checker
            .types
            .get(e.id.0 as usize)
            .and_then(|t| t.as_ref())
    }

    /// The function passed for a callback: a closure literal's body, or a
    /// named function's.
    fn root(&mut self, arg: &'a Expr) {
        match &arg.kind {
            ExprKind::Closure(c) => self.block(&c.body),
            _ => match self.checker.callees.get(&arg.id) {
                Some(key) if self.funs.contains_key(key) => {
                    let key = key.clone();
                    self.follow(&key, arg.span);
                }
                _ => self.report(Breaks::Unseen, "a function value", arg.span),
            },
        }
    }

    /// Walks `key`'s body once, from a call at `at`.
    fn follow(&mut self, key: &str, at: Span) {
        if !self.seen.insert(key.to_string()) {
            return;
        }
        let Some(f) = self.funs.get(key).copied() else {
            return;
        };
        self.chain.push((f.name.name.clone(), at));
        self.block(&f.body);
        self.chain.pop();
    }

    fn report(&mut self, breaks: Breaks, what: &str, span: Span) {
        if !self.reported.insert((span.start, span.end)) {
            return;
        }
        let mut d = Diagnostic::error(
            "E0482",
            format!("a `realtime` callback can't {}: {what}", breaks.verb()),
            span,
        )
        .with_help(
            "`realtime` callbacks run on C's real-time thread, where allocating or waiting can stall it: do the work in a task, handing it values through a `Ring`",
        );
        for (name, at) in &self.chain {
            d = d.with_label(
                *at,
                format!("`{name}` is called here, on the way from the callback"),
            );
        }
        self.found.push(d);
    }

    fn block(&mut self, block: &'a Block) {
        for stmt in &block.stmts {
            self.stmt(stmt);
        }
        if let Some(tail) = &block.tail {
            self.expr(tail);
        }
    }

    fn stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.kind {
            StmtKind::Assign {
                op, place, value, ..
            } => {
                // A view's element is C's memory: written in place.
                if let ExprKind::Index(base, _, _) = &place.kind
                    && !matches!(self.ty(base), Some(Type::View(..)))
                {
                    self.report(
                        Breaks::Allocates,
                        "writing an array's element may copy the array first",
                        place.span,
                    );
                }
                if *op == AssignOp::Add && self.ty(place) == Some(&Type::String) {
                    self.report(Breaks::Allocates, "`+=` builds a new string", place.span);
                }
                self.expr(place);
                self.expr(value);
            }
            StmtKind::For { iter, body, .. } => {
                match iter {
                    ForIter::Range(a, b) => {
                        self.expr(a);
                        self.expr(b);
                    }
                    ForIter::Array(e) => {
                        match self.ty(e) {
                            Some(Type::Array(_) | Type::String | Type::View(..)) => {}
                            Some(Type::Chan(..)) => self.report(
                                Breaks::Waits,
                                "a loop over a receiver waits for each value",
                                e.span,
                            ),
                            _ => self.report(
                                Breaks::Unseen,
                                "a loop over an iterator calls its `next`",
                                e.span,
                            ),
                        }
                        self.expr(e);
                    }
                    ForIter::MutOf(e) => {
                        self.report(
                            Breaks::Allocates,
                            "a loop changing an array's elements may copy the array first",
                            e.span,
                        );
                        self.expr(e);
                    }
                }
                self.block(body);
            }
            _ => self.children(stmt_children(stmt)),
        }
    }

    fn expr(&mut self, e: &'a Expr) {
        if self.checker.conversions.contains_key(&e.id) {
            self.report(
                Breaks::Allocates,
                "converting to an interface value boxes it",
                e.span,
            );
        }
        match &e.kind {
            ExprKind::Interp(_) | ExprKind::Format(_) => {
                self.report(
                    Breaks::Allocates,
                    "this string is built at run time",
                    e.span,
                );
            }
            ExprKind::Array(_) | ExprKind::Repeat(..) => {
                self.report(Breaks::Allocates, "this array is made at run time", e.span);
            }
            ExprKind::Binary(op, _, a, _) => match self.ty(a) {
                Some(Type::String) if *op == BinOp::Add => {
                    self.report(Breaks::Allocates, "`+` builds a new string", e.span);
                }
                Some(Type::Param(_)) => self.report(
                    Breaks::Unseen,
                    "an operator on a type parameter, whose implementation isn't known here",
                    e.span,
                ),
                _ => {}
            },
            ExprKind::Closure(_) => {
                // A closure's body runs only where it is called, which is
                // checked there; making one that captures allocates.
                if self
                    .checker
                    .captures
                    .get(&e.id)
                    .is_some_and(|c| !c.is_empty())
                {
                    self.report(
                        Breaks::Allocates,
                        "this closure captures, so its environment is allocated",
                        e.span,
                    );
                }
                return;
            }
            ExprKind::Spawn(..) => {
                self.report(Breaks::Allocates, "starting a task", e.span);
                return;
            }
            ExprKind::Scope(_) => {
                self.report(Breaks::Waits, "a scope waits for its tasks", e.span);
                return;
            }
            ExprKind::Select(_) => {
                self.report(Breaks::Waits, "`select` waits for a case", e.span);
                return;
            }
            ExprKind::Shield(_) => {
                self.report(Breaks::Waits, "a shield is for code that waits", e.span);
                return;
            }
            ExprKind::Call(callee, args) if !self.call(e, callee, args) => return,
            _ => {}
        }
        self.children(expr_children(e));
    }

    fn children(&mut self, children: Vec<Child<'a>>) {
        for c in children {
            match c {
                Child::Expr(e) => self.expr(e),
                Child::Block(b) => self.block(b),
            }
        }
    }

    /// A call's own rule; false when its arguments aren't walked (a panic's).
    fn call(&mut self, call: &'a Expr, callee: &'a Expr, args: &'a [Expr]) -> bool {
        if let ExprKind::Name(n) = &callee.kind
            && n == "panic"
        {
            // An error path: its message may be built.
            return false;
        }
        if let ExprKind::Field(base, op) = &callee.kind
            && matches!(&base.kind, ExprKind::Name(n) if n == "$os")
        {
            let what = format!("`$os.{}`, the runtime's I/O and waiting", op.name);
            self.report(Breaks::Waits, &what, call.span);
            return true;
        }
        // An interface value's method: which implementation runs is chosen
        // at run time.
        if let ExprKind::Field(base, name) = &callee.kind
            && matches!(self.ty(base), Some(Type::Interface(..)))
        {
            let what = format!("`{}`, a method of an interface value", name.name);
            self.report(Breaks::Unseen, &what, call.span);
            return true;
        }
        if let Some(key) = self.checker.callees.get(&call.id).cloned() {
            if key == super::REF_NEW {
                self.report(Breaks::Allocates, "`Ref.new` makes a box", call.span);
            } else if [super::CHAN_NEW, super::CHAN_UNBOUNDED, super::RING_NEW]
                .contains(&key.as_str())
            {
                self.report(Breaks::Allocates, "making a channel or a ring", call.span);
            } else if let Some(e) = self.externs.get(&key) {
                if e.blocking.is_some() {
                    let what = "a `blocking` extern runs on a helper thread while its caller waits";
                    self.report(Breaks::Waits, what, call.span);
                }
            } else if self.funs.contains_key(&key) {
                self.follow(&key, call.span);
            } else {
                let what = format!("`{key}`, which this check can't follow");
                self.report(Breaks::Unseen, &what, call.span);
            }
            return true;
        }
        let ExprKind::Field(base, name) = &callee.kind else {
            self.report(Breaks::Unseen, "a call through a function value", call.span);
            return true;
        };
        let name = name.name.as_str();
        if matches!(name, "show" | "debug" | "to_string") {
            self.report(Breaks::Allocates, "this builds a string", call.span);
            return true;
        }
        let Some(ty) = self.ty(base) else {
            return true;
        };
        let rule = match ty {
            Type::Chan(End::Ring, _) if matches!(name, "try_send" | "is") => None,
            Type::Chan(..) if name == "is" => None,
            Type::Chan(..) => Some((Breaks::Waits, "a channel's methods take its lock")),
            Type::Task(_) => Some((Breaks::Waits, "a task's methods wait or lock")),
            Type::Actor(..) => Some((Breaks::Waits, "an actor call waits for its mailbox")),
            Type::Param(_) => Some((Breaks::Unseen, "a method of a type parameter's value")),
            Type::View(..) if name == "to_array" => {
                Some((Breaks::Allocates, "it copies the view into a new array"))
            }
            Type::Array(_) | Type::String if READS.contains(&name) => None,
            Type::Array(_) | Type::String => Some((
                Breaks::Allocates,
                "this method may allocate a new array or string",
            )),
            _ => None,
        };
        if let Some((breaks, what)) = rule {
            let what = format!("`{name}`: {what}");
            self.report(breaks, &what, call.span);
        }
        let _ = args;
        true
    }
}

#[cfg(test)]
mod tests;
