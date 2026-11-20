//! Calls on interface values: `s.area()` is
//! checked against the interface's signature and dispatched at run time.

use super::applied::Applied;
use super::methods::Method;
use super::{Checker, errors};
use crate::ast::{Expr, Ident};

impl Checker {
    /// True if `name` is a method `Dir` has and `ReadDir`, `key`, lacks: a
    /// write through a folder narrowed to reading.
    fn writes_through_read_dir(&self, key: &str, name: &str) -> bool {
        key == super::impls::READ_DIR
            && self
                .interfaces
                .get(super::impls::DIR)
                .is_some_and(|dir| dir.items.iter().any(|r| r.name == name))
    }

    /// Method `name` of a value of type `interface`: the interface's item at
    /// its arguments, recorded as the callee `<key>.<name>` so codegen
    /// dispatches it. `None` after E0415 when the interface has no such method.
    pub(super) fn through_interface(
        &mut self,
        call: &Expr,
        interface: &Applied,
        name: &Ident,
    ) -> Option<Method> {
        let key = &interface.key;
        // An extending interface's value reaches its ancestors' items too.
        let items: Vec<_> = self
            .value_items(interface)
            .into_iter()
            .map(|(_, r)| r)
            .collect();
        let Some(req) = items.iter().find(|r| r.name == name.name).cloned() else {
            let names: Vec<String> = items.iter().map(|r| format!("`{}`", r.name)).collect();
            let help = if self.writes_through_read_dir(key, &name.name) {
                "a `ReadDir` only reads; take a `Dir` to write".to_string()
            } else if names.is_empty() {
                format!("`{key}` declares no methods")
            } else {
                format!("`{key}` declares {}", names.join(", "))
            };
            let ty = interface.ty();
            self.report(errors::no_method(&name.name, &ty, name.span, Some(help)));
            return None;
        };
        self.callees.insert(call.id, format!("{key}.{}", name.name));
        Some(Method {
            receiver: req.receiver,
            params: req.params,
            muts: req.muts,
            ret: req.ret,
            span: req.span,
            generics: Vec::new(),
            outer: 0,
            key: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{SHAPES, diagnostics, errors, ok};
    use crate::check::{Structs, Type};

    fn e(body: &str) -> Vec<(&'static str, String)> {
        let src = format!("{SHAPES}{body}\nfun main() {{}}");
        errors(&src)
            .into_iter()
            .map(|(c, t)| (c, t.to_string()))
            .collect()
    }

    fn help(body: &str) -> String {
        let src = format!("{SHAPES}{body}\nfun main() {{}}");
        diagnostics(&src)[0].help.clone().expect("a help")
    }

    #[test]
    fn a_read_only_folder_is_a_read_dir_that_cant_write() {
        use crate::check::Type;
        use crate::check::test_util::{errors, let_type, ok};
        let read_dir = Type::Interface("$prelude.ReadDir".into(), Vec::new());
        assert_eq!(
            let_type("fun main(fs: Fs) { let r = fs.cwd().read_only(); }", "r"),
            read_dir
        );
        let result = |t: Type| Type::Enum("$prelude.Result".into(), vec![t, Type::String]);
        assert_eq!(
            let_type(
                "fun main(fs: Fs) { let s = fs.cwd().read_only().dir(\"a\"); }",
                "s"
            ),
            result(read_dir)
        );
        let src = "fun main(fs: Fs) { let w = fs.cwd().read_only().write_file(\"x\", \"y\"); }";
        assert_eq!(errors(src), [("E0415", "write_file")]);
        let help = crate::check::test_util::diagnostics(src)[0].help.clone();
        assert_eq!(
            help.as_deref(),
            Some("a `ReadDir` only reads; take a `Dir` to write")
        );
        // A `ReadDir` isn't a `Dir`.
        let wants_dir = "fun save(d: Dir) {}\nfun main(fs: Fs) { save(fs.cwd().read_only()); }";
        assert!(!errors(wants_dir).is_empty());
        // A program's own fake implements the ten items.
        ok("struct Empty {}\n\
            impl ReadDir for Empty {\n\
                fun dir(self, path: string): Result<ReadDir, string> = Err(\"none\");\n\
                fun read_file(self, path: string): Result<string, string> = Err(\"none\");\n\
                fun read_bytes(self, path: string): Result<u8[], string> = Ok([]);\n\
                fun list(self, path: string): Result<string[], string> = Ok([]);\n\
                fun exists(self, path: string): bool = false;\n\
                fun is_dir(self, path: string): bool = false;\n\
                fun is_file(self, path: string): bool = false;\n\
                fun size(self, path: string): Result<i64, string> = Ok(0);\n\
                fun open(self, path: string): Result<File, string> = Err(\"none\");\n\
                fun path(self): Result<string, string> = Ok(\"/\");\n\
            }\n\
            fun count(d: ReadDir): i64 = d.list(\".\").unwrap_or([]).len;\n\
            fun main() { let n = count(Empty {}); }");
    }

    #[test]
    fn methods_are_called_through_the_interface() {
        let checked = ok(&format!(
            "{SHAPES}fun f(s: Shape): f64 = s.area();\n\
             fun g(s: Shape) {{ let mut t = s; t.grow(2.0); }}\n\
             fun main() {{}}"
        ));
        let callees: Vec<&String> = checked.callees.values().collect();
        assert!(callees.contains(&&"Shape.area".to_string()), "{callees:?}");
        assert!(callees.contains(&&"Shape.grow".to_string()), "{callees:?}");
        assert_eq!(
            e("fun f(s: Shape): i64 = s.area();"),
            [("E0403", "s.area()".into())]
        );
        assert_eq!(
            e("fun f(s: Shape) { let n = s.size(); }"),
            [("E0415", "size".into())]
        );
        assert_eq!(
            e("fun f(s: Shape) { let n = s.area(1); }"),
            [("E0405", "s.area(1)".into())]
        );
        assert_eq!(
            e("fun f(s: Shape) { let mut t = s; t.grow(\"x\"); }"),
            [("E0403", "\"x\"".into())]
        );
    }

    #[test]
    fn mut_self_calls_need_a_mutable_place() {
        assert_eq!(e("fun f(s: Shape) { s.grow(1.0); }")[0].0, "E0501");
        assert_eq!(
            e("fun mk(): Shape = Circle { r: 1.0 };\nfun f() { mk().grow(1.0); }"),
            [("E0502", "mk()".into())]
        );
        ok(&format!(
            "{SHAPES}fun main() {{ let mut xs: Shape[] = [Circle {{ r: 1.0 }}]; xs[0].grow(2.0); }}"
        ));
    }

    #[test]
    fn fields_are_out_of_reach() {
        assert_eq!(e("fun f(s: Shape): f64 = s.r;"), [("E0410", "r".into())]);
        assert!(help("fun f(s: Shape): f64 = s.r;").contains("type pattern"));
    }

    #[test]
    fn interface_values_have_no_equality() {
        assert_eq!(
            e("fun f(a: Shape, b: Shape): bool = a == b;"),
            [("E0404", "a == b".into())]
        );
        assert!(help("fun f(a: Shape, b: Shape): bool = a == b;").contains("match"));
        assert_eq!(
            e("fun g<T: Eq>(x: T) {}\nfun f(s: Shape) { g(s); }")[0].0,
            "E0419"
        );
        assert_eq!(
            e("struct H { s: Shape } with Eq\nfun f(a: H, b: H): bool = a == b;")[0].0,
            // A struct is `Eq` through its fields: the path names `s`.
            "E0451"
        );
    }

    #[test]
    fn interface_values_dont_cross_the_c_boundary() {
        assert_eq!(
            e("extern fun c_area(s: Shape): f64;"),
            [("E0409", "Shape".into())]
        );
    }

    #[test]
    fn interface_values_are_deep() {
        let shape = Type::Interface("Shape".into(), Vec::new());
        let mut structs = Structs::new();
        assert!(shape.is_deep(&structs));
        structs.insert("H".into(), vec![("s".into(), shape)]);
        assert!(Type::Struct("H".into(), vec![]).is_deep(&structs));
        assert!(!Type::F64.is_deep(&structs));
    }
}
