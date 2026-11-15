## FEATURE: `mut` parameters for Lugha functions: a function changes its caller's variable in place; milestone 166, in v18's Phase 1 ahead of `Eq`/`Key`.

**Status:** proposed 2026-11-15. Waiting for approval.

## OBJECTIVE
```lugha
struct Counter { n: i64 }

fun bump(mut c: Counter, by: i64) {
    c.n = c.n + by;
}

fun fill(mut buf: string[], word: string, times: i64) {
    for _ in 0..times { buf.push(word); }
}

fun main() {
    let mut c = Counter { n: 0 };
    bump(mut c, 2);
    bump(mut c, 3);
    println(c.n);                 // 5

    let mut words: string[] = [];
    fill(mut words, "hi", 2);
    println(words);               // ["hi", "hi"]

    let fixed = Counter { n: 0 };
    // bump(mut fixed, 1);        E0501: `fixed` isn't `let mut`
    // bump(c, 1);                E0443: `bump` takes `mut c`: write `bump(mut c, 1)`
    // both(mut c, mut c);        E0508: the same place twice
}
```

## CONTEXT
- Why now: `Key`'s `hash` takes a `mut` hasher [EQ-02], and Lugha functions have no `mut` parameters; you chose to add them first, as their own PRP.
- v18 spec §4 "Mutability", §5 calls and `mut self` (E0501, E0502, E0508), §8 "`ptr` and `mut` parameters" (milestone 103: the same syntax for externs).
- Code: `parser/` (`param`, `receiver_params`, call arguments with `PassMode`), `ast::Param::mode`; `check/out_params.rs` (E0443 for externs), `check/method_calls.rs` (`no_alias`, E0508), `check/assign.rs` (`check_place_for`), `check/arguments.rs`, `check/liveness/` (moves, keeps); `codegen/out_params.rs`, `codegen/function.rs` (parameters), `codegen/methods.rs` (`mut self` by pointer).

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Syntax `mut c: Counter`, the word before the name**, as externs' `mut buf: u8[]` (milestone 103) and `mut self` already write it; the call writes `mut place`, as externs' calls do. The Draft and ledger write `h: mut Hasher`; I'd change EQ-02's example to `mut h: Hasher` so the language has one spelling. (This is the one call that touches approved text.)
2. **By pointer, like `mut self`**: the function works on the caller's place itself, not a copy that goes back afterwards (externs' copy-in, copy-out). No copy of a large struct or array, and the same behaviour as `mut self`.
3. **The rules** (as discussed):
   - Marked at both ends: a `mut` parameter needs `mut place` at the call, and `mut` before an argument needs a `mut` parameter. Either missing is **E0443**, extended from externs.
   - Only mutable places: `mut x` needs `x` to be `let mut`, a `mut` parameter or `mut self`, or a field or element of one (`mut p.inner`, `mut xs[0]`): **E0501**. `mut 5`, `mut f()`: **E0502**.
   - No aliasing in a call: no other argument may be the same place or a place inside it, or one it is inside. `both(mut a, mut a)`, `f(mut a, a.x)` and, for a method, `a.f(mut a)` are **E0508**, its rule widened from receivers to every `mut` argument. A fresh value (a number or string read out, an operator's or call's result) doesn't alias, as for `mut self` today.
4. **Second-class without new rules.** Inside the function, the parameter is a mutable place: it can be assigned, changed through fields and elements, given to `mut self` methods, and passed on as `mut`. Any other use (returning it, storing it, capturing it in a closure) reads a copy, by value semantics, so a reference never escapes the call.
5. **What can't take or pass one:**
   - `spawn f(mut x)` is **E0438** (a spawn that changes a copy), as `spawn p.bump()` already is.
   - An actor method's `mut` parameter is **E0439**: messages carry values.
   - A closure's `mut` parameter stays **E0209** (DEF-15 defers it).
   - A function with a `mut` parameter can't be a function value, since function types can't spell `mut`: **E0406**.
6. **Interfaces, generics and dispatch:** interface items, `impl` blocks and default bodies may have `mut` parameters, an `impl` must match them (E0422 otherwise), and calls through bounds and interface values pass the pointer. Generic functions take `mut x: T` like any other parameter.
7. **Liveness:** a `mut` argument isn't a last use. The caller's variable is still live, since the function changes it in place, so it is never moved into the call.

## REQUIREMENTS

### Must Do
- Parse `mut name: T` in function, method and interface-item parameters, and `mut place` in their calls (externs already parse both).
- Checker: decisions 3–7; hover and signatures show `mut`.
- Codegen: `mut` parameters by pointer, through direct, method, bound, default and interface-value calls.
- Formatter, and the language server's signature help, keep `mut`.
- Docs: E0443, E0508, E0438, E0439, E0406 pages and spec rows widened; v18 spec §4 "Mutability", §5 calls, §6 functions, §11 row 166, "Changes since v17"; CHANGELOG; the Draft and ledger's `mut h: Hasher` (decision 1).

### Must NOT Do
- `mut` closure parameters (DEF-15); `mut` in function types; `mut` return values or `mut` fields (references as values).
- Any change to extern parameters (they keep copy-in, copy-out).
- `Eq`, `Key`, `Hasher` (PRP-199).

## DESIGN
- **No new error codes:** E0443, E0501, E0502, E0508, E0438, E0439 and E0406 cover each case, their messages naming `mut` parameters.
- **ABI:** a `mut` parameter is a pointer to the caller's slot, as a `mut self` receiver is; symbols unchanged. Copy-on-write arrays reached through it are made unique before a write, as through `mut self`.
- **Security:** none: no external input, `unsafe`, dependency, `git` or process changes.

## TESTS
- [ ] `tests/programs/functions/mut_params/`: the OBJECTIVE; a struct, an array (push, element write), a nested place (`mut p.inner`, `mut xs[1]`), passing a `mut` parameter on, `mut self` methods on one; a generic `mut x: T`; an interface item with a `mut` parameter called through a bound, a default and an interface value; copies made from it don't change with it.
- [ ] Errors, one `.la` + `.stderr` each: E0443 both ways; E0501; E0502; E0508 (same place twice, a place inside it, a method's receiver); E0438 (`spawn`); E0439 (actor); E0406 (as a value); E0422 (an `impl` whose `mut` doesn't match).
- [ ] Unit tests in `check/` for each rule; parser and formatter tests; liveness (decision 7).
- [ ] `cargo test --lib`, `--test programs`, `--test spec`, `--test explain`, `--test parser`, `--test fmt`, `--test out_params`, the LSP signature tests.

## ACCEPTANCE
- [ ] The OBJECTIVE prints `5` and `["hi", "hi"]`; each commented line gives its error.
- [ ] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every `pub` item documented; spec, CHANGELOG, Draft and ledger updated.
