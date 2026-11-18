//! Actors in the checker. An
//! actor's state is a struct and its methods an `impl` (the parser splits
//! the declaration), so struct and method checking apply inside it. Outside,
//! the actor's name is `Type::Actor`, a handle: `spawn` of the state gives
//! one, method calls on it are messages, and its fields are private. What
//! crosses an actor's boundary must be sendable (E0475).

use super::expr::Expect;
use super::members::{Detail, Member, MemberKind, signature};
use super::{Checker, Partial, Type, builtins, errors};
use crate::ast::{Expr, ExprKind, Ident, Item, Program};
use crate::span::Span;

/// A handle's built-in methods; an actor may not declare them.
const BUILT_IN: [&str; 2] = ["stop", "is"];

/// The size `mailbox n` gives, when `n` is a positive integer literal.
pub fn mailbox_size(n: &Expr) -> Option<u64> {
    match n.kind {
        ExprKind::Int(v) if v > 0 => Some(v),
        _ => None,
    }
}

impl Checker {
    /// `ty` as written outside actor `ty`'s own body: its handle.
    pub(super) fn as_handle(&self, ty: Type) -> Type {
        match ty {
            Type::Struct(key, args)
                if args.is_empty()
                    && self.actors.contains(&key)
                    && self.in_actor.as_deref() != Some(key.as_str()) =>
            {
                Type::Actor(key)
            }
            ty => ty,
        }
    }

    /// The actor declared by `item`'s struct or `impl`, if it is one.
    pub(super) fn actor_of(&self, item: &Item) -> Option<String> {
        match item {
            Item::Struct(s) if s.actor.is_some() => Some(self.key(&s.name.name)),
            Item::Impl(i) if i.actor => Some(self.key(&i.ty.name)),
            _ => None,
        }
    }

    /// The handle `spawn` of an actor's state gives, or `None` when `ty`
    /// isn't an actor's state. E0439 for a struct literal of a plain struct.
    pub(super) fn spawned_actor(&mut self, operand: &Expr, ty: &Type) -> Option<Type> {
        match ty {
            Type::Struct(key, _) if self.actors.contains(key) => Some(Type::Actor(key.clone())),
            Type::Struct(key, _) if matches!(operand.kind, ExprKind::StructLit(_)) => {
                let message =
                    format!("`{key}` isn't an actor: `spawn` starts actors and runs calls");
                self.report(errors::actor_misuse(message, operand.span));
                Some(Type::Error)
            }
            _ => None,
        }
    }

    /// E0439 for an actor's state made anywhere but in the actor or right
    /// after `spawn`: a literal or a call giving it.
    pub(super) fn state_outside(&mut self, expr: &Expr, ty: &Type) {
        let Type::Struct(key, _) = ty else { return };
        let origin = matches!(expr.kind, ExprKind::StructLit(_) | ExprKind::Call(..));
        if origin
            && self.actors.contains(key)
            && self.in_actor.as_deref() != Some(key.as_str())
            && self.spawn_operand != Some(expr.id)
        {
            let message = format!(
                "an actor's state exists only inside the actor: start `{key}` with `spawn`"
            );
            self.report(errors::actor_misuse(message, expr.span));
        }
    }

    /// A method call on a handle: a message. Built-ins (`stop`, `join`,
    /// `is`) are `None` here and go through `check::builtins`.
    pub(super) fn actor_call(
        &mut self,
        call: &Expr,
        key: &str,
        name: &Ident,
        args: &[Expr],
        expect: Option<&Expect>,
    ) -> Option<Type> {
        if BUILT_IN.contains(&name.name.as_str()) {
            return None;
        }
        let state = Type::Struct(key.to_string(), Vec::new());
        let Some(m) = self
            .method(&state, &name.name)
            .filter(|m| m.receiver.is_some())
        else {
            let help = if name.name == "join" {
                "an actor ends with its scope, which reports its failure: there is nothing to join"
            } else {
                "an actor's own methods and `stop` and `is` are its messages"
            };
            let help = Some(help.to_string());
            self.report(errors::no_method(
                &name.name,
                &Type::Actor(key.to_string()),
                name.span,
                help,
            ));
            return Some(self.unchecked(args));
        };
        self.method_visible(key, &name.name, name.span);
        self.hover.function(name.span, m.span, Some(call));
        self.callees.insert(call.id, m.key.clone());
        Some(self.arguments(call, &name.name, &m, args, (&[], expect)))
    }

    /// E0439 for a field read or written through a handle.
    pub(super) fn handle_field(&mut self, key: &str, field: &Ident) {
        let message = format!(
            "`{}` is a field of actor `{key}`: only its methods can reach it",
            field.name
        );
        self.report(errors::actor_misuse(message, field.span));
    }

