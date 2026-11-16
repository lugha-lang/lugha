## FEATURE: Collection literals `Map { "ann": 31 }`, `Set { 1, 2 }`, `Map {}`, through the prelude interfaces `FromEntries` and `FromItems`; `#{…}` is removed; milestone 178, in v18's Phase 1.

**Status:** proposed 2026-11-16. Approved 2026-11-16.

## OBJECTIVE
```lugha
// bag.la
public struct Bag { public counts: Map<string, i64> }

impl FromItems<string> for Bag {
    fun from_items(items: string[]): Bag {
        let mut counts: Map<string, i64> = Map {};
        for x of items {
            counts[x] = counts.get(x).unwrap_or(0) + 1;
        }
        Bag { counts: counts }               // in Bag's own module: a struct literal
    }
}

// main.la
import bag { Bag };

fun main() {
    let ages = Map { "ann": 31, "bo": 40 };
    let primes = Set { 2, 3, 5, 7 };
    let index = Map<string, i64[]> {};
    let empty: Set<char> = Set {};
    let name = "cy";
    let more = Map { name: 22 };             // keys are expressions
    let fruit = Bag { "apple", "apple", "pear" };
    println(ages.debug());                   // Map { "ann": 31, "bo": 40 }
    println(primes.debug());                 // Set { 2, 3, 5, 7 }
    println("{index.len} {empty.len} {more["cy"]}");
    println(fruit.counts["apple"]);          // 2
    if ages == (Map { "bo": 40, "ann": 31 }) {
        println("same");
    }
    let old = #{"a": 1};                     // E0213: write `Map { "a": 1 }`
}
```

