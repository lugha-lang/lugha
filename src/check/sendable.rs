//! The sendable rule: a value crossing to another task — a spawned call's
//! arguments, captures and result, a channel's elements, what an actor
//! keeps, takes and gives — must be `Sendable`. The prelude's `Sendable`
//! is an `auto` interface: its derive blocks apply to every type whose
//! parts meet them, so `unmet_derived` works it out field by field. The
//! kinds no block can describe are decided here: a `Ref` and a plain
//! function value never are, an interface value is when its interface
//! requires `Sendable`, and a function's own type is when its captures are.

use std::collections::HashSet;

use super::derive_meets::Unmet;
use super::scopes::SCOPE;
use super::{Checker, FunOrigin, Type, errors};
use crate::ast::{Expr, ExprKind};
use crate::span::Span;

/// The prelude's `Sendable`.
pub(super) const SENDABLE: &str = "$prelude.Sendable";

/// A sendable check met in a declaration, run once derive blocks are known,
/// with the type parameters in scope where it was met.
pub(super) struct Pending {
    ty: Type,
    head: String,
    span: Span,
    type_params: Vec<super::generics::Generic>,
}

impl Checker {
    /// For `unmet_derived`, at `Sendable`: whether `ty` is decided here
    /// (`Some`), sendable (`Some(None)`) or not, with its path and why; `None`
    /// leaves it to the derive blocks.
    pub(super) fn sendable_part(
        &self,
        ty: &Type,
        path: &str,
        seen: &mut HashSet<Type>,
    ) -> Option<Option<(String, Unmet)>> {
        let fail = || Some(Some((path.to_string(), Unmet::Missing(ty.clone()))));
        match ty {
            Type::Ref(_) | Type::Fun(..) | Type::Task(_) | Type::View(..) => fail(),
            Type::Struct(key, _) if key == SCOPE => fail(),
            // Its C library allows it only on the thread that made it.
            Type::Handle(key) if self.pinned.contains(key) => fail(),
            // One task owns an actor's state; others only send it messages.
            Type::Actor(..) => Some(None),
            // A channel is sendable when what it carries is.
            Type::Chan(_, inner) => Some(self.unmet_derived(SENDABLE, inner, path, seen)),
            Type::Interface(key, _) => match self.requires_sendable(key) {
                true => Some(None),
                false => fail(),
            },
            Type::Own(own) => Some(match &own.origin {
                FunOrigin::Named(_) => None,
                FunOrigin::Closure(_, _, captured) => captured.iter().find_map(|(name, t)| {
                    self.unmet_derived(SENDABLE, t, &format!("{path} (its capture `{name}`)"), seen)
                }),
            }),
            _ => None,
        }
    }

    /// The checks waiting for derive blocks; from now on they run at once.
    pub(super) fn run_pending_sends(&mut self) {
        self.sends_ready = true;
        for p in std::mem::take(&mut self.pending_sends) {
            let saved = std::mem::replace(&mut self.type_params, p.type_params);
            self.must_send(&p.ty, &p.head, p.span);
            self.type_params = saved;
        }
    }

    /// True if interface `key` is or extends `Sendable`.
    pub(super) fn requires_sendable(&self, key: &str) -> bool {
        self.extends_or_is(key, SENDABLE)
    }

    /// Why a value of type `ty` can't cross to another task, or `None`;
    /// `whole` names the value in the reason (`it`, or a capture's name).
    pub(super) fn unsendable(&self, ty: &Type, whole: &str) -> Option<String> {
        let (path, why) = self.unmet_derived(SENDABLE, ty, "", &mut HashSet::new())?;
        let part = match path.as_str() {
            "" => whole.to_string(),
            "[]" => format!("an element of {whole}"),
            _ => format!("`{}`", path.trim_start_matches('.')),
        };
        Some(match why {
            Unmet::Missing(Type::Ref(inner)) => {
                format!("{part} is a `Ref<{inner}>`, which two tasks would share")
            }
            Unmet::Missing(t @ Type::Fun(..)) => {
                format!("{part} is a function value, `{t}`, which may have captured anything")
            }
            Unmet::Missing(Type::Interface(key, _)) => format!(
                "{part} is a `{}` value, whose interface doesn't require `Sendable`",
                key.rsplit('.').next().unwrap_or(&key)
            ),
            Unmet::Missing(Type::Task(_)) => format!("{part} is a task handle"),
            Unmet::Missing(Type::Struct(k, _)) if k == SCOPE => format!("{part} is a scope handle"),
            Unmet::Missing(t @ Type::Handle(_)) => {
                format!("{part} is `{t}`, a pinned handle, which stays on the thread that made it")
            }
            Unmet::Missing(t) => format!("{part} is a `{t}`, which isn't sendable"),
            Unmet::Unbounded(name) => {
                format!("{part} is the type parameter `{name}`, not bounded `Sendable`")
            }
            Unmet::Bound(_, t) | Unmet::NotAsked(t) => {
                format!("{part} holds a `{t}`, which isn't sendable")
            }
            Unmet::Union(_) => format!("{part} holds a C union"),
        })
    }

