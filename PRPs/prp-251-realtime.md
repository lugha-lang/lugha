## FEATURE: `cb: realtime fun(…)`: a `direct` callback the compiler checks never allocates or waits, in its body or in any Lugha function it calls (Clang's `nonblocking` model), an error otherwise; milestone 219, the last §4.8 PRP (borrowed views of C's memory come after, in a PRP of their own).

**Status:** approved 2026-11-21.

## OBJECTIVE
A sensor or MIDI callback on C's real-time thread is checked to be safe there:

```
extern fun midi_start(cb: realtime fun(i64, i64, data), u: data);

fun main(console: Console) {
    let (notes, incoming) = Ring<i64>.new(256);
    midi_start(fun(note: i64, velocity: i64) {
        if velocity > 0 {
            match notes.try_send(note) { .Sent => {}, _ => {} }   // fine: never waits
        }
    });
}
```

```
error[E0482]: a `realtime` callback can't allocate: this string is built at run time
  --> midi.la:6:33
   |
 6 |         console.println("note {note}");
   |                         ^^^^^^^^^^^^^
   = note: `realtime` callbacks run on C's real-time thread, where allocating can stall it
```

## CONTEXT
- **Spec:** the Draft, §4.8 rule 4 and the ledger's CBK-04 (revised 2026-11-21): `realtime fun(…)` is a `direct` callback checked at compile time, an error, not a lint. Spec §8 (callbacks, `direct`/`spawned`), §4 (`Ring<T>`, channels).
- **Research (discovery):**
  - Clang's `[[clang::nonblocking]]`/`nonallocating` are explicit marks, checked through the call graph; an unmarked callee is assumed to allocate and block; indirect calls are unsafe unless their type carries the mark (FunctionEffectAnalysis).
  - Rust's `assert_no_alloc` marks a zone explicitly and checks at run time.
  - The real-time rules (Bencina): no allocation, no locks, no waiting, no I/O.
- **The user's calls:**
  - the word `realtime` (explicit), its check an error;
  - this PRP first, borrowed views of C's buffers next. Until then a `realtime` callback takes numbers, `bool`, handles and structs by value, not arrays or strings (they're copies).
- **Code:** the checker's callback plan (`check/callbacks.rs`: `CParam::Callback`), the typed program (`Checked`: callees, conversions, captures, instances), a new pass `check/realtime.rs`; parser (`CallbackRun::Realtime`); codegen treats `realtime` exactly as `direct`.
- **Decisions:** none open.
- **Security:** nothing on CLAUDE.md's list.

