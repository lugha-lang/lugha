## FEATURE: `ReadDir`, a folder that can only be read, as its own capability interface: `dir.read_only()` gives one, and writing through it is a compile error instead of a run-time `Err`; milestone 237.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
fun summarize(console: Console, data: ReadDir): Result<(), string> {
    for name of data.list(".")? {
        console.println("{name}: {data.size(name)?} bytes");
    }
    data.write_file("x", "y")?;     // error[E0415]: no method `write_file` on `ReadDir`
                                    //   = help: a `ReadDir` only reads; take a `Dir` to write
    Ok(())
}

fun main(console: Console, fs: Fs) {
    let _ = summarize(console, fs.cwd().read_only());
}
```

## CONTEXT
- **Asked for by:** the user (2026-11-20): split `Dir` so a read-only folder is a type, with a separate `ReadDir` reached by an explicit `.read_only()` (the user's choice over `Dir: ReadDir`).
- **Spec:**
  - §6 Capabilities: the `Fs` and `Dir` bullets, and confinement, "`dir.read_only()` drops writing";
  - §6's `Dir` and `File` tables;
  - §9's `Dir` runtime paragraph.
- **Code:**
  - `src/prelude/capabilities.la` (the `Dir` interface);
  - `capabilities_real.la` (`$Dir { h, read_only: bool }`, `writable()`, giving `Err("the folder is read-only")` today);
  - `test_caps.la` (`$test_dir`);
  - `src/check/entry.rs` (capability names);
  - `runtime/dir.c`, unchanged.
- **Uses to migrate:**
  - `tests/programs/capabilities/dir_confinement/confined.la` (`ro.write_file` as a run-time `Err` becomes a compile-time case);
  - `tests/programs/capabilities/dir_files/fake.la` (a `Dir` fake gains `read_only()`);
  - the skill (`concepts/10-capabilities`, `library/console-and-files`, `tasks/command-line-tool`).
- **Security:** this is a narrowing of a capability, which CLAUDE.md doesn't list. It's made stronger, not weaker: no write path remains on a read-only folder. No `unsafe`, and no new dependency.
- **Decisions:** the user chose the separate interface with `.read_only()`; none open.

## DESIGN
1. **`interface ReadDir: Sendable`** in the prelude:
   - `read_file`, `read_bytes`, `list`, `exists`, `is_dir`, `is_file`, `size` and `path`, as on `Dir`;
   - `open(path): Result<File, string>`, a `File` open for reading;
   - `dir(path): Result<ReadDir, string>`, a folder inside it, read-only too.
2. **`Dir` doesn't extend `ReadDir`.**
   - It keeps every method it has.
   - `read_only(self): ReadDir` replaces `read_only(self): Dir`.
   - A `Dir` is never converted to a `ReadDir` on its own: the call site writes `.read_only()`.
3. **The real one:**
   - `$ReadDir { h }` implements `ReadDir` over the same confined handle.
   - `$Dir` loses its `read_only` flag and `writable()`.
   - `Err("the folder is read-only")` no longer exists, because nothing can ask.
4. **The checker's help:** a method a `ReadDir` lacks but a `Dir` has (`write_file`, `make_dir`, …) is E0415 with help "a `ReadDir` only reads; take a `Dir` to write".
5. **Unchanged:**
   - `Fs` (`fs.cwd()`, `fs.dir(path)` give a `Dir`);
   - confinement;
   - what `main` and tests may take (a `ReadDir` comes from a `Dir`);
   - process lockdown.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **`sync` stays on `Dir` only:** it makes written data durable, which only matters to a writer.
2. **`ReadDir` isn't a reserved name** (INT-21: no syntax depends on it). A module may declare its own, as with `Dir`.
3. **No `ReadDir` parameter for `main` or tests:** one is made from a `Dir` by `.read_only()`, so the capability list stays as it is.
4. **A fake read-only folder implements `ReadDir`:** ten items instead of `Dir`'s twenty-odd.

## REQUIREMENTS
### Must Do
- Design 1–5.
- **Migrate:**
  - the two test programs;
  - the skill pages;
  - any spec example using `read_only`.
- **Spec:**
  - §6 (the `Dir` bullet and table, a `ReadDir` row set, confinement's narrowing sentence);
  - §9's runtime paragraph;
  - §11 row 237;
  - a "Changes since v17" bullet marked **Breaking** (`dir.read_only()` is a `ReadDir`; code passing it where a `Dir` is expected, which would have failed at its first write, no longer compiles).
- CHANGELOG.
### Must NOT Do
- Change confinement, `Fs`, `File`, or the runtime's `dir.c`.
- Add a write-only folder.

## TESTS
- [x] `--lib`:
  - [x] `dir.read_only()` has type `ReadDir`;
  - [x] `ReadDir`'s methods have their types;
  - [x] `write_file` on a `ReadDir` is E0415 with the help;
  - [x] a `ReadDir` passed where a `Dir` is expected is E0403 or E0422;
  - [x] a program's own `ReadDir` fake implementing the interface is accepted.
- [x] `tests/programs/capabilities/dir_confinement/`:
  - [x] reading through `read_only()` and its `dir(…)` still confined;
  - [x] an `e0415_read_only.la` case.
- [x] `tests/programs/capabilities/dir_files/fake.la`: the fake's `read_only()`.
- [x] Targeted: `--lib`, `programs`, `spec`, `skill`, `skill_library`, `skill_tasks`, `comments`, and the `tests/*.rs` files whose inline programs call `read_only` (found by grep).

## ACCEPTANCE
- [x] No method of a `ReadDir` can change the file system.
- [x] `lughac skill` teaches `ReadDir`, and every example still runs.
- [x] `cargo fmt --check` and clippy clean; the targeted tests pass.
