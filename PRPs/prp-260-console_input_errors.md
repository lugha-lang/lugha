## FEATURE: `Console.read_line()` returns `Result<Option<string>, string>` like `File.read_line`: a line that isn't UTF-8 is an `Err` (and is skipped), a read error is an `Err` rather than the end of input; `read_bytes()` returns `Result<u8[], string>` likewise; milestone 228.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
fun main(console: Console): i32 {
    loop {
        match console.read_line() {
            .Ok(.Some(line)) => console.println(line),
            .Ok(.None) => return 0,                          // end of input
            .Err(e) => { console.eprintln(e); return 1; }   // "line 3 is not valid UTF-8", or "stdin: Input/output error"
        }
    }
}
```
```
let line = console.read_line()?.unwrap_or("");   // in a function returning Result<_, string>
let all = console.read_bytes()?;                  // Err on a read error, not []
```
(Today `read_line` returns invalid bytes inside a `string`, and both calls report a read error the same way as the end of input.)

## CONTEXT
- **Found by:** the v18 spec review (2026-11-20).
- **Spec:**
  - §6 `Console` (L1551);
  - §6 `File`'s `read_line` (L1380), the shape this PRP copies;
  - `string_from_utf8` (L1203): "the only way from bytes to a `string`";
  - the runtime table (L1904, L1906).
- **Code:**
  - `runtime/input.c` `lugha_rt_read_line` (`getline` on a helper; -1 is both EOF and error) and `lugha_rt_read_stdin_bytes`;
  - `src/prelude/capabilities.la` (the `Console` interface), `capabilities_real.la` (`$Console`, and `$File.read_line`/`text`, the model), `narrow.la` (`OneStream`);
  - `src/check/input.rs` and `src/codegen/function.rs` (the `read_line` intrinsic's type and call);
  - `src/check/float_ops.rs` (the `$os` built-ins' signatures).
- **Why it matters:**
  - A `string` holding invalid UTF-8 breaks what every other source of strings promises. Code that checks or escapes text before passing it on (to a terminal, a file name, JSON or HTML) can then be given bytes it never expected.
  - A failed read that looks like the end of input makes a program act on partial input as if it were complete.
- **Security:** this fixes how untrusted input is validated (CLAUDE.md: never trust external input). No `unsafe`, and no new dependency. It is not on the explicit review list, but I'd like you to review it anyway.
- **Decisions:** none open.

## DESIGN
1. **`Console.read_line(): Result<Option<string>, string>`**, with the same rules as `File.read_line`:
   - The line comes back without its `\n` and one `\r` before it. A last line without `\n` is still a line.
   - `Ok(None)` means the end of input.
   - A line that isn't UTF-8 is `Err("line N is not valid UTF-8")`. N counts every line `read_line` has read from stdin, and the bad line is skipped, so the next call reads the line after it.
   - A read error is `Err("stdin: <the system's text>")`.
   - Stdout is still flushed first, so a prompt shows.
2. **`Console.read_bytes(): Result<u8[], string>`:**
   - It gives everything left on stdin, `Ok([])` at the end of input.
   - A read error is `Err("stdin: <the system's text>")`.
3. **Who decides what UTF-8 is:**
   - The runtime only hands over bytes: the prelude-only `read_line` intrinsic now gives `Result<Option<(i64, u8[])>, string>`, the line's number and its bytes.
   - The prelude turns the bytes into a `string` with `string_from_utf8`, as `Env` does, so the C code never decides what UTF-8 is.
   - `read_stdin_bytes` gains the `Result` shape.
4. **Narrowed consoles:** `stdout_only()` and `stderr_only()` give `Ok(None)` and `Ok([])`, as they do now.
5. **Migration:** every `console.read_line()` and `console.read_bytes()` in the tests, the error pages, the spec's examples and std gets the new shape. There are about 20 calls; most become `?` or a `match`.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Copy `File.read_line`'s shape** rather than inventing a second one: the same type, the same message, and the same skip-the-bad-line rule. One way to read a line.
2. **`read_bytes` is included.** It has the same read-error-as-end bug, and `File.read` already returns a `Result`. Fixing only `read_line` would leave two input calls that disagree.
3. **No replacement of bad bytes with U+FFFD** (which is what text from C gets). Input is data the program must check, and a silent replacement would hide the problem. A program that wants that behaviour can read bytes and decode them with `chars()`.
4. **The line count lives in the runtime,** because stdin is one stream for the whole process. Every `Console`, including copies and ones sent to other tasks, shares it.

## REQUIREMENTS
### Must Do
- Design 1–5.
- Spec:
  - §6 `Console`;
  - the runtime table rows;
  - §11 row 228;
  - a "Changes since v17" bullet marked **Breaking**.
- CHANGELOG.
### Must NOT Do
- Change `print`, `eprint` or `flush`, or `File`'s methods.
- Change how a prompt is flushed, or the helper thread that does the read.

## TESTS
- [x] `tests/programs/text/io/`:
  - [x] stdin with an invalid line between two good ones gives `Ok`, `Err("line 2 is not valid UTF-8")`, then `Ok` for line 3, then `Ok(None)`;
  - [x] a last line without `\n`, and `\r\n` endings;
  - [x] `read_bytes` of everything left on stdin.
- [x] `tests/cli.rs` (or the stdin test file): stdin as a directory (`< .`), so `read` fails with `EISDIR`, gives `Err("stdin: Is a directory")` from both calls, not the end of input.
- [x] `--lib`: `console.read_line()` has type `Result<Option<string>, string>`, and `console.read_bytes()` has type `Result<u8[], string>`.
- [x] Targeted: `--lib`, `programs`, `spec`, `explain`, `comments`, and every `tests/*.rs` file whose inline programs call `read_line` or `read_bytes` on a console (found by grep).

## ACCEPTANCE
- [x] No `string` from stdin holds invalid UTF-8.
- [x] A read error is never reported as the end of input.
- [x] `Console.read_line` and `File.read_line` share one shape and one message.
- [x] `cargo fmt --check` and clippy clean; the targeted tests pass.