## DESIGN
1. **Syntax:** `cb: realtime fun(…)`, a third word beside `direct` and `spawned`, before `fun`. It runs as `direct` does (during C's call, on C's thread, can't wait, captures `Sendable`).
2. **The check (E0482):** the closure or function passed for a `realtime` callback, and every Lugha function it calls (prelude and std included, generic ones per instance), may not:
   - **Allocate:**
     - build text: interpolation, `+` on strings, conversions to `string`, `show`;
     - make or grow an array, map or set: literals, `push`, `map`, `collect`;
     - write an element of an array (which may copy it first);
     - `Ref.new`, convert to an interface value (a box), make a closure that captures, make a channel or ring;
     - `spawn`, start a task or actor.
   - **Wait:** a channel's `send`, `recv`, `try_send` and `try_recv` (they lock), `select`, `join`, `task.sleep`, `yield_now`, an actor call, a scope, I/O through capabilities (`console`, files, sockets), a `blocking` extern.
   - **Call what it can't see:** a function value or an interface value's method, whose target isn't known when compiling.
3. **Allowed:**
   - arithmetic, comparisons, `match`, loops, locals of plain types;
   - reading captures, strings and arrays;
   - structs and enums by value;
   - `try_send` on a `RingSender` (the one way out);
   - plain externs (the declaration vouches for C, as everywhere);
   - panics (an error path, reported as a direct callback's panic is).
4. **Parameters:** a `realtime` callback can't take `string`, `Option<string>`, `T[]` or `Ref<T[]>` (each a copy C's call makes): E0482 at the declaration, pointing to the next PRP's views.
5. **The error** names the construct and, when it is reached through calls, the chain: ``… `fill` calls `helper` (line 12), which builds a string (line 30)``.
6. **Dropping the last holder of a value may free it.** That is implicit in reference counting and not checked; with nothing allocated in the callback, only values handed back (`.Full(v)`) can be freed there. This is stated in the spec.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **One code, E0482,** for everything `realtime` forbids, its message saying which rule; the fix is always "do less here, hand it to a task through a `Ring`".
2. **The check works on the typed program in the checker,** not on LLVM IR: errors point at source and run with `lughac check` and in the editor. Its list of what allocates is conservative (an array element write counts, as it may copy).
3. **Plain externs are trusted** and `blocking` ones refused: the extern declaration is already where a program vouches for C.
4. **Calls through function or interface values are refused** (Clang's rule for unmarked indirect calls), since their targets aren't known; named functions and methods are followed.
5. **A channel's `try_send` and `try_recv` count as waiting:** they take the channel's lock, which a reader on another thread may hold. A `RingSender`'s `try_send` is the allowed way out.
6. **Panics are allowed,** as an error path that can't be ruled out (overflow, bounds) without making `realtime` unusable.

## REQUIREMENTS

### Must Do
- Design items 1–6; E0482 with its `docs/errors/` page.
- Spec §8 (`realtime`), §11 row 219, "Changes since v16"; the Draft §4.8 (rule 4's `realtime`); CHANGELOG; TODO (the `realtime` item gone, the views PRP added); the language server and VS Code grammar know `realtime` (extension version bump).

### Must NOT Do
- Borrowed views of C's memory: the next PRP.
- Run-time checking (Rust's `assert_no_alloc`, Clang's RealtimeSanitizer): compile time only.

## TESTS
- [x] `tests/programs/c/realtime/`:
  - [x] a `realtime` callback writing a ring, doing arithmetic, calling a helper and a plain extern: checks and runs;
  - [x] E0482 for each rule: interpolation, an array literal, `push`, an element write, `Ref.new`, an interface conversion, a capturing closure, a channel's `send` and `try_recv`, `task.sleep`, `console.println`, a `blocking` extern, a function-value call, a `string` parameter;
  - [x] E0482 through a chain of two helpers, naming both;
  - [x] a generic helper (unit tests): one passing values allowed, one calling a type parameter's method refused (see the notes).
- [x] `tests/rings.rs` (beside the ring cases, which have the C harness): a `realtime` callback on C's thread filling a ring, on one and four workers.
- [x] `--lib` unit tests for the analysis (each rule, the chain, recursion between helpers).
- [x] Targeted: `--lib`, `programs`, `foreign_threads`, `rings`, `callbacks`, `explain`, `spec`, `parser`, `fmt`, `lsp_semantic_tokens`, `comments`; `editors/vscode` `npm test`.

## ACCEPTANCE
- [x] A `realtime` callback that allocates or waits, directly or through Lugha functions it calls, doesn't compile, and the error says where and why.
- [x] One that only computes and writes rings compiles and runs as a `direct` callback.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [x] Spec, the Draft, CHANGELOG, TODO, the error page and the editor tooling updated.

## BUILD NOTES
- **The pass** is `src/check/realtime/` (the walk in `walk.rs`, unit tests in `tests.rs`), run after type checking over the typed program. It starts at each argument for a `realtime` parameter and follows named functions and methods (prelude and std included) once each; a cycle of helpers is walked once.
- **Generic helpers are checked once, not per instance:** a type parameter's method or operator can't be followed (its code depends on the instance), so it counts as a call that can't be checked. That is stricter than the PRP's "per instance", but simpler and never wrong; a helper that only passes `T` values along is allowed.
- **`console.println` is reported as an interface method** (`Console` is an interface value), which is what it is: the message says it can't be checked, rather than "I/O". Std's own I/O, reached through named functions, ends in `$os.*` and is reported as waiting.
- **The allowed case is a unit test,** not a program in `tests/programs/c/realtime/`: a program there would need its C function linked. The run itself is `tests/rings.rs`.
- **The E0482 cases** are three programs: every rule in `e0482.la` (16 errors), a chain of two helpers, and refused parameter types.
- **The editor:** the language server colours `realtime` before `fun` as a keyword; the VS Code grammar and the extern snippet know it (extension 0.39.0).
