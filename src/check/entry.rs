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
pub const ENTRY_CAPABILITIES: [(&str, &str); 2] = [
    ("$prelude.Console", "$prelude.$console"),
    ("$prelude.Fs", "$prelude.$fs"),
];

/// The intrinsics only the prelude and std may call; a program reaches them
/// through a capability.
pub(super) const AMBIENT: [&str; 10] = [
    "print",
    "println",
    "eprint",
    "eprintln",
    "read_line",
    "read_file",
    "write_file",
    "append_file",
    "args",
    "exit",
];

impl Checker {
    /// True if `params` are things the runtime supplies, each once:
    /// `string[]`, `Console`, `Fs`. Records the prelude functions that make
    /// the capabilities, as uses of `owner` (`main` or a test).
    pub(super) fn entry_params(&mut self, owner: &str, params: &[Type], span: Span) -> bool {
        let mut seen: Vec<&Type> = Vec::new();
        for p in params {
            let supplied = match p {
                Type::Array(element) => **element == Type::String,
                Type::Interface(key, args) => {
                    args.is_empty() && ENTRY_CAPABILITIES.iter().any(|(c, _)| c == key)
                }
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
                && let Some((_, make)) = ENTRY_CAPABILITIES.iter().find(|(c, _)| c == key)
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
            "read_line" => ("Console", "`console.read_line()`".to_string()),
            "print" | "println" | "eprint" | "eprintln" => {
                ("Console", format!("`console.{name}(…)`"))
            }
            "read_file" | "write_file" | "append_file" => ("Fs", format!("`fs.cwd().{name}(…)`")),
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
        for bad in [
            "fun main(n: i64) {}",
            "fun main(a: string[], b: string[]) {}",
            "fun main(c: Console, d: Console) {}",
            "fun main(d: Dir) {}",
        ] {
            assert_eq!(errors(bad), [("E0304", "main")], "{bad}");
        }
    }

    #[test]
    fn ambient_calls_are_e0471() {
        for (body, name) in [
            ("println(1);", "println"),
            ("let l = read_line();", "read_line"),
            ("let t = read_file(\"p\");", "read_file"),
            ("let a = args();", "args"),
            ("exit(1);", "exit"),
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
