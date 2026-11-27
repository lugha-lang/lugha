//! `lughac build --shared --jni <package.Class>`: Kotlin's or Java's door to
//! a Lugha library. For each function C calls by name (`extern fun … { …
//! }`), the library gains a JNI function, `Java_<package>_<Class>_<method>`,
//! and the class's source declares it: a Kotlin `object` (`--jni-lang
//! kotlin`, the default) or a Java class (`--jni-lang java`), whose JNI
//! names are the same, so the glue is one for both.
//!
//! The glue converts Java's strings exactly (UTF-16 to Lugha's UTF-8 and
//! back, never JNI's "modified UTF-8"), catches a Lugha panic and throws it
//! as `<Class>.Panic`, and reaches JNI only through the entries of its
//! function table it declares itself, at the positions the JNI
//! specification fixes, so building it needs no JDK or NDK header.

use std::collections::{HashMap, HashSet};

use crate::ast::{FunDecl, Item, Type, TypeKind};
use crate::diagnostic::Diagnostic;
use crate::modules::Module;
use crate::span::Span;

/// The language of the class's source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Lang {
    Kotlin,
    Java,
}

/// What `--jni` adds to a library: the glue's C, the class's source (its
/// path under the library's folder, and text), and the JNI functions the
/// library shows.
pub(super) struct Door {
    pub glue: String,
    pub source: (std::path::PathBuf, String),
    pub symbols: Vec<String>,
}

/// How one Lugha type crosses to Java.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
    Boolean,
    /// `char`: a code point, `int` in Java.
    Char,
    Str,
    /// `Option<string>`: a `String` that may be `null`.
    OptStr,
    /// A handle, or `Option` of one: the pointer as a `long`.
    Handle,
}

impl Kind {
    /// JNI's C type for it.
    fn jni(self) -> &'static str {
        match self {
            Kind::Byte => "int8_t",
            Kind::Short => "int16_t",
            Kind::Int | Kind::Char => "int32_t",
            Kind::Long | Kind::Handle => "int64_t",
            Kind::Float => "float",
            Kind::Double => "double",
            Kind::Boolean => "uint8_t",
            Kind::Str | Kind::OptStr => "void *",
        }
    }

    /// The export's own C type for it (as `c_header` writes it).
    fn c(self, result: bool) -> &'static str {
        match self {
            Kind::Byte => "int8_t",
            Kind::Short => "int16_t",
            Kind::Int => "int32_t",
            Kind::Long => "int64_t",
            Kind::Float => "float",
            Kind::Double => "double",
            Kind::Boolean => "bool",
            Kind::Char => "uint32_t",
            Kind::Str if result => "char *",
            Kind::Str | Kind::OptStr => "const char *",
            Kind::Handle => "void *",
        }
    }

    pub(super) fn kotlin(self) -> &'static str {
        match self {
            Kind::Byte => "Byte",
            Kind::Short => "Short",
            Kind::Int | Kind::Char => "Int",
            Kind::Long | Kind::Handle => "Long",
            Kind::Float => "Float",
            Kind::Double => "Double",
            Kind::Boolean => "Boolean",
            Kind::Str => "String",
            Kind::OptStr => "String?",
        }
    }

    pub(super) fn java(self) -> &'static str {
        match self {
            Kind::Byte => "byte",
            Kind::Short => "short",
            Kind::Int | Kind::Char => "int",
            Kind::Long | Kind::Handle => "long",
            Kind::Float => "float",
            Kind::Double => "double",
            Kind::Boolean => "boolean",
            Kind::Str | Kind::OptStr => "String",
        }
    }
}

/// What Java can take: for E0487's help.
const JAVA_TAKES: &str = "Java takes `i8`, `i16`, `i32`, `i64`, `f32`, `f64`, `bool`, `char` (an `int`), `string`, `Option<string>` and handles (a `long`); it has no unsigned integers and no struct values";

/// How `ty` crosses to Java, given the extern types (`handles`); `None`
/// when it can't.
fn kind(ty: &Type, handles: &HashSet<String>) -> Option<Kind> {
    let handle = |t: &Type| match &t.kind {
        TypeKind::Named(n) => handles.contains(n),
        TypeKind::Applied(a) => handles.contains(&a.name.name),
        _ => false,
    };
    Some(match &ty.kind {
        TypeKind::I8 => Kind::Byte,
        TypeKind::I16 => Kind::Short,
        TypeKind::I32 => Kind::Int,
        TypeKind::I64 => Kind::Long,
        TypeKind::F32 => Kind::Float,
        TypeKind::F64 => Kind::Double,
        TypeKind::Bool => Kind::Boolean,
        TypeKind::Char => Kind::Char,
        TypeKind::String => Kind::Str,
        TypeKind::Option(inner) if inner.kind == TypeKind::String => Kind::OptStr,
        TypeKind::Option(inner) if handle(inner) => Kind::Handle,
        _ if handle(ty) => Kind::Handle,
        _ => return None,
    })
}

/// `word_count` → `wordCount`: Kotlin's and Java's way.
fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in name.chars() {
        if c == '_' && !out.is_empty() {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// A name as JNI writes it in a C name: `_` is `_1`, and anything but an
/// ASCII letter or digit `_0xxxx`, its UTF-16 units in hex.
fn escape(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => out.push(c),
            '_' => out.push_str("_1"),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("_0{unit:04x}"));
                }
            }
        }
    }
    out
}

/// `Java_<package>_<Class>_<method>` for `class` (`com.example.Words`).
pub(super) fn jni_name(class: &str, method: &str) -> String {
    let parts: Vec<String> = class.split('.').map(escape).collect();
    format!("Java_{}_{}", parts.join("_"), escape(method))
}