    /// The declaration rules (E0439): no type parameters, no generic
    /// methods, no method named like a built-in, and no `Ref` (or what may
    /// hold one) in a field, parameter or result.
    pub(super) fn check_actors(&mut self, program: &Program) {
        for item in &program.items {
            let Some(key) = self.actor_of(item) else {
                continue;
            };
            match item {
                Item::Struct(s) => {
                    if let Some(first) = s.type_params.first() {
                        let message = "an actor can't have type parameters";
                        self.report(errors::actor_misuse(message, first.name.span));
                    }
                    if let Some(n) = &s.mailbox
                        && mailbox_size(n).is_none()
                    {
                        let message = "a mailbox holds a positive whole number of messages, written as a literal: `mailbox 100`";
                        self.report(errors::actor_misuse(message, n.span));
                    }
                    let fields = self.structs[&key].fields.clone();
                    for (field, (_, ty)) in s.fields.iter().zip(&fields) {
                        self.shares_nothing(&key, ty, field.ty.span);
                    }
                }
                Item::Impl(i) => {
                    for f in &i.methods {
                        if BUILT_IN.contains(&f.name.name.as_str()) {
                            let message =
                                format!("`{}` is a built-in method of every actor", f.name.name);
                            self.report(errors::actor_misuse(message, f.name.span));
                        }
                        if let Some(first) = f.type_params.first() {
                            let message = "an actor's methods can't have type parameters";
                            self.report(errors::actor_misuse(message, first.name.span));
                        }
                        let Some(m) = self
                            .methods
                            .get(&key)
                            .and_then(|m| m.get(&f.name.name))
                            .cloned()
                        else {
                            continue;
                        };
                        if m.receiver.is_none() {
                            // A function without `self` builds the state, inside the actor.
                            continue;
                        }
                        for (param, ty) in f.params.iter().zip(&m.params) {
                            // A message carries values, never a sender's place.
                            if let Some((crate::ast::PassMode::Mut, at)) = param.mode {
                                let message = "an actor's message can't take a `mut` parameter: messages carry values";
                                self.report(errors::actor_misuse(message, at));
                                continue;
                            }
                            self.shares_nothing(&key, ty, param.ty.span);
                        }
                        if let Some(written) = &f.ret {
                            self.shares_nothing(&key, &m.ret, written.span);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// What an actor keeps, takes or gives crosses to its task: E0475
    /// unless sendable.
    fn shares_nothing(&mut self, key: &str, ty: &Type, span: Span) {
        let name = key.rsplit('.').next().unwrap_or(key);
        let head = format!("actor `{name}` can't hold or exchange a `{ty}`");
        self.must_send(ty, &head, span);
    }
}

/// What can follow `handle.` (completion): the actor's methods taking a
/// receiver — its messages — and the built-in `stop`, `join` and `is`;
/// never its fields.
pub fn handle_members(partial: &Partial, key: &str) -> Vec<Member> {
    let mut out: Vec<Member> = partial
        .members
        .get(key)
        .map(|t| {
            t.members
                .iter()
                .filter(|m| m.kind == MemberKind::Method)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let handle = Type::Actor(key.to_string());
    for name in BUILT_IN {
        let m = builtins::builtin(&handle, name).expect("every handle has the built-ins");
        out.push(Member {
            name: name.to_string(),
            kind: MemberKind::Method,
            detail: Detail::Text(signature(name, None, &m.params, &m.ret)),
            decl: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type, ok};

    const COUNTER: &str = "actor Counter {
        count: i64,
        fun new(start: i64): Counter = Counter { count: start };
        fun add(mut self, n: i64) { self.count += n; }
        fun get(self): i64 = self.count;
        fun bump(mut self): i64 { self.add(1); self.count }
    }\n";

    #[test]
    fn spawn_gives_a_handle_and_calls_are_messages() {
        let src = format!(
            "{COUNTER}fun main(root: Scope) {{ let c = root.spawn(Counter {{ count: 0 }}); c.add(5); let n = c.get(); }}"
        );
        assert_eq!(let_type(&src, "c"), Type::Actor("Counter".into()));
        assert_eq!(let_type(&src, "n"), Type::I64);
        // `join` is gone: the actor's scope reports how it ended.
        let src = format!(
            "{COUNTER}fun main(root: Scope) {{ let c = root.spawn(Counter.new(3)); c.join(); }}"
        );
        assert_eq!(errors(&src), [("E0415", "join")]);
        ok(&format!(
            "{COUNTER}fun total(cs: Counter[]): i64 {{
                let mut t = 0;
                for c of cs {{ t += c.get(); }}
                t
            }}
            fun main(root: Scope) {{ let c = root.spawn(Counter.new(1)); c.stop(); let same = c.is(c); let t = total([c]); }}"
        ));
    }

    #[test]
    fn a_handle_needs_no_let_mut_for_mut_self_messages() {
        ok(&format!(
            "{COUNTER}fun main(root: Scope) {{ let c = root.spawn(Counter.new(0)); let n = c.bump(); }}"
        ));
    }

    #[test]
    fn handles_are_not_eq() {
        let src = format!(
            "{COUNTER}fun main(root: Scope) {{ let c = root.spawn(Counter.new(0)); let b = c == c; }}"
        );
        assert_eq!(errors(&src), [("E0404", "c == c")]);
    }

    #[test]
    fn actor_misuse_is_e0439() {
        for (body, at) in [
            (
                "let c = root.spawn(Counter.new(0)); let n = c.count;",
                "count",
            ),
            ("let c = root.spawn(Counter.new(0)); c.count = 1;", "count"),
            ("let s = Counter { count: 0 };", "Counter { count: 0 }"),
            ("let s = Counter.new(0);", "Counter.new(0)"),
        ] {
            let src = format!("{COUNTER}fun main(root: Scope) {{ {body} }}");
            assert_eq!(errors(&src), [("E0439", at)], "{body}");
        }
        assert_eq!(
            errors(
                "struct P { x: i64 }\nfun main(root: Scope) { let p = root.spawn(P { x: 1 }); }"
            ),
            [("E0439", "P { x: 1 }")]
        );
    }

    #[test]
    fn declaration_rules_are_e0439() {
        for (actor, at) in [
            ("actor A<T> { x: T }", "T"),
            ("actor A { x: i64, fun stop(self) {} }", "stop"),
            ("actor A { x: i64, fun f<T>(self, t: T) {} }", "T"),
            ("actor A mailbox 0 { x: i64 }", "0"),
            ("actor A mailbox \"big\" { x: i64 }", "\"big\""),
        ] {
            let src = format!("{actor}\nfun main() {{}}");
            assert_eq!(errors(&src)[0], ("E0439", at), "{actor}");
        }
        // What crosses its boundary must be sendable.
        for (actor, at) in [
            ("actor A { r: Ref<i64> }", "Ref<i64>"),
            (
                "actor A { x: i64, fun f(self, g: fun(i64): i64) {} }",
                "fun(i64): i64",
            ),
            (
                "struct S { r: Ref<i64> }\nactor A { x: i64, fun f(self): S[] = []; }",
                "S[]",
            ),
        ] {
            let src = format!("{actor}\nfun main() {{}}");
            assert_eq!(errors(&src)[0], ("E0475", at), "{actor}");
        }
        // Channels and other handles may cross; plain data too.
        ok("actor B { n: i64 }
            actor A { c: Sender<i64>, b: B, fun f(self, xs: string[]): (i64, bool) = (1, true); }
            fun main() {}");
    }

    #[test]
    fn an_actor_starts_in_a_scope() {
        ok(&format!(
            "actor Log mailbox 10 {{ n: i64 }}\n{COUNTER}fun main() {{ let r = scope s {{ let c = s.spawn(Counter.new(0)); let l = s.spawn(Log {{ n: 0 }}); }}; }}"
        ));
        assert_eq!(
            errors(&format!(
                "{COUNTER}fun main() {{ let c = spawn Counter.new(0); }}"
            )),
            [("E0474", "spawn Counter.new(0)")]
        );
    }

    #[test]
    fn unknown_messages_are_e0415() {
        let src = format!(
            "{COUNTER}fun main(root: Scope) {{ let c = root.spawn(Counter.new(0)); c.reset(); }}"
        );
        assert_eq!(errors(&src), [("E0415", "reset")]);
        let src = format!(
            "{COUNTER}fun main(root: Scope) {{ let c = root.spawn(Counter.new(0)); let d = c.new(1); }}"
        );
        assert_eq!(errors(&src), [("E0415", "new")]);
    }

    #[test]
    fn completion_offers_messages_not_fields() {
        let partial = crate::check::test_util::partial(&format!("{COUNTER}fun main() {{}}"));
        let names: Vec<String> = super::handle_members(&partial, "Counter")
            .into_iter()
            .map(|m| m.name)
            .collect();
        for name in ["add", "get", "bump", "stop", "is"] {
            assert!(names.contains(&name.to_string()), "{name} in {names:?}");
        }
        assert!(!names.contains(&"count".to_string()), "{names:?}");
        assert!(!names.contains(&"new".to_string()), "{names:?}");
    }
}
