//! The C header `lughac build --shared` writes beside a library: each
//! function C calls, declared as C calls it, after the extern structs and
//! handles their parameters and results name, so C never declares one by
//! hand, wrongly.

use std::collections::{HashMap, HashSet};

use crate::ast::{FunDecl, Item, StructDecl, Type, TypeKind};
use crate::modules::Module;

/// The header for a library named after `stem`, from its program's
/// modules (the bundled ones have no exports or C types).
pub(super) fn header(stem: &str, modules: &[Module]) -> String {
    let (mut structs, mut handles, mut exports) = (HashMap::new(), HashSet::new(), Vec::new());
    for program in modules.iter().filter_map(|m| m.program.as_ref()) {
        for item in &program.items {
            match item {
                Item::Struct(s) if s.c_layout.is_some() => {
                    structs.insert(s.name.name.as_str(), s);
                }
                Item::ExternType(t) => {
                    handles.insert(t.name.name.clone());
                }
                Item::Fun(f) if f.export.is_some() => exports.push(f),
                _ => {}
            }
        }
    }
    // Only what the exports name, and what those structs hold.
    let mut used: Vec<&StructDecl> = Vec::new();
    let mut named: HashSet<String> = HashSet::new();
    let mut pending: Vec<&Type> = exports
        .iter()
        .flat_map(|f| f.params.iter().map(|p| &p.ty).chain(f.ret.as_ref()))
        .collect();
    while let Some(ty) = pending.pop() {
        let inner = match &ty.kind {
            TypeKind::Option(inner) => inner,
            _ => ty,
        };
        let Some(name) = name_of(inner) else { continue };
        if !named.insert(name.to_string()) {
            continue;
        }
        if let Some(s) = structs.get(name) {
            used.push(s);
            pending.extend(s.fields.iter().map(|f| &f.ty));
        }
    }
    let guard: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    let mut out = format!(
        "/* {stem}.h: the functions C calls in this Lugha library, written by lughac. */\n\
         #ifndef LUGHA_{guard}_H\n#define LUGHA_{guard}_H\n\n\
         #include <stdbool.h>\n#include <stdint.h>\n\n\
         #ifdef __cplusplus\nextern \"C\" {{\n#endif\n\n"
    );
    let mut opaque: Vec<&String> = named.iter().filter(|n| handles.contains(*n)).collect();
    opaque.sort();
    for name in &opaque {
        out.push_str(&format!("typedef struct {name} {name};\n"));
    }
    if !opaque.is_empty() {
        out.push('\n');
    }
    for s in in_dependency_order(&used) {
        out.push_str(&c_struct(s, &handles));
    }
    for f in exports {
        out.push_str(&prototype(f, &handles));
    }
    out.push_str("\n#ifdef __cplusplus\n}\n#endif\n\n#endif\n");
    out
}

/// The name a written type refers to: `T` of `m.T` too.
fn name_of(ty: &Type) -> Option<&str> {
    match &ty.kind {
        TypeKind::Named(name) => Some(name),
        TypeKind::Applied(applied) => Some(&applied.name.name),
        _ => None,
    }
}

/// The C type of `ty`; a string is `const char *` coming in, `char *`
/// going back (C's own copy).
fn c_type(ty: &Type, handles: &HashSet<String>, result: bool) -> String {
    let named = |name: &str| {
        if handles.contains(name) {
            format!("{name} *")
        } else {
            name.to_string()
        }
    };
    match &ty.kind {
        TypeKind::I8 => "int8_t".into(),
        TypeKind::I16 => "int16_t".into(),
        TypeKind::I32 => "int32_t".into(),
        TypeKind::I64 => "int64_t".into(),
        TypeKind::U8 => "uint8_t".into(),
        TypeKind::U16 => "uint16_t".into(),
        TypeKind::U32 | TypeKind::Char => "uint32_t".into(),
        TypeKind::U64 => "uint64_t".into(),
        TypeKind::F32 => "float".into(),
        TypeKind::F64 => "double".into(),
        TypeKind::Bool => "bool".into(),
        TypeKind::String if result => "char *".into(),
        TypeKind::String => "const char *".into(),
        TypeKind::Option(inner) if inner.kind == TypeKind::String => "const char *".into(),
        TypeKind::Option(inner) => name_of(inner).map_or("void *".into(), named),
        _ => name_of(ty).map_or("void *".into(), named),
    }
}

/// `R name(params);`, a string result noted as the caller's to free.
fn prototype(f: &FunDecl, handles: &HashSet<String>) -> String {
    let export = f.export.as_ref().expect("only exports are declared");
    let symbol = export.symbol.as_ref().unwrap_or(&f.name).name.as_str();
    let ret = f
        .ret
        .as_ref()
        .map_or("void".into(), |t| c_type(t, handles, true));
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| join(&c_type(&p.ty, handles, false), &p.name.name))
        .collect();
    let params = if params.is_empty() {
        "void".to_string()
    } else {
        params.join(", ")
    };
    let note = match f.ret.as_ref().map(|t| &t.kind) {
        Some(TypeKind::String) => "/* The result is the caller's: free it with free(). */\n",
        _ => "",
    };
    format!("{note}{}({params});\n", join(&ret, symbol))
}

/// `type name`, with no space after a pointer's `*`.
fn join(ty: &str, name: &str) -> String {
    if ty.ends_with('*') {
        format!("{ty}{name}")
    } else {
        format!("{ty} {name}")
    }
}

/// An extern struct or union, as C declares it.
fn c_struct(s: &StructDecl, handles: &HashSet<String>) -> String {
    let kind = if s.union.is_some() { "union" } else { "struct" };
    let name = &s.name.name;
    let mut out = format!("typedef {kind} {name} {{\n");
    for field in &s.fields {
        let ty = c_type(&field.ty, handles, false);
        out.push_str(&format!("    {};\n", join(&ty, &field.name.name)));
    }
    out.push_str(&format!("}} {name};\n\n"));
    out
}

/// The structs, each after the structs its fields hold by value.
fn in_dependency_order<'a>(structs: &[&'a StructDecl]) -> Vec<&'a StructDecl> {
    let by_name: HashMap<&str, &StructDecl> =
        structs.iter().map(|s| (s.name.name.as_str(), *s)).collect();
    let mut placed: HashSet<&str> = HashSet::new();
    let mut order = Vec::new();
    fn visit<'a>(
        s: &'a StructDecl,
        by_name: &HashMap<&str, &'a StructDecl>,
        placed: &mut HashSet<&'a str>,
        order: &mut Vec<&'a StructDecl>,
    ) {
        if !placed.insert(s.name.name.as_str()) {
            return;
        }
        for field in &s.fields {
            if let Some(inner) = name_of(&field.ty).and_then(|n| by_name.get(n)) {
                visit(inner, by_name, placed, order);
            }
        }
        order.push(s);
    }
    for s in structs {
        visit(s, &by_name, &mut placed, &mut order);
    }
    order
}
