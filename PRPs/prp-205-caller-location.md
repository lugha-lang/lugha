## FEATURE: `@caller_location`: a marked function's panics report its caller's line; `unwrap`, `expect`, `assert`, `assert_eq` and `assert_ne` become prelude code; milestone 173, in v18's Phase 1.

**Status:** proposed 2026-11-15.

## OBJECTIVE
```lugha
@caller_location
fun positive(n: i64): i64 {
    if n <= 0 {
        panic("{n} isn't positive");          // reported where `positive` was called
    }
    n
}

@caller_location
fun checked(n: i64): i64 = positive(n);         // a marked call inside a marked one passes it on

fun main() {
    println(positive(3));                       // 3
    let o: Option<i64> = None;
    println(o.unwrap_or(7));                    // 7
    println(checked(-1));                       // panic: -1 isn't positive at main.la:16:13
}
```

What the prelude now writes, instead of codegen's `unwrap` and assertions:

```lugha
impl<T> Option<T> {
    @caller_location
    fun unwrap(self): T = match self {
        Some(x) => x,
        None => panic("unwrap on None"),
    };
    …
}

@caller_location
fun assert_eq<T: Eq>(a: T, b: T) {
    if a != b {
        panic("assert_eq failed: left {a.debug()}, right {b.debug()}");
    }
}
```

## CONTEXT
- Draft v18 §1.7 rule 7 [PRE-05]; §5 (attributes: `@caller_location` joins `@test`, E0210 covers misuse).
- Built on PRP-204 (the prelude's blocks of built-in types' own methods, where `unwrap` and `expect` go) and PRP-202 (`Debug`, which the messages use).
- v18 spec: §5 intrinsics table (`assert`, `assert_eq`, `assert_ne`, `skip`); `unwrap`/`expect`; §9 ("`unwrap` and `expect` are built in, not prelude functions, so their panic points at the caller").
- Code:
  - `parser/attributes.rs` (`@test` only);
  - `check/builtins.rs` (`unwrap`, `expect`), `check/test_fns.rs` (the assertions);
  - `codegen/unwrap.rs`, `codegen/assertions.rs`;
  - codegen's function and method declarations, `call`, `call_method`, instances, and `panic`'s lowering.
- Messages today, kept exactly:
  - `unwrap on None`, `unwrap on Err: <debug form>`, `expect`'s own text;
  - `assertion failed`, `assert_eq failed: left …, right …`, `assert_ne failed: both …`;
  - each at the call: a method's name, or a function call's start.

## DECISIONS MADE IN DISCOVERY (mine, for review; nothing here needed the user)
1. **`@caller_location`** goes before a function or a type's own method, in a program or the prelude. Before anything else it is E0210:
   - an interface's item or block, or a derive block;
   - an actor's method, `main`, or a `@test` function;
   - an extern.
   An interface block's item is called through tables and bounds, where no call site is at hand.
2. **What it changes:** in a marked function, `panic(…)` and calls of other marked functions report the location of the call that reached it. A marked function called from a marked one passes its caller's location on, so a chain reports the first unmarked caller. The compiler's own panics inside it (overflow, an index out of bounds) keep their own place, as do panics in functions it calls that aren't marked.
3. **The location:** for a method call, its name, as `unwrap` reports today (`o.unwrap()` at `unwrap`); for a function call, its start.
4. **How:** a marked function takes three hidden trailing parameters (the file, line and column), which every call passes: constants at an ordinary call site, its own at a marked one. It is never a value: `let f = positive;` is E0406, with help saying why. No runtime state, so green threads can't mix locations up.
5. **The prelude takes over:**
   - `unwrap` and `expect` on `Option` (in `impl<T> Option<T>`) and on `Result` (in `impl Result<T, E>`);
   - `assert`, `assert_eq<T: Eq>` and `assert_ne<T: Eq>`, as marked functions.
   Their messages stay exactly as they are. `codegen/unwrap.rs` and the assertions in `codegen/assertions.rs` go. `skip(reason)` stays an intrinsic: it talks to the test runner.
6. **`assert_eq` and `assert_ne` are ordinary generic calls.** `assert_eq(x, 5)` with `x: u8` works as before. An argument that can't be typed alone, first (`assert_eq(None, o)`), is E0412 as in any generic call; nothing writes that today. A type without `Eq` is the same E0422/E0451 as before.
7. **Codes:** none new. E0210 gains `@caller_location`'s misuse, and E0406 a marked function used as a value.
8. **Performance:** `unwrap` becomes a call of a small prelude method instead of inline code, which `-O2` inlines. The bench runs before and after.

## REQUIREMENTS

### Must Do
- Parser: `@caller_location` on functions and methods (inside `impl` blocks too); E0210 for misuse.
- Prelude: `unwrap`, `expect`, `assert`, `assert_eq`, `assert_ne`.
- Checker: the marked set, E0210 and E0406; `check/builtins.rs`'s `unwrap`/`expect` and `check/test_fns.rs`'s assertions removed (`skip` kept).
- Codegen: hidden location parameters on marked functions and their instances; passing them at every call; `panic` inside a marked function using them; `codegen/unwrap.rs` and the assertions removed.
- Test runner: panic lines parse as before (`lughac test`'s JSON).
- Language server: hover on `unwrap` and `assert_eq` reaches the prelude.
- Docs: E0210 and E0406 pages; v18 spec (attributes, the intrinsics table, `unwrap`/`expect`, §9 notes, §11 row 173, "Changes since v17"); CHANGELOG; TODO.

### Must NOT Do
- A `Location` value programs can read (`caller_location()`), or locations for the compiler's own panics.
- `@caller_location` on closures or through function values.
- Moving `skip`, `panic` or other intrinsics.

## TESTS
- [ ] `tests/programs/errors/caller_location/`:
  - the OBJECTIVE (a panic, `.stderr` and exit code);
  - a chain of marked functions;
  - a marked method;
  - an unmarked function between two marked ones;
  - `unwrap`, `expect`, `assert`, `assert_eq` and `assert_ne` failing (their messages and locations unchanged).
- [ ] Errors: `@caller_location` on an interface item, an actor method and `main` (E0210); a marked function as a value (E0406).
- [ ] Unit tests: the parser's attribute on methods; the checker's marked set; codegen's hidden parameters and constants at a call site.
- [ ] The full `cargo test` (every `assert_eq` test goes through the prelude now), the test runner's JSON tests, and the bench before and after.

## ACCEPTANCE
- [ ] The OBJECTIVE prints and panics as shown; each misuse gives its error.
- [ ] Every existing program and test passes with identical output and panic messages.
- [ ] The full `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every `pub` item documented; spec, CHANGELOG, TODO updated.
