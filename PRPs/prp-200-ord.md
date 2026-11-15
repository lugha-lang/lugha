## FEATURE: `Ord` as an interface: order asked for (`with Ord`), tuples ordered by a derive block, floats out of `Ord` with `f64.total_order`; milestone 168, in v18's Phase 1.

**Status:** proposed 2026-11-15. Waiting for approval.

## OBJECTIVE
```lugha
struct Version { major: i64, minor: i64 } with Ord      // fields in order; implies Eq

enum Level { Low, Mid, High } with Ord                  // variants in declaration order

struct Score { name: string, points: i64 }

impl Eq for Score { fun eq(self, other: Score): bool = self.points == other.points; }

impl Ord for Score {                                   // by hand: by points only
    fun cmp(self, other: Score): Ordering = if self.points < other.points {
        Ordering.Less
    } else if self.points > other.points {
        Ordering.Greater
    } else {
        Ordering.Equal
    };
}

fun main() {
    println(Version { major: 1, minor: 2 } < Version { major: 1, minor: 10 });   // true
    println(Level.High > Level.Low);                                           // true
    println([Score { name: "a", points: 3 }, Score { name: "b", points: 1 }].sort()[0].name);  // b
    println([(2, "b"), (1, "z"), (2, "a")].sort());                            // [(1, "z"), (2, "a"), (2, "b")]
    println(1.5 < 2.5);                                                        // true: floats still have `<`
    println([2.5, -0.0, 1.0].sort_by(f64.total_order));                        // [-0.0, 1.0, 2.5]
    // [2.5, 1.0].sort()       E0451: `f64` isn't `Ord`, with help to use `f64.total_order`
    // Note { … } < Note { … } E0422: `Note` doesn't implement `Ord`
}
```

## CONTEXT
- Draft v18 §1.2 rules 7–8 [EQ-08, EQ-09], §1.7 rule 6 [PRE-04]; INT-14 (`Ord` only through `with` or `impl`).
- Built on PRP-197 (`with`), PRP-199 (`Eq`, interfaces extending interfaces).
- v18 spec §4 "User-defined order" (v15, milestone 59), "Bounds", ordering of tuples; §5 comparison operators.
- Code: `src/prelude/ops.la` (`Ordering`, `sort`), `src/prelude/eq.la`; `check/ordering.rs` (`has_cmp`), `check/bounds.rs` (`ord_ok`), `check/ops.rs`; `codegen/ordering.rs` (calls `cmp`), the tuple comparison in codegen; `check/float_bits.rs` (`f64.from_bits`, the model for `f64.total_order`).

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **`interface Ord: Eq { fun cmp(self, other: Self): Ordering; }`** in the prelude, derivable. `Ord` extends `Eq` (an order says which values are equal), so `with Ord` implies `with Eq`, and implementing `Ord` by hand needs `Eq`. `T: Ord` is written as today.
2. **Derive blocks** [EQ-09, PRE-04]:
   - Integers, `char`, `string` and `()`.
   - Tuples, element by element.
   - Structs, field by field in declaration order, the first unequal field deciding.
   - Enums, by variant position in declaration order, then payloads.
   - Floats, `bool`, arrays and `Option` aren't `Ord`, as `bool`, arrays and `Option` aren't today. A tuple with a float in it is no longer ordered.
3. **`<`, `<=`, `>`, `>=`:**
   - On numbers (floats included), `string` and `char`, they stay inline [EQ-08].
   - On structs, enums and tuples, they go through `Ord`: E0422 when the type didn't ask, E0451 naming a part that can't be ordered.
   - A type's own `cmp` method no longer counts: it becomes `impl Ord for T`.
   - The compiler's tuple comparison goes, replaced by the tuple block [PRE-04].
4. **`f64.total_order` and `f32.total_order`** are built-in associated functions, like `f64.from_bits`: `fun(f64, f64): bool`, true when the first comes before the second in IEEE 754's total order (`-NaN < -inf < … < -0.0 < 0.0 < … < inf < NaN`). They can be values (`xs.sort_by(f64.total_order)`) or called. `xs.sort()` on floats is E0451 with help naming them.
5. **The prelude's `sort`, `max` and `min`** keep `T: Ord`.
6. **No new codes:** E0422, E0451 and E0419, with a dedicated message for floats (as for `Key`).
7. **Migration by hand, in this PRP:** programs, std modules and spec examples that order their own types get `with Ord` or `impl Ord`. Programs that ordered tuples holding floats, or sorted float arrays, get `f64.total_order` or a different key. Outputs stay the same.

## REQUIREMENTS

### Must Do
- Prelude: `Ord` with its blocks; the tuple block replacing codegen's tuple comparison.
- Checker: decisions 1–6; `f64.total_order`/`f32.total_order` as values and calls.
- Codegen: `<` on structs, enums and tuples through `Ord` (derived or `cmp@Ord`); the old tuple code removed.
- Migrate every affected `.la` file, std module, spec example and error page.
- Docs: E0451, E0422, E0419 pages where they show order; v18 spec (§4 order, bounds, tuples; §5; §11 row 168; "Changes since v17"); CHANGELOG with a **Breaking** entry; TODO.

### Must NOT Do
- Built-in types implementing a program's interfaces (the next PRP).
- `Ord` for floats, `bool`, arrays or `Option`.
- Partial orders or `PartialOrd`; `sort_by` changes beyond accepting `total_order`.

## DESIGN
- **Messages:** E0451 "`f64` isn't `Ord`: floats have `<` but no total order of their own", help "sort with `xs.sort_by(f64.total_order)`, or compare by another key".
- **Performance:** tuples compare through an inlined prelude block instead of inline codegen; `bench/` sorts checked before and after.
- **No ABI or symbol change.**
- **Security:** none: no external input, `unsafe`, dependency, `git` or process changes.

## TESTS
- [ ] `tests/programs/interfaces/ord/`: the OBJECTIVE; `with Ord` on a generic struct; an enum with payloads; tuples nested in structs; `max`/`min`/`sort` on derived types; `f64.total_order` and `f32.total_order` as values and calls, with `NaN` and `-0.0`.
- [ ] Errors, one `.la` + `.stderr` each: `<` without `Ord` (E0422); `with Ord` over a float field (E0451 at the declaration); `impl Ord` without `Eq`; `[1.5].sort()`; `(1.5, 2) < (2.5, 1)`.
- [ ] Unit tests in `check/` for each rule and for `total_order`'s order of special values.
- [ ] `cargo test --lib`, `--test programs`, `--test spec`, `--test explain`, `--test ordering`; the benchmarks that sort.

## ACCEPTANCE
- [ ] The OBJECTIVE prints the six lines shown; each commented line gives its error.
- [ ] Every migrated program, std test and spec example passes with unchanged output.
- [ ] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every `pub` item documented; spec, CHANGELOG, TODO updated.