## CONTEXT
- Draft v18 §1.6 [LIT-01..06]: `Map { k: v }`, `Set { a }`, `Map {}`, `Set {}`, typed from context or type arguments; `#{…}` removed; keys are expressions, an unknown bare name gets help suggesting `"name"`; prelude interfaces `FromEntries<K, V>` and `FromItems<T>` give any type the forms; braces after a type name are fields for a struct, entries or items for a collection type; parentheses in conditions; debug form `Map { "a": 1 }`, `Set { 1, 2 }`.
- Decided with the user: a type implementing either interface is still built with a struct literal **in the module that declares it** (that's how `Map` and `Set` build themselves); everywhere else its braces are a collection.
- Built on PRP-209: written type arguments on struct literals (`Map<string, i64> {}`).
- Today: `#{…}` is its own token and grammar (`parser/hashed.rs`), checked as calls of `$map_of`/`$set_of` (`check/maps.rs`) and lowered in `codegen/maps.rs`; the first entry decides map or set, and `#{}` needs an expected type.
- Measured: about 100 `#{` in `.la` files (38 files), 17 in the spec, 3 error pages, 23 Rust files; printed maps and sets in `.stdout` files.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The interfaces**, in the prelude:
   ```lugha
   interface FromEntries<K, V> { fun from_entries(keys: K[], values: V[]): Self; }
   interface FromItems<T> { fun from_items(items: T[]): Self; }
   ```
   `Map<K: Key, V>` implements `FromEntries<K, V>` and `Set<T: Key>` implements `FromItems<T>`, with the bodies `$map_of` and `$set_of` have today.
2. **Parsing:** after a type name (and `m.Name`, and written type arguments), braces hold elements that are `expr: expr`, `expr` or `..base`. All `name: expr` with plain names (or `{}`) can be a struct literal; mixing pairs and items is E0201 as in `#{…}`. The parser keeps both readings where both are possible, and the checker picks one, so nothing in the grammar depends on types.
3. **Which reading:**
   - a struct that implements neither interface: fields, as today;
   - a type that implements one: entries (`FromEntries`) or items (`FromItems`), except in the module that declares it, where it is a struct literal (the user's decision);
   - `{}` on a collection type: `FromEntries` if it implements that, else `FromItems`;
   - the wrong form (items for a `FromEntries` type, entries for a `FromItems` type, items or a non-name key for a plain struct) is a new **E0470**, "`Point` isn't built from items", with help naming the interface to implement or the form to write.
4. **Meaning:** a collection literal is the call `T.from_entries([k, …], [v, …])` or `T.from_items([a, …])`, with the literal's written type arguments on `T` and its expected type passed through, so `Map {}` takes its types from context or `Map<string, i64> {}`, and E0412 otherwise (help: write them). Elements are evaluated left to right; a later equal key replaces the earlier value (as now).
5. **Keys are expressions** (`Map { name: 1 }` uses `name`); a bare key name that nothing defines is E0301 with the help ``write `"name"` for the string``.
6. **W0105** (a key written twice) applies to literal keys of the new form; E0441 (float keys) stays.
7. **Conditions:** like struct literals, they need parentheses (E0205 otherwise).
8. **Debug form:** `Map { "ann": 31 }`, `Set { 1, 2 }`, `Map {}`, `Set {}`.
9. **`#{`** stays a token only to give **E0213** (new parse error), "`#{…}` is gone", with help `Map { … }` or `Set { … }`; the parser skips the literal and goes on.
10. **Removed:** `ExprKind::Hashed`, `HashLiteral`, `HashKind`, the checker's and codegen's literal paths; `$map_of`/`$set_of` stay as the prelude's helpers. The loop-label special case moves with the key parsing (`Map { a: loop { … } }` is an entry).
11. **Ledger:** the module rule is added as LIT-07 (approved).
12. **Breaking:** every `#{…}`; printed maps and sets read differently.

## REQUIREMENTS

### Must Do
- Prelude: the two interfaces, the `Map` and `Set` impls, the debug forms.
- Parser: brace elements per decision 2; `#{` is E0213.
- Checker: decision 3–7; E0470.
- Codegen: lower the chosen reading (the call); remove the hashed path.
- Formatter, sexp printer: print the elements as written.
- Migration of every `#{` (the old checker's own record of map or set decides each, so `#{}` becomes the right one).
- Docs: E0213 and E0470 pages; W0105, E0441 and E0205 pages; v18 spec (grammar, §4 maps and sets, interfaces, printing, the error table, §11 row 178, "Changes since v17", Breaking); CHANGELOG; ledger LIT-07.

### Must NOT Do
- Struct field shorthand (`P { x, y }` meaning `P { x: x, y: y }`).
- Literals for arrays or other prelude types beyond `Map` and `Set`.
- A `=>` form.

## TESTS
- [x] `tests/programs/collections/literals/`: the OBJECTIVE minus its error (runs), a user `FromEntries` type, nested literals, type arguments and context, keys as expressions, W0105, E0213, E0470 (each wrong form), E0301 with the string help, E0412 for `Map {}` without a type, E0205 in a condition.
- [x] Parser and checker unit tests; formatter round-trip.
- [x] The full `cargo test`.

## ACCEPTANCE
- [x] The OBJECTIVE gives E0213 at `#{` only, and once that line goes prints as its comments say, then `same`.
- [x] The full `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [x] Spec, CHANGELOG, ledger and the error pages updated.

## CHANGES DURING THE BUILD
- **The objective's `Bag` lives in its own module** (`bag.la`): by the module rule, `Bag { "apple", … }` in `Bag`'s own file is its struct literal, so the objective as first written gave E0470 there.
- **W0105 is for the prelude's `Map` and `Set` only:** a program's `FromItems` type may want repeats (a bag).
- **`Map {}` with nothing to type it** gives one E0412, "cannot infer the type arguments of `Map {}`", with help to write them, instead of errors on the hidden arrays.
- **E0205 for collections:** in a condition, `Name {` followed by any one token and `:` or `,` is a misplaced literal (it can't start a block), so `if m == Map { "a": 1 } {` gets the parentheses help.
- **`FromEntries` and `FromItems` are nominal** (met only by `impl … for T`) and visible by bare name in every module, like `Display`.
- **Walkers follow the checker's choice:** `Checked::collections` lists the call each collection literal stands for; liveness, hoisting and codegen walk that call, otherwise the struct's fields.
- **Migration:** the old checker recorded whether each `#{…}` was a map or a set before the parser changed, so every `#{}` became the right `Map {}` or `Set {}`; the VS Code grammar drops its `#{` rule (extension version bumped).