    /// E0475 unless a value of type `ty` may cross to another task: `head`
    /// says which value and how (`"`Job` can't be passed to another task"`).
    /// True if it reported.
    pub(super) fn must_send(&mut self, ty: &Type, head: &str, span: Span) -> bool {
        if matches!(ty, Type::Error | Type::Never | Type::Unit) {
            return false;
        }
        // In a declaration, before derive blocks are known: checked later.
        if !self.sends_ready {
            self.pending_sends.push(Pending {
                ty: ty.clone(),
                head: head.to_string(),
                span,
                type_params: self.type_params.clone(),
            });
            return false;
        }
        let Some(why) = self.unsendable(ty, "it") else {
            return false;
        };
        self.report(errors::not_sendable(head, &why, span));
        true
    }

    /// `Chan<T>.new()`'s written `T`, if any: what the channel carries.
    pub(super) fn carried(&mut self, written: Option<&Type>, span: Span) {
        if let Some(t) = written {
            let head = format!("a channel can't carry `{t}` to another task");
            self.must_send(t, &head, span);
        }
    }

    /// `impl key for ty`: E0476 for `Sendable` itself; E0475 when `key`
    /// requires `Sendable` and `ty` isn't.
    pub(super) fn sendable_impl(&mut self, key: &str, ty: &Type, span: Span) {
        if self.interfaces.get(key).is_some_and(|i| i.auto) {
            let message = "`Sendable` is worked out from a type's parts, never implemented";
            self.report(errors::sendable_claimed(message, span));
        } else if self.requires_sendable(key) {
            let name = key.rsplit('.').next().unwrap_or(key);
            let head = format!("`{ty}` can't implement `{name}`, which requires `Sendable`");
            self.must_send(ty, &head, span);
        }
    }

    /// `expr`, of type `ty`, converting to interface `to`, which requires
    /// `Sendable`: E0475 unless `ty` is. A function value is when it is a
    /// named function, or a closure whose captures are. True if it may.
    pub(super) fn sendable_conversion(&mut self, expr: &Expr, ty: &Type, to: &Type) -> bool {
        let head = format!("a `{ty}` can't become a `{to}`, which requires `Sendable`");
        let Type::Fun(params, ret) = ty else {
            return !self.must_send(ty, &head, expr.span);
        };
        let own = match self.fun_origins.get(&expr.id) {
            Some(origin) => Type::Own(Box::new(super::OwnFun {
                origin: origin.clone(),
                params: params.clone(),
                ret: (**ret).clone(),
            })),
            None => ty.clone(),
        };
        !self.must_send(&own, &head, expr.span)
    }

    /// `s.spawn(call)`: the call's arguments and receiver, then what it
    /// captures (a closure literal's captures included), then its result.
    pub(super) fn spawn_crossings(&mut self, closure: &Expr, call: &Expr, ret: &Type) {
        let mut reported = false;
        if let ExprKind::Call(callee, args) = &call.kind {
            let receiver = match &callee.kind {
                ExprKind::Field(base, _) if self.local_receiver(base) => Some(&**base),
                _ => None,
            };
            for e in args.iter().chain(receiver) {
                let ty = self.types[e.id.0 as usize].clone().unwrap_or(Type::Error);
                let head = format!("a `{ty}` can't be passed to another task");
                reported |= self.must_send(&ty, &head, e.span);
            }
        }
        // A capture already reported as an argument isn't again.
        if !reported {
            let names = self.captures.get(&closure.id).cloned().unwrap_or_default();
            for name in names {
                let Some(local) = self.local(&name) else {
                    continue;
                };
                let head = format!("a spawned call can't capture `{name}`");
                if self.must_send(&local.ty, &head, call.span) {
                    break;
                }
            }
        }
        let head = format!("a `{ret}` can't come back from another task as its result");
        self.must_send(ret, &head, call.span);
    }

