## FEATURE: What's left from lugha-dev's review at 0.18.1: a closure given to a `Result`'s `and_then` takes its error type from what's expected, E0424's and E0429's explanations match v18, `lughac test` names files by their path as `build` does, and the spec says where a function value prints its signature.

**Status:** approved 2026-11-22.

## OBJECTIVE
```lugha
fun main(console: Console) {
    let good: Result<i64, string> = Ok(1);
    console.println(good.and_then(fun(n) = Ok(n + 1)).debug());
}
```
prints `Ok(2)`. Today, at 0.18.1, it gives:

```
error[E0412]: cannot infer the type argument `E`
error[E0403]: expected fun(i64): Result<U, string>, found fun(i64): {error}
```

The same shape on an `Option` already works: `.and_then(fun(n) = if n > 0 { Some(n) } else { None })`.

Also:
- `lughac explain E0424` no longer says "`main` returns nothing".
- `lughac explain E0429` writes its pattern `let .Some(n) = o else` and no longer names `exit`.
- `lughac test --diagnostics=json` names `src/main.la` where it says `main.la` today.

## CONTEXT
- **The user's request** (2026-11-22): lugha-dev rechecked its 25-point list against 0.18.1, and 18 are fixed. This PRP takes the rest that belong to `lughac`; the last two are decisions below.
- **Spec:**
  - §4 closures (milestone 118, L31): a closure whose expected result is partly known takes its result from its body.
  - §4 function values, L823: "Own types print as their signature and the function: `fun(i64): i64 {closure 1 in main}`, `fun(i64): i64 {double}`".
  - §10 `lughac test` and the JSON diagnostics (milestone 244): a diagnostic's `file` is its file's path, as for `build`.
- **Code:**
  - `src/check/closures.rs` L48–53: with an expected result that isn't fully known, `ret` is `None`, so the body is checked with no expectation, and `Ok(n + 1)` can't learn `E`.
  - `docs/errors/E0424.md` L5–7 and `docs/errors/E0429.md` L3 and L7.
  - `src/driver/test.rs` and `src/driver/json.rs`: a diagnostic's `file` is `source.file_at(span)`'s name, which `test` gives as the bare file name.
  - `src/codegen/printing.rs` L79: a function value, or an own function type, prints `<fun>`.
- **Security:** none of CLAUDE.md's list.
- **Decisions:** none open.

## DESIGN
1. **A partly known closure result guides the body.**
   - When the expected result has unknowns, as in `Result<U, string>` with `U` to infer, the body is checked against it with those unknowns left open. `Ok(n + 1)` then takes `E = string` from it, and `U` is inferred from the body as now.
   - A fully unknown expectation behaves as today.
   - No new diagnostic.
2. **E0424's explanation:** "`main` returns nothing or an `i32` exit code, never an `Option` or a `Result`, so `?` can't be used there". The rest stays.
3. **E0429's explanation:** its first line writes `let .Some(n) = o else { … };`. The list of ways out is `return`, `break`, `continue` or a call that never returns, such as `panic`, with no `exit`.
4. **Every diagnostic names its file by its path from where `lughac` ran,** in `build`, `run`, `check`, `fmt` and `test` alike: `src/main.la`, `src/shapes.la`, `test/shapes_test.la`. Today only `build`'s main file keeps its path; every other module is named from its own folder (`shapes.la`), and in `lughac test` every file is (`main.la`). A file outside that folder (a git package's) keeps its full path. The human text's `-->` line follows, since it's rendered from the same name. (Scope grown mid-way, user's choice, 2026-11-22: the PRP first said "as `build` does", which isn't one rule.)
5. **Spec, L823:** says the signature form is how an own type prints in a diagnostic, and that `.debug()` on any function value prints `<fun>`, as `printing.rs` does.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The spec changes for function values, not the compiler.** Every function value's `.debug()` prints `<fun>` today, bound or not; printing a signature for own types only would make two function values print differently by how they were passed. The signature form stays where it helps, in diagnostics. (lugha-dev, 8.3 Generic functions, saw `<fun>` and didn't teach the signature form.)
2. **The output cap for `std.process` isn't in this PRP.** PRP-278 left captured output unlimited on purpose; a cap is a new API (a limit argument, or a `run_limited`) and needs its own discovery. lugha-dev's runner cuts output itself meanwhile.
3. **E0424 and E0429 are explanation fixes only:** their messages and help already read right at 0.18.1.
4. **No new error codes.**

## REQUIREMENTS
### Must Do
- The four changes in DESIGN 1–4, and the spec line in DESIGN 5.
- CHANGELOG under [Unreleased], and the spec's §11 row.

### Must NOT Do
- No change to what `.debug()` prints.
- No change to `lughac test`'s results JSON (`--json` on stdout): only the diagnostics' `file`.

## TESTS
- [ ] Checker: `good.and_then(fun(n) = Ok(n + 1))` on a `Result<i64, string>` types as `Result<i64, string>`, runs and prints `Ok(2)`; the `Option` case still works; a closure with no expectation is unchanged.
- [ ] `lughac explain E0424` and `E0429`: the pages' examples still compile as each page says (the existing explain tests), with the new wording.
- [ ] Driver: `lughac test --diagnostics=json` on a package with an error in `src/main.la` gives `"file":"src/main.la"`, and one in `test/a_test.la` gives `"file":"test/a_test.la"`; `lughac build` of `src/main.la` with an error in an imported `src/shapes.la` names `src/shapes.la`. Program cases whose expected stderr names a module change with it.

## ACCEPTANCE
- [ ] The objective's program prints `Ok(2)`.
- [ ] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Spec, CHANGELOG updated.
