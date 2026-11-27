//! The class `--jni` writes beside a library (`jni.rs`): a Kotlin `object`
//! of `@JvmStatic external fun`s, or a Java class of `static native`
//! methods, whose JNI names are the same.

use super::jni::{JAVA_RESERVED, Kind, Method};

/// Kotlin's hard keywords, which a name escapes with backticks.
const KOTLIN_HARD: [&str; 28] = [
    "as",
    "break",
    "class",
    "continue",
    "do",
    "else",
    "false",
    "for",
    "fun",
    "if",
    "in",
    "interface",
    "is",
    "null",
    "object",
    "package",
    "return",
    "super",
    "this",
    "throw",
    "true",
    "try",
    "typealias",
    "typeof",
    "val",
    "var",
    "when",
    "while",
];

/// A name Kotlin can write: a hard keyword in backticks.
fn kotlin_name(name: &str) -> String {
    if KOTLIN_HARD.contains(&name) {
        format!("`{name}`")
    } else {
        name.to_string()
    }
}

/// `Words.kt`: an `object` of `@JvmStatic external` functions.
pub(super) fn kotlin(package: Option<&str>, class: &str, lib: &str, methods: &[Method]) -> String {
    let mut out =
        String::from("// Written by lughac: Kotlin's door to a Lugha library. Don't edit.\n");
    if let Some(p) = package {
        out.push_str(&format!("package {p}\n"));
    }
    out.push_str(&format!(
        "\nobject {class} {{\n    init {{\n        System.loadLibrary(\"{lib}\")\n    }}\n\n    \
         /** A panic in the Lugha library, with its message. */\n    \
         class Panic(message: String) : RuntimeException(message)\n"
    ));
    for m in methods {
        let params: Vec<String> = m
            .params
            .iter()
            .map(|(n, k)| format!("{}: {}", kotlin_name(n), k.kotlin()))
            .collect();
        let ret = m.ret.map_or(String::new(), |k| format!(": {}", k.kotlin()));
        out.push_str(&format!(
            "\n    @JvmStatic external fun {}({}){ret}\n",
            kotlin_name(&m.name),
            params.join(", ")
        ));
    }
    out.push_str("}\n");
    out
}

/// `Words.java`: a class of `static native` methods.
pub(super) fn java(package: Option<&str>, class: &str, lib: &str, methods: &[Method]) -> String {
    let mut out =
        String::from("// Written by lughac: Java's door to a Lugha library. Don't edit.\n");
    if let Some(p) = package {
        out.push_str(&format!("package {p};\n"));
    }
    out.push_str(&format!(
        "\npublic final class {class} {{\n    static {{\n        System.loadLibrary(\"{lib}\");\n    }}\n\n    \
         private {class}() {{\n    }}\n\n    \
         /** A panic in the Lugha library, with its message. */\n    \
         public static final class Panic extends RuntimeException {{\n        \
         public Panic(String message) {{\n            super(message);\n        }}\n    }}\n"
    ));
    for m in methods {
        let params: Vec<String> = m
            .params
            .iter()
            .map(|(n, k)| {
                // A parameter named as a Java word gets a trailing `_`.
                let n = if JAVA_RESERVED.contains(&n.as_str()) {
                    format!("{n}_")
                } else {
                    n.clone()
                };
                format!("{} {n}", k.java())
            })
            .collect();
        let ret = m.ret.map_or("void", Kind::java);
        out.push_str(&format!(
            "\n    public static native {ret} {}({});\n",
            m.name,
            params.join(", ")
        ));
    }
    out.push_str("}\n");
    out
}
