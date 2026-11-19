## FEATURE: Every extern callback says how C's call runs it: `cb: direct fun(…)` runs during C's call, `cb: spawned fun(…)` starts a task in the registering scope; no default, nothing implied by a callback's shape; milestone 216.

**Status:** approved 2026-11-20.

## OBJECTIVE
A reader knows what each callback does from its declaration alone:

```
extern fun sort(xs: i64[], cmp: direct fun(i64, i64): i32);           // during C's call
extern fun each_word(f: direct fun(string, data), u: data);            // during C's call
extern fun on_click(w: Window, f: spawned fun(Window, data), u: data); // a task per call
```

```
$ lughac check old.la
error[E0435]: say how C's call runs this callback: `direct fun(…)` (during the call) or `spawned fun(…)` (as a task)
  --> old.la:1:27
  |
1 | extern fun on_tick(cb: fun(i64, data), user: data);
  |                        ^^^^^^^^^^^^^^
```

## CONTEXT
- **Why:** milestone 215 decided "queued or inline" from a callback's shape (`data`, a result), invisible where it is declared, and accepted `inline` where it changed nothing. The user's rule (2026-11-20): several ways are fine when each earns its place, but what happens must be visible in the syntax. They chose the words `direct` and `spawned` (`inline` reads as compiler inlining in C, C++ and Kotlin; `spawned` reuses Lugha's word for starting a task), written before `fun`, as `blocking` is before `fun` in `extern blocking fun`.
- **Ledger:** CBK-02, CBK-03, CBK-04 revised (approved 2026-11-20); SUP-27 records milestone 215's default as superseded.
- **Spec:** §8 (callbacks: queued and inline, the scope's end), §7 (closures handed to C), §9, the error table; the Draft §4.8.
- **Code:**
  - Parser and AST: `src/parser/out_params.rs`, `Param.inline`.
  - Checker: `src/check/callbacks.rs` (`CParam::Callback`'s `queued`, E0435, E0481, the `Sendable` check).
  - Codegen: `src/codegen/callbacks.rs` reads `queued` and changes only its name.
  - Runtime: the "can't wait" panic in `runtime/waiting.c`.
  - Formatter, s-expressions, the language server's tokens, the VS Code grammar.
  - Tests: `tests/callbacks.rs`, `scope_callbacks.rs`, `c_strings.rs`, `c_memory.rs`, `c_arrays.rs`, `extern_structs.rs`, `arm.rs`, `blocking_externs.rs`, `common/c_shapes.rs`, `tests/programs/c/`.
  - Error pages: E0435, E0436, E0479, E0481.
- **Decisions:** none open.
- **Security:** nothing on CLAUDE.md's list.

## DESIGN
1. **Syntax:** an extern's callback parameter is `name: direct fun(…)` or `name: spawned fun(…)`. `direct` and `spawned` are words only there (names everywhere else, as `blocking` is), so `let f: spawned fun()` fails to parse as any unknown word before a type does.
2. **`direct`:** what milestone 215 called inline. It runs during C's call, in the calling task, can't wait, cancellation waits for it, and a panic gives C a zero value and fails the registering scope. It is allowed on every callback.
3. **`spawned`:** what milestone 215 called queued, with the same rules as before:
   - It needs `data` and no result, since C can't wait for a value from a task: E0435 otherwise, saying why.
   - Its captures must be `Sendable` (E0475).
   - It can't take `Ref<T[]>` (E0481).
4. **Neither written:** E0435, naming both words. `inline` (milestone 215's mark) is gone; `fun(…) inline` is a parse error pointing at the new spelling.
5. **Messages:** E0475's and E0481's help name `direct`/`spawned`; the runtime panic for waiting reads `a direct C callback can't wait: C is waiting for it; declare it `spawned fun(…)` (with `data`, no result) so it runs as a task`.
6. **No behaviour changes** beyond the spelling: the same entries, runtime calls and tests' outcomes.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **E0435 for a missing or misplaced word,** as the existing "malformed callback declaration" code, rather than a new one: the fix is in the declaration, as for `data` and `release`.
2. **Words, not keywords:** `direct` and `spawned` stay usable as names, as `blocking`, `pinned`, `data` and `release` are.
3. **`fun(…) inline` gets a pointed parse error** for one release's worth of help moving code, not silently accepted.
4. **`direct` with a result or without `data` is fine** (it's the only thing those can be), and must still be written, as the rule is "every callback says".

## REQUIREMENTS

### Must Do
- Design items 1–6; the error pages E0435, E0436, E0475 (help), E0479 and E0481 updated to the new spelling.
- **Breaking:** every extern callback declaration gains `direct` or `spawned`; milestone 215's `inline` is removed. CHANGELOG under Breaking, with the mapping (unmarked `data` + no result → `spawned`; `inline` or anything else → `direct`).
- Spec §7, §8, §9, the error table, "Changes since v16", §11 row 216; the Draft §4.8 (rules 2–4 as the ledger now says); TODO; the language server and grammar (`direct`, `spawned` before `fun` in extern parameters; `inline` gone), extension version bump.

### Must NOT Do
- Callbacks on C's own threads, `Ring<T>`, the allocation lint: the next PRP.
- Change what any callback does at run time.

## TESTS
- [x] `tests/programs/c/callbacks/`:
  - [x] E0435 for a callback with neither word, and for `spawned` with a result or without `data`;
  - [x] the parse error for `fun(…) inline`;
  - [x] `direct`/`spawned` parse and format; `direct` and `spawned` still work as names.
- [x] Every existing callback test passes with the new spelling and unchanged output: `tests/callbacks.rs`, `scope_callbacks.rs`, `c_strings.rs`, `c_memory.rs`, `c_arrays.rs`, `extern_structs.rs`, `arm.rs`, `blocking_externs.rs`.
- [x] `--lib` (parser, checker and token unit tests), `programs`, `explain`, `spec`, `fmt`, `parser`, `lsp_semantic_tokens`, `comments`; `editors/vscode` `npm test`.

## ACCEPTANCE
- [x] No extern callback compiles without `direct` or `spawned`; each behaves as milestone 215's inline or queued did.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [x] Spec, the Draft, CHANGELOG (Breaking), TODO, the error pages and the editor tooling are updated.

## BUILD NOTES
- **Migration by script:** every extern callback in `tests/`, `src/` (unit tests), the error pages and the spec was rewritten under milestone 215's rules: unmarked with `data` and no result became `spawned`, everything else `direct`; `fun(…) inline` became `direct fun(…)`. Two patterns needed a second pass: renamed externs inside Rust strings (escaped quotes) and `inline` after a result type.
- **Names follow the words:** `CParam::Callback`'s `queued` is now `spawned`, `codegen/queued_callbacks.rs` is `codegen/spawned_callbacks.rs`, the E0481 builder `spawned_ref`; the program cases became `direct_spawned.la`, `e0435_unsaid.la`, `e0201_inline.la`, `e0475_spawned.la`.
- **No benchmarks:** nothing changed at run time but the "can't wait" panic's wording.
- **The VS Code extern snippet** offers `spawned` or `direct` as a choice.
