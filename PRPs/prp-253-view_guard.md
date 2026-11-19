## FEATURE: `p.view(n, f)` needs the `ffi` permission outside the program's own code, and panics for an `n` too large for a view's length; milestone 221.

**Status:** approved 2026-11-19.

## OBJECTIVE
A package the program never trusted with C can't read or write C's memory, even when another package hands it a `CPtr`:

```
// lugha.toml of the program
[packages]
sqlite = { path = "../sqlite", allow = ["ffi"] }
report = { path = "../report" }            # no grant

// ../report/src/lib.la
import sqlite;
public fun dump(p: CPtr, n: u64) {
    p.view(n, fun(v: View<u8>) { … });
}
```

```
error[E0333]: package `report` reads C's memory without the `ffi` permission
  --> ../report/src/lib.la:4:7
   |
 4 |     p.view(n, fun(v: View<u8>) { … });
   |       ^^^^ `p.view` trusts the length it is given
   = help: grant it in lugha.toml, if you trust it: `report = { …, allow = ["ffi"] }`
```

A length no view can hold is a panic, never a wrapped length:

```
fun main() {
    let p = some_c_buffer();
    p.view(0xffff_ffff_ffff_fff0, fun(v: View<u32>) { let _ = v[0]; });  // panics: view length too large
}
```

## CONTEXT
- **Found by:** the v18 spec review (2026-11-19), §8 findings 4 and 6.
- **Spec:**
  - §8 "C memory" and "Views" (L1669–1681): `p.view(n, f)`, "`n` is where memory safety becomes the program's".
  - §6 "The `ffi` permission" (L1278): which packages may run C, and the grant that covers dependencies.
  - E0333's row (L2059) and `docs/errors/E0333.md`.
- **Code:**
  - `src/modules/ffi.rs`: the permission today. It sees only the syntax tree: `extern fun`, `extern type`, `[link]`. It returns the holders (`loaded.ffi`), and its `covered` map (each package reachable from a grant) is local.
  - `src/check/views.rs` `cptr_view`: type-checks `p.view`. This is the only place a `CPtr` call is known to be `view`.
  - `src/codegen/views.rs` `cptr_view` / `view_bytes`: stores the `u64` `n` straight into the view's `i64` length, and multiplies `n × sizeof T` unchecked.
- **The hole:**
  - A `CPtr` is a built-in handle, and `.view` isn't gated, so any package handed one (from a granted package's public API) can make a `MutView` of any length. That means arbitrary reads and writes in the process, which is everything the `ffi` grant exists to keep from untrusted packages.
  - Separately, an `n` above `i64::MAX` becomes a negative length. The bounds check compares unsigned, so it treats that length as huge.
- **Decisions:** none open. The `ffi` permission is on CLAUDE.md's security list, so this PRP needs your explicit review.

## DESIGN
1. **The gate.** In a module of a package other than the program's own, the bundled prelude or std, `p.view(n, f)` is E0333 unless a grant covers that package. This is the grant `extern` already needs: `allow = ["ffi"]` on the package, or on a package that depends on it.
   - E0333's page and spec row gain the case: "or reads C's memory through `p.view`".
   - The message names `p.view`, and the help is E0333's existing grant line.
2. **Where it's checked.**
   - The modules stage exposes, beside `loaded.ffi`, the set of packages a grant covers (today the `covered` map local to `ffi::check`).
   - The checker reads that set at `cptr_view`. Stages still depend only backwards: `check` reads `modules`' output.
   - Each ungranted package reports once, at its first `.view`, as E0333 reports one `extern` per package.
3. **The length.**
   - `p.view(n, f)` panics with `view length too large` when `n > i64::MAX / sizeof(T)`.
   - The check runs before the null and alignment checks. A null `p` with `n` 0 stays an empty view.
   - So a view's `len` is always a non-negative `i64`, and `n × sizeof T` never overflows.
