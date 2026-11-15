## FEATURE: The prelude's methods on built-in types are ordinary `impl` blocks: `impl string`, `impl<T> T[]`, `impl char`, `impl<T> Option<T>`, `impl Result<T, E>`; milestone 172, in v18's Phase 1.

**Status:** proposed 2026-11-15. Approved 2026-11-15.

## OBJECTIVE
What the prelude now holds (`src/prelude/core.la`), instead of free functions behind a table of method names:

```lugha
impl<T> T[] {
    /** A new array of `f` applied to each element, in order. */
    fun map<U>(self, f: fun(T): U): U[] { … }
    fun filter(self, keep: fun(T): bool): T[] { … }
    fun fold<A>(self, init: A, f: fun(A, T): A): A { … }
    …
}

impl<T: Ord> T[] {
    fun sort(self): T[] = self.sort_by(fun(a: T, b: T): bool = a < b);
}

impl string[] {
    fun join(self, sep: string): string { … }
}

impl string {
    fun trim(self): string { … }
    …
}
```

Programs see no difference:

```lugha
fun main() {
    println(["b", "a"].sort().map(fun(s: string): string = s.trim()).join("+"));   // a+b
    println(Some(2).map(fun(n: i64): i64 = n * 10).unwrap_or(0));                   // 20
    println('x'.to_upper());                                                       // X
    // [2.5, 1.0].sort()      E0451, as before: `f64` isn't `Ord`
    // impl string { … }      E0466 in a program, as before
}
```

## CONTEXT
- Draft v18 §1.7 rule 5 [PRE-03]: "The prelude writes ordinary `impl` blocks for built-in types … so their methods are ordinary methods". The ledger names what it replaces: the name table in `check/helpers.rs`. Rule 10 [PRE-07] keeps array growth and the other compiler-made operations in the compiler.
- Built on PRP-201 (blocks for built-in types, hidden owners, instantiation per type) and PRP-203 (E0466 for `impl f64 { … }` in a program).
- Code:
  - `check/helpers.rs`: `helper_function`'s table maps 38 method names to prelude functions (`str_trim`, `option_map`, …), and `helper_call` checks them.
  - `codegen/methods.rs`: `prelude_target`.
  - `check/members.rs`: completion builds on `helper_function`.
  - `src/prelude/core.la`, `iter.la`, `text.la`, `ops.la`, `maps.la`: the functions.
- Not the helper table: `check/builtins.rs` (`push`, `pop`, `clear`, `slice`, `to_bytes`, the integer methods, `to_bits`, `Ref`/`Chan`/task/actor methods, `unwrap`/`expect`) is made by codegen, and `check/iter_helpers.rs` serves any iterator.

## DECISIONS MADE IN DISCOVERY (mine, for review; nothing here needed the user)
1. **Blocks of a built-in type's own methods,** with no interface: `impl string { … }`, `impl char { … }`, `impl<T> T[] { … }`, `impl<T> Option<T> { … }`, with bounds (`impl<T: Ord> T[]`) and at concrete types (`impl string[]`).
   - Only the prelude may write them; anywhere else it is E0466, as today.
   - `Result`, `Map` and `Set` are prelude types and already take ordinary blocks: `result_map` and its kin move into `impl Result<T, E> { … }`.
2. **Every helper moves, unchanged in behaviour:**
   - the array helpers `map`, `filter`, `fold`, `sort_by`, `any`, `all`, `iter`, plus `sort` (`T: Ord`) and `join` (`string[]`);
   - the string helpers `lines`, `split`, `trim`, `contains`, `starts_with`, `ends_with`, `bytes`, `chars`, `char_count`;
   - the seven `char` methods;
   - `Option`'s `unwrap_or`, `ok_or`, `map`, `is_some`, `is_none`, `and_then`;
   - `Result`'s `unwrap_or`, `ok`, `map`, `map_err`, `is_ok`, `is_err`, `and_then`.
   `check/helpers.rs`'s table goes.
