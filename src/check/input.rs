//! Input and interpolation: the intrinsics
//! `args`, `parse_i64` and `parse_f64`, and `"a {x} b"` strings; and the
//! I/O intrinsics.

use super::expr::Expect;
use super::results::RESULT;
use super::{Checker, Type, errors};
use crate::ast::{Expr, Part};
use crate::format;

/// The input and I/O intrinsics, checked here rather than in
/// `check::call`, except `eprint`/`eprintln`, which follow `print`'s rules.
pub(super) const INPUT: [&str; 14] = [
    "args",
    "parse_i64",
    "parse_u64",
    "parse_f64",
    "read_line",
    "eprint",
    "eprintln",
    "exit",
    // Bytes.
    "read_stdin_bytes",
    "string_from_utf8",
    // C memory.
    "c_read",
    "c_write",
    "flush",
    // A `format` call reaching here has no literal template (E0432).
    "format",
];

/// The built-in type of an untyped C pointer: a handle
/// no module declares.
pub const CPTR: &str = "CPtr";

/// A resource the runtime made and frees when its last holder goes (a
/// folder's descriptor, a running hash): counted, unlike `CPtr`. Only the
/// prelude and std name it.
pub const RESOURCE: &str = "$Resource";

/// The prelude function behind an intrinsic written in Lugha.
pub fn prelude_helper(intrinsic: &str) -> Option<&'static str> {
    Some(match intrinsic {
        "parse_i64" => "text_parse_i64",
        "parse_u64" => "text_parse_u64",
        "string_from_utf8" => "text_from_utf8",
        _ => return None,
    })
}

impl Checker {
    /// `args()`, `parse_i64(s)`, `parse_f64(s)`, or an I/O intrinsic.
    pub(super) fn input_intrinsic(&mut self, call: &Expr, name: &str, args: &[Expr]) -> Type {
        if name == "format" {
            return self.format_without_template(call, args);
        }
        let result = |t: Type| Type::Enum(RESULT.to_string(), vec![t, Type::String]);
        let bytes = || Type::Array(Box::new(Type::U8));
        let cptr = || Type::Handle(CPTR.to_string());
        let (params, ret) = match name {
            "args" => (Vec::new(), Type::Array(Box::new(Type::String))),
            "parse_i64" => (vec![Type::String], Type::Option(Box::new(Type::I64))),
            "parse_f64" => (vec![Type::String], Type::Option(Box::new(Type::F64))),
            "parse_u64" => (vec![Type::String], Type::Option(Box::new(Type::U64))),
            "read_line" => (Vec::new(), Type::Option(Box::new(Type::String))),
            "read_stdin_bytes" => (Vec::new(), bytes()),
            "string_from_utf8" => (vec![bytes()], result(Type::String)),
            // C memory: copies only, through a `CPtr`.
            "c_read" => (vec![cptr(), Type::U64, Type::U64], bytes()),
            "c_write" => (vec![cptr(), Type::U64, bytes()], Type::Unit),
            // Stdout's buffer, out now.
            "flush" => (Vec::new(), Type::Unit),
            // `exit` never returns.
            _ => (vec![Type::I32], Type::Never),
        };
        if args.len() != params.len() {
            self.report(errors::arity(name, params.len(), args.len(), call.span));
            self.unchecked_args(args);
            return ret;
        }
        for (arg, ty) in args.iter().zip(params) {
            self.expect_type(arg, &Expect::of(ty));
        }
        // These are written in Lugha in the prelude, and compiled only when a program uses them.
        if let Some(helper) = prelude_helper(name) {
            let key = super::qualify(&[crate::modules::PRELUDE_PATH.to_string()], helper);
            self.used(call, &key, &[]);
        }
        ret
    }

    /// `"a {x} b"`: each interpolated value prints (E0403 for `void`),
    /// and the whole is a `string`.
    pub(super) fn interpolation(&mut self, parts: &[Part]) -> Type {
        for part in parts {
            let Part::Expr(e, format) = part else {
                continue;
            };
            let ty = self.value(e, None);
            if self.displayed(&ty, e.span) {
                self.printed(e, &ty);
            }
            // `{x:spec}`: the specifier must parse and fit `x`.
            if let Some(f) = format
                && !matches!(ty, Type::Error | Type::Never)
            {
                let class = if ty.is_integer() {
                    format::Value::Int
                } else if ty.is_float() {
                    format::Value::Float
                } else {
                    format::Value::Other
                };
                if let Err(why) = format::parse(&f.text).and_then(|s| format::check(&s, class)) {
                    self.report(errors::bad_format(why, f.span));
                }
            }
        }
        Type::String
    }
}

#[cfg(test)]
mod tests {
    use crate::check::Type;
    use crate::check::test_util::{errors, let_type};

