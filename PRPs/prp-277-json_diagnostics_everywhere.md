## FEATURE: JSON diagnostics carry their human text as `"rendered"`, and `lughac fmt` and `lughac test` take `--diagnostics=json` like `build`, `run` and `check`.

**Status:** approved 2026-11-21.

## OBJECTIVE
```
$ lughac check --diagnostics=json bad.la
{"severity":"error","code":"E0401",…,"help":null,"rendered":"error[E0401]: float literal where i32 expected\n  --> bad.la:3:17\n  |\n3 |     let x: i32 = 1.5;\n  |                  ^^^ expected i32\n"}

$ lughac test --json --diagnostics=json broken_test.la      # compile errors as JSON lines on stderr
$ lughac fmt --diagnostics=json broken.la                    # syntax errors as JSON lines on stderr
```
Today a tool reading JSON can't show lughac's own text without rebuilding it, and `test` and `fmt` give compile and syntax errors only as text.

## CONTEXT
- **The user's request** (2026-11-21): "fix them", for review points 15–17; this PRP takes 15 and 17 (16, `std.process`, is the next one).
- **Spec:** §9 "JSON diagnostics" (the fields, and the E0401 example line); §9's command table (`fmt`, `test`); `lughac test`'s "For tools" paragraph ("diagnostics stay on stderr").
- **Code:**
  - `src/driver/json.rs`: `line` writes the object; `src/driver/render.rs`: `human` renders the same `Report`.
  - `src/driver/cli.rs`: `Fmt` and `Test` have no `diagnostics` argument, and `Command::format` gives them `Human`; their errors already go through the driver's `Output`, which writes either format.
- **Security:** none of CLAUDE.md's list. The VS Code extension isn't changed: what it starts stays as it is.
- **Decisions:** none open.

## DESIGN
1. **`"rendered"`.** Every JSON diagnostic gains a last field, `"rendered"`: the human form of that diagnostic exactly as `lughac` prints it without `--diagnostics=json` (no colour), without the blank line after it and without the closing "For more about …" hint, which covers several diagnostics. Internal errors have it too.
2. **`--diagnostics=human|json` on `fmt` and `test`,** as on `build`, `run`, `check` and `update`, default `human`. With `json`, the diagnostics they print on stderr (syntax errors for `fmt`; compile errors, and the internal errors reported as diagnostics, for `test`) are JSON lines.
3. **`--json` stays what it was:** the test results on stdout. It doesn't imply `--diagnostics=json`; a tool asks for both.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The same flag on every command,** not `test --json` switching stderr too: one flag means one thing everywhere, and a tool already passing `--diagnostics=json` to `check` passes it to `test`.
2. **`rendered` leaves out the "For more about" hint:** it is a footer for a run's diagnostics together, not part of one.
3. **No change to the VS Code extension:** it can adopt the flag later; changing what it starts needs its own review.

## REQUIREMENTS
### Must Do
- Design 1–3.
- **Spec:** §9's JSON diagnostics paragraph and example line; the command table rows for `fmt` and `test`; the "For tools" paragraph; a Changes bullet.
- **Elsewhere:** CHANGELOG.
### Must NOT Do
- Change the human format, the other JSON fields, or `--json`'s stdout stream.

## TESTS
- [ ] `--lib`, `driver/json.rs`: the E0401 line ends with `"rendered"` equal to `render::human`'s text less the blank line; an internal error has it too; quotes and newlines are escaped.
- [ ] `tests/cli.rs`: `check --diagnostics=json` lines carry `"rendered"`; `fmt --diagnostics=json` on a file with a syntax error gives JSON lines and exit 1.
- [ ] `tests/test_runner.rs`: `test --json --diagnostics=json` on a file with a type error gives a JSON diagnostic line on stderr and the summary's `uncompiled` on stdout.
- [ ] `tests/spec.rs` (the E0401 example in both formats).
- [ ] **Targeted:** `--lib`, `cli`, `test_runner`, `spec`, `comments`.

## ACCEPTANCE
- [ ] The objective's three commands behave as shown.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
