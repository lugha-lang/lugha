## FEATURE: `View<T>` and `MutView<T>`: bounds-checked windows onto C's memory that never outlive the call they were given to, taken by `direct` and `realtime` callbacks and made from a `CPtr`, replacing callback `Ref<T[]>` and `c_read`/`c_write`; milestone 220.

**Status:** approved 2026-11-19.

## OBJECTIVE
A real-time audio callback fills C's buffer in place, and C memory is read and written without copies:

```
// C: void audio_start(void (*cb)(float *out, size_t n, void *u), void *u);
extern fun audio_start(cb: realtime fun(MutView<f32>, data), u: data);

fun gain(out: MutView<f32>, k: f32) {
    for i in 0..out.len { out[i] = out[i] * k; }
}

fun main() {
    let (levels, incoming) = Ring<f32>.new(64);
    audio_start(fun(out: MutView<f32>) {
        gain(out, 0.5);                                // written straight into C's buffer
        match levels.try_send(out[0]) { _ => {} }
    });
}
```

```
let blob = sqlite3_column_blob(stmt, 0);
let n = sqlite3_column_bytes(stmt, 0) as u64;
blob.view(n, fun(bytes: View<u8>) {
    let head = bytes.slice(0, 16).to_array();         // the copy, written as one
    let sum = checksum(bytes);                         // read in place
});
```

```
error[E0483]: a view can't outlive the call it was given to
  --> audio.la:9:17
   |
 9 |         saved = Some(out);
   |                      ^^^ stored here
   = help: copy what you need with `to_array()`
```

## CONTEXT
- **Spec:**
  - §8 "Arrays in callbacks": `T[]` is C's `const T *, size_t`, copied; `Ref<T[]>` is `T *, size_t`, copied in and back.
  - §8 "C memory": `CPtr`, `c_read`/`c_write`, the program vouching for lengths.
  - §8 callbacks: `direct`, `spawned`, `realtime`. §5 `mut` parameters: function types can't spell `mut`, and closures take none.
  - §11's out-of-scope "bounds-checked byte buffer" is about Lugha's own bytes; this is C's memory only.
- **Research (discovery):**
  - C#'s `Span<T>` (a `ref struct`) and Nim's `openArray` keep a view from escaping with one fixed rule and no lifetimes: no field, no boxing, no capture, no generic argument. C# had to close generic arguments and suspension points too.
  - Rust uses lifetimes (`&mut [f32]` in cpal and the jack crate); Swift's `Span` uses non-escapable types; Java's `MemorySegment` checks liveness at run time; Go's `unsafe.Slice` and Zig check nothing.
  - Every safe design limits elements to plain numbers: bytes from C can be invalid for `bool`, enums and pointers.
  - C APIs mostly pass a pointer then a length, but lengths also sit in struct fields (zlib, libusb), come back signed (OpenSSL, SQLite), or count frames rather than samples (PortAudio, JACK).
- **The user's calls:**
  - views replace `c_read`/`c_write` (one way where possible);
  - callbacks choose by word: `direct` and `realtime` take views, `spawned` takes `T[]` (it runs after C's call returns); `Ref<T[]>` is removed.
- **Code:**
  - The checker's callback plan (`check/callbacks.rs`: `CParam::Callback`, the `T[]`/`Ref<T[]>` counting); `check/input.rs` (`c_read`/`c_write`); a new `check/views.rs` for the escape rule.
  - Codegen: `codegen/callbacks.rs` (the pointer and count arguments); `codegen/io.rs` (`c_read`/`c_write`); `runtime/cmem.c`.
- **Decisions:** none open.
- **Security:** nothing on CLAUDE.md's list (no `unsafe`; `CPtr.view` trusts the program's length exactly as `c_read` did, at one place instead of every call).

## DESIGN
1. **Two types:**
   - `View<T>` reads C's memory.
   - `MutView<T>` reads and writes it.
   - Both are a pointer and a count of `T`s. `T` is an integer or float type (`u8` … `i64`, `f32`, `f64`), or an extern struct (or union) holding only those, at any depth; anything else is E0409. (Widened during the build, the user's call: a WebAssembly host's `Val[]` buffers would otherwise have no way into a `direct` callback.)
   - Mutability is in the type, not a `mut` word, because function types and closures can't spell `mut`.
   - A `MutView<T>` is accepted where a `View<T>` is expected; the reverse is E0403.
