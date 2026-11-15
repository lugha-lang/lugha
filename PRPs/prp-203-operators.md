## FEATURE: Operators through generic interfaces: `impl Mul<f64> for Vec2`, `impl Mul<Vec2> for f64`, the result from the block; milestone 171, in v18's Phase 1.

**Status:** proposed 2026-11-15. Approved 2026-11-15.

## OBJECTIVE
```lugha
struct Vec2 { x: f64, y: f64 } with Eq

impl Add for Vec2 {                                  // `Add` is `Add<Vec2>`: Vec2 + Vec2
    fun add(self, o: Vec2): Vec2 = Vec2 { x: self.x + o.x, y: self.y + o.y };
}

impl Mul<f64> for Vec2 {                             // Vec2 * f64
    fun mul(self, k: f64): Vec2 = Vec2 { x: self.x * k, y: self.y * k };
}

impl Mul<Vec2> for f64 {                             // f64 * Vec2, in Vec2's module; its result is a Vec2
    fun mul(self, v: Vec2): Vec2 = v * self;
}

impl Neg for Vec2 {
    fun neg(self): Vec2 = Vec2 { x: -self.x, y: -self.y };
}

fun twice<T: Add>(x: T): T = x + x;                  // T + T gives T
fun scaled<T: Mul<f64>>(x: T): T = x * 2.0;

fun main() {
    let v = Vec2 { x: 1.0, y: 2.0 };
    let w = 2.0 * v + v * 0.5;
    println((-w).debug());                           // Vec2 { x: -2.5, y: -5.0 }
    let mut u = v;
    u *= 3.0;
    println("{u.x} {twice(v).y} {scaled(v).x} {twice(5)}");   // 3.0 4.0 2.0 10
    println(Mul.mul(v, 2.0).y);                      // 4.0
    // v * v                     E0404: no `impl Mul<Vec2> for Vec2`; there are `Mul<f64>`
    // impl Vec2 { fun add(self, o: Vec2): Vec2 = … }    no longer an operator: `v + v` is E0404
    // impl f64 { fun mul(self, v: Vec2): Vec2 = … }      E0466: write `impl Mul<Vec2> for f64`
}
```

