## FEATURE: The `ffi` permission: a package may declare `extern` C code (or link C through `[link]`) only when the root `lugha.toml` grants it `allow = ["ffi"]`; a grant covers the package and everything it depends on, and `lugha.lock` and `lughac update` show who holds it; milestone 188, in v18's Phase 2. **Decides which code may run C: needs the user's security review before any code.**

**Status:** proposed 2026-11-16. Approved 2026-11-16 (security review).

## OBJECTIVE
```toml
# lugha.toml of a program
[packages]
sqlite = { git = "https://example.com/sqlite.git", version = "3.0", allow = ["ffi"] }
json   = { git = "https://example.com/json.git",   version = "1.2" }   # pure Lugha: nothing to grant
```
`sqlite` declares `extern fun sqlite3_open(…)` and depends on `zlib`, which links `libz` through its `[link]` table: both may, through the one grant. If `json` (or anything it pulls in) declared an `extern`, the build would stop:
```
error[E0333]: package `json` declares C code (`extern fun crc32`) but has no `ffi` permission
  --> json/src/crc.la:3:1
  = help: C code can do anything, so the program must trust it: add `allow = ["ffi"]` to `json`'s entry in lugha.toml
```
`lugha.lock` records every package that holds `ffi` and the grant it came through, and `lughac update` says so when one newly does:
```
zlib now runs C code, through `sqlite`'s ffi grant
```

## CONTEXT
- Draft v18 §2.2 rule 4 [SEC-04], approved: "A package may declare `extern` functions only if the root `lugha.toml` grants it `ffi`, indirect dependencies included. A package with `ffi` is documented as fully trusted, since C can do anything." [CAP-16]: the only per-package permission is `ffi`.
- Decided with the user: **a grant bubbles down**: `allow = ["ffi"]` on an entry covers that package and every package it depends on, directly or not; the lock file and `lughac update` make the reach visible, so a dependency newly running C after an update shows up.
- Today any package may declare `extern fun`/`extern type` and a `[link]` table (C libraries and files, `src/link/`), with no permission.
- Code: `src/modules/manifest.rs`, `inline.rs` (the inline entry, where `allow` goes), `packages.rs` (the names each manifest lists, for the closure), `lock.rs`, `src/modules/mod.rs` (`Loaded`), `src/check` or `src/modules` for the check, `src/driver/update.rs`; `docs/errors/E0333.md`.

## DESIGN — for the security review

### What needs the permission
A package other than the program's own (the root package) and other than the bundled prelude and std, whose modules declare any `extern fun` or `extern type`, or whose manifest has a `[link]` table. The program's own code never needs it: its author is the one granting. Callbacks and `c_read`/`c_write` need a C pointer, which only comes through an `extern`, so they are covered by it.

### Granting
- Only the **root** `lugha.toml` grants, on an entry of its `[packages]`: `allow = ["ffi"]` (the only permission; `"ffi"` the only value). The inline entry learns this one list-valued key; any other value, or `allow` in a package's own manifest, is E0321 (`only the program's lugha.toml grants permissions`).
- **A grant covers the closure:** the granted package and every package reachable from it through its manifests' entries. A package reachable from both a granted and an ungranted entry is covered (the program trusted it once).
- **Unused grants** are not errors: a grant on a package that declares no C is harmless (and keeps working if it starts to).

### The check
After every module is loaded: for each package that needs the permission and isn't covered, E0333 at its first `extern` declaration (or its `[link]` table's header), with help naming the root entry to change — the package itself when it is a direct dependency, else the direct dependency that brings it in (`add allow = ["ffi"] to sqlite's entry: zlib is one of its dependencies`). The build stops before codegen and before anything in `[link]` is compiled or `pkg-config` runs: an ungranted package's C is never touched.

