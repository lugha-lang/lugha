## FEATURE: Four small rules from the v18 spec review: `debug`'s release error covers the program's own code only (CAP-09a), one loop rule for arrays, maps and sets (IMP-07), `\u{…}` in strings (LEX-01), and `Dir` methods with nothing to return give `Result<(), string>` (CAP-18); milestone 224.

**Status:** approved 2026-11-20 (the user's "do the ledger's decisions in one swoop": built back to back, merged once local checks pass).

## OBJECTIVE
```
// a dependency's lib.la — the program's -O2 build no longer fails here
public fun parse(s: string): i64 { debug(s); s.len }   // prints nothing at -O2

let mut xs = [1, 2, 3];
for x of xs { xs.push(x * 10); }    // was E0507; runs 3 times, xs = [1, 2, 3, 10, 20, 30]

console.println("tab\u{9}bell\u{7}smile\u{1F600}");

dir.make_dir("out")?;               // Result<(), string>, was Result<i64, string> with Ok(0)
```

## CONTEXT
- **Ledger:** CAP-09a, IMP-07, LEX-01 and CAP-18 (2026-11-20).
- **Spec:**
  - §5 "for over an array" ("The body may not assign to `xs`…");
  - `debug` and E0465;
  - §2 string escapes;
  - §6 `Dir`'s methods.
- **Code:**
  - `driver/pipeline.rs` `build` (E0465);
  - `codegen/runtime.rs` (`debug`), `codegen/lower.rs` (`Lowerer`), `codegen/emit.rs`;
  - `check/array.rs` `for_of` (the guard; `for … of mut` keeps its own in `check/for_mut.rs`);
  - `lexer/string.rs` `escape`, `lexer/char_lit.rs` `unicode_escape`;
  - `prelude/capabilities.la`, `prelude/capabilities_real.la`.
- **Safety of IMP-07:** the array loop already holds the array (`rc_keep` in `codegen/array.rs`), so a write in the body copies it and the loop goes on over what it started with.

## DESIGN
1. **CAP-09a.**
   - E0465 is reported only for `debug` calls in the program's own package: the main file's package, its modules included.
   - Codegen learns whether it builds a release (`-O2`). There, `debug(x)` compiles to `x`, printing nothing; only calls left in dependencies reach codegen.
2. **IMP-07.**
   - A plain `for x of xs` no longer guards `xs`: the body may assign to it, push to it or pass it as `mut`, and the loop goes through the array as it was when it started.
   - `for x of mut xs` keeps E0507, since it writes each element back.
3. **LEX-01.** A string takes `\u{H…}` (one to six hex digits, a Unicode scalar value), with the same errors as a char's, as E0103.
4. **CAP-18.**
   - `make_dir`, `make_dirs`, `remove_file` and `remove_dir` give `Result<(), string>`.
   - `write_bytes`, `append_bytes`, `write_file` and `append_file` keep `Result<i64, string>`, which is the number of bytes written.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **"The program's own package"** is the main file's package (`None` or the package its manifest names), so a program's own modules count and its dependencies don't.
2. **The release flag rides with the lowering settings,** not `Instruments`, which is documented as never for shipping.
3. **The bytes-written results stay:** they are values, not placeholders.

## REQUIREMENTS
### Must Do
- Design 1–4.
- Spec §2 escapes, §5 loops, the `debug` paragraph, §6 `Dir`, §11 row 224, and a "Changes since v17" bullet marked Breaking for CAP-18.
- E0507's page notes that a plain loop no longer guards; CHANGELOG.
### Must NOT Do
- Change `for … of mut`, or `x.debug()`.

## TESTS
- [ ] `--lib`: a string's `\u{41}`, `\u{1F600}` and its errors; a plain loop pushing to its array checks; `of mut` still E0507.
- [ ] `tests/programs/iterators/loop_rule/`: a loop pushing to its own array (stdout).
- [ ] An end-to-end test of a dependency's `debug` at `-O2` building and printing nothing, and the program's own still E0465 (the `debug` tests' file).
- [ ] Targeted: `--lib`, `programs`, `spec`, `explain`, `comments`, the capability, `Dir` and `debug` test files.

## ACCEPTANCE
- [ ] A dependency's stray `debug` never blocks a release build.
- [ ] Arrays, maps and sets loop by one rule.
- [ ] Strings and chars take the same escapes.
- [ ] No `Dir` method returns a meaningless `Ok(0)`.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
