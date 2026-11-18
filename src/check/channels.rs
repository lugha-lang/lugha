//! Channels in the checker: `Chan<T>.new(n)` and `Chan<T>.unbounded()`
//! make a channel and give its two ends, `(Sender<T>, Receiver<T>)`, `T`
//! written or taken from the expected type. Each end's methods are
//! built in (`check::builtins`); `Chan` itself is only the maker.

use super::expr::Expect;
use super::{Checker, End, Type, errors};
use crate::ast::{Expr, Ident};

/// What codegen finds `Chan<T>.new(n)` by in `Checked::callees`.
pub const CHAN_NEW: &str = "$Chan.new";
/// What codegen finds `Chan<T>.unbounded()` by in `Checked::callees`.
pub const CHAN_UNBOUNDED: &str = "$Chan.unbounded";
/// The prelude's `TrySend<T>`, what `try_send` gives.
pub const TRY_SEND: &str = "$prelude.TrySend";
/// The prelude's `TryRecv<T>`, what `try_recv` gives.
pub const TRY_RECV: &str = "$prelude.TryRecv";

/// How a channel is made, for help on the old ways.
const MAKE: &str = "make a channel with `Chan<T>.new(n)`, holding up to `n` values (0: each handed over directly), or `Chan<T>.unbounded()`";

/// The element type `(Sender<T>, Receiver<T>)` carries.
fn ends_element(ty: &Type) -> Option<&Type> {
    match super::tuples::elements(ty)? {
        [Type::Chan(End::Send, t), Type::Chan(End::Recv, _)] => Some(t),
        _ => None,
    }
}

/// Help for `name` called on the wrong end of a channel.
pub(super) fn wrong_end(receiver: &Type, name: &str) -> Option<String> {
    match (receiver, name) {
        (Type::Chan(End::Recv, _), "send" | "try_send" | "close") => Some(format!(
            "only a `Sender` can `{name}`; this is the `Receiver` end"
        )),
        (Type::Chan(End::Send, _), "recv" | "try_recv") => Some(format!(
            "only a `Receiver` can `{name}`; this is the `Sender` end"
        )),
        _ => None,
    }
}

impl Checker {
    /// `Chan<T>.new(n)` or `Chan<T>.unbounded()`, `written` being `T` if
    /// written; any other `Chan.f(…)` is E0415.
    pub(super) fn chan_new(
        &mut self,
        call: &Expr,
        name: &Ident,
        args: &[Expr],
        written: Option<Type>,
        expect: Option<&Expect>,
    ) -> Type {
        let (key, arity) = match name.name.as_str() {
            "new" => (CHAN_NEW, 1),
            "unbounded" => (CHAN_UNBOUNDED, 0),
            _ => {
                self.report(errors::chan_maker(&name.name, name.span).with_help(MAKE));
                return self.unchecked(args);
            }
        };
        if args.len() != arity {
            let wrong = errors::arity(&name.name, arity, args.len(), call.span);
            self.report(wrong.with_help(MAKE));
            return self.unchecked(args);
        }
        if let [size] = args {
            self.expect_type(size, &Expect::of(Type::I64));
        }
        self.callees.insert(call.id, key.to_string());
        self.effect("Chan.new", "makes a channel", call.span);
        let element = match (written, expect.map(|e| &e.ty)) {
            (Some(t), _) => t,
            (None, Some(Type::Error)) => return Type::Error,
            (None, Some(ty)) if ends_element(ty).is_some() => {
                ends_element(ty).expect("matched").clone()
            }
            _ => {
                self.report(errors::chan_unknown(call.span));
                return Type::Error;
            }
        };
        let ends = vec![
            Type::Chan(End::Send, Box::new(element.clone())),
            Type::Chan(End::Recv, Box::new(element)),
        ];
        self.tuple_type(ends)
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type, ok};

    #[test]
    fn a_channel_is_two_ends() {
        ok("fun main() {
            let (tx, rx) = Chan<i64>.new(1);
            tx.send(1);
            let v: Option<i64> = rx.recv();
            let w = rx.try_recv();
            let t = tx.try_send(2);
            tx.close();
            let same = tx.is(tx);
            let (utx, urx) = Chan<string>.unbounded();
            for s of urx { let n = s.len; }
        }");
        let src = "fun main() { let (tx, rx) = Chan<i64>.new(0); let v = rx.recv(); let w = rx.try_recv(); let t = tx.try_send(1); let a = tx; let b = rx; }";
        assert_eq!(let_type(src, "v"), Type::Option(Box::new(Type::I64)));
        assert_eq!(let_type(src, "w").to_string(), "TryRecv<i64>");
        assert_eq!(let_type(src, "t").to_string(), "TrySend<i64>");
        assert_eq!(let_type(src, "a").to_string(), "Sender<i64>");
        assert_eq!(let_type(src, "b").to_string(), "Receiver<i64>");
        ok("fun drain(rx: Receiver<i64>): i64 {
                let mut sum = 0;
                for x of rx { sum += x; }
                sum
            }
            fun make(): (Sender<i64>, Receiver<i64>) = Chan.new(2);
            fun main() {}");
    }

    #[test]
    fn what_a_channel_carries_must_be_known() {
        assert_eq!(
            errors("fun main() { let (tx, rx) = Chan.new(0); }"),
            [("E0412", "Chan.new(0)")]
        );
    }

    #[test]
    fn channel_mistakes() {
        let pre = "fun main() { let (tx, rx) = Chan<i64>.new(0); ";
        for (body, code, at) in [
            ("tx.send(\"x\"); }", "E0403", "\"x\""),
            ("rx.close(); }", "E0415", "close"),
            ("rx.send(1); }", "E0415", "send"),
            ("tx.recv(); }", "E0415", "recv"),
            ("let d = Chan<i64>.make(); }", "E0415", "make"),
            ("let d = Chan<i64>.buffered(1); }", "E0415", "buffered"),
            ("let d = Chan<i64>.new(\"x\"); }", "E0403", "\"x\""),
            ("let d = Chan<i64>.new(); }", "E0405", "Chan<i64>.new()"),
            ("let b = tx == tx; }", "E0404", "tx == tx"),
        ] {
            let src = format!("{pre}{body}");
            assert_eq!(errors(&src), [(code, at)], "{body}");
        }
        assert_eq!(
            errors("fun f(c: Chan<i64>) {}\nfun main() {}"),
            [("E0305", "Chan<i64>")]
        );
    }
}
