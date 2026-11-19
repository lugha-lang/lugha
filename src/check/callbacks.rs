//! Callbacks into Lugha: a function type in an
//! extern's signature is a C function pointer; `data` marks where C hands
//! user data back, and `release` the function C calls when it drops the
//! callback. Callers write neither: lughac passes the pinned closure and
//! `lugha_rt_cb_release`.

use std::collections::HashMap;

use super::{Checker, Type, errors};
use crate::ast::{
    CallbackRun, Expr, ExprKind, ExternDecl, Param, PassMode, Type as Annotation, TypeKind,
};
use crate::span::Span;

/// How one parameter of an extern reaches C, in declaration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CParam {
    /// An ordinary value: the next argument of the call.
    Value,
    /// A C function pointer: the next argument, a function value of type
    /// `fun(params): ret`. `data` is where C passes the user data back
    /// among the C function's parameters, if it does. `spawned`: C's call
    /// starts a task in the registering scope (a callback with `data`, no
    /// result, written `spawned fun(…)`); else it runs during C's call.
    Callback {
        params: Vec<Type>,
        ret: Type,
        data: Option<usize>,
        spawned: bool,
        /// Written `realtime fun(…)`: checked never to allocate or wait
        /// (`check/realtime.rs`); otherwise run as `direct`.
        realtime: bool,
    },
    /// The user data C hands back: lughac passes the pinned closure.
    Data,
    /// The function C calls when it drops the callback: `lugha_rt_cb_release`.
    Release,
    /// `out x`, `ptr e` or `mut x`: a pointer to
    /// a slot C reads or writes.
    Pointer(PassMode),
    /// `xs: T[]` or `mut xs: T[]`: a pointer to the
    /// array's elements for the call, which C may write when `mut`.
    Elements { mutable: bool },
}

/// Every extern's plan whose parameters aren't all plain values, by key.
pub type CParams = HashMap<String, Vec<CParam>>;

/// `data` or `release` written as a bare type name.
fn marker(ty: &Annotation) -> Option<&'static str> {
    match &ty.kind {
        TypeKind::Named(name) if name == "data" => Some("data"),
        TypeKind::Named(name) if name == "release" => Some("release"),
        _ => None,
    }
}

/// A parameter's whole span, name to type.
fn param_span(p: &Param) -> Span {
    Span::new(p.name.span.start, p.ty.span.end)
}

/// True for the types a callback may give back to C: extern structs too.
fn crosses_into_callback(ty: &Type, c_structs: &std::collections::HashSet<String>) -> bool {
    ty.is_numeric()
        || matches!(ty, Type::Bool | Type::Handle(_) | Type::Error)
        || matches!(ty, Type::Option(inner) if matches!(**inner, Type::Handle(_)))
        || matches!(ty, Type::Struct(key, _) if c_structs.contains(key))
}

/// True for the types a callback may take from C: those, and C's text,
/// copied. A string never goes back: C would keep a
/// pointer into Lugha's memory.
fn crosses_from_c(ty: &Type, c_structs: &std::collections::HashSet<String>) -> bool {
    crosses_into_callback(ty, c_structs)
        || matches!(ty, Type::String)
        || matches!(ty, Type::Option(inner) if **inner == Type::String)
}