    #[test]
    fn interpolated_strings_are_strings() {
        let src = "fun main() { let n = 2; let f = 2.5; let s = \"x\"; let t = \"{n} {f} {true} {s} {n + 1}\"; }";
        assert_eq!(let_type(src, "t"), Type::String);
    }

    #[test]
    fn only_printable_values_interpolate() {
        // A value interpolates its `Display`; anything else, its debug form.
        assert_eq!(
            errors("fun f() {}\nfun main() { let t = \"{f()}\"; }"),
            [("E0422", "f()")]
        );
        crate::check::test_util::ok(
            "struct P { x: i64 }\nfun main() { let t = \"{[1].debug()} {P { x: 1 }.debug()}\"; }",
        );
    }

    #[test]
    fn input_intrinsics_have_their_types() {
        assert_eq!(
            let_type("fun main(args: string[]) { let a = args; }", "a"),
            Type::Array(Box::new(Type::String))
        );
        assert_eq!(
            let_type("fun main() { let n = parse_i64(\"1\"); }", "n"),
            Type::Option(Box::new(Type::I64))
        );
        assert_eq!(
            let_type("fun main() { let f = parse_f64(\"1.5\"); }", "f"),
            Type::Option(Box::new(Type::F64))
        );
        assert_eq!(
            errors("fun main() { let n = parse_i64(true); }"),
            [("E0403", "true")]
        );
        // The arguments come as `main`'s parameter, not from `args()`.
        assert_eq!(
            errors("fun main() { let a = args(); }"),
            [("E0471", "args")]
        );
        assert_eq!(errors("fun args() {}\nfun main() {}"), [("E0302", "args")]);
    }

    #[test]
    fn io_intrinsics_have_their_types() {
        let result = |a: Type| Type::Enum("$prelude.Result".into(), vec![a, Type::String]);
        let t = |body: &str, name: &str| {
            let_type(
                &format!("fun main(console: Console, fs: Fs) {{ {body} }}"),
                name,
            )
        };
        assert_eq!(
            t("let l = console.read_line();", "l"),
            Type::Option(Box::new(Type::String))
        );
        assert_eq!(
            t("let f = fs.cwd().read_file(\"p\");", "f"),
            result(Type::String)
        );
        assert_eq!(
            t("let w = fs.cwd().write_file(\"p\", \"t\");", "w"),
            result(Type::I64)
        );
        assert_eq!(
            t("let a = fs.cwd().append_file(\"p\", \"t\");", "a"),
            result(Type::I64)
        );
        crate::check::test_util::ok(
            "fun main(console: Console) { console.eprint(1); console.eprintln(\"x\"); console.eprintln(\"\"); }",
        );
        // A program ends by returning from `main`; `exit` is gone.
        assert_eq!(errors("fun main() { exit(1); }"), [("E0471", "exit")]);
    }

    #[test]
    fn io_intrinsics_check_their_arguments() {
        assert_eq!(
            errors("fun main(console: Console) { let l = console.read_line(1); }"),
            [("E0405", "console.read_line(1)")]
        );
        assert_eq!(
            errors("fun main(fs: Fs) { let w = fs.cwd().write_file(1, \"t\"); }"),
            [("E0401", "1")]
        );
        assert_eq!(
            errors("fun f() {}\nfun main(console: Console) { console.eprint(f()); }"),
            [("E0422", "f()")]
        );
        assert_eq!(errors("fun exit() {}\nfun main() {}"), [("E0302", "exit")]);
    }

    #[test]
    fn format_specifiers_fit_their_values() {
        let e = |body: &str| {
            errors(&format!(
                "struct P {{ x: i64 }}\nfun main() {{ let n = 1; let f = 2.5; {body} }}"
            ))
            .into_iter()
            .map(|(c, t)| (c, t.to_string()))
            .collect::<Vec<_>>()
        };
        assert_eq!(e("let t = \"{n:.2}\";")[0].0, "E0431");
        assert_eq!(e("let t = \"{f:x}\";")[0].0, "E0431");
        assert_eq!(e("let s = \"x\"; let t = \"{s:05}\";")[0].0, "E0431");
        assert_eq!(e("let t = \"{n:#}\";")[0].0, "E0431");
        assert_eq!(e("let t = \"{n:q}\";")[0].0, "E0431");
        assert_eq!(e("let t = \"{n:2000}\";")[0].0, "E0431");
        crate::check::test_util::ok(
            "struct P { x: i64 }\nimpl Display for P { fun show(self): string = \"p\"; }\n\
             fun main() { let p = P { x: 1 }; let n: u8 = 3; let t = \"{p:>20} {n:#010b} {2.5:+<8.1}\"; }",
        );
    }
}
