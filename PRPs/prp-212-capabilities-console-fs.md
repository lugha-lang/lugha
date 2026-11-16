## FEATURE: Capabilities, first step: `main` takes `args`, `Console` and `Fs` as parameters; the ambient intrinsics `print`, `println`, `eprint`, `eprintln`, `read_line`, `read_file`, `write_file`, `append_file`, `args` and `exit` are removed; milestone 180, in v18's Phase 2.

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
fun greet(console: Console, name: string) {
    console.println("hello, {name}");
}

fun main(args: string[], console: Console, fs: Fs): i32 {
    greet(console, "world");
    console.println(42);                         // any `Display` value
    let here = fs.cwd();
    match here.write_file("note.txt", "a line\n") {
        .Ok(n) => console.println("wrote {n} bytes"),
        .Err(e) => console.eprintln("can't write: {e}"),
    }
    let _ = here.append_file("note.txt", "another\n");
    match here.read_file("note.txt") {
        .Ok(text) => console.print(text),
        .Err(e) => console.eprintln(e),
    }
    console.println(args.len > 0);
    match console.read_line() {
        .Some(line) => console.println("read: {line}"),
        .None => console.println("no input"),
    }
    println("old");                              // E0471: `println` needs a `Console`
    0
}
```

## CONTEXT
- Draft v18 §2.1 [CAP-01..17] and §1.7 rule 8 [PRE-06]: no ambient authority; capabilities as interfaces in the prelude; `main` declares what it needs as parameters matched by type, any order, each at most once, returning `()` or `i32` (E0304 otherwise); printing needs a `Console`; `exit` is removed; the working directory is `fs.cwd()`, a `Dir`; `debug(x)` needs no capability.
- Decided with the user: Phase 2 removes the ambient intrinsics early, in this first PRP, rather than adding capabilities alongside them; the codebase is migrated with scripts in this repository, with no `lughac migrate` command.
- Later PRPs in Phase 2: `Net`, `Clock`, `Env`, `Proc`, `Entropy`, `Signals` and std taking capabilities (`std.fs`, `std.net`, … stay ambient until then); the test runner's fakes; narrowing (`fs.dir`, `read_only`, `net.only`, …) with `Dir` confinement (`openat2`); the `ffi` permission; process lockdown. `root: Scope` comes with scopes (Phase 5).
- Measured: about 2,000 print calls in 369 test programs, 1,500 of them with a non-string argument; 33 files print from a function other than `main`; 17 use `read_file`, `write_file`, `append_file`, `read_line` or `args`; one uses `exit`; std and the prelude call none (only doc comments mention them). The spec, nearly every error page and many Rust tests' programs print.
- Code: `check/env.rs` (intrinsics, `main`), `check/call.rs`, `check/input.rs`, `codegen/io.rs`, `codegen/input.rs`, the C `main` the compiler emits (`codegen`), `driver/test_build.rs` (tests' entry), `src/prelude/`.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The capabilities in this PRP:** `Console` and `Fs` (with `Dir`), prelude interfaces visible in every module like `Display` (a module's own name hides them):
   ```lugha
   interface Console {
       fun print(self, x: Display);
       fun println(self, x: Display);
       fun eprint(self, x: Display);
       fun eprintln(self, x: Display);
       fun read_line(self): Option<string>;
   }
   interface Fs { fun cwd(self): Dir; }
   interface Dir {
       fun read_file(self, path: string): Result<string, string>;
       fun write_file(self, path: string, text: string): Result<i64, string>;
       fun append_file(self, path: string, text: string): Result<i64, string>;
   }
   ```
   The methods behave exactly as the intrinsics did (flushing, `\r\n`, messages). Taking a `Display` value keeps `println(x)` → `console.println(x)` a rename at every call: the argument converts implicitly, as any value does where an interface is expected. A bare `println()` becomes `console.println("")`.
2. **`Dir` isn't confined yet:** in this PRP `fs.cwd()` is the starting folder and its paths resolve as before (`..` and absolute paths included), the same reach the intrinsics had. Narrowing (`fs.dir(path)`) and confinement come together in their own PRP; the spec says so until then.
3. **`main`'s parameters:** any of `string[]` (the arguments, `args[0]` the program's path, as `args()` gave), `Console` and `Fs`, matched by type, any names, any order, each at most once; result `()` or `i32`. Another parameter type, or a repeated one, is E0304 naming the types allowed.
4. **Tests:** a `@test` function takes the same parameters (`console: Console`, `fs: Fs`); the runner gives it the real ones for now, as programs had. The runner's fakes and integration-only `Fs` are a later PRP.
5. **The intrinsics** `print`, `println`, `eprint`, `eprintln`, `read_line`, `read_file`, `write_file`, `append_file`, `args` and `exit` are gone from programs: a call is a new **E0471**, "`println` needs a `Console`", whose help names the capability and the call to write (`console.println(…)`, `fs.cwd().read_file(…)`, an `args: string[]` parameter of `main`; for `exit`, return an `i32` from `main`). Their names stay reserved (E0302). The prelude's real `Console`, `Fs` and `Dir` still reach the runtime through them, so codegen and the runtime don't change.
6. **`debug(x)`, `panic` and the assertions** keep working without a capability.
7. **How `main` gets its values:** the C `main` the compiler emits builds each declared capability (the prelude's real implementations, converted to their interfaces) and the arguments array, and calls `main` with them in the declared order.
8. **Migration** (this repository only): the compiler's own E0471 spans drive a script for `main` (add the parameters it needs, rename the calls); functions other than `main` that print (33 files) get a `console: Console` parameter by hand; Rust tests' programs, the spec's programs and the error pages' examples by script, checked by their tests; `exit` (one program) by hand.
9. **Not in this PRP:** narrowing, the other capabilities, std's ambient functions, the test runner's fakes, `root: Scope`, process lockdown.

## REQUIREMENTS

### Must Do
- Prelude: `Console`, `Fs`, `Dir` and their real implementations.
- Checker: `main`'s and tests' parameters (E0304); E0471 for the removed intrinsics outside bundled sources.
- Codegen: the C entry passes the declared values; the test entry likewise.
- Migration of programs, std docs, spec, error pages, README, bench, editor samples and snippets (the VS Code version bumped if they change).
- Docs: E0471's page; E0304's page; v18 spec (§5 intrinsics table, §6 entry point, the capabilities section's first part, the error table, §11 row 180, "Changes since v17", Breaking); CHANGELOG; CLAUDE.md if a command example prints.

### Must NOT Do
- `Net`, `Clock`, `Env`, `Proc`, `Entropy`, `Signals`; std modules taking capabilities.
- Narrowing or `Dir` confinement; the `ffi` permission; process lockdown.
- A user-facing migration command.

## TESTS
- [ ] `tests/programs/capabilities/console_fs/`: the OBJECTIVE minus its error (runs, with stdin), `main` in every parameter order, a fake `Console` passed to a helper, E0471 for each removed intrinsic, E0304 for a wrong or repeated parameter.
- [ ] A `@test` taking `Console` and `Fs`.
- [ ] Checker unit tests for `main`'s parameters and E0471.
- [ ] `python3 bench/run.py --check` to see printing through `Console` costs nothing measurable.
- [ ] The full `cargo test`.

## ACCEPTANCE
- [ ] The OBJECTIVE gives E0471 at `println("old")` only, and once that line goes runs as its comments say.
- [ ] The full `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Spec, CHANGELOG and the error pages updated.
