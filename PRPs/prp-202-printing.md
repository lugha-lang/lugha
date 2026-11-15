## FEATURE: Printing through `Debug` and `Display`: `Debug` for every type, `Display` by hand, `x.debug()` and `debug(x)`; milestone 170, in v18's Phase 1.

**Status:** proposed 2026-11-15. Approved 2026-11-15.

## OBJECTIVE
```lugha
struct Point { x: i64, y: i64 }

impl Display for Point {
    fun show(self): string = "({self.x}, {self.y})";
}

enum Shape { Dot, Circle(f64) }

fun main() {
    let p = Point { x: 1, y: 2 };
    println(p);                                  // (1, 2)
    println("at {p}");                           // at (1, 2)
    println(p.debug());                          // Point { x: 1, y: 2 }
    println([p, p].debug());                     // [Point { x: 1, y: 2 }, Point { x: 1, y: 2 }]
    println(Some("ada").debug());                // Some("ada")
    println(Shape.Circle(1.5).debug());          // Circle(1.5)
    println("ada".debug());                      // "ada"
    let q = debug(p.x + 1);                      // stderr: [main.la:16] p.x + 1 = 2
    println(q);                                  // 2
    // println([1, 2]);                          E0422: `i64[]` doesn't implement `Display`; help: `[1, 2].debug()`
    // println(Shape.Dot);                       E0422: `Shape` doesn't implement `Display`
    // lughac build -O2 with a `debug(x)`        E0465: `debug` is for development builds
}
```

## CONTEXT
- Draft v18 §1.7 rules 2–3 [PRE-02, OPN-02], §1.1 rule 15 [INT-14: `Display` never derivable, `Debug` universal], §2.1 rule 9 [CAP-09: `debug(x)` ambient, stderr, file and line, an error in release builds]. Ledger: PRE-02 replaces `codegen/show.rs`; SUP-17.
- Discussed with the user: containers follow Rust's split. They are `Debug` only, never `Display`, and their debug form is printed with `.debug()`.
- Built on PRP-197 (derive blocks on request, with the prelude's types exempt), PRP-199/200 (the prelude's `Eq`, `Key`, `Ord` by block) and PRP-201 (blocks for built-in types, which the prelude now uses for `Display` on numbers).
- v18 spec §5 "How values print" (v12; strings quoted inside, own `show`), §4 interface values (print as the value inside), the intrinsics `print`, `println`, `eprint`, `eprintln`, `to_string`, interpolation.
- Code: `src/prelude/iter.la` (`interface Display { fun show(self): string; }`, met by name today); `check/printing.rs` (every type printable), `check/call.rs` (the print intrinsics); `codegen/show.rs` (the built-in forms), `codegen/show_iface.rs` (interface values); `check/derive_intrinsics.rs` (`derive.fields`, `derive.variants`, used by `json.Json`).
- Measured: 35 test programs print a struct or enum without its own `show`; 86 print containers of built-in types; 29 files (70 methods) have their own `show`.
- Out of reach here: "`Error` requires `Display`" [OPN-10]: the prelude has no `Error` interface yet. The ambient `println` stays until the capabilities PRP [PRE-06, CAP-08].

## DECISIONS MADE IN DISCOVERY (mine, for review; containers are the user's)
1. **`interface Debug { fun debug(self): string; }`**, in the prelude, implemented for every type without asking [INT-14]. The prelude's derive blocks cover:
   - numbers, `bool` and `()`;
   - `char` (`'a'`) and `string` (`"ada"`, quoted and escaped);
   - arrays (`[1, 2]`), `Option` (`Some(1)`, `None`), tuples (`(1, "x")`), `Map` (`#{"a": 1}`), `Set` (`#{1}`);
   - structs (`Point { x: 1, y: 2 }`), and enums including `Result` (`Circle(1.5)`, `Err("no")`).
   Each part is shown in its own debug form. The forms are today's built-in ones, except that a type's `Display` is never used inside them.
