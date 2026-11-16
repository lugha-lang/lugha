## FEATURE: Explicit type arguments at uses (`max<i64>(a, b)`, `Chan<Job>.new(16)`), and `as` only where nothing is lost, with `try_into`, `truncate`, `saturate`, `round`, `floor` and `ceil` for the rest; milestone 177, in v18's Phase 1.

**Status:** proposed 2026-11-16. Approved 2026-11-16.

## OBJECTIVE
```lugha
struct Pair<T> { a: T, b: T }

fun pick<T>(xs: T[]): Option<T> = if xs.len > 0 { Some(xs[0]) } else { None };
fun bigger<T: Ord>(a: T, b: T): T = if a > b { a } else { b };

fun main() {
    // Type arguments, written where inference can't or shouldn't decide.
    let none = pick<string>([]);
    let first = bigger<i64>;                      // a function value
    let p = Pair<u8> { a: 1, b: 2 };
    let o = Option<i64>.None;
    let words = [1, 2].map<string>(fun(n) = "{n}");
    println(first(3, 4));                         // 4

    // `as` converts only where no information can be lost.
    let small: u8 = 200;
    let wide = small as i64;                      // fine: widening
    let f = small as f64;                         // fine: exact
    let big: i64 = 300;
    let b = big as u8;                            // E0469: `i64` to `u8` can lose information

    // The losses, named.
    println(big.truncate<u8>());                  // 44: the low bits
    println(big.saturate<u8>());                  // 255: clamped
    println(big.try_into<u8>().debug());          // None
    println(2.5.round<i64>());                    // 3: halves away from zero
    println((-2.5).floor<i64>());                 // -3
    println(9_007_199_254_740_993.round<f64>());  // 9007199254740992.0
    let huge = 1.0e300.round<i64>();              // panics: conversion out of range
}
```

## CONTEXT
- Draft v18 §1.5 [TYA-01..07]: type arguments may be written at a use, optional; `_` leaves one to inference; on generic calls, function values, associated functions of generic types, a method's own parameters, struct (and collection) literals and variants; the context rule for `<`; `Stack::<i64>` stays E0207; errors E0418 (count), E0419 (bound), E0403 (contradiction).
- Draft v18 §1.4 rules 3–5 [IMP-03, OPN-03]: `as` converts only where no information can be lost; `x.try_into<u8>()` gives an `Option`; lossy conversions name the loss, `truncate<u8>()`, `saturate<u8>()`, `round<i64>()` and its siblings.
- Decided with the user: both in one PRP (the conversions are spelled with type arguments).
- Today: `<` after a name is always a comparison; `as` converts every numeric pair (spec §4 "Casts"), and the CLAUDE.md known issue says `as` truncates or saturates silently.
- Measured: about 200 `as` casts in `.la` files (most `as u8`, `as i64`, `as f64`), plus the spec and Rust tests' programs; three generic casts (`n as T`).
- Code: `parser/expr.rs` (postfix), `parser/primary.rs`, `ast/expr.rs`; `check/infer.rs` (`infer` already takes pre-bound arguments), `check/call.rs`, `check/fun_values.rs`, `check/construct.rs`, `check/struct_literal.rs`, `check/arguments.rs`, `check/ops.rs` (casts), `check/numbers.rs`; `codegen/cast.rs`; `formatter/`.

## DECISIONS MADE IN DISCOVERY (mine, for review)

### Type arguments
1. **Syntax:** `name<T, …>` on a name or a member name, under the draft's context rule: `<` begins type arguments when a list of types closed by `>` is followed at once by `(`, `.`, `{`, `)`, `,`, `;`, `]`, `}` or the end of the input; otherwise it is a comparison, as today. The parser tries the type list without reporting, and goes back if the rule fails. `_` stands for one argument left to inference.
2. **Where they go:** generic function calls and values (`max<i64>(a, b)`, `max<i64>`), a method's own type parameters (`xs.map<string>(f)`, also the conversion methods), associated functions of generic types (`Chan<Job>.new(16)`, `Stack<i64>.new()`), struct literals (`Pair<u8> { … }`, `m.Pair<u8> { … }`), and variants (`Option<i64>.None`, `Shape<T>.Circle(r)`). **Collection literals** (`Map<string, i64> {}`) come with the map literals PRP, which doesn't exist yet.
3. **Meaning:** the written arguments bind those parameters before inference runs; `_` and missing inference work as today. All or none: a list's length must be the parameter count (E0418, `` `max` takes 1 type argument but 2 were given ``), so `f<i64>` on `fun f<A, B>` is E0418; write `f<i64, _>`. A bound not met is E0419; an argument the code contradicts is E0403 at that argument, as if the parameter had been inferred.
4. **Where they can't go:** on something without type parameters (`println<i64>`, a non-generic struct), it's E0418 "takes 0 type arguments". On a local holding a function value, the same.
5. **E0412's help** now suggests writing the arguments: "write them: `pick<string>(…)`".
6. **`Stack::<i64>`** stays E0207; its help becomes `Stack<i64>`.
7. **Storage:** an `Expr` gets an optional list of written type arguments, so every existing match on names and fields keeps working, and codegen never sees them (it reads the instances the checker records).