3. **Finding a method on a built-in value:** the compiler's built-ins first (`push`, `unwrap`, …), then the prelude's blocks of the type, then interface blocks (PRP-201). A method of a block whose bounds the type misses is reported as the bound's error: `[2.5].sort()` stays E0451, "`f64` isn't `Ord`".
4. **The free functions go:** `map(xs, f)`, `str_trim(s)` and the like are no longer names a program can call. No program, std module or spec example calls them; `char_from(n)`, a documented function, stays.
5. **Hidden owners** for the blocks, as for PRP-201's: `$prelude.$array`, `$prelude.$string`, `$prelude.$char`, `$prelude.$option` (a block with bounds or at a concrete type gets its own, `$prelude.$array@Ord`). Symbols follow: `lugha_fn_$prelude/$array.map<i64, string>`.
6. **What stays:** the compiler built-ins of `check/builtins.rs` [PRE-07] (`unwrap`/`expect` move with `@caller_location`, PRE-05), and the iterator helpers of `check/iter_helpers.rs`, which belong to any `Iter` (TODO).
7. **Gained:** hover and go to definition on `s.trim()` reach the prelude's method and its doc comment; completion lists the blocks' methods.
8. **Codes:** none new; no change a program can see but decision 4.

## REQUIREMENTS

### Must Do
- Parser: `impl<…> Type { … }` and `impl string { … }` without an interface.
- Prelude: the helpers as methods of the blocks; their doc comments kept.
- Checker: the blocks, their owners and methods; method lookup on built-in receivers; `helpers.rs`'s table removed.
- Codegen: calls of the blocks' methods as instances; `prelude_target` removed.
- Language server: completion, hover and definition from the blocks.
- Docs: v18 spec (the prelude's description in §5/§7, §11 row 172, "Changes since v17", Breaking for decision 4); CHANGELOG; TODO (the iterator helpers).

### Must NOT Do
- Moving the compiler built-ins, `unwrap`/`expect` or the iterator helpers.
- Letting programs or std add methods to built-in types.
- Changing any method's behaviour or signature.

## DESIGN
- **Reuse:** a built-in type's own block is recorded like PRP-201's blocks, with no interface, so lookup, instantiation and codegen's instance search follow the same path.
- **Performance:** none: the same prelude code, called as a method; the bench runs before and after.
- **Security:** none: no external input, `unsafe`, dependency, `git` or process changes.

## TESTS
- [x] `tests/programs/collections/prelude_methods/`: the OBJECTIVE, every moved method once, and chains mixing them with compiler built-ins and iterator helpers.
- [x] Errors: `impl string { … }` in a program (E0466); `[2.5].sort()` (E0451); `map(xs, f)` as a function (E0301).
- [x] Unit tests: the parser's own-method blocks for built-in types; method lookup order; completion and hover on a moved method.
- [x] The full `cargo test`, since every program uses these methods; bench before and after.

## ACCEPTANCE
- [x] The OBJECTIVE prints the lines shown; each commented line gives its error.
- [x] Every existing program passes unchanged.
- [x] The full `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [x] Every `pub` item documented; spec, CHANGELOG, TODO updated.

## CHANGES DURING THE BUILD
- **Each method begins `let xs = self;`** (or `s`, `o`, `r`, `c`): the old function bodies moved unchanged, so no body was rewritten by hand. Arrays are copy-on-write and strings pointers, so the copy costs nothing.
- **The parser** reads `impl<T> T[] { … }` and `impl string { … }` without `for`; a program's `impl f64 { … }` now reaches E0466 through the same path.
- **E0466's help** names any built-in type, not only numbers.
- **Completion's candidate list** keeps only the compiler's built-ins (`join` among them, for tasks and actors); the prelude's methods come from the blocks.
- **Three panic messages** quote prelude lines, which moved: `bad_key`, `overflow`, `split_panic` (outputs regenerated).

