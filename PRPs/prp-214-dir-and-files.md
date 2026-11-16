## FEATURE: Capabilities, third step: everything `std.fs` did becomes `Dir`'s (folders, what exists, sizes, renames, modes, syncing, file streams as the prelude's `File`), bytes go through `Dir` and `Console`, and the last ambient file and console intrinsics (`read_bytes`, `write_bytes`, `append_bytes`, `read_stdin_bytes`, `flush`) are E0471; `std.fs` is removed; milestone 182, in v18's Phase 2.

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
fun main(console: Console, fs: Fs): i32 {
    let here = fs.cwd();
    let made = here.make_dirs("out/logs");
    let w = here.write_bytes("out/logs/a.bin", [1, 2, 3]);
    console.println(here.read_bytes("out/logs/a.bin").debug());     // Ok([1, 2, 3])
    console.println(here.list("out/logs").debug());                 // Ok(["a.bin"])
    console.println("{here.is_dir("out")} {here.size("out/logs/a.bin").debug()}"); // true Ok(3)
    match here.create("out/notes.txt") {
        .Ok(f) => {
            let a = f.write_text("one\ntwo\n");
            f.close();
        }
        .Err(e) => console.eprintln(e),
    }
    let f = here.open("out/notes.txt").unwrap();
    console.println(f.read_line().debug());                         // Ok(Some("one"))
    f.close();
    let r = here.rename("out/notes.txt", "out/n.txt");
    console.println(here.exists("out/notes.txt"));                  // false
    let x = here.remove_file("out/n.txt");
    let y = here.remove_file("out/logs/a.bin");
    let z = here.remove_dir("out/logs");
    let v = here.remove_dir("out");
    console.flush();
    0
}
```
`import std.fs;` is gone, and `read_bytes(p)` outside the prelude and std is E0471: "`read_bytes` needs an `Fs`".

## CONTEXT
- Draft v18 §2.1 [CAP-01, CAP-02, CAP-07, CAP-11, CAP-17]: files and folders need `Fs`, narrowed to `Dir`; each capability is an interface; the prelude declares them with their core methods.
- Follows PRP-212 (`Console`, `Fs`, `Dir`, E0471) and PRP-213 (`Dir.path`). Interfaces may give default methods, so a fake implements only the core.
- Measured: `std.fs` is imported by 12 files (`fs.open`, `size`, `exists`, `remove_file`, `sync`, `rename`, `create`, `set_mode`, `make_dir`, …, about 70 calls); `read_bytes`, `write_bytes`, `append_bytes`, `read_stdin_bytes` and `flush()` have 37 uses in 27 files.
- Code: `src/prelude/capabilities.la` (grows past 300 lines: the real implementations move to a new `src/prelude/capabilities_real.la`), `src/std/fs.la` (removed), `src/check/entry.rs` (`AMBIENT`), `src/lib.rs`.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **`Dir`'s core** (what a fake must write), each behaving exactly as the `std.fs` function or intrinsic it replaces, paths relative to the folder:
   `read_bytes(path): Result<u8[], string>`, `write_bytes(path, bytes)` and `append_bytes(path, bytes): Result<i64, string>`; `list(path): Result<string[], string>` (was `list_dir`); `make_dir`, `remove_file`, `remove_dir(path): Result<i64, string>`; `exists`, `is_dir`, `is_file(path): bool`; `size(path): Result<i64, string>`; `rename(from, to)`, `set_mode(path, mode)`, `sync(path): Result<(), string>`; `open`, `create`, `append(path): Result<File, string>`; `path()`.
2. **`Dir`'s default methods**, written once over the core: `read_file`, `write_file`, `append_file` (text over the byte methods, when the messages come out the same; otherwise they stay core) and `make_dirs` (over `is_dir` and `make_dir`).
3. **`File` is a prelude interface**: `read(max)`, `read_line()`, `write(bytes)`, `close()`, with `write_text(s)` a default over `write`. The real one (`$File`, today's `std.fs.File`) moves into the prelude unchanged. An interface, not a struct, so a fake `Dir` can return fake files.
4. **`Console` gains** `read_bytes(): u8[]` (all of standard input, as `read_stdin_bytes()`) and `flush()`: core methods, since a default would reach the real streams behind a fake.
5. **E0471** covers the five intrinsics: "`read_bytes` needs an `Fs`" with help `fs.cwd().read_bytes(…)`; `read_stdin_bytes` and `flush` need a `Console` (`console.read_bytes()`, `console.flush()`). The names stay reserved.
6. **`std.fs` is removed**: nothing is left that isn't a method. This also ends the clash between `import std.fs` and a parameter named `fs`.
7. **Not confined yet**: `fs.cwd()` still reaches what the process can (as in PRP-212); `fs.dir(path)` comes with confinement.
8. **Migration** by hand and script, in this repository only, as before.

## REQUIREMENTS

### Must Do
- Prelude: `Dir`'s new core and default methods, `File`, `Console.read_bytes` and `flush`, their real implementations (split into `capabilities_real.la`).
- Checker: the five intrinsics in `AMBIENT` (E0471 help for each); `File` visible by bare name like the capabilities.
- std: remove `std.fs`.
- Spec (`std.fs` section folded into the capabilities paragraph; §11 row 182; Changes; Breaking), CHANGELOG, error pages using the removed calls.

### Must NOT Do
- `fs.dir(path)`, `read_only()` and confinement (security review).
- `Net`; the test runner's fakes.
- Any runtime change: the methods call the existing `$os` built-ins and intrinsics.

## TESTS
- [ ] `tests/programs/capabilities/dir_files/`: the objective; a fake `Dir` that implements only the core and uses `read_file` and `make_dirs` from the defaults; a fake `File`; E0471 for each of the five.
- [ ] `tests/programs/std/fs/`, `file_streams/`, `fs_size/` and the other importers, migrated (the folder becomes `capabilities/dir_*` where it tests `Dir`).
- [ ] Unit (`check/entry.rs`): the five are E0471.
- [ ] Full `cargo test`.

## ACCEPTANCE
- [ ] The objective prints as commented and exits 0, leaving nothing behind.
- [ ] Outside the prelude and std, nothing reads or writes a file, a folder or the standard streams except through `Console`, `Fs`, `Dir` or `File`.
- [ ] `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated where they change