4. **Spec wording** (the same paragraph): "everything else is checked" becomes "the length, and that the memory stays alive and C's for as long as `f` runs (a call inside `f` that frees it, or a wait while another task does, leaves the view reading freed memory), are the program's to vouch for; the rest is checked". This is what PRP-252's decision 4 already meant, stated where a reader looks.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Gate `.view`, not `CPtr` itself.**
   - Holding, passing and comparing a `CPtr` can't touch memory, and a package passing one between two granted packages shouldn't need a grant.
   - Gating the name `CPtr` would also miss a `CPtr` reached by inference (`let p = sqlite.blob(s);`).
   - `.view` is the one operation that turns a `CPtr` into reads and writes.
2. **Not views in general.**
   - A function taking a `View<T>` or `MutView<T>`, or indexing one, is bounds-checked against a length someone trusted already. That someone is a granted package's callback or `.view`.
   - Gating those would make every buffer-processing library need a grant for no safety gain.
3. **Reuse E0333,** not a new code. It is the same error ("this package needs the `ffi` permission"), the same fix and the same page.
4. **Checked in the checker, not the modules stage.** Only the checker knows that the receiver is a `CPtr`. A syntactic guess (any `.view(` with two arguments) would misfire on a UI library's own `view` method.
5. **`lugha.lock`'s `[ffi]` table and `lughac update` keep listing packages by their `extern` and `[link]` only.**
   - A package that only views C memory still needs its explicit grant to build, so nothing reaches C memory unseen in `lugha.toml`.
   - Listing it in the lock would mean running the checker in `lughac update`. If you want that, it's a separate change.
6. **The panic text is `view length too large`,** in the style of `NULL CPtr given to view`.
7. **The length check is for `p.view` only, not for views C passes to callbacks.**
   - A callback's length comes from C, which a grant already trusts completely.
   - A `realtime` callback shouldn't gain a panic path for a value C controls anyway.

## REQUIREMENTS

### Must Do
- Design items 1–4.
- E0333's page (a second example: a `.view` in an ungranted package) and its spec row.
- The spec's §6 permission paragraph names `p.view`, and §8's views paragraph gets the new wording and the panic.
- CHANGELOG, §11 row 221, and a "Changes since v17" bullet (**Breaking**: an ungranted package's `.view` no longer builds).

### Must NOT Do
- Gate `CPtr` itself, `CPtr.null`, or functions taking views (decisions 1–2).
- Change `lugha.lock`'s format or `lughac update` (decision 5).
- Touch how `extern`, `[link]` or grants are found: `ffi.rs` only exposes the set it already computes.

## TESTS
- [ ] `tests/ffi_permission.rs`:
  - [ ] an ungranted package's `p.view` is E0333 naming `p.view`;
  - [ ] the same package granted directly builds;
  - [ ] the same package covered through a granted dependent builds;
  - [ ] the program's own `p.view` needs no grant;
  - [ ] an ungranted package passing a `CPtr` along, or taking a `View<u8>` parameter, builds.
- [ ] `tests/c_memory.rs`:
  - [ ] `n` of `i64::MAX / 4 + 1` with `View<u32>` panics `view length too large`;
  - [ ] `n` of `u64::MAX` with `View<u8>` panics;
  - [ ] `n` 0 of a null `CPtr` is still an empty view;
  - [ ] an `n` at the limit with a non-null pointer reaches the alignment check, not the length panic.
- [ ] `--lib`: a unit test for the covered-set lookup in `ffi.rs`.
- [ ] Targeted: `--lib`, `ffi_permission`, `c_memory`, `explain`, `comments`, `spec`.

## ACCEPTANCE
- [ ] No package without a grant can make a view of C's memory; the program's own code, std and the prelude are unaffected.
- [ ] No `p.view` gives a view whose length is negative or whose byte size overflows.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Spec, E0333's page and CHANGELOG updated.