2. **What a view does** (none of it allocates, except `to_array`):
   - `v.len`, an `i64` (as an array's, so `for i in 0..v.len` indexes it); `v[i]`, bounds-checked as arrays are (`index out of bounds`);
   - `v[i] = x` (E0506 on a `View`) and `v.copy_from(xs: T[])` (E0415 on a `View`) on a `MutView` only ( `copy_from` panics unless the lengths match);
   - `for x of v`; `v.slice(from, to)`, a smaller view of the same kind, panicking out of range as a string's does;
   - `v.to_array()`, a copy, a `T[]`.
3. **The escape rule (E0483):** a view can be a parameter (of a function, method or closure) or a `let` local, and can be read, indexed, written, sliced, iterated and passed as an argument. It can't be:
   - returned, or the result type of any function;
   - stored in a field, element, tuple, `Option`, `Result`, `Ref`, map, set or global;
   - captured by a closure (the callback's own parameter included);
   - sent to a channel, ring, task or actor;
   - a type argument (`id(v)` for `fun id<T>(x: T): T`), or converted to an interface value.
   The rule is checked on types, so a view-typed expression anywhere else is E0483, which names where it would escape to.
4. **Callbacks, by word:**
   - `direct` and `realtime` callbacks take `View<T>` (C's `const T *, size_t`) and `MutView<T>` (`T *, size_t`): two C arguments, pointer then count. Nothing is copied; writes reach C at once.
   - `spawned` callbacks take `T[]` (a copy), since they run after C's call has returned.
   - `T[]` in a `direct` or `realtime` callback, and `Ref<T[]>` in any callback, are E0484, whose help names the view to take.
5. **From a `CPtr`:**
   - `p.view(n, f)` calls `f(v)` with a view of `n` `T`s at `p`. `f`'s parameter type is written, `View<T>` or `MutView<T>`, and gives `T` and the kind. `n` is a `u64`, and the call's value is `f`'s.
   - The program vouches for `n`, as it did for `c_read`'s length: this is the one place C's memory is trusted.
   - A null `p` panics unless `n` is 0 (an empty view). A `p` not aligned for `T` panics.
   - For an offset, use `p.view(off + n, fun(v: View<u8>) { … v.slice(off, off + n) … })`: no pointer arithmetic.
6. **Removed (Breaking):**
   - `c_read(p, off, len)` becomes `p.view(off + len, fun(v: View<u8>) = v.slice(off, off + len).to_array())`.
   - `c_write(p, off, bytes)` becomes `p.view(…, fun(v: MutView<u8>) { v.slice(off, off + bytes.len).copy_from(bytes); })`.
   - Callback `Ref<T[]>` becomes `MutView<T>`; callback `T[]` in a `direct` callback becomes `View<T>`, plus `.to_array()` if kept.
   - E0481 (a `spawned` callback taking `Ref<T[]>`) is no longer reported; its page says so.
7. **`realtime`:** views are allowed and their parameters are no longer E0482. Indexing, slicing, writing and `copy_from` are allowed; `to_array` allocates, so it is refused.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **`View<T>` and `MutView<T>`,** two types, rather than `mut View<T>`: the `mut` word can't appear in function types or on closure parameters, and a callback's type must say whether C's memory is written.
2. **Elements are integers, floats and extern structs holding only those,** so any bytes from C are a valid value (C#, Swift and Java draw the same line). `bool`, `char`, handles and `CPtr` stay out, inside structs too.
3. **One escape rule, checked on types,** like C#'s `ref struct`, and no lifetimes. A view in a `let` local is allowed, so slices can be named.
4. **A view may be held across a wait** (a `recv` inside `p.view`'s closure): the memory stays C's, and the program already vouches for how long it lives, as SQLite's "until the next step" shows the check can't. Callback views can't wait anyway (`direct` and `realtime` can't).
5. **The length is in `T`s, never bytes or frames.** A binding multiplies frames by channels itself, and overflow panics.
6. **Overlapping views are allowed** (C may hand the same buffer as input and output); `copy_from` copies as `memmove` does.
7. **No deprecation period:** `c_read`, `c_write` and callback `Ref<T[]>` go in this PRP. Earlier I said they'd be deprecated for a release first. But nothing in std or the prelude uses them, and a breaking change is listed under Breaking.
8. **Struct-held buffers** (zlib's `next_in`/`avail_in`, libusb's `buffer`/`length`) are read from the extern struct and passed to `p.view`. Strided and per-channel (`void **`) layouts aren't covered.
9. **Two codes:** E0483 for a view escaping and E0484 for a callback parameter that should be a view.

## REQUIREMENTS

### Must Do
- Design items 1–7. E0483 and E0484 with their `docs/errors/` pages; E0481's page marked as no longer reported.
- The language server's completion and hover know the methods of `View` and `MutView`, and of `CPtr.view`.
- Spec §8 (the views section, replacing "Arrays in callbacks" and `c_read`/`c_write`), §4's built-in table, §11 row 220, "Changes since v16" (Breaking); the Draft §4.8; CHANGELOG; TODO (the views item gone).
- `tests/c_memory.rs` and the callback cases rewritten to views.

### Must NOT Do
- Views of Lugha's own arrays, or general slices: a separate feature, if ever.
- Pointer arithmetic, typed reads at an offset (`read_u32_le`), or element types other than numbers.
- Lifetimes, or run-time liveness checks.

## TESTS
- [x] `tests/programs/c/views/` (errors; the rejected programs need no C):
  - [x] E0483 for each escape: return, a field, an `Option`, an array element, a capture, a channel send, a generic argument, an interface conversion, a result type;
  - [x] E0484: `T[]` in a `direct` callback, `Ref<T[]>` in any;
  - [x] E0409: `View<string>`, `View<bool>`; E0403: a `View` where a `MutView` is expected; E0415: writing through a `View`.
- [x] `tests/c_memory.rs` (with C):
  - [x] a `direct` callback reading a `View<i32>` and writing a `MutView<f64>`; C sees each write at once;
  - [x] `CPtr.view`: read, write, `slice`, `to_array`, `copy_from`, an empty view of `NULL`;
  - [x] panics: out of bounds, a null `CPtr` with `n > 0`, a misaligned `View<f32>`, `copy_from` of the wrong length.
- [x] `tests/rings.rs`: a `realtime` callback scaling a `MutView<f32>` in place on C's thread and reporting through a ring.
- [x] `--lib` unit tests: the escape rule (each case and the allowed ones: a `let`, a slice, passing down), `realtime` with views.
- [x] Targeted: `--lib`, `programs`, `c_memory`, `callbacks`, `rings`, `foreign_threads`, `scope_callbacks`, `explain`, `spec`, `comments`, `lsp_completion`.

## ACCEPTANCE
- [x] A `direct` or `realtime` callback reads and writes C's buffer in place, with no copy or allocation; a `spawned` one gets a copy.
- [x] A view can't leave the call it was given to, and the error says where it would go.
- [x] `c_read`, `c_write` and callback `Ref<T[]>` are gone, and every use in the tests is rewritten.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [x] Spec, the Draft, CHANGELOG, TODO, the error pages and the language server updated.

## BUILD NOTES
- **One change of scope, the user's call:** views of extern structs holding only integers and floats (`View<Val>`), since a WebAssembly host's buffers of structs would otherwise have had no way into a `direct` callback once `T[]` and `Ref<T[]>` went. Which structs qualify is checked once every struct is known (`views::plain_elements`), over the written types.
- **The escape rule** follows the checker's existing rule for scope and task handles (E0472): a view nested in any value's type is reported where that value is made; fields, variants, results and captures where they are declared; type arguments where a generic function is used. A view isn't `Sendable`, so a channel of views gets E0475 as well.
- **`MutView<T>` fits `View<T>`** in `Type::fits`: the two are one representation (`{ pointer, count }`), so nothing is converted.
- **Smaller calls made on the way:** `v.len` is an `i64`, as an array's; writing through a `View` is E0506 (as writing into a string); a view's `slice` panics with `slice out of bounds of a view`, without the numbers; `p.view` counts as reaching outside the program, so no constant calls it.
- **Completion** lists a view's methods and `len`, and `p.view(n, f)` through an entry in the built-in table that only completion reaches (calls are typed by `cptr_view`).
- **`runtime/cmem.c` is gone** with `c_read` and `c_write`; nothing in std or the prelude used them.
- **`src/check/callbacks.rs` was 559 lines before this PRP** (over the limit already); E0484's wording moved to `errors/views.rs`, leaving it at 547. A split is proposed in the report, not made.
- **Research:** the survey was written from the agent's own knowledge, not fetched pages; the spec and docs cite no source.
