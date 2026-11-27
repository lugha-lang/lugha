//! The C of `--jni`'s glue: for each export, the JNI function Kotlin or
//! Java calls, converting its arguments, calling the export with its panic
//! caught, throwing that panic as `<Class>.Panic`, and converting its
//! result. Written by `jni.rs`, compiled into the library.

/// What the glue needs of one export.
pub(super) struct Export {
    /// Its method's name, for the JNI function's.
    pub method: String,
    /// Its C name, which the glue calls.
    pub symbol: String,
    pub params: Vec<Param>,
    pub ret: Option<Ret>,
}

pub(super) struct Param {
    pub name: String,
    /// JNI's C type, and the export's.
    pub jni: &'static str,
    pub c: &'static str,
    /// A `String`: converted to UTF-8; `nullable` when `null` is `None`.
    pub string: bool,
    pub nullable: bool,
}

pub(super) struct Ret {
    pub jni: &'static str,
    pub c: &'static str,
    /// A `String`, from the export's `malloc`ed copy.
    pub string: bool,
}

/// What every glue file holds before its functions: JNI's few function
/// table entries, at the positions the JNI specification fixes, and the
/// conversions between Java's UTF-16 and Lugha's UTF-8.
const PRELUDE: &str = r#"#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* JNI's environment points at a pointer to its function table, whose
 * entries the JNI specification numbers and never moves. */
typedef void *const *LjTable;
#define LJ(env, n, type) ((type)((*(LjTable *)(env))[n]))
enum {
    LJ_FIND_CLASS = 6,
    LJ_THROW_NEW = 14,
    LJ_DELETE_LOCAL_REF = 23,
    LJ_NEW_STRING = 163,
    LJ_GET_STRING_LENGTH = 164,
    LJ_GET_STRING_CHARS = 165,
    LJ_RELEASE_STRING_CHARS = 166
};

void lugha_rt_export_catch(void);
char *lugha_rt_export_caught(void);

/* Throws a new `cls` with the UTF-8 message `text`, given to JNI as its
 * "modified UTF-8": NUL as C0 80, a character past U+FFFF as two
 * three-byte surrogates. */
__attribute__((unused)) static void lj_throw(void *env, const char *cls, const char *text) {
    size_t n = strlen(text);
    unsigned char *m = malloc(n * 2 + 1);
    size_t at = 0;
    for (size_t i = 0; m != NULL && i < n;) {
        unsigned char c = (unsigned char)text[i];
        if (c >= 0xF0 && i + 3 < n) {
            uint32_t cp = ((uint32_t)(c & 0x07) << 18) | ((uint32_t)(text[i + 1] & 0x3F) << 12) |
                          ((uint32_t)(text[i + 2] & 0x3F) << 6) | (uint32_t)(text[i + 3] & 0x3F);
            cp -= 0x10000;
            uint32_t units[2] = {0xD800 + (cp >> 10), 0xDC00 + (cp & 0x3FF)};
            for (int u = 0; u < 2; u++) {
                m[at++] = (unsigned char)(0xE0 | (units[u] >> 12));
                m[at++] = (unsigned char)(0x80 | ((units[u] >> 6) & 0x3F));
                m[at++] = (unsigned char)(0x80 | (units[u] & 0x3F));
            }
            i += 4;
        } else {
            m[at++] = c;
            i++;
        }
    }
    if (m != NULL) {
        m[at] = '\0';
    }
    void *k = LJ(env, LJ_FIND_CLASS, void *(*)(void *, const char *))(env, cls);
    if (k != NULL) {
        LJ(env, LJ_THROW_NEW, int32_t (*)(void *, void *, const char *))(env, k, m != NULL ? (char *)m : "panic");
        LJ(env, LJ_DELETE_LOCAL_REF, void (*)(void *, void *))(env, k);
    }
    free(m);
}

/* A Java string as UTF-8 (`malloc`ed), or NULL for `null`; `*failed` when
 * it couldn't be read or holds a NUL (which would end C's copy early), an
 * exception then pending. A lone surrogate is U+FFFD, as Lugha's strings
 * hold only whole characters. */
