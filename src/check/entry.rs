//! What `main` and a test may take: the command-line arguments and the
//! capabilities the runtime supplies, matched by type, each at most once.
//! And the ambient intrinsics a program may no longer call: printing,
//! reading input and files, the arguments and `exit` need a capability.

use super::{Checker, Type};
use crate::ast::{Expr, ExprId, ExprKind};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// The capabilities `main` may take, each with the prelude function that
/// makes the real one.
pub const ENTRY_CAPABILITIES: [(&str, &str); 7] = [
    ("$prelude.Console", "$prelude.$console"),
    ("$prelude.Fs", "$prelude.$fs"),
    ("$prelude.Clock", "$prelude.$clock"),
    ("$prelude.Env", "$prelude.$env"),
    ("$prelude.Proc", "$prelude.$proc"),
    ("$prelude.Entropy", "$prelude.$entropy"),
    ("$prelude.Net", "$prelude.$net"),
];

/// What a test may take: each capability, the prelude function making a
/// plain test's (`None`: only an integration test may take it), and the one
/// making an integration test's. A plain test's are safe stand-ins: a fresh
/// folder, a fixed clock, an empty environment, seeded randomness, loopback.
pub const TEST_CAPABILITIES: [(&str, Option<&str>, &str); 8] = [
    (
        "$prelude.Console",
        Some("$prelude.$console"),
        "$prelude.$console",
    ),
    (
        "$prelude.Dir",
        Some("$prelude.$test_dir"),
        "$prelude.$test_dir",
    ),
    (
        "$prelude.Clock",
        Some("$prelude.$test_clock"),
        "$prelude.$clock",
    ),
    ("$prelude.Env", Some("$prelude.$test_env"), "$prelude.$env"),
    (
        "$prelude.Entropy",
        Some("$prelude.$test_entropy"),
        "$prelude.$entropy",
    ),
    ("$prelude.Net", Some("$prelude.$test_net"), "$prelude.$net"),
    ("$prelude.Fs", None, "$prelude.$fs"),
    ("$prelude.Proc", None, "$prelude.$proc"),
];

/// Whose entry point is being built: `main`'s, or a test's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Entry {
    Main,
    Test { integration: bool },
}

/// The prelude function that makes capability `key` for `entry`; `None`
/// when that entry can't take it.
pub fn maker(key: &str, entry: Entry) -> Option<&'static str> {
    match entry {
        Entry::Main => ENTRY_CAPABILITIES
            .iter()
            .find(|(c, _)| *c == key)
            .map(|(_, make)| *make),
        Entry::Test { integration } => TEST_CAPABILITIES
            .iter()
            .find(|(c, _, _)| *c == key)
            .and_then(|(_, plain, real)| if integration { Some(*real) } else { *plain }),
    }
}

/// The bits `lugha_rt_lockdown` takes for the capability kinds an entry
/// was given (`runtime/lockdown.c`'s `LOCK_*`): the kernel limits the
/// process to them. A test's `Dir` is its folder.
pub fn lockdown_kinds(params: &[Type]) -> i64 {
    params
        .iter()
        .map(|p| match p {
            Type::Interface(key, _) => match key.as_str() {
                "$prelude.Fs" => 1,
                "$prelude.Net" => 2,
                "$prelude.Proc" => 4,
                "$prelude.Clock" => 8,
                "$prelude.Dir" => 16,
                _ => 0,
            },
            _ => 0,
        })
        .fold(0, |kinds, bit| kinds | bit)
}

/// The intrinsics only the prelude and std may call; a program reaches them
/// through a capability.
pub(super) const AMBIENT: [&str; 15] = [
    "print",
    "println",
    "eprint",
    "eprintln",
    "read_line",
    "read_stdin_bytes",
    "flush",
    "read_file",
    "write_file",
    "append_file",
    "read_bytes",
    "write_bytes",
    "append_bytes",
    "args",
    "exit",
];

impl Checker {
    /// True if `params` are things the runtime supplies to `entry`, each
    /// once: for `main`, `string[]` and the capabilities; for a test, its
    /// capabilities. Records the prelude functions that make them, as uses of
    /// `owner` (`main` or a test).
    pub(super) fn entry_params(
        &mut self,
        (owner, entry): (&str, Entry),
        params: &[Type],
        span: Span,
    ) -> bool {
        let mut seen: Vec<&Type> = Vec::new();
        for p in params {
            let supplied = match p {
                Type::Array(element) => entry == Entry::Main && **element == Type::String,
                Type::Interface(key, args) => args.is_empty() && maker(key, entry).is_some(),
                Type::Error => true,
                _ => false,
            };
            if !supplied || seen.contains(&p) {
                return false;
            }
            seen.push(p);
        }
        // Only the span matters to a use: where the capability is asked for.
        let anchor = Expr {
            id: ExprId(0),
            span,
            kind: ExprKind::Tuple(Vec::new()),
            type_args: None,
        };
        let saved = std::mem::replace(&mut self.owner, owner.to_string());
        for p in params {
            if let Type::Interface(key, _) = p
                && let Some(make) = maker(key, entry)
            {
                self.used(&anchor, make, &[]);
            }
        }
        self.owner = saved;
        true
    }