2. **Types with no structure to show** keep their fixed debug forms, from the compiler: functions (`<fun>`), `Ref`, tasks, channels, actor handles, extern handles and `CPtr`. An interface value shows the value inside it. So `debug(x)` and `x.debug()` work on any type, a bare `T` included, with no bound.
3. **`impl Debug for T { … }` by hand** replaces the derived form wherever `T` appears, as for `Eq`. `with Debug` is E0463, as asking twice is, with help saying every type already implements `Debug`.
4. **`Display` is a block interface,** `interface Display { fun show(self): string; }`, never derivable:
   - The numbers, `bool`, `char` and `string` implement it in the prelude (`impl Display for i64`, through PRP-201's blocks), printing as today: strings unquoted.
   - Containers, `()`, functions, handles and interface values don't, unless their interface extends `Display` (`interface Shape: Display`).
   - A struct or enum implements it only with `impl Display for T`; a type's own `show` method no longer counts.
5. **What needs `Display`:**
   - `print`, `println`, `eprint`, `eprintln` and `to_string(x)`, each with one argument;
   - interpolation `"{x}"`, its parts included.
   Without it, E0422 "`P` doesn't implement `Display`". The help says to write `impl Display for P`, or print the debug form with `p.debug()`. On a bare `T`, the help names the bound `T: Display`.
6. **`debug(x)`** is an ambient prelude function [CAP-09]:
   - It writes `[file:line] <expression as written> = <debug form>` to stderr, the file as the build names it, like Rust's `dbg!`.
   - It returns `x`, so it can wrap an expression in place.
   - In an optimized build (`-O2`, the only release build lughac has) it is E0465, "`debug` is for development builds", at each call. `lughac check` and `test` accept it.
7. **Panics that show a value** use its debug form, as Rust does: `unwrap()` and `expect` on an `Err`, `assert_eq` and `assert_ne`. `Err("boom").unwrap()` panics with `"boom"`, quoted.
8. **Replaced:**
   - `codegen/show.rs` and the compiler-made `lugha_show_<type>` helpers become the prelude's blocks. Only the fixed forms of decision 2 stay in codegen, with `show_iface.rs` for interface values.
   - The name-based "a struct with `show` prints through it" goes [INT-03].
9. **Codes:** E0422 for a missing `Display`; one new code, E0465, with its page.
10. **Migration by hand, in this PRP:**
    - own `show` methods become `impl Display for T`;
    - printing a struct or enum without one either gains an `impl Display` that keeps the program's output, or switches to `.debug()`;
    - printing containers becomes `.debug()`;
    - the same for std modules, spec examples and error pages.
    Outputs stay the same, except where a container held a type with its own `show`, or where a panic shows an `Err`.

## REQUIREMENTS

### Must Do
- Prelude: `Debug` with its blocks, `Display` by block, `impl Display` for the numbers, `bool`, `char` and `string`, and `debug(x)`.
- Checker: decisions 1–7 and 9; `debug(x)` recording its call's file, line and source text; E0465 when the driver builds at `-O2`.
- Codegen: printing and interpolation through `Display`, inline for numbers, `bool`, `char` and `string` as today; panics through `Debug`; `codegen/show.rs` removed.
- Language server: `debug` and `.debug()` complete and hover like any prelude item.
- Migrate every affected `.la` file, std module, spec example (§10 outputs unchanged) and error page.
- Docs: E0422 page (`Display` example), new E0465 page; v18 spec (§5 "How values print" rewritten, §4 `Debug`/`Display`, intrinsics, §11 row 170, "Changes since v17", Breaking); CHANGELOG; TODO.

### Must NOT Do
- Removing the ambient `print`/`println` or adding `Console` (the capabilities PRP).
- The `Error` interface [OPN-10]; format specifiers (`{x:?}`, width, precision); pretty multi-line debug output.
- `Display` for containers or any derived `Display`.

## DESIGN
- **Debug forms in Lugha:** the struct and enum blocks use `derive.fields` and `derive.variants` with each part's `debug()`. Floats print through the runtime's shortest round-trip form, as today.
- **Performance:** numbers, `bool`, `char` and `string` still print inline. Containers and structs build their text in prelude code, which is inlined at `-O2`. The benchmarks print only numbers and strings and are checked before and after.
- **Symbols:** the prelude's blocks get ordinary hidden-owner instances (`lugha_fn_$prelude/Debug@array.debug<i64>`); `lugha_show_*` goes.
- **Security:** none: no external input, `unsafe`, dependency, `git` or process changes. `debug` writes only to stderr.

## TESTS
- [x] `tests/programs/text/printing/`:
  - the OBJECTIVE;
  - every debug form above: nested, strings and chars escaped, empty containers, maps and sets, `Result`, generic structs, recursive enums, functions, `Ref`, tasks, channels, interface values;
  - `impl Debug` by hand inside containers;
  - `Display` on an interface that extends it;
  - `to_string` and interpolation;
  - `debug(x)`'s stderr line and its returned value.
- [x] Errors, one `.la` + `.stderr` each:
  - `println(xs)`, `"{opt}"` and `to_string(p)` without `Display` (E0422);
  - `"{x}"` on a bare `T` (E0422, help `T: Display`);
  - `with Debug` (E0463);
  - `debug` at `-O2` (E0465, through the driver test).
- [x] Unit tests: the checker's rules; the driver's E0465; the formatter on `debug(…)`.
- [x] `cargo test --lib`, `--test programs`, `--test spec`, `--test explain`, `--test lsp_completion`, the driver tests touched; bench before/after.

## ACCEPTANCE
- [x] The OBJECTIVE prints the lines shown; each commented line gives its error.
- [x] Every migrated program, std test and spec example passes with its output unchanged, except the documented panic and nested-`show` cases.
- [x] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [x] Every `pub` item documented; spec, CHANGELOG, TODO updated.

## CHANGES DURING THE BUILD
- **Containers follow Rust's split** (the user's call in discovery): `Display` never covers arrays, tuples, `Option`, `Result`, maps, sets or `()`; their text is `.debug()`.
- **Interface values** print when their interface declares `show(self): string` (through the table); `interface Shape: Display` can't be a value type yet (E0464), so it isn't the way.
- **The debug forms** are the prelude's blocks (`src/prelude/debug.la`), dispatched by one `lugha_debug_<type>` helper per type, which calls the block's or the hand-written `impl Debug`'s instance; numbers, `bool`, `char` and `string` stay inline, and the fixed forms (`<fun>`, `Ref(…)`, `<task>`, `<chan>`, `<actor N>`, `<Handle>`, `<union>`) and interface values stay in codegen. Strings and chars are quoted through `$string.quote` and `$string.quote_char`, built-ins over the runtime's existing quoting.
- **`derive.name(x)`**, a new field intrinsic, gives a struct block its type's bare name.
- **Every `debug` reaches one pseudo-callee, `$debug`,** as `==` reaches `$equal`: the checker lists the type's debug instance (and, for an interface value, those of every type converted to it).
- **`x.debug()` on an actor handle** is its debug form unless the actor has a `debug` message.
- **PRP-201's test interface** `Show { fun show(self) }` now collides with `Display`'s `show` on built-in types (E0460, as two interfaces with one item name do); its tests name the item `label`.
- **Std:** `TimeError` and `DecodeError` get `impl Display` from their old `show`; a printed `Result` of them shows the struct, `Err(TimeError { message: "…" })`.
- **Migration:** 7 files' own `show` became `impl Display` and about 110 programs, 40 spec examples and 14 error pages got `.debug()` mechanically, from the compiler's own E0422 spans; outputs are unchanged except where a container held a type with its own `show` (6 programs, 2 spec examples) and the `unwrap` panic.