__attribute__((unused)) static char *lj_utf8(void *env, void *s, bool *failed) {
    if (s == NULL) {
        return NULL;
    }
    int32_t n = LJ(env, LJ_GET_STRING_LENGTH, int32_t (*)(void *, void *))(env, s);
    const uint16_t *u = LJ(env, LJ_GET_STRING_CHARS, const uint16_t *(*)(void *, void *, void *))(env, s, NULL);
    if (u == NULL) {
        *failed = true;
        return NULL;
    }
    if (memchr(u, 0, (size_t)n * sizeof *u) != NULL) {
        for (int32_t i = 0; i < n; i++) {
            if (u[i] == 0) {
                LJ(env, LJ_RELEASE_STRING_CHARS, void (*)(void *, void *, const uint16_t *))(env, s, u);
                lj_throw(env, "java/lang/IllegalArgumentException",
                         "a string holding a NUL character can't reach Lugha");
                *failed = true;
                return NULL;
            }
        }
    }
    char *out = malloc((size_t)n * 3 + 1);
    size_t at = 0;
    for (int32_t i = 0; out != NULL && i < n; i++) {
        uint32_t cp = u[i];
        if (cp >= 0xD800 && cp < 0xDC00 && i + 1 < n && u[i + 1] >= 0xDC00 && u[i + 1] < 0xE000) {
            cp = 0x10000 + ((cp - 0xD800) << 10) + (u[i + 1] - 0xDC00);
            i++;
        } else if (cp >= 0xD800 && cp < 0xE000) {
            cp = 0xFFFD;
        }
        if (cp < 0x80) {
            out[at++] = (char)cp;
        } else if (cp < 0x800) {
            out[at++] = (char)(0xC0 | (cp >> 6));
            out[at++] = (char)(0x80 | (cp & 0x3F));
        } else if (cp < 0x10000) {
            out[at++] = (char)(0xE0 | (cp >> 12));
            out[at++] = (char)(0x80 | ((cp >> 6) & 0x3F));
            out[at++] = (char)(0x80 | (cp & 0x3F));
        } else {
            out[at++] = (char)(0xF0 | (cp >> 18));
            out[at++] = (char)(0x80 | ((cp >> 12) & 0x3F));
            out[at++] = (char)(0x80 | ((cp >> 6) & 0x3F));
            out[at++] = (char)(0x80 | (cp & 0x3F));
        }
    }
    LJ(env, LJ_RELEASE_STRING_CHARS, void (*)(void *, void *, const uint16_t *))(env, s, u);
    if (out == NULL) {
        lj_throw(env, "java/lang/OutOfMemoryError", "a string's copy for Lugha");
        *failed = true;
        return NULL;
    }
    out[at] = '\0';
    return out;
}

/* A Java string of Lugha's UTF-8 `text` (whole characters). */
__attribute__((unused)) static void *lj_string(void *env, const char *text) {
    size_t n = strlen(text);
    uint16_t *u = malloc((n + 1) * sizeof *u);
    if (u == NULL) {
        lj_throw(env, "java/lang/OutOfMemoryError", "a string's copy for Java");
        return NULL;
    }
    size_t at = 0;
    for (size_t i = 0; i < n;) {
        unsigned char c = (unsigned char)text[i];
        uint32_t cp;
        int len = c < 0x80 ? 1 : c < 0xE0 ? 2 : c < 0xF0 ? 3 : 4;
        cp = len == 1 ? c : len == 2 ? (c & 0x1F) : len == 3 ? (c & 0x0F) : (c & 0x07);
        for (int k = 1; k < len && i + (size_t)k < n; k++) {
            cp = (cp << 6) | ((unsigned char)text[i + (size_t)k] & 0x3F);
        }
        i += (size_t)len;
        if (cp >= 0x10000) {
            cp -= 0x10000;
            u[at++] = (uint16_t)(0xD800 + (cp >> 10));
            u[at++] = (uint16_t)(0xDC00 + (cp & 0x3FF));
        } else {
            u[at++] = (uint16_t)cp;
        }
    }
    void *s = LJ(env, LJ_NEW_STRING, void *(*)(void *, const uint16_t *, int32_t))(env, u, (int32_t)at);
    free(u);
    return s;
}
"#;