    /// True while checking the prelude or std, which may call the ambient
    /// intrinsics to implement the capabilities.
    pub(super) fn bundled_module(&self) -> bool {
        self.module
            .first()
            .is_some_and(|m| m.starts_with('$') || m == "std")
    }

    /// E0471: an ambient intrinsic called from a program, with help naming
    /// the capability and the call to write.
    pub(super) fn ambient_call(&mut self, name: &str, span: Span) {
        let (capability, instead) = match name {
            "read_line" | "flush" => ("Console", format!("`console.{name}()`")),
            "read_stdin_bytes" => ("Console", "`console.read_bytes()`".to_string()),
            "print" | "println" | "eprint" | "eprintln" => {
                ("Console", format!("`console.{name}(…)`"))
            }
            "read_file" | "write_file" | "append_file" | "read_bytes" | "write_bytes"
            | "append_bytes" => ("Fs", format!("`fs.cwd().{name}(…)`")),
            "args" => (
                "string[]",
                "an `args: string[]` parameter of `main`".to_string(),
            ),
            _ => ("", "return an `i32` from `main`".to_string()),
        };
        let message = match name {
            "exit" => "`exit` is gone: a program ends by returning from `main`".to_string(),
            "args" => "`args()` is gone: `main` takes the arguments as a parameter".to_string(),
            _ => {
                let article = if capability == "Fs" { "an" } else { "a" };
                format!("`{name}` needs {article} `{capability}`")
            }
        };
        let help = match capability {
            "Console" | "Fs" => {
                let param = capability.to_lowercase();
                format!(
                    "take `{param}: {capability}` in `main` (and pass it on), then call {instead}"
                )
            }
            _ => instead,
        };
        self.report(Diagnostic::error("E0471", message, span).with_help(help));
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{errors, ok};

    #[test]
    fn main_takes_what_the_runtime_supplies_by_type() {
        ok("fun main() {}");
        ok("fun main(args: string[], console: Console, fs: Fs): i32 { 0 }");
        ok("fun main(fs: Fs, out: Console, argv: string[]) {}");
        ok("fun main(e: Entropy, p: Proc, v: Env, c: Clock, out: Console, n: Net) {}");
        for bad in [
            "fun main(n: i64) {}",
            "fun main(a: string[], b: string[]) {}",
            "fun main(c: Console, d: Console) {}",
            "fun main(d: Dir) {}",
            "fun main(a: Clock, b: Clock) {}",
            "fun main(a: Net, b: Net) {}",
            "fun main(c: Conn) {}",
        ] {
            assert_eq!(errors(bad), [("E0304", "main")], "{bad}");
        }
    }

    #[test]
    fn lockdown_kinds_are_the_entrys_capabilities() {
        use super::lockdown_kinds;
        use crate::check::Type;
        let cap = |name: &str| Type::Interface(format!("$prelude.{name}"), Vec::new());
        assert_eq!(lockdown_kinds(&[]), 0);
        assert_eq!(lockdown_kinds(&[cap("Console"), cap("Entropy")]), 0);
        assert_eq!(
            lockdown_kinds(&[cap("Fs"), cap("Net"), cap("Proc"), cap("Clock"), cap("Dir")]),
            31
        );
        assert_eq!(
            lockdown_kinds(&[Type::Array(Box::new(Type::String)), cap("Net")]),
            2
        );
    }

    #[test]
    fn ambient_calls_are_e0471() {
        for (body, name) in [
            ("println(1);", "println"),
            ("let l = read_line();", "read_line"),
            ("let t = read_file(\"p\");", "read_file"),
            ("let a = args();", "args"),
            ("exit(1);", "exit"),
            ("let b = read_bytes(\"p\");", "read_bytes"),
            (
                "let w = write_bytes(\"p\", \"x\".to_bytes());",
                "write_bytes",
            ),
            (
                "let a = append_bytes(\"p\", \"x\".to_bytes());",
                "append_bytes",
            ),
            ("let i = read_stdin_bytes();", "read_stdin_bytes"),
            ("flush();", "flush"),
        ] {
            let src = format!("fun main() {{ {body} }}");
            assert_eq!(errors(&src), [("E0471", name)], "{body}");
        }
        // The names stay reserved: a program can't declare its own.
        assert_eq!(
            errors("fun println(x: i64) {}\nfun main() {}"),
            [("E0302", "println")]
        );
    }
}