impl Checker {
    /// The parameters callers write and their types, for extern `e`; its
    /// C plan goes to `c_params` under `key` when it has callbacks (E0409, E0435).
    pub(super) fn extern_params(&mut self, e: &ExternDecl, key: &str) -> (Vec<Param>, Vec<Type>) {
        let (mut written, mut types, mut plan) = (Vec::new(), Vec::new(), Vec::new());
        let (mut data, mut release, mut with_data) = (None::<Span>, None::<Span>, Vec::new());
        for p in &e.params {
            // A helper thread can't run the Lugha code a callback is.
            if e.blocking.is_some()
                && (marker(&p.ty).is_some() || matches!(p.ty.kind, TypeKind::Fun(..)))
            {
                self.report(errors::blocking_callback(param_span(p)));
            }
            match marker(&p.ty) {
                Some("data") => {
                    if data.is_some() {
                        self.report(errors::callback_decl(
                            "an extern takes one `data` parameter",
                            param_span(p),
                        ));
                    }
                    data = data.or(Some(param_span(p)));
                    plan.push(CParam::Data);
                    continue;
                }
                Some(_) => {
                    if release.is_some() {
                        self.report(errors::callback_decl(
                            "an extern takes one `release` parameter",
                            param_span(p),
                        ));
                    }
                    release = release.or(Some(param_span(p)));
                    plan.push(CParam::Release);
                    continue;
                }
                None => {}
            }
            let ty = match &p.ty.kind {
                TypeKind::Fun(params, ret) => {
                    let run = p.run.map(|(r, _)| r);
                    if run.is_none() {
                        self.report(errors::callback_decl(
                            "say how C's call runs this callback: `direct fun(…)` (during the call), `spawned fun(…)` (as a task) or `realtime fun(…)` (during the call, checked never to allocate or wait)",
                            p.ty.span,
                        ));
                    }
                    let callback = self.callback(&p.ty, params, ret.as_deref(), run);
                    if let CParam::Callback { data: Some(_), .. } = callback {
                        with_data.push(p.ty.span);
                    }
                    let CParam::Callback { params, ret, .. } = &callback else {
                        unreachable!("`callback` makes a callback")
                    };
                    let ty = Type::Fun(params.clone(), Box::new(ret.clone()));
                    self.annotations.insert(p.ty.span, ty.clone());
                    plan.push(callback);
                    ty
                }
                _ => {
                    let ty = self.resolve(&p.ty);
                    let mode = p.mode.map(|(m, _)| m);
                    plan.push(match (&ty, mode) {
                        (Type::Array(_), m) => CParam::Elements {
                            mutable: m == Some(PassMode::Mut),
                        },
                        (_, Some(m)) => CParam::Pointer(m),
                        (_, None) => CParam::Value,
                    });
                    ty
                }
            };
            written.push(p.clone());
            types.push(ty);
        }
        match (data, with_data.as_slice()) {
            (Some(at), []) => self.report(errors::callback_decl(
                "no callback takes this `data` back: write `data` among a callback's parameters",
                at,
            )),
            (None, [first, ..]) => self.report(errors::callback_decl(
                "this callback takes `data`, but the extern has no `data` parameter to pass it",
                *first,
            )),
            (_, [_, second, ..]) => self.report(errors::callback_decl(
                "only one callback can take the extern's `data`",
                *second,
            )),
            _ => {}
        }
        if let (Some(at), None) = (release, data) {
            self.report(errors::callback_decl(
                "`release` frees a callback's `data`, and this extern has none",
                at,
            ));
        }
        if plan.iter().any(|c| *c != CParam::Value) {
            self.c_params.insert(key.to_string(), plan);
        }
        (written, types)
    }

