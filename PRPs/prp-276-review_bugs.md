## FEATURE: Fixes from a reader's review: `with Key` or `with Ord` beside a hand-written `Eq` is E0463, `sum` works on a `T: Num` iterator, an enum's associated function is a value, a tuple's missing case prints as a tuple, and `lughac check` takes a file with `main` as `run` does; the spec agrees with itself on a derive block's items as methods.

**Status:** approved 2026-11-21.

## OBJECTIVE
```
struct P { a: i64, b: i64 } with Key
impl Eq for P { fun eq(self, other: Self): bool = self.a == other.a; }
```
```
error[E0463]: `P` derives `Eq` through `with Key`, and implements it by hand
```
Today this compiles, and `assert_key_laws(P { a: 1, b: 2 }, P { a: 1, b: 3 })` panics: the derived hash covers `b`, the written `eq` doesn't.

```
struct Bag<T> { xs: T[] }
impl<T: Num> Bag<T> { fun total(self): T = self.xs.iter().sum(); }   // today E0415

enum E { A, B }
impl E { fun make(n: i64): E = E.A; }
let g = E.make;                                                       // today E0415 "no variant"

match (true, false) { (true, _) => …, (false, true) => … }
// today: `.()(false, false)` not covered; now: `(false, false)` not covered
```
And with `src/main.la` holding `import demo.greeting;` in package `demo`, `lughac run src/main.la` works but `lughac check src/main.la` is E0312 "module `demo.demo.greeting` not found".

## CONTEXT
- **The user's request** (2026-11-21): a reader's review of 22 points, checked one by one; the spec and skill points and four help texts were fixed already. This PRP takes the compiler points: 1, 2, 18, 20, 21, and 22 as it reproduces now.
- **Spec:**
  - §4, line 671: "`with Key` implies `with Eq`"; line 782: "`with Ord` (which implies `with Eq`)"; line 796: "deriving and writing one interface for one type is E0463".
  - §4, line 794 (milestone 169): a block for a built-in type "gives `x.item()` … The prelude's derive blocks count the same way: `3.cmp(4)`, `"a".eq("b")`". Line 796 (milestones 141–142): a block's items "never become methods of `X` (`5.show()` stays E0415)". These contradict each other; the compiler follows 794.
  - §4 iterators, line 911: `it.sum()` "for a number type `T` only".
  - §5, lines 1143 and 1769: 1143 says `lughac check` builds any file in a package's `source` folder as that package's module; 1769 says only a file without `fun main` is checked as a library.
- **Code:**
  - `src/check/derive_with.rs`: `check_one` finds a hand-written impl of the derived key only, not of what the key implies.
  - `src/check/iter_helpers.rs`: `"sum" if element.is_numeric()`; the prelude's `iter_sum` is already `<T: Num>`.
  - `src/check/construct.rs`: `variant_value` reports "no variant" before `function_field` (`fun_values.rs`, which handles enums) is tried.
  - `src/check/usefulness.rs`: `show` prints a tuple's hidden constructor (`()`, from `tuples.rs`) as a variant.
  - The driver's `check`: a file in the `source` folder is taken as a package module even with `main`, so its imports are package-relative.
- **Security:** none of CLAUDE.md's list.
- **Decisions:** none open.

## DESIGN
1. **E0463 for a derived `Eq` beside a written one.** `with Key` and `with Ord` derive `Eq` too; a hand-written `impl Eq` beside either is E0463: "`P` derives `Eq` through `with Key`, and implements it by hand", labelled at the `impl`, with help "keep one: write `impl Key for P` by hand too, or drop the `impl Eq` block". A user interface that extends another implies nothing (that is E0451's "missing extended" case, unchanged).
2. **`sum` on a `T: Num` iterator.** An element that is a type parameter bounded by `Num` or `Float` takes the same `iter_sum`; each instance is concrete. Anything else stays E0415.
3. **An enum's associated function is a value.** `E.make` with no variant `make` and an associated function `make` is that function's value, as `S.make` is; a variant of that name still wins.
4. **A tuple's missing case prints as a tuple:** `(false, false)`, nested ones too (`.P((true, _))`).
5. **`lughac check` of a file with `fun main`** inside a package's `source` folder takes it as a main file, as `build` and `run` do (line 1769), so `import demo.greeting;` resolves the same in all three. A file without `main` is still checked as the package's module (line 1143, reworded).
6. **E0312's help for a package's own name inside it.** A module of package `demo` importing `demo.greeting` (found nowhere, as `demo.demo.greeting`) gets help: "inside package `demo`, import its modules without its name: `import greeting;`".
7. **Spec, line 796:** its parenthetical changes to agree with line 794: a block for one written type (`derive Desc for i64`, the prelude's `derive Show for i64`) gives `x.item()` like an `impl` block; a block covering every struct, enum or tuple doesn't make its items methods of those types (`p.describe()` on a struct stays E0415).

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Review point 2 is a spec fix, not a compiler change.** Line 794 is the newer rule, names `3.cmp(4)` as working, and programs may use it; line 796's parenthetical predates it.
2. **`with Ord` gets the same E0463 as `with Key`:** the spec says both imply `with Eq`, and a derived order beside a written `eq` disagrees the same way (W0108 already warns of the reverse).
3. **`check` follows `run` for a file with `main`** rather than `run` following `check`: the spec's own command table says so, and nothing imports a main file, so the package-module reading buys it nothing.
4. **No new error codes:** E0463, E0415, E0417 and E0312 keep their meanings.
5. **Review point 19 (closure typing in `and_then`) isn't included:** it no longer reproduces.

## REQUIREMENTS
### Must Do
- Design 1–7.
- **Spec:** line 796's parenthetical; line 1143's wording (a file without `main`); a Changes bullet marked **Breaking** for design 1.
- **Elsewhere:** CHANGELOG (Fixed, and Breaking for design 1).
### Must NOT Do
- Change what derive blocks do for built-in types.
- Change how a package's modules import each other.

## TESTS
- [ ] **`tests/programs/`:**
  - [ ] `interfaces/eq_key/e0463_key.la` and `interfaces/ord/e0463_ord.la` (rejected);
  - [ ] `iterators/…/sum_generic.la`: `sum` inside `impl<T: Num>` for `i64` and `f64`;
  - [ ] `types/enums/…/assoc_value.la`: `E.make` as a value, called, passed; a variant beside a function of another name;
  - [ ] `patterns/…/e0417_tuple.la` (rejected, the stderr showing `(false, false)`).
- [ ] **`--lib`:** usefulness's `show` for a tuple and a nested tuple; E0312's help naming the package.
- [ ] **`tests/check_cli.rs` or the driver's tests:** `check` of `src/main.la` importing `demo.greeting` passes, as `run` does; a file without `main` is still a module.
- [ ] **Targeted:** `--lib`, `programs`, `spec`, `skill`, the driver's `check` tests, `comments`.

## ACCEPTANCE
- [ ] Each objective behaves as shown.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
