//! Walking the checked program's bodies for the `realtime` check: each
//! statement's and expression's children, one level down, and every
//! expression in a block.

use crate::ast::{Block, Expr, ExprKind, ForIter, Stmt, StmtKind};

/// One level down: an expression or a block.
pub(super) enum Child<'a> {
    Expr(&'a Expr),
    Block(&'a Block),
}

/// Visits every expression in `block`, outermost first; `f` returns false
/// to skip an expression's children.
pub(super) fn each_in_block<'a>(block: &'a Block, f: &mut dyn FnMut(&'a Expr) -> bool) {
    for stmt in &block.stmts {
        each_in(stmt_children(stmt), f);
    }
    if let Some(tail) = &block.tail {
        each_in_expr(tail, f);
    }
}

fn each_in<'a>(children: Vec<Child<'a>>, f: &mut dyn FnMut(&'a Expr) -> bool) {
    for c in children {
        match c {
            Child::Expr(e) => each_in_expr(e, f),
            Child::Block(b) => each_in_block(b, f),
        }
    }
}

fn each_in_expr<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a Expr) -> bool) {
    if f(e) {
        each_in(expr_children(e), f);
    }
}

/// `stmt`'s expressions and blocks, one level down.
pub(super) fn stmt_children(stmt: &Stmt) -> Vec<Child<'_>> {
    use Child::{Block as B, Expr as E};
    match &stmt.kind {
        StmtKind::Let { init, .. } => vec![E(init)],
        StmtKind::LetElse { init, else_, .. } => {
            let mut out = vec![E(init)];
            out.extend(else_.as_deref().map(B));
            out
        }
        StmtKind::Assign { place, value, .. } => vec![E(place), E(value)],
        StmtKind::Expr { expr, .. } => vec![E(expr)],
        StmtKind::While { cond, body, .. } => vec![E(cond), B(body)],
        StmtKind::WhileLet {
            scrutinee, body, ..
        } => vec![E(scrutinee), B(body)],
        StmtKind::For { iter, body, .. } => {
            let mut out = match iter {
                ForIter::Range(a, b) => vec![E(a), E(b)],
                ForIter::Array(e) | ForIter::MutOf(e) => vec![E(e)],
            };
            out.push(B(body));
            out
        }
        StmtKind::Return(Some(e)) => vec![E(e)],
        StmtKind::Break { value: Some(e), .. } => vec![E(e)],
        StmtKind::Defer(inner) => stmt_children(inner),
        StmtKind::Return(None) | StmtKind::Break { .. } | StmtKind::Continue { .. } => Vec::new(),
    }
}

/// `e`'s expressions and blocks, one level down.
pub(super) fn expr_children(e: &Expr) -> Vec<Child<'_>> {
    use Child::{Block as B, Expr as E};
    match &e.kind {
        ExprKind::Tuple(xs) | ExprKind::Array(xs) => xs.iter().map(E).collect(),
        ExprKind::Interp(parts) => parts
            .iter()
            .filter_map(|p| match p {
                crate::ast::Part::Expr(x, _) => Some(E(x)),
                _ => None,
            })
            .collect(),
        ExprKind::Format(call) => call.args.iter().map(E).collect(),
        ExprKind::Try(x) | ExprKind::Unary(_, x) | ExprKind::Cast(x, _) | ExprKind::Pass(_, x) => {
            vec![E(x)]
        }
        ExprKind::Field(x, _) => vec![E(x)],
        ExprKind::Binary(_, _, a, b) | ExprKind::Index(a, _, b) | ExprKind::Repeat(a, b) => {
            vec![E(a), E(b)]
        }
        ExprKind::Call(callee, args) => std::iter::once(E(callee))
            .chain(args.iter().map(E))
            .collect(),
        ExprKind::StructLit(lit) => {
            let mut out: Vec<Child<'_>> = lit.fields.iter().map(|(_, v)| E(v)).collect();
            out.extend(lit.base.as_deref().map(E));
            out
        }
        ExprKind::If { cond, then, else_ } => {
            let mut out = vec![E(cond), B(then)];
            out.extend(else_.as_deref().map(E));
            out
        }
        ExprKind::IfLet(parts) => {
            let mut out = vec![E(&parts.scrutinee), B(&parts.then)];
            out.extend(parts.else_.as_deref().map(E));
            out
        }
        ExprKind::Block(b) => vec![B(b)],
        ExprKind::Loop { body, .. } => vec![B(body)],
        ExprKind::Match(scrutinee, arms) => {
            let mut out = vec![E(scrutinee)];
            for arm in arms {
                out.extend(arm.guard.as_ref().map(E));
                out.push(E(&arm.body));
            }
            out
        }
        ExprKind::Closure(c) => vec![B(&c.body)],
        ExprKind::Spawn(scope, call) => {
            let mut out: Vec<Child<'_>> = scope.as_deref().map(E).into_iter().collect();
            out.push(E(call));
            out
        }
        ExprKind::Scope(s) => vec![B(&s.body)],
        ExprKind::Shield(b) => vec![B(b)],
        ExprKind::Select(_)
        | ExprKind::Int(_)
        | ExprKind::Char(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::Name(_) => Vec::new(),
    }
}
