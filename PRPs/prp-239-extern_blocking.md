## FEATURE: `extern blocking fun`: a C function declared `blocking` runs on a helper thread while its task waits, so a slow C call (a database query, a compression library, a C client's network call) no longer stops every task; milestone 207.

**Status:** proposed 2026-11-18.

## OBJECTIVE
```
extern blocking fun slow_query(id: i64): i64;   // C: waits a second on a database

import std.task;

fun main(console: Console) {
    let r = scope s {
        s.spawn(fun() { console.println("row {slow_query(7)}"); }());
        for i in 0..3 {
            task.sleep(100);
            console.println("tick {i}");
        }
    };
    r.unwrap();
}
```
prints `tick 0`, `tick 1`, `tick 2`, then `row …`. Without `blocking`, the C call holds the program's only thread for the whole second and the ticks come after it.

## CONTEXT
- **Draft v18 §4.6** rule 7: blocking work, `extern blocking` calls included, runs on the bounded helper pool while its task waits [SCH-07]; rule 8: other `extern` calls run inline [SCH-08]; §4.7 rule 1: helper threads have their own C stacks [STK-01]; §1 rule 6: `blocking` is a contextual word [OPN-08]; §5's list of new syntax: `extern blocking fun`. §4.8's callbacks on other threads (`Ring<T>`, `queued`) are PRP 6 of the phase.
- **Built on** PRP-238 (milestone 206): `runtime/helpers.c` and `jobs.c`, the pool, `LUGHA_HELPERS`.
- **Spec today:** §8 "Extern functions" and "Callbacks"; §6 "Blocking calls" (extern calls run on the program's thread).
- **Code:** `src/parser/item.rs` (`extern_item`), `src/ast/mod.rs` (`ExternDecl`), `src/formatter/items.rs`, `src/parser/sexp/item.rs`, `src/check/callbacks.rs` (`extern_params`), `src/check/env.rs` (E0306), `src/codegen/declare.rs`, `src/codegen/c_stack.rs` (the C-stack pass), `src/codegen/callbacks.rs` (`c_body`, every C entry into Lugha), `runtime/stacks.c`, `runtime/jobs.c`.
- **Security:** no `unsafe`, nothing on CLAUDE.md's list (C code runs as it does now, on another thread); the runtime guard below closes the one new way C could misuse a thread.
- **Decisions:** none open.

## DESIGN
1. **Syntax:** `blocking` right after `extern`: `extern blocking fun f(…): R;` and `extern blocking "c_name" fun f(…);`. `blocking` means this only there; it stays an ordinary name everywhere else. The formatter keeps it; the S-expression printer shows `(extern blocking f …)`.
2. **A call runs on a helper thread:** the C-stack pass (`codegen::c_stack`) already turns every C call into a frame of its arguments and a per-site thunk. For a function marked blocking (an LLVM attribute, `lugha-blocking`, on its declaration), the call goes through `lugha_rt_c_blocking(thunk, frame)` instead of `lugha_rt_c_call`: the runtime gives the thunk and frame to a helper as a job, which runs the C function on the helper's own stack, and parks the task until it's done. The frame stays on the task's stack, which can't go away while the task waits (item 3).
3. **The wait can't be cut short:** C may be using pointers into the task's values (strings, arrays, `out` parameters), so the task waits for the call to return even when cancelled; the cancellation is noticed at its next pause, as for a plain extern call. A deadline therefore can't interrupt a blocking C call; the C function's own timeout must.
4. **Inline where a helper can't be used:** in a deterministic run, before lockdown and inside a C callback, a blocking extern runs like a plain one, on the C stack (`lugha_rt_c_call`).
5. **No callbacks on a helper thread:** Lugha code can't run there yet (the allocator and counts aren't thread-safe). A blocking extern taking a callback (a `fun(…)` parameter, `data` or `release`) is **E0479**: ``a `blocking` extern can't take a callback: it runs on a helper thread``. A callback C stored earlier and calls during a blocking call is caught at run time: every C entry into Lugha (`c_body`) first calls `lugha_rt_cb_enter()`, which on a helper thread ends the program with ``panic: a C callback ran on a helper thread, during a `blocking` extern call``. A thread-local flag makes the check one load.
6. **Agreement:** one C function declared `blocking` in one place and not in another is E0306 (one C function declared with two signatures), as two different parameter lists are.
7. **Each call may run on a different helper thread:** C state tied to a thread (thread-locals, `errno` between calls, a library requiring one thread) isn't kept from one call to the next. `extern type X pinned` (PRP 6) is for that.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **`blocking` before the symbol** (`extern blocking "c_name" fun`): the word qualifies the declaration as `extern` does, and reads as the Draft writes it.
2. **Uncancellable waits** (design item 3) over copying arguments: arguments may be pointers C reads and writes, so a copy can't be made generally; Lugha already holds that C keeping a Lugha pointer past the call is undefined.
3. **E0479 and the runtime guard** rather than allowing callbacks: PRP 6 brings callbacks on other threads with their full rules; until then, refusing is the safe default.
4. **Agreement through E0306** rather than per-call-site marking: one C function has one declaration in the program's IR, so its calls agree.
5. **No VS Code grammar change:** contextual words (`independent`, `within`) aren't highlighted specially today; `blocking` follows them. The language server's semantic tokens mark it as a keyword where it is one, as they do `extern`'s `type`.

## REQUIREMENTS

### Must Do
- The parser, AST, formatter and S-expression printer; E0479 (with its `docs/errors/` page) and E0306's extension; the `lugha-blocking` attribute and the C-stack pass's routing; `lugha_rt_c_blocking` and `lugha_rt_cb_enter` (`RUNTIME_SYMBOLS`); the language server's keyword token.
- Spec §8 (externs, callbacks), §6 (blocking calls), §9 (the runtime table), "Changes since v16", §11 row 207; CHANGELOG.

### Must NOT Do
- Callbacks on helper or foreign threads, `Ring<T>`, `pinned` (PRP 6); cutting a blocking call short; several workers (PRP 5).

## TESTS
- [ ] `tests/parser.rs`: `extern blocking fun` with and without a symbol; `blocking` still a name elsewhere.
- [ ] `tests/fmt.rs`: the formatter keeps `blocking`.
- [ ] `tests/programs/` (rejected programs): E0479 for a callback, `data` and `release` parameter; E0306 for a C function declared blocking and not.
- [ ] `tests/callbacks.rs` (a C file of its own): ticks print while a blocking C function sleeps; a blocking C function writing an `out` parameter and reading a string; a deadline doesn't cut a blocking call short (the scope fails once it returns); a stored callback called during a blocking call ends the program with the message; a deterministic run keeps the call inline.
- [ ] `tests/lsp_semantic_tokens.rs`: `blocking` is a keyword after `extern`.
- [ ] Targeted: `--lib`, `parser`, `fmt`, `programs`, `callbacks`, `helpers`, `c_arrays`, `c_strings`, `extern_structs`, `out_params`, `explain`, `spec`, `lsp_semantic_tokens`, `arm`.

## ACCEPTANCE
- [ ] A blocking extern doesn't stop other tasks; rejected and guarded where Lugha code would run on a helper.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [ ] Spec, CHANGELOG and the error page updated.