/// Java's reserved words, which no Java method may be named.
pub(super) const JAVA_RESERVED: [&str; 53] = [
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "void",
    "volatile",
    "while",
    "true",
    "false",
    "null",
];

/// Whether `class` (`com.example.Words`) names a class Java can hold:
/// identifiers joined by `.`.
pub(super) fn valid_class(class: &str) -> bool {
    class.split('.').all(|part| {
        let mut chars = part.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !JAVA_RESERVED.contains(&part)
    })
}

/// One export as Java sees it.
pub(super) struct Method<'a> {
    pub f: &'a FunDecl,
    pub name: String,
    pub params: Vec<(String, Kind)>,
    pub ret: Option<Kind>,
}

/// The door for `class` in `lang` to the exports of `modules`, for the
/// library `lib` (its name for `System.loadLibrary`); E0487 for each
/// export Java can't call.
pub(super) fn door(
    modules: &[Module],
    (class, lang, lib): (&str, Lang, &str),
) -> Result<Door, Vec<Diagnostic>> {
    let (mut handles, mut exports) = (HashSet::new(), Vec::new());
    for program in modules.iter().filter_map(|m| m.program.as_ref()) {
        for item in &program.items {
            match item {
                Item::ExternType(t) => {
                    handles.insert(t.name.name.clone());
                }
                Item::Fun(f) if f.export.is_some() => exports.push(f),
                _ => {}
            }
        }
    }
    let (mut methods, mut errors) = (Vec::new(), Vec::new());
    let mut seen: HashMap<String, Span> = HashMap::new();
    for f in exports {
        let refuse = |what: String, span: Span| {
            Diagnostic::error("E0487", format!("Java can't call this: {what}"), span)
                .with_help(JAVA_TAKES)
        };
        let mut params = Vec::new();
        for p in &f.params {
            match kind(&p.ty, &handles) {
                Some(k) => params.push((camel(&p.name.name), k)),
                None => errors.push(refuse(format!("`{}`", p.ty.written()), p.ty.span)),
            }
        }
        let ret = match &f.ret {
            None => None,
            Some(t) => match kind(t, &handles) {
                Some(Kind::OptStr) | None => {
                    errors.push(refuse(format!("`{}` as a result", t.written()), t.span));
                    None
                }
                k => k,
            },
        };
        let name = camel(&f.name.name);
        if lang == Lang::Java && JAVA_RESERVED.contains(&name.as_str()) {
            let help = "rename the Lugha function: Java can't name a method this";
            errors.push(
                Diagnostic::error(
                    "E0487",
                    format!("`{name}` is a word Java reserves"),
                    f.name.span,
                )
                .with_help(help),
            );
        }
        match seen.get(&name) {
            Some(first) => errors.push(
                Diagnostic::error(
                    "E0487",
                    format!("Java would see two methods `{name}`"),
                    f.name.span,
                )
                .with_label(*first, "named so here first")
                .with_help("rename one: `word_count` and `wordCount` are one name to Java"),
            ),
            None => {
                seen.insert(name.clone(), f.name.span);
            }
        }
        methods.push(Method {
            f,
            name,
            params,
            ret,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let symbols = methods.iter().map(|m| jni_name(class, &m.name)).collect();
    let glue = super::jni_glue::glue(class, &methods_view(&methods));
    let (package, simple) = match class.rsplit_once('.') {
        Some((package, simple)) => (Some(package), simple),
        None => (None, class),
    };
    let ext = if lang == Lang::Kotlin { "kt" } else { "java" };
    let mut path: std::path::PathBuf =
        package.map_or_else(Default::default, |p| p.split('.').collect());
    path.push(format!("{simple}.{ext}"));
    let text = match lang {
        Lang::Kotlin => super::jni_source::kotlin(package, simple, lib, &methods),
        Lang::Java => super::jni_source::java(package, simple, lib, &methods),
    };
    Ok(Door {
        glue,
        source: (path, text),
        symbols,
    })
}

/// What the glue needs of each method.
fn methods_view(methods: &[Method]) -> Vec<super::jni_glue::Export> {
    methods
        .iter()
        .map(|m| {
            let export = m.f.export.as_ref().expect("only exports are methods");
            super::jni_glue::Export {
                method: m.name.clone(),
                symbol: export.symbol.as_ref().unwrap_or(&m.f.name).name.clone(),
                params: m
                    .params
                    .iter()
                    .map(|(n, k)| super::jni_glue::Param {
                        name: n.clone(),
                        jni: k.jni(),
                        c: k.c(false),
                        string: matches!(k, Kind::Str | Kind::OptStr),
                        nullable: *k == Kind::OptStr,
                    })
                    .collect(),
                ret: m.ret.map(|k| super::jni_glue::Ret {
                    jni: k.jni(),
                    c: k.c(true),
                    string: k == Kind::Str,
                }),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{camel, escape, jni_name, valid_class};

    #[test]
    fn names_follow_kotlin_java_and_jni() {
        assert_eq!(camel("word_count"), "wordCount");
        assert_eq!(camel("pick"), "pick");
        assert_eq!(camel("to_utf8_text"), "toUtf8Text");
        assert_eq!(escape("a_b"), "a_1b");
        assert_eq!(escape("é"), "_000e9");
        assert_eq!(
            jni_name("com.example.Words", "wordCount"),
            "Java_com_example_Words_wordCount"
        );
        assert_eq!(jni_name("my_app.W", "f"), "Java_my_1app_W_f");
        assert!(valid_class("com.example.Words") && valid_class("Words"));
        assert!(!valid_class("com..Words") && !valid_class("com.class.W") && !valid_class("1x"));
    }
}