    /// True if `base`, a call's receiver, is a value (not a type or module).
    fn local_receiver(&self, base: &Expr) -> bool {
        !matches!(self.types[base.id.0 as usize], Some(Type::Unit) | None)
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{errors, ok};

    /// The codes `src` reports, in order.
    fn codes(src: &str) -> Vec<&'static str> {
        errors(src).into_iter().map(|(c, _)| c).collect()
    }

    #[test]
    fn plain_data_channels_actors_and_capabilities_cross() {
        ok(
            "struct Job { id: i64, tags: string[], at: (i64, Option<string>) }
            enum Step { Go(Job), Stop }
            actor A { n: i64, fun get(self): i64 = self.n; }
            fun work(j: Job, s: Step, c: Sender<Job>, a: A, console: Console): i64 = j.id;
            fun main(console: Console, root: Scope) {
                let (c, jobs) = Chan<Job>.new(0);
                let a = root.spawn(A { n: 1 });
                let k = 2;
                let r = scope s {
                    s.spawn(work(Job { id: 1, tags: [], at: (1, None) }, Step.Stop, c, a, console));
                    s.spawn(fun(): i64 { k + 1 }());
                };
            }",
        );
    }

    #[test]
    fn a_ref_never_crosses() {
        let shared = "struct Shared { r: Ref<i64> }\nfun use_it(s: Shared) {}\n";
        assert_eq!(
            errors(&format!(
                "{shared}fun main() {{ let x = Shared {{ r: Ref.new(0) }}; let r = scope s {{ s.spawn(use_it(x)); }}; }}"
            )),
            [("E0475", "x")]
        );
        assert_eq!(
            codes(
                "fun main() { let r = Ref.new(0); let t = scope s { s.spawn(fun() { r.set(1); }()); }; }"
            ),
            ["E0475"]
        );
        assert_eq!(
            codes("fun main() { let c = Chan<Ref<i64>>.new(0); }"),
            ["E0475"]
        );
        assert_eq!(
            codes(
                "fun make(): Ref<i64> = Ref.new(0);\nfun main() { let t = scope s { s.spawn(make()); }; }"
            ),
            ["E0475"]
        );
    }

    #[test]
    fn functions_cross_by_what_they_captured() {
        let job = "interface Job: fun(): i64 + Sendable {}\nfun run(j: Job): i64 = j();\nfun one(): i64 = 1;\n";
        ok(&format!(
            "{job}fun main() {{ let k = 2; let a = run(one) + run(fun(): i64 = k); }}"
        ));
        assert_eq!(
            codes(&format!(
                "{job}fun main() {{ let r = Ref.new(0); let a = run(fun(): i64 = r.get()); }}"
            )),
            ["E0475"]
        );
        // A plain function value hides what it captured.
        assert_eq!(
            codes(&format!(
                "{job}fun pass(f: fun(): i64): i64 = run(f);\nfun main() {{}}"
            )),
            ["E0475"]
        );
    }

    #[test]
    fn generic_values_cross_when_bounded() {
        ok(
            "fun send<T: Sendable>(s: Scope, v: T, c: Sender<T>) { s.spawn(c.send(v)); }\nfun main() {}",
        );
        assert!(
            codes(
                "fun send<T>(s: Scope, v: T, c: Sender<T>) { s.spawn(c.send(v)); }\nfun main() {}"
            )
            .iter()
            .all(|c| *c == "E0475")
        );
    }

    #[test]
    fn capabilities_must_be_sendable_and_sendable_is_never_claimed() {
        assert_eq!(
            codes("struct Fake { n: Ref<i64> }
                impl Entropy for Fake { fun bytes(self, n: i64): u8[] = []; fun next_u64(self): u64 = 0; }
                fun main() {}"),
            ["E0475"]
        );
        assert_eq!(
            codes("struct P { x: i64 }\nimpl Sendable for P {}\nfun main() {}"),
            ["E0476"]
        );
        assert_eq!(
            codes("struct P { x: i64 } with Sendable\nfun main() {}"),
            ["E0476"]
        );
        assert_eq!(codes("auto interface Mine {}\nfun main() {}"), ["E0476"]);
    }
}
