## FEATURE: Sizes in bytes: `8.kb()`, `64.mb()`, `1.gb()` on integers, in powers of 1024; milestone 174, in v18's Phase 1.

**Status:** proposed 2026-11-16. Approved 2026-11-16.

## OBJECTIVE
```lugha
fun main() {
    println(8.kb());                     // 8192
    println(64.mb());                    // 67108864
    println(1.gb());                     // 1073741824
    let pages: u32 = 3;
    println((pages * 4).kb());           // 12288, a u32
    let huge = 9_000_000_000.gb();       // panic: integer overflow, as `*` would
}
```

## CONTEXT
- Draft v18 §1.7 rule 9 [PRE-08]: "Sizes are written with prelude methods on integers that return bytes: `8.kb()`, `64.mb()`, `1.gb()`." Used later by scope headers (MEM-11 `scope s memory 64.mb()`, STK-05 `scope s stack 64.mb()`), which aren't built yet.
- Decided with the user: powers of 1024, under the short names.
- Built on PRP-204: the prelude's blocks of a built-in type's own methods (`impl string { … }`), here `impl i64 { … }` and the other integer types.
- Code: `src/prelude/` (a new block per integer type).

## DECISIONS MADE IN DISCOVERY (mine, for review; the base is the user's)
1. **`kb()`, `mb()` and `gb()`** on every integer type: `n * 1024`, `n * 1024 * 1024` and `n * 1024 * 1024 * 1024`. Each returns the receiver's own type, so `x.kb()` with `x: u32` is a `u32`.
2. **Overflow panics,** as any integer `*` does (`integer overflow`, at the multiplication in the prelude): `9_000_000_000.gb()` doesn't fit an `i64`. `kb`, `mb` and `gb` aren't `@caller_location`, since the compiler's own panics keep their place there.
   - *Alternative left out:* marking them with an explicit check and `panic` would report the caller's line. The prelude's line is what `x * 1024` would show too.
3. **No `tb()`,** no float receivers and no `kib`-style names: the draft names three and the user chose the base. More can come with the scope headers that use them.
4. **Codes:** none.

## REQUIREMENTS

### Must Do
- Prelude: an `impl` block per integer type with `kb`, `mb` and `gb`, each documented.
- Docs: v18 spec (§5 integer methods table, §11 row 174, "Changes since v17"); CHANGELOG.

### Must NOT Do
- Scope headers (`memory`, `stack`): their own PRPs.
- Sizes as a type (`Bytes`): a plain integer, as the draft says.

## TESTS
- [x] `tests/programs/numbers/sizes/`: the OBJECTIVE (its overflow panic as a `.stderr`), every integer type once, and chains (`(n * 4).kb()`).
- [x] Unit test: the result type follows the receiver.
- [x] `cargo test --lib`, `--test programs`, `--test spec`.

## ACCEPTANCE
- [x] The OBJECTIVE prints and panics as shown.
- [x] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [x] Spec and CHANGELOG updated.

## CHANGES DURING THE BUILD
- **Not on `i8` or `u8`:** `1024` doesn't fit either, so even `1.kb()` couldn't exist; `n.kb()` there is E0415. Decision 1's "every integer type" is the six from `i16` and `u16` up.

