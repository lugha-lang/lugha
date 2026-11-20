## FEATURE: Warning W0108 when a type's `==` and `<` can disagree: it derives `Eq` (`with Eq`, `with Key`) but writes `Ord` by hand, so `a == b` may be false while neither `a < b` nor `b < a`; milestone 236.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
struct Run { loss: f64, name: string } with Eq

impl Ord for Run {
    fun cmp(self, o: Run): Ordering = … compares loss only …
}
```
```
warning[W0108]: `Run`'s `==` and `<` can disagree
  --> main.la:1:8
   |
 1 | struct Run { loss: f64, name: string } with Eq
   |        ^^^ `==` compares every field (`with Eq`)
 3 | impl Ord for Run {
   |      --- `<` compares what this `cmp` compares
   |
   = help: write `impl Eq for Run` comparing what `cmp` compares, so `a == b` exactly when `a.cmp(b)` is `Ordering.Equal`; or derive both, `with Ord`
```

## CONTEXT
- **Asked for by:** the user (2026-11-20), from five readers' review of §10, where two asked for a warning about the Ordering example's `Run`.
- **Spec:**
  - §4's `Eq` and `Ord`: `Ord: Eq`, `with Ord` implies `with Eq`, and a hand-written `impl Ord`;
  - the Ordering example in §10;
  - the warnings table in §9.
- **Code:**
  - `check/derive_with.rs` (what `with` derives);
  - `check/impls.rs` (hand-written blocks);
  - the warnings' builders in `check/errors/`.
- **Decisions:** none open.

## DESIGN
1. **When it fires:** a struct or enum whose `Eq` is derived (`with Eq`, or `with Key`, which brings `Eq`) and whose `Ord` is hand-written (`impl Ord for T { fun cmp … }`), in the same program. One warning per type, at the type's name, with a label at the `impl Ord`.
2. **When it doesn't:**
   - both derived (`with Ord`): they agree by construction;
   - both hand-written: the author wrote both and the warning can't judge them;
   - `Ord` alone: `Ord: Eq` makes that E0422 already.
3. **Code:** the next warning code, **W0108**, with a page in `docs/errors/W0108.md`. It is a warning, not an error: the program builds, and `lughac check` shows it.
4. **The §10 Ordering example** stops teaching the pattern: `Run` gets an `impl Eq` comparing `loss`, as `cmp` does. Its output is unchanged.
5. **The skill:** structs-and-enums and interfaces mention it where `with Eq` and `impl Ord` meet.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Only derived `Eq` with a hand-written `Ord`:** the one combination where disagreement is likely and silent. Comparing the two bodies is undecidable, so two hand-written blocks get no warning.
2. **A warning, not an error:** a type may deliberately order by one key while comparing all fields (a priority queue entry). The help shows how to make them agree, and nothing stops the program.
3. **No way to silence it per type for now:** Lugha has no warning attributes yet. If one is needed, that's a separate decision.

## REQUIREMENTS
### Must Do
- Design 1–5.
- Spec: §4 (the `Ord` paragraph), §9's warning table, §11 row 236, a "Changes since v17" bullet.
- CHANGELOG.
### Must NOT Do
- Change what `==` or `<` do.
- Warn for types the program didn't declare.

## TESTS
- [x] `--lib`:
  - [x] W0108 for `with Eq` plus `impl Ord`, and for `with Key` plus `impl Ord`;
  - [x] none for `with Ord`, for `impl Eq` plus `impl Ord`, or for `with Eq` alone.
- [x] `tests/programs/derive/…/w0108.la` with its `.stderr`.
- [x] `explain`: W0108's page and its example.
- [x] Targeted: `--lib`, `programs`, `spec`, `explain`, `skill`, `comments`.

## ACCEPTANCE
- [x] The Ordering example in §10 no longer shows a type whose `==` and `<` disagree.
- [x] `cargo fmt --check` and clippy clean; the targeted tests pass.
