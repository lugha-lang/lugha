## FEATURE: `extern struct`s by value on Windows — milestone 271

## OBJECTIVE

A program built for Windows passes an `extern struct` to and from C by value, as on Linux and macOS, following Windows x64's rules the way clang and MSVC do. E0337 is retired.

```lugha
extern struct Point { x: f64, y: f64 }
extern struct Rgba { r: u8, g: u8, b: u8, a: u8 }
extern fun length(p: Point): f64;     // today E0337 for Windows; now builds
extern fun tint(c: Rgba): Rgba;

fun main(console: Console) {
    console.println(length(Point { x: 3.0, y: 4.0 }));       // 5
    console.println(tint(Rgba { r: 1, g: 2, b: 3, a: 4 }).a);
}
```

## CONTEXT

- **Read first:**
  - `src/codegen/c_abi_arm.rs`, the model: a classifier plus `c_sig_arm`, over the shared `Arg`/`Ret` of `c_calls.rs`;
  - `src/codegen/c_abi_win.rs`, the refusal this replaces;
  - `tests/common/c_shapes.rs`, the programs every ABI runs;
  - the spec's extern-struct paragraph and E0337's row.
- **Windows x64's rules** (Microsoft's "x64 calling convention", the same for MinGW, which zig uses):
  - an argument of exactly 1, 2, 4 or 8 bytes travels as one integer of that width, in a general register or its stack slot, even if its fields are floats;
  - any other size travels as a pointer to a copy the caller makes (no `byval`);
  - a result of 1, 2, 4 or 8 bytes comes back in `rax`; any other comes back through a hidden pointer the caller passes first (`sret`).
- **What exists:** `Arg::Split`, `Arg::Indirect` and `Ret::Memory` already do all three for AArch64, entries C calls (callbacks) included. Only the classification is new.
- **Follows:** PRP-297 (milestone 269).

## REQUIREMENTS

### Must Do

- **`src/codegen/c_abi_win.rs`** becomes the Windows lowering:
  - `classify_win(ty)`: `Int(bits)` for a size of 1, 2, 4 or 8 bytes, else `Indirect`;
  - `c_sig_win`, chosen in `c_sig` when the target is Windows, as `c_sig_arm` is for AArch64.
- **Everywhere a struct crosses:** an `extern`'s parameters and result, and a callback's (C calling Lugha) parameters and result.
- **E0337 retired:**
  - `struct_refusals` and its call in `src/driver/pipeline.rs` go;
  - `docs/errors/E0337.md` becomes a "Retired in v18" page, as E0464's is: the code is never reported and never reused;
  - the spec's E0337 row says it's retired.
- **Docs:**
  - the spec's extern-struct paragraph gains Windows' rules;
  - §11 row 271, CHANGELOG, TODO (D1–D3 loses "lifting E0337");
  - CLAUDE.md.

### Must NOT Do

- **No change on Linux or macOS:** their IR stays byte for byte the same (the existing IR tests hold).
- **No `__vectorcall`, no other calling convention:** a C function is the platform's default convention, as everywhere.

## DESIGN

- **Files:** `src/codegen/c_abi_win.rs` (≈150 lines with its unit tests, from 59). `c_calls.rs` gains a three-line branch.
- **No new crate, no `unsafe`.**
- **Pieces:** a 1-, 2-, 4- or 8-byte struct is `i8`, `i16`, `i32` or `i64`, read from and written to the struct's memory as one piece, as `Split` already does, so nothing past its own bytes is touched.

## TESTS

- [ ] **Unit (`c_abi_win.rs`):**
  - sizes 1, 2, 4, 8 → `Int(8)`, `Int(16)`, `Int(32)`, `Int(64)`, floats included (`{f32, f32}` → `i64`, `{f64}` → `i64`);
  - sizes 3, 12, 16, 24 → `Indirect` (`{u8, u8, u8}`, `{f32, f32, f32}`, `Point`, three `i64`s);
  - an IR test: `declare i64 @len(i64)` for `V2`, `declare i64 @big(ptr)` with no `byval`, `declare void @make(ptr sret(`, `declare i32 @tint(i32)`;
  - Linux's and the ARM IR tests unchanged.
- [ ] **End to end (`tests/windows.rs`):** every program in `tests/common/c_shapes.rs` (every shape, large structs and full registers, callbacks) built for Windows with zig compiling `side.c`, and run under Wine. The C side computes every result, so a wrong rule shows as a wrong number. Wine runs the real x86-64 code, so the calling convention is checked exactly as on Windows.
- [ ] **E0337's test** (`a_struct_passed_to_c_by_value_is_e0337_for_windows_only`) becomes a check that the same program builds for Windows.
- [ ] `tests/explain.rs` still passes with E0337's page retired.

## ACCEPTANCE

- [ ] The c_shapes programs print the same under Wine as on Linux.
- [ ] `cargo fmt --check`, clippy, `--lib`, `--test windows`, `--test extern_structs`, `--test arm` and `--test explain` are clean.

## DECIDED HERE (for review)

1. **Wine is enough to check it.** The calling convention belongs to the compiled code, not to the OS, so Wine running zig's C and our code checks it exactly. CI's Windows job adds no case of its own.
2. **Floats in a 1-, 2-, 4- or 8-byte struct go as an integer,** as Windows says. `{f32, f32}` is one `i64`, not two float registers as on System V.
3. **A big argument is a plain pointer to a copy,** reusing AArch64's `Arg::Indirect`. The copy is the caller's, so C may change it without Lugha's value changing.
4. **E0337 is retired, not removed:** its page stays, saying it's never reported, so an old message still explains itself.

## MADE WHILE BUILDING (for review)

5. **The shared C side uses `long long`, not `long`** (`tests/common/c_shapes.rs`). `long` is 32 bits on Windows and 64 on Linux and macOS, so the same `i64` declarations would have been wrong there. `long long` is 64 bits everywhere, so Linux's and ARM's runs are unchanged.
6. **The test runs each shape at -O0 and -O2,** as the ARM test does.
7. **The suite builder no longer knows E0337,** and the spec's `--target` row is corrected: it still said lockdown wasn't applied on Windows.