    /// One callback type `fun(params): ret` written in an extern.
    fn callback(
        &mut self,
        whole: &Annotation,
        params: &[Annotation],
        ret: Option<&Annotation>,
        run: Option<CallbackRun>,
    ) -> CParam {
        let spawned = run == Some(CallbackRun::Spawned);
        let realtime = run == Some(CallbackRun::Realtime);
        let (mut types, mut data) = (Vec::new(), None);
        for (i, p) in params.iter().enumerate() {
            match marker(p) {
                Some("data") if data.is_none() => data = Some(i),
                Some(_) => {
                    let message = "a callback takes `data` once, and never `release`";
                    self.report(errors::callback_decl(message, whole.span));
                }
                None => {
                    let ty = self.resolve(p);
                    // A view or `T[]`: C's pointer and count. A view's
                    // elements were checked when it was resolved.
                    let counted = match &ty {
                        Type::View(..) => true,
                        Type::Array(e) => crosses_into_callback(e, &self.c_structs),
                        _ => false,
                    };
                    // An array of what can't cross is E0409, not a view.
                    let buffer = match &ty {
                        Type::Array(_) if !counted => None,
                        _ => errors::callback_buffer(&ty, spawned, p.span),
                    };
                    if let Some(d) = buffer {
                        self.report(d);
                    } else if !counted && !crosses_from_c(&ty, &self.c_structs) {
                        self.report(errors::callback_type(p.span));
                    }
                    types.push(ty);
                }
            }
        }
        let ret = match ret {
            None => Type::Unit,
            Some(r) if marker(r).is_some() => {
                self.report(errors::callback_decl(
                    "a callback can't return `data` or `release`",
                    r.span,
                ));
                Type::Error
            }
            Some(r) => {
                let ty = self.resolve(r);
                if !crosses_into_callback(&ty, &self.c_structs) {
                    self.report(errors::callback_type(r.span));
                }
                ty
            }
        };
        // A task can't hand C a value, nor be found without its user data.
        if spawned && (data.is_none() || ret != Type::Unit) {
            let message = match data {
                None => {
                    "a `spawned` callback needs `data`: its scope is found through the user data C hands back"
                }
                Some(_) => {
                    "a `spawned` callback can't return a value: C would wait for a task; make it `direct`"
                }
            };
            self.report(errors::callback_decl(message, whole.span));
        }
        // C's call copies a string into a new Lugha value.
        if realtime {
            for (p, ty) in params.iter().filter(|p| marker(p).is_none()).zip(&types) {
                let copied = match ty {
                    Type::String => true,
                    Type::Option(inner) => **inner == Type::String,
                    _ => false,
                };
                if copied {
                    self.report(errors::realtime_param(ty, p.span));
                }
            }
        }
        let runs_as_task = spawned && data.is_some() && ret == Type::Unit;
        CParam::Callback {
            params: types,
            ret,
            data,
            spawned: runs_as_task,
            realtime,
        }
    }