/// The glue for `class`'s `exports`.
pub(super) fn glue(class: &str, exports: &[Export]) -> String {
    let mut out = format!("/* Written by lughac: the JNI functions of {class}. */\n");
    out.push_str(PRELUDE);
    let panic = format!("{}$Panic", class.replace('.', "/"));
    for e in exports {
        out.push_str(&function(class, &panic, e));
    }
    out
}

/// One export's JNI function.
fn function(class: &str, panic: &str, e: &Export) -> String {
    let c_params: Vec<String> = e.params.iter().map(|p| p.c.to_string()).collect();
    let c_ret = e.ret.as_ref().map_or("void", |r| r.c);
    let mut out = format!(
        "\n{c_ret} {}({});\n",
        e.symbol,
        if c_params.is_empty() {
            "void".to_string()
        } else {
            c_params.join(", ")
        }
    );
    let jni_params: Vec<String> = e
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{} p{i}", p.jni))
        .collect();
    let jni_ret = e.ret.as_ref().map_or("void", |r| r.jni);
    let mut sig = vec!["void *env".to_string(), "void *cls".to_string()];
    sig.extend(jni_params);
    out.push_str(&format!(
        "\n{jni_ret} {}({}) {{\n    (void)cls;\n",
        super::jni::jni_name(class, &e.method),
        sig.join(", ")
    ));
    let zero = match &e.ret {
        None => "",
        Some(r) if r.jni == "void *" => " NULL",
        Some(_) => " 0",
    };
    let strings: Vec<usize> = (0..e.params.len())
        .filter(|&i| e.params[i].string)
        .collect();
    if !strings.is_empty() {
        out.push_str("    bool failed = false;\n");
        for &i in &strings {
            out.push_str(&format!("    char *s{i} = NULL;\n"));
        }
    }
    let free_all = |upto: usize| -> String {
        strings
            .iter()
            .filter(|&&i| i < upto)
            .map(|i| format!("    free(s{i});\n"))
            .collect()
    };
    for &i in &strings {
        let p = &e.params[i];
        out.push_str(&format!("    s{i} = lj_utf8(env, p{i}, &failed);\n"));
        out.push_str(&format!(
            "    if (failed) {{\n{}        return{zero};\n    }}\n",
            free_all(i).replace("    free", "        free")
        ));
        if !p.nullable {
            out.push_str(&format!(
                "    if (p{i} == NULL) {{\n{}        lj_throw(env, \"java/lang/NullPointerException\", \"{} is null\");\n        return{zero};\n    }}\n",
                free_all(i).replace("    free", "        free"),
                p.name
            ));
        }
    }
    let args: Vec<String> = e
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| match p.c {
            _ if p.string => format!("s{i}"),
            "bool" => format!("p{i} != 0"),
            "uint32_t" => format!("(uint32_t)p{i}"),
            "void *" => format!("(void *)(intptr_t)p{i}"),
            _ => format!("p{i}"),
        })
        .collect();
    out.push_str("    lugha_rt_export_catch();\n");
    let call = format!("{}({})", e.symbol, args.join(", "));
    match &e.ret {
        None => out.push_str(&format!("    {call};\n")),
        Some(r) => out.push_str(&format!("    {} r = {call};\n", r.c)),
    }
    out.push_str(&free_all(usize::MAX));
    out.push_str(&format!(
        "    char *panic = lugha_rt_export_caught();\n    if (panic != NULL) {{\n        lj_throw(env, \"{panic}\", panic);\n        free(panic);\n        return{zero};\n    }}\n"
    ));
    match &e.ret {
        None => {}
        Some(r) if r.string => {
            out.push_str("    void *j = lj_string(env, r);\n    free(r);\n    return j;\n")
        }
        Some(r) => out.push_str(&match r.c {
            "bool" => "    return r ? 1 : 0;\n".to_string(),
            "uint32_t" => "    return (int32_t)r;\n".to_string(),
            "void *" => "    return (int64_t)(intptr_t)r;\n".to_string(),
            _ => "    return r;\n".to_string(),
        }),
    }
    out.push_str("}\n");
    out
}