### Lossless `as`
8. **What `as` still converts** (nothing lost, every value round-trips):
   - a type to itself; integers to integers whose range holds the source's (`u8 as i16`, `i32 as i64`, `u32 as u64`; never signed to unsigned);
   - integers to floats that hold every value exactly: `i8`, `i16`, `u8`, `u16` to `f32`; any integer up to 32 bits to `f64`;
   - `f32 as f64`;
   - `char` to `u32`, `i32`, `i64`, `u64`; `u8 as char`;
   - a type parameter bounded by `Num` or `Float` when the cast is lossless for every type the bound admits (`T: Float` to `f64`; `f32` to `T: Float`). `n as T` for `T: Num` isn't.
9. **Everything else that was a cast** is a new error **E0469**, "`as` can lose information: `i64` to `u8`", whose help names the methods that fit the pair (`truncate`, `saturate`, `try_into` for integers; `saturate`, `round`, `floor`, `ceil`, `try_into` from a float; `round`, `try_into` to a float). `bool` and other non-numbers stay E0408.
10. **The methods**, on every number (and type parameters bounded by `Num` or `Float`), each taking the target as a type argument (any number, or a parameter so bounded):
    - `try_into<U>(): Option<U>`: `Some` exactly when `U` holds the value exactly (a float to an integer must be whole and in range; an integer to a float must be representable). Also `n.try_into<char>()` for any integer, the replacement for `n as char`, beside `char_from`.
    - `truncate<U>()`: integers to integers only; keeps the low bits (two's complement), as `as` did. A float or a `T: Num` is E0469 with help.
    - `saturate<U>()`: the nearest value of `U` within its range. An integer clamps; a float to an integer rounds toward zero first and NaN gives 0 (what `as` did); to a float, the nearest value, finite values clamped to the finite range.
    - `round<U>()`: the nearest value of `U`; from a float to an integer, halves away from zero (like `math.round`). Out of range, or NaN to an integer, **panics** with "conversion out of range", like integer overflow; `saturate` is the clamping form. From an integer to an integer it is exact or panics. This is how `i64` reaches `f64` (`n.round<f64>()`) and `f64` reaches `f32`.
    - `floor<U>()`, `ceil<U>()`: from a float, rounding down or up, then as `round`. Only float receivers.
11. **Migration** keeps today's behaviour exactly: integer narrowing becomes `truncate`, float to integer `saturate`, a lossy integer to float or `f64` to `f32` `round`, `n as char` `try_into<char>().unwrap_or('\u{FFFD}')` where the value may be invalid (by hand: nine sites). Driven by E0469's spans, as the pattern migration was.
12. **Codes:** E0469 (new); E0418, E0419, E0412, E0207 gain uses or help. **Breaking:** lossy casts, and `<` after a name followed by a closing `>` and one of the listed tokens (`f(a < b, c > (d))`), which is now type arguments.

## REQUIREMENTS

### Must Do
- Parser: type arguments by the context rule on names, member names and struct literal names; `_`.
- Checker: each place in decision 2 binds the written arguments; E0418/E0419/E0403 per decision 3; the `as` table; E0469; the six methods.
- Codegen: the six methods (and `try_into<char>`), panics with the call's place.
- Formatter: prints written type arguments.
- Docs: E0469's page; E0418, E0412, E0207, E0408 pages; v18 spec (grammar, §4 casts and inference, the method table, the error table, §11 row 177, "Changes since v17", Breaking); CHANGELOG; CLAUDE.md's known issue about `as`.

### Must NOT Do
- Map and set literals, so no `Map<string, i64> {}`.
- Type arguments in patterns.
- `From`/`Into`-style conversion interfaces.

## TESTS
- [x] `tests/programs/generics/type_arguments/`: every form in decision 2 runs; E0418 (count, non-generic), E0419, E0403 at an argument; `f(a < b, c > d)` stays comparisons.
- [x] `tests/programs/numbers/conversions/`: the `as` table (allowed casts run), E0469 per kind, every method on edge values (`-1.truncate<u8>()`, `NaN.saturate<i64>()`, `2.5.round<i64>()`, `-2.5.round<i64>()`, `u64.MAX.round<f64>()`, `try_into` both ways), the `round` panic as a `.stderr`, generic receivers and targets.
- [x] Parser unit tests for the context rule; checker unit tests per decision.
- [x] The full `cargo test`.

## ACCEPTANCE
- [x] The OBJECTIVE gives E0469 at `big as u8` only, and once that line goes prints `4`, `44`, `255`, `None`, `3`, `-3`, `9007199254740992.0` and panics at the last line.
- [x] The full `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [x] Spec, CHANGELOG, CLAUDE.md and the error pages updated.

## CHANGES DURING THE BUILD
- **`check/mod.rs` split** (agreed with the user): `Checker::new` moved to `check/new.rs`, so the struct could take the field recording which written type arguments a use took.
- **Literals and `as`:** a number literal takes the target type when it fits (`0 as u64`, `200 as u8`; `300 as u8` is E0402), and an integer literal a float holds exactly becomes one (`1 as f64`), so no migration rewrote a literal into a method.
- **The target may come from the expected type:** `let b: u8 = n.truncate();`; neither is E0412 with help to write it.
- **`Option<T>.None` and `Option<T>.Some(x)`** are new forms (the draft's `Option<i64>.None` needed them); `Ref<T>.new(v)` and `Chan<T>.new()` take written arguments too.
- **`Stack::<i64>`** is E0207 with the help "write the type arguments right after the name: `Stack<…>`".
- **Generic programs:** `n as T` became `n.round<T>()` (exact or a panic) or `x.saturate<U>()`; `numbers_as_t`'s `narrow(300)` now prints 255 (clamped) instead of 44.
- **The objective** uses its own `bigger<T>`: the prelude has no generic `max` function.