## CONTEXT
- Draft v18 §1.1 rule 4 [INT-04], rule 5 [INT-05], rules 2 and 8 [INT-02, INT-16]; §1.3 (keeps milestones 158–159's meaning and E0457).
- Decided with the user: the right-hand type is the interface's argument, defaulting to `Self`; the result is whatever the block's method returns, as in Rust, and a bound means a `T` result (`T: Mul<f64>` is `T * f64 → T`).
- Built on PRP-196 (nominal blocks), PRP-201 (blocks for built-in types, with the one-block-per-built-in-type limit this lifts).
- v18 spec §4 "Operator bounds", §5 "Operator overloading" (milestones 52, 158, 159), E0457, E0458.
- Code: `src/prelude/ops.la`; `modules/overloads.rs` (renaming same-named methods), `check/overloads.rs`, `check/operators.rs`, `check/foreign_impls.rs` (`impl f64 { … }`, E0458), `check/builtin_impls.rs`; `codegen/operators.rs`.
- Measured: 9 test programs and 10 spec methods define operator methods; 2 test programs apply an operator to an interface value.

## DECISIONS MADE IN DISCOVERY (mine, for review; the result type is the user's)
1. **The prelude's interfaces** become `interface Add<R = Self> { fun add(self, other: R): Self; }`, and likewise `Sub`, `Mul`, `Div` and `Rem`; `Neg` stays `fun neg(self): Self`. They are block interfaces: met only through `impl Mul<R> for T`, never by a method's name [INT-03].
2. **Default type arguments for interfaces:** an interface parameter may have a default, `R = Self`, written last, which may name `Self` and earlier parameters; a use that leaves trailing arguments out takes the defaults (`impl Add for Vec2` is `Add<Vec2>`, `T: Add` is `Add<T>`). Every interface may use it, not only the prelude's.
3. **The result is the block's** (the user's call): an operator block's method may return any type, and `a op b` has that type; `Self` in the interface stands for what a bound promises. So `T: Mul<f64>` holds for a type whose `impl Mul<f64>` returns that type, and `x * 2.0` there is a `T`; a block returning another type doesn't meet the bound (E0422, saying what it returns).
4. **Choosing the block:** `a op b` takes the block of `a`'s type whose argument is `b`'s exact type, with no conversion, as milestone 158 did. A literal must fit exactly one (`2` fits the integer types, `2.0` the float ones), else E0457; none is E0404, listing the arguments there are. A literal on the left fits the number types with a block for the right operand (E0457 when several). `v.mul(x)` and `Mul.mul(v, x)` choose the same way.
5. **Several blocks per type,** one per argument [INT-05], for built-in types too: `impl Mul<Vec2> for f64` and `impl Mul<Mat> for f64` coexist. This lifts PRP-201's one-block-per-built-in-type limit for every generic interface: blocks overlap only when their types and their arguments could both match.
6. **Where:** the usual homes (the interface's module, the type's module, a type it's built from), and for an operator interface also the module of a program type in its argument: `impl Mul<Vec2> for f64` in `Vec2`'s module [INT-04]. Elsewhere, E0459.
7. **Built-ins stay:** numbers keep their operators, `+` on strings and arrays stays, and they meet `T: Add` (and the rest) as built-ins; `Num` and `Float` are unchanged.
8. **Retired:**
   - operators through a method's name (`impl Vec2 { fun add(…) }`);
   - several same-named methods on one type (milestone 158's renaming, `modules/overloads.rs`);
   - `impl f64 { … }` and `impl shapes.W { … }` holding arithmetic (milestone 159). Such a block is now E0466, "methods for a type this module doesn't declare", with help naming the operator block to write. E0458 is retired.
   - Operators on interface values. An interface value isn't an operator type (`interface Scaled: Mul<f64>` can't be a value type, E0464), so `s * 2.0` is E0404, with help to call `s.mul(2.0)`.
9. **Codes:** E0404, E0422, E0457, E0459 and E0461 as above; one new code, E0466; E0458 retired with its page saying so.
10. **Migration by hand, in this PRP:** the 9 programs, the spec's operator examples (milestones 52, 158, 159) and the error pages move to `impl Op<R> for T`; outputs unchanged.

## REQUIREMENTS

### Must Do
- Parser: defaults on interface type parameters.
- Prelude: the generic operator interfaces.
- Checker: decisions 1–9, replacing `check/overloads.rs` and `check/foreign_impls.rs`'s arithmetic; `modules/overloads.rs` removed.
- Codegen: operators, compound assignment, `v.mul(x)` and qualified calls through the chosen block's method, on structs, enums, handles and built-in types; through a bound, each instance's block.
- Language server: hover and go to definition on an operator's use reach its block's method, as today.
- Docs: E0457, E0459, E0404 pages; E0458 page retired; new E0466 page; v18 spec (§4 operator bounds and interface defaults, §5 operator overloading rewritten, §11 row 171, "Changes since v17", Breaking); CHANGELOG; TODO.

### Must NOT Do
- Associated types, or a bound naming another result (`T: Mul<f64, Out = Vec2>`).
- Overloading `==`, `<`, indexing or the bit operators differently from today.
- Operators on interface values through a table.

## DESIGN
- **Checking:** the operator's block is found like an `impl` block's item, through the `impls` table for structs, enums and handles and through the blocks for built-in types, keyed by the argument; the call records the block's method, as qualified calls do.
- **Symbols:** blocks' methods keep their tagged names (`mul@Mul<f64>`), so the renamed `mul(f64)` symbols go; no other ABI change.
- **Performance:** none: numbers keep their inline operators, and a block's method is an ordinary call, as before.
- **Security:** none: no external input, `unsafe`, dependency, `git` or process changes.

## TESTS
- [x] `tests/programs/interfaces/operators/`, rewritten:
  - the OBJECTIVE;
  - `Sub`, `Div`, `Rem` and `Neg` on an enum;
  - a generic struct's block with bounds;
  - two right-hand types, chosen by a literal and by a variable;
  - a number on the left from the right-hand type's module (a directory case);
  - compound assignment, `v.mul(x)` and `Mul.mul(v, x)`;
  - `T: Add` on numbers, strings, arrays and a struct;
  - default type arguments on a program's own interface.
- [x] Errors, one `.la` + `.stderr` each:
  - no block for the operand (E0404);
  - a literal fitting two (E0457);
  - a block returning another type used as `T: Mul<f64>` (E0422);
  - a number-on-the-left block in a third module (E0459);
  - `impl f64 { … }` (E0466);
  - `s * 2.0` on an interface value (E0404);
  - a misplaced default (E0201).
- [x] Unit tests for the parser's defaults, the checker's choice and placement, and codegen's call.
- [x] `cargo test --lib`, `--test programs`, `--test spec`, `--test explain`, and the full suite, since this changes what type-checks.

## ACCEPTANCE
- [x] The OBJECTIVE prints the lines shown; each commented line gives its error.
- [x] Every migrated program and spec example passes with unchanged output.
- [x] The full `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [x] Every `pub` item documented; spec, CHANGELOG, TODO updated.

## CHANGES DURING THE BUILD
- **A bound gives an operator only when it is an operator interface** (or `Num`/`Float`): `fun thrice<T: Scales>(s: T): T = s * 3.0`, where `Scales` has its own `mul`, is E0404; it was name-based. `T: Scales + Mul<f64>` gives it. The operator calls through that bound, never another bound's item of the same name.
- **`Mul.mul(v, x)`** chooses its block by `x` too, like `v * x` and `v.mul(x)`.
- **Codegen picks a built-in type's block by its arguments** (`Derives::find_at`, `target_at`), and a built-in type uses a block only where the checker chose one: a `T: Add` instance at `string` keeps the built-in `+`.
- **Checking a long chain** `a + b + c + …` stays linear: a number-on-the-left check probes the right operand first and the left one only when some block takes that right-hand type (the first version probed every prefix, exponentially, and hung `runtime/small_frames`).
- **E0422 for a block giving another type** says so ("its `impl Mul<Meters>` gives Area, not Meters"), instead of asking for a block that exists.
- **`check/derive.rs`** passed 500 lines; its pattern matching moved to `check/derive_match.rs`.
- **The migration** was mechanical: each inherent operator method became its own block (10 test programs, 2 spec examples, 2 error pages); two programs changed by hand (an interface value's `s * 2.0` became `s.mul(2.0)`; a bound became `T: Scales + Mul<f64>`). Outputs unchanged.

