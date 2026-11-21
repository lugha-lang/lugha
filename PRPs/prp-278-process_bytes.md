## FEATURE: `Proc` runs programs whose output isn't text: `run_bytes` and `run_bytes_with` give `ProcessBytes`, its output as `u8[]`; captured output stays unlimited, and the spec says so plainly; milestone 245.

**Status:** approved 2026-11-21.

## OBJECTIVE
```
fun main(console: Console, proc: Proc) {
    let packed = proc.run_bytes_with("gzip", ["-c"], "hello\n".to_bytes()).unwrap();
    console.println(packed.code);                 // 0
    console.println(packed.stdout.len > 0);       // true: gzip's bytes, not UTF-8
    let back = proc.run_bytes_with("gzip", ["-dc"], packed.stdout).unwrap();
    console.println(string_from_utf8(back.stdout).unwrap());   // hello
}
```
Today `proc.run("gzip", …)` is `Err("stdout is not valid UTF-8")`, the output lost, and there is no way to send a program bytes.

## CONTEXT
- **The user's choices** (2026-11-21): bytes variants beside `run`, as `Fs` has `read_file` and `read_bytes`; no output cap, documented, with the planned per-scope memory limit (MEM-11) as the general guard.
- **Spec:** §5 "`Proc`" (milestone 181): "There is no timeout and no limit on output size", and `Err` when "its output isn't valid UTF-8"; §8's `lugha_rt_spawn` row; §11.
- **Code:**
  - `src/prelude/capabilities.la`: `interface Proc` (`run`, `run_with`, `call`) and `ProcessOutput`.
  - `src/prelude/capabilities_real.la`: `$Proc` and `$started`, which checks UTF-8.
  - `$os.start(cmd, args, input: string, mode)`: `src/check/float_ops.rs` (its type), `src/codegen/os.rs`, `runtime/process.c`'s `lugha_rt_spawn`, which copies `input` as a `LughaString`.
  - The one hand-written `Proc` besides the real one: `tests/programs/capabilities/clock_env_proc_entropy/fake.la`.
- **Security:** **`runtime/process.c` is on CLAUDE.md's list** (how programs are started). The change there is only the input's type: it is read as a `u8[]` (`LughaArray`: length, capacity, bytes) instead of a `string`, copied the same way into the job before the child starts. Nothing about how the program is started, its arguments, `PATH`, descriptors or environment changes. The diff will be shown for your review before merging.
- **Decisions:** none open.

## DESIGN
1. **`Proc` gains two methods:**
   - `run_bytes(cmd, args): Result<ProcessBytes, string>`: `run`, its output as bytes, never checked;
   - `run_bytes_with(cmd, args, input: u8[]): Result<ProcessBytes, string>`: `run_with`, its input bytes too.
   - `Err` only when the program couldn't start or an argument held a NUL byte.
2. **`ProcessBytes`,** a prelude struct like `ProcessOutput`: `{ code: i32, stdout: u8[], stderr: u8[] } with Eq`.
3. **`run` and `run_with` don't change:** text, and `Err` when the output isn't UTF-8, whose message now names `run_bytes`: `stdout is not valid UTF-8: use run_bytes for bytes`.
4. **One way in:** `$os.start` takes its input as `u8[]`; `run_with` passes `input.to_bytes()`. So `process.c` reads one kind of input.
5. **No output limit,** said plainly: the spec, the interface's doc comment and the skill's processes page state that `run`, `run_with` and the bytes variants keep everything the program writes in memory, with no limit, so a program that may write without end should be given its input and bounded some other way.
6. **Breaking:** a hand-written `Proc` must add the two methods (E0422 names them).

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **No default bodies** for the new methods: a default can't do what they promise (an `Err` saying "not implemented" would be a silent difference between the real and a fake `Proc`), and the break is one line per method.
2. **`$os.start` takes `u8[]` for every mode,** rather than a second input path: `run_with`'s string becomes bytes once, a copy the runtime already makes, and `process.c` keeps one shape.
3. **The UTF-8 error points at `run_bytes`:** the error is where people learn the other way exists.

## REQUIREMENTS
### Must Do
- Design 1–6, and `fake.la` gains the two methods.
- **Spec:** §5 `Proc` (the methods, `ProcessBytes`, the limit sentence); §8's `lugha_rt_spawn` row (input bytes); §11 row 245; a Changes bullet marked **Breaking**.
- **Elsewhere:** the skill's capabilities and env-and-processes pages; CHANGELOG.
### Must NOT Do
- Change how a program is started, its arguments, environment, descriptors or exit code.
- Add an output limit or a streaming API.

## TESTS
- [ ] **`tests/programs/capabilities/clock_env_proc_entropy/`** (beside `process.la`): `run_bytes` of `printf '\377\376'` (run directly, no shell), bytes that aren't UTF-8; `run_bytes_with` round-tripping bytes through `cat`; `run` of the same program still `Err` with the new message; a NUL in an argument still `Err`.
- [ ] `fake.la` with the two methods; a fake without them is E0422 (a rejected case).
- [ ] **Targeted:** `--lib`, `programs`, `leaks`, the process tests (`tests/process.rs` if present), `spec`, `skill`, `comments`.

## ACCEPTANCE
- [ ] The objective prints `0`, `true`, `hello`.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass; you have reviewed the `process.c` diff.