    /// E0436 for each argument of a call to extern `key` going to a
    /// callback without `data` that isn't a function's name or a closure
    /// capturing nothing: C keeps only a code pointer. E0475 for one going
    /// to a callback with `data` that isn't sendable: a `spawned` one runs
    /// as another task, a `direct` one may run on a thread C made.
    pub(super) fn static_callbacks(&mut self, key: &str, args: &[Expr]) {
        let Some(plan) = self.c_params.get(key) else {
            return;
        };
        let written: Vec<&CParam> = plan
            .iter()
            .filter(|c| matches!(c, CParam::Value | CParam::Callback { .. }))
            .collect();
        // For each argument: a callback C may call from elsewhere, and
        // whether it is `spawned`.
        let shared: Vec<Option<bool>> = written
            .iter()
            .map(|c| match c {
                CParam::Callback {
                    data: Some(_),
                    spawned,
                    ..
                } => Some(*spawned),
                _ => None,
            })
            .collect();
        let bare: Vec<bool> = written
            .iter()
            .map(|c| matches!(c, CParam::Callback { data: None, .. }))
            .collect();
        for (arg, shared) in args.iter().zip(shared) {
            let ty = self.types[arg.id.0 as usize].clone();
            let (Some(spawned), Some(ty)) = (shared, ty) else {
                continue;
            };
            let how = match spawned {
                true => "a `spawned` callback, run as a task,",
                false => "a `direct` callback, which may run on a thread C made,",
            };
            // A named function always crosses; a closure when each of its
            // captures does; any other function value never.
            match self.fun_origins.get(&arg.id).cloned() {
                Some(super::FunOrigin::Named(_)) => {}
                Some(super::FunOrigin::Closure(_, _, captured)) => {
                    for (name, t) in captured {
                        let head = format!("{how} can't capture `{name}`");
                        if self.must_send(&t, &head, arg.span) {
                            break;
                        }
                    }
                }
                None => {
                    let head =
                        format!("{how} can't be a function value whose captures are unknown");
                    self.must_send(&ty, &head, arg.span);
                }
            }
        }
        for (arg, bare) in args.iter().zip(bare) {
            let fixed = match &arg.kind {
                ExprKind::Closure(_) => self.captures.get(&arg.id).is_none_or(Vec::is_empty),
                // A function's name (or `m.f`) is what the checker records as a callee.
                _ => self.callees.contains_key(&arg.id),
            };
            if bare && !fixed {
                self.report(errors::callback_captures(arg.span));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_util::{errors, ok};

    const CONNECT: &str = "extern type W;\n\
        extern fun connect(w: W, f: direct fun(W, data), user: data, destroy: release, flags: i32): u64;\n\
        extern fun sort(n: i64, cmp: direct fun(i64, i64): i32);\n\
        extern fun make(): W;\n";

    fn with(body: &str) -> String {
        format!("{CONNECT}{body}")
    }

    #[test]
    fn callers_write_neither_data_nor_release() {
        ok(&with(
            "fun by(a: i64, b: i64): i32 = if a < b { -1 } else { 1 };\n\
             fun main(console: Console) {\n\
                 let (count, _counted) = Chan<i64>.unbounded();\n\
                 connect(make(), fun(w: W) { count.send(1); }, 0);\n\
                 connect(make(), fun(w) { console.println(w.debug()); }, 1);\n\
                 sort(3, by);\n\
                 sort(3, fun(a, b) = 0);\n\
             }",
        ));
    }

    #[test]
    fn a_spawned_callbacks_captures_must_cross_to_its_task() {
        let externs = "extern fun on(f: spawned fun(i64, data), user: data);\n";
        let code = |src: &str| errors(src).first().map(|e| e.0);
        ok(&format!(
            "{externs}fun main(console: Console) {{ let n = 2; on(fun(x: i64) {{ console.println(x + n); }}); }}"
        ));
        let shared = format!(
            "{externs}fun main() {{ let r = Ref.new(0); on(fun(x: i64) {{ r.set(x); }}); }}"
        );
        assert_eq!(code(&shared), Some("E0475"));
        let value = format!("{externs}fun g(f: fun(i64)) {{ on(f); }}\nfun main() {{}}");
        assert_eq!(code(&value), Some("E0475"));
        let array = "extern fun each(f: spawned fun(Ref<i64[]>, data), user: data);\nfun main() {}";
        assert_eq!(code(array), Some("E0484"));
    }

    #[test]
    fn named_functions_and_sendable_closures_go_where_c_hands_data_back() {
        ok(&with(
            "fun on(w: W) {}\n\
             fun main() { connect(make(), on, 0); connect(make(), fun(w: W) { on(w); }, 0); }",
        ));
        // A function value's captures are unknown: it may run on C's thread.
        let picked = with(
            "fun on(w: W) {}\n\
             fun pick(): fun(W) = on;\n\
             fun main() { let f = pick(); connect(make(), f, 0); }",
        );
        let found = errors(&picked);
        assert_eq!(found[0].0, "E0475", "{found:?}");
    }

    #[test]
    fn arguments_have_the_callback_type_without_data() {
        assert_eq!(
            errors(&with(
                "fun main() { connect(make(), fun(w: W, x: i64) {}, 0); }"
            )),
            [("E0403", "fun(w: W, x: i64) {}")]
        );
        assert_eq!(
            errors(&with("fun main() { connect(make(), fun(w: W) {}, 0, 0); }")),
            [("E0405", "connect(make(), fun(w: W) {}, 0, 0)")]
        );
    }

    #[test]
    fn without_data_only_capture_free_functions_go_to_c() {
        let src = with(
            "fun main() {\n\
                 let k = 2;\n\
                 sort(3, fun(a, b) = if a * k < b { -1 } else { 1 });\n\
                 let f = fun(a: i64, b: i64): i32 = 0;\n\
                 sort(3, f);\n\
             }",
        );
        assert_eq!(
            errors(&src),
            [
                ("E0436", "fun(a, b) = if a * k < b { -1 } else { 1 }"),
                ("E0436", "f")
            ]
        );
    }

    #[test]
    fn callback_declarations_are_checked() {
        for (decl, code, at) in [
            (
                "extern fun f(cb: spawned fun(data, data), u: data);",
                "E0435",
                "fun(data, data)",
            ),
            (
                "extern fun f(cb: direct fun(i64), u: data);",
                "E0435",
                "u: data",
            ),
            ("extern fun f(cb: spawned fun(data));", "E0435", "fun(data)"),
            (
                "extern fun f(cb: spawned fun(data), u: data, v: data);",
                "E0435",
                "v: data",
            ),
            (
                "extern fun f(cb: direct fun(i64), r: release);",
                "E0435",
                "r: release",
            ),
            (
                "extern fun f(a: spawned fun(data), b: spawned fun(data), u: data);",
                "E0435",
                "fun(data)",
            ),
            (
                "extern fun f(cb: direct fun(string[]));",
                "E0409",
                "string[]",
            ),
            (
                "extern fun f(cb: direct fun(fun(i64)));",
                "E0409",
                "fun(i64)",
            ),
            (
                "extern fun f(cb: direct fun(i64): string);",
                "E0409",
                "string",
            ),
            ("extern fun f(): fun(i64);", "E0409", "fun(i64)"),
            (
                "extern fun f(cb: direct fun(i64): data, u: data);",
                "E0435",
                "data",
            ),
        ] {
            let src = format!("{decl}\nfun main() {{}}");
            let found = errors(&src);
            assert_eq!(
                found.first().map(|(c, _)| *c),
                Some(code),
                "{decl}: {found:?}"
            );
            assert_eq!(found[0].1, at, "{decl}: {found:?}");
        }
    }

    #[test]
    fn strings_come_from_c_but_never_go_back() {
        ok(
            "extern fun lines(f: spawned fun(string, Option<string>, data), u: data);\n\
            extern fun title(): string;\nextern fun find(k: string): Option<string>;\n\
            extern fun label(s: Option<string>);\n\
            fun main(console: Console) { lines(fun(a, b) { console.println(a); }); console.println(title()); label(find(\"k\")); }",
        );
        assert_eq!(
            errors("extern fun f(cb: direct fun(i64): string);\nfun main() {}"),
            [("E0409", "string")]
        );
        assert_eq!(
            errors("extern fun f(cb: direct fun(i64): Option<string>);\nfun main() {}"),
            [("E0409", "Option<string>")]
        );
    }

    #[test]
    fn data_and_release_mean_nothing_outside_externs() {
        assert_eq!(
            errors("fun f(x: data) {}\nfun main() {}"),
            [("E0435", "data")]
        );
        assert_eq!(
            errors("fun main() { let r: release = 1; }"),
            [("E0435", "release")]
        );
    }

    #[test]
    fn renamed_externs_share_a_c_function() {
        ok("extern type A;\nextern type B;\n\
            extern \"g_connect\" fun on_a(x: A, f: spawned fun(A, data), u: data);\n\
            extern \"g_connect\" fun on_b(x: B, f: spawned fun(B, i64, data), u: data);\n\
            extern \"puts\" fun say(s: string): i32;\n\
            fun main() { say(\"hi\"); }");
        assert_eq!(
            errors("extern \"lugha_rt_alloc\" fun grab(n: i64): i64;\nfun main() {}"),
            [("E0306", "\"lugha_rt_alloc\"")]
        );
        assert_eq!(
            errors("extern \"not a name\" fun f();\nfun main() {}"),
            [("E0306", "\"not a name\"")]
        );
        // Plain declarations of one C function still agree (unchanged).
        assert_eq!(
            errors("extern fun abs(x: i32): i32;\nextern fun abs(x: i64): i64;\nfun main() {}"),
            [("E0302", "abs")]
        );
    }
}