### Making the reach visible
- `lugha.lock` gains an `[ffi]` table, written whenever the lock is: `zlib = { through = "sqlite" }` for each package holding `ffi` (`through` its own name when granted directly). It is a record for people and diffs; the check never reads it.
- `lughac update` compares the new reach with the lock's old `[ffi]` table and prints one line per package that newly holds `ffi`: `zlib now runs C code, through `sqlite`'s ffi grant`.

### For the security review, in one list
1. Which code needs the permission: `extern fun`, `extern type`, `[link]`; not the root package, not bundled modules.
2. Only the root manifest grants; grants cover the dependency closure (the user's decision).
3. The check runs before anything C is compiled or `pkg-config` runs.
4. The lock's `[ffi]` table and `lughac update`'s line as the way a widened reach is noticed.
5. What `ffi` can't contain: once granted, C runs with the whole process's rights. Process lockdown (SEC-05) is the next PRP.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **E0333** (next free in the modules range), with a page in `docs/errors/`.
2. **`extern type` counts** as C code: it exists only to pass C pointers around.
3. **The error is reported once per package**, at its first declaration, not at every `extern`.
4. **The lock's `[ffi]` table lists path packages too** when a lock is written; a program with no git packages writes no lock, as today, and its manifest's grants are the whole record.

## REQUIREMENTS

### Must Do
- `allow = ["ffi"]` on root entries; E0321 elsewhere or for other values.
- The closure, the check (E0333), before the C link steps.
- `lugha.lock`'s `[ffi]` table; `lughac update`'s line.
- Spec (packages section: manifests, the lock; §8 C interop; the error table; §11 row 188; Changes, Breaking), CHANGELOG, `docs/errors/E0333.md`, CLAUDE.md (the security list mentions what `ffi` gates).
- The repository's packages that declare `extern` or `[link]` in tests get their grants.

### Must NOT Do
- Process lockdown; sandboxed C (`sandbox = true`, a later milestone).
- Any change to how `[link]` input is run (`src/link/`): only whether it runs.

## TESTS
- [x] `src/modules` unit tests: `allow` parsed on root entries, refused in a package's manifest and for other values; the closure (direct, indirect, shared).
- [x] Integration (`tests/ffi_permission.rs`, path packages in temp folders): an ungranted package with `extern fun` → E0333 at it, help naming its own entry; an ungranted indirect one → help naming the direct dependency; `extern type` alone and `[link]` alone also need it; a grant on the parent covers the child; the root program's own `extern` needs nothing; a grant on a pure package is fine; nothing in `[link]` is compiled for an ungranted package.
- [x] Lock and update: the `[ffi]` table written with `through`; `lughac update` printing the new-reach line (a git fixture as the existing update tests use).
- [x] Targeted runs: `--lib`, the module, link and update test files, `programs`; CI the rest.

## ACCEPTANCE
- [x] No package's C code compiles or links unless the root granted it or a package that depends on it.
- [x] The lock file names every package holding `ffi` and its grant; `lughac update` names every package newly holding it.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass
- [x] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated where they change

## CHANGES DURING THE BUILD
- **The root is the manifest at the top of the build** (`manifest_folder`), not just the `None` package: when the program's folder is itself a package, its manifest is read a second time as that package, and its grants must still count.
- **The lock lists only packages that run C**, not every package a grant reaches: a pure dependency of a granted package isn't news.
- **`lughac update` prints its lines only when it rewrites the lock**: a program without git packages keeps no lock, so every update would otherwise call every holder new.
- **Two fixtures got grants:** `src/modules/link_tests.rs` and `tests/link_libs.rs` each use a package with a `[link]` table.
- **The unit tests** for `allow` are in `src/modules/inline.rs` and `lock.rs`; the closure and E0333 are covered end to end in `tests/ffi_permission.rs`.
- **Targeted runs only:** `--lib`, `ffi_permission`, `link_libs`, `lsp_docs`, `modules`, `test_runner`, `git_packages`, `programs`, `explain`; CI the rest.
