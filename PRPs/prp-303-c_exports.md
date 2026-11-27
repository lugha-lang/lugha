## FEATURE: Lugha functions C calls by name, and shared libraries — milestone 284

## OBJECTIVE

An `extern fun` with a body is a Lugha function C calls by its C name; `lughac build --shared` makes a shared library of a program's exported functions, with a C header, that any C, Rust, Python or Kotlin (stage 2, PRP-304) host loads and calls.

```lugha
// words.la
extern fun word_count(text: string): i64 {
    text.split(" ").filter(fun(w: string): bool = w != "").len
}

extern "words_longest" fun longest(text: string): string {
    let mut best = "";
    for w of text.split(" ") {
        if w.len > best.len { best = w; }
    }
    best
}
```
```sh
lughac build --shared words.la -o libwords.so    # also writes words.h
```
```c
#include "words.h"
int64_t n = word_count("the quick brown fox");   // 4
char *w = words_longest("the quick brown fox");  // "quick", freed by the caller with free()
free(w);
```

From Lugha, an `extern fun` with a body is an ordinary function, called as any other.

🔒 **Security-sensitive:** C now calls into Lugha by name, and the runtime runs inside a process it doesn't own (no lockdown, no signal handlers of its own). This needs your review before any code.

## CONTEXT

- **Read first:** the spec's section 8 (extern functions, callbacks, `lugha_rt_cb_run`, threads C made), `src/codegen/c_calls.rs` and `c_abi*.rs` (C's calling rules, both directions for callbacks), `runtime/tasks/callbacks.c`, `runtime/tasks/stacks.c` (`lugha_rt_main`), `runtime/core/lugha_rt.c` (start-up: signals, stdio, lockdown), `src/driver/pipeline.rs` and `src/link/` (the link), `src/parser/` (extern declarations).
- **Spec:** section 8 gains "exported functions"; section 9's build gains `--shared`. A language change the Draft doesn't plan yet: this PRP is that plan (decision 230 when it ships).
- **Decisions:** the user chose (2026-11-27) `extern fun` with a body over a new mark (one word, C-facing, both directions; the body says which side defines it), and **no capabilities for exports in this PRP**: a later PRP may let a library's manifest grant some, supplied when C calls. Android's side (JNI, Kotlin) is PRP-304, on top of this.

## RESEARCH (summary)

- **Go** (`-buildmode=c-shared`, `//export`): the runtime starts when the library loads; a C thread calling in enters Go's scheduler and may block; strings in are copied, strings out are `C.CString`s the caller frees.
- **Rust** (`cdylib`, `extern "C"` + `#[no_mangle]`): no runtime; since 1.81 a panic reaching C aborts the process.
- **Nim, Haskell, OCaml:** the host must call an init function (`NimMain`, `hs_init`, `caml_startup`) first: a step every host forgets once.
- **Zig** `export fn`; **Swift** `@_cdecl`; **C** itself: `extern` for both directions.
- **Pony** (capability-secure): a library has no `Main`, so no authority unless the host passes it. The same here, for now.
- **Inside a host process,** a language runtime must leave process-wide state alone: ART (Android) and the JVM handle `SIGSEGV` themselves for null checks, so a library installing its own handler breaks the app.

## REQUIREMENTS

### Must Do

**The language**
- `extern fun name(params): R { body }` (and `= expr;`) declares a Lugha function exported under the C symbol `name`; `extern "symbol" fun name(…) { … }` under `symbol` (as for imports: a C identifier, not starting `lugha_`, E0306).
- Its parameters are numbers, `bool`, handles, `Option` of a handle, extern structs, `string` and `Option<string>` (copied from C's `const char *`, as a callback's); its result is any of those but `Option<string>`, or none. Anything else, `out`, `data`, `release`, a callback parameter, a capability or a type parameter is E0485, naming what C can't pass.
- A capability parameter is E0485 too, with help: "an exported function gets no capabilities: C has none to give".
- From Lugha, it is called as any function, by its Lugha name.
- Two exports with one C symbol in a build are E0486, at the second.

**Calls from C**
- Each export is a C function with C's calling rules (`c_sig`, the callbacks' code in reverse): it converts C's arguments, runs the body, converts the result.
- **It runs as a task:** the calling C thread hands the call to the runtime's workers and waits for its result, so the body may do anything a task does: start scopes and tasks, wait on channels, `join`, sleep. Every task it starts ends before it returns (scopes guarantee that). Several C threads may call at once; each call is its own task.
- **The runtime starts on the first call,** once, whichever thread makes it (no init function for the host to forget, and nothing at load time).
- **A returned `string`** is a `malloc`ed, NUL-terminated copy C owns and frees with `free()`; one holding a NUL byte panics (C couldn't tell where it ends).
- **A panic** in an export prints its message, as a task's panic does, and ends the process (exit 101): no value could stand for "failed", and unwinding never enters C.

**`lughac build --shared`**
- Builds a shared library of every export in the program (its packages' included), named as `-o` says (default `lib<stem>.so`; `lib<stem>.dylib`, `<stem>.dll` by target), with a C header beside it (`<stem>.h`): each export's C prototype, with `stdint.h` types and a struct for each extern struct used.
- A program with `main` is E0338 with `--shared` ("a shared library has no `main`: its exports are its entry points"); one with no export is E0338 too.
- Targets: Linux with glibc (x86-64 and ARM64), Android (both), macOS and Windows. Linux with musl is refused (E0338): its executables are static and it has no shared libraries to speak of.
- **Inside a host, the runtime leaves the process alone:** no signal handlers (a C stack's overflow isn't reported; a Lugha stack's still panics, by the entry check), no lockdown, stdio as the host set it, no Ctrl-C handling. It is never unloaded (`dlclose` keeps it).

**Tests**
- Parser and checker: an extern with a body parses (both forms); E0485 for each kind of parameter and result it refuses; E0486; an export called from Lugha.
- `tests/exports.rs` (Linux): a C host built with `cc` against `--shared` libraries: numbers, `bool`, strings in and out, extern structs by value, a handle passed through, an export that starts a scope of tasks and waits, two C threads calling at once, a panic ending the host with 101 and the message, the header compiling with `-Wall -Werror`, and E0338's cases.
- Android, Windows and macOS: a `--shared` build links (their hosts later: Android's in PRP-304).

**Docs:** spec section 8 (exported functions) and section 9 (`--shared`), the skill's extern page, `docs/errors/E0485.md`, `E0486.md`, `E0338.md`, §11 row 284, DECISIONS 230, CHANGELOG, TODO, CLAUDE.md's commands.

### Must NOT Do

- No capabilities for exports (a later PRP, with grants in `lugha.toml`).
- No arrays or views across, no `out` parameters, no callbacks taking or returning exports: later, as needed.
- No init function for the host to call, and no work at load time.
- No signal handlers, lockdown or stdio changes inside a host.
- No unloading.

## DESIGN

- **Parser:** `extern fun` gains an optional body; the AST's extern function holds it. An extern with a body is a function the checker types as any other, plus the export rules.
- **Codegen:** the body is an ordinary Lugha function (`lugha_fn_…`); the export is a C-ABI wrapper under the C symbol that converts arguments, calls `lugha_rt_export_run(body_thunk, frame)` and converts the result. The callbacks' argument code is reused.
- **Runtime** (`runtime/tasks/exports.c`, new): `lugha_rt_export_run` starts the runtime once (`pthread_once`-like, without `main`: workers, the C stacks, no signals or lockdown), queues a task running the thunk, and parks the C thread on a futex word until the task ends, then returns. A panic in the task reports and exits.
- **Link:** `--shared` links with `-shared` (and the NDK's, zig's or `cc`'s equivalents), the runtime compiled for a library (`LUGHA_RT_LIBRARY`, which skips the process-wide setup), and only the exports visible (`-fvisibility=hidden` for the rest).
- **Header:** written by the driver from the checked exports.
- **Errors:** E0485 (an export's parameter or result C can't pass), E0486 (two exports, one symbol), E0338 (`--shared` and a program's shape or target).
- **No new crate, no `unsafe` Rust.**
- **What a reviewer should check hardest:** the runtime started from a C thread mid-way through a host's life; nothing process-wide touched in library mode; the calling thread waiting without spinning; strings copied in and out, never a Lugha pointer handed to C.

## TESTS

- [ ] Parser: `extern fun f(x: i64): i64 { x }` and `= x;` parse; without a body as today.
- [ ] Checker: each refused parameter and result kind is E0485 (a capability with its help); E0486; calling an export from Lugha type-checks as a call.
- [ ] `tests/exports.rs`: the C host cases above, each output and exit compared.
- [ ] E0338: `main` with `--shared`, no exports, a musl target.
- [ ] Android, Windows, macOS: `--shared` builds link.
- [ ] `cargo test --test programs` unchanged (no program's behaviour changes).

## ACCEPTANCE

- [ ] A C host calls every kind of export, from several threads, and a panic ends it with 101.
- [ ] `cargo fmt --check`, clippy and the touched tests are clean; each file at or under 500 lines.

## DECIDED HERE (for review)

1. **Every export runs as a task, its caller waiting,** rather than in place on the caller's stack as a `direct` callback does: the body may use everything (scopes, channels, sleep), as Go's exports may. The cost is a hand-off per call (a few microseconds); an in-place form for hot paths can come later, marked in its syntax.
2. **The runtime starts on the first call,** not at load and not through an init function: no step for the host to forget (Nim, Haskell, OCaml), and nothing running before the host is ready (load-time constructors run inside `dlopen` and `System.loadLibrary`).
3. **A panic ends the process,** as Rust's does across C: an export has no value meaning "failed", and returning a zero (as a `direct` callback does) would hide it.
4. **A returned string is a copy C frees with `free()`,** as Go's `C.CString`: the C convention, nothing new to learn. A NUL inside panics rather than C seeing half of it.
5. **`--shared`, not `--lib`:** `lughac init --lib` already means a source package; `--shared` says a shared library, as Go's `c-shared` does.
6. **A header is written beside the library,** so C never declares an export by hand, wrongly.
7. **In a host, nothing process-wide:** no signal handlers (ART's and the JVM's own `SIGSEGV` handling must stay), no lockdown (the host's process isn't the program's to limit), stdio untouched.
8. **Musl has no `--shared`:** its builds are static by design.

## MADE WHILE BUILDING (for review)

9. **An exported function is an ordinary function with an export mark** (`FunDecl::export`): the parser reads `extern fun …` with a body into a function, so checking, codegen and the formatter treat it as any other, plus its C entry.
10. **One runtime build serves programs and libraries:** whether the runtime is up decides it. A program's `main` started it; otherwise the first export call starts it as a library, on a home thread whose `main` task only sleeps, with `c_may_call` true so the deadlock check never fires. The home thread says the runtime is up only once its `main` task runs, and a second caller during that start waits for it: two races a loop of 300 runs found and that 15 runs of the tests no longer show.
11. **Each export's C entry and body are marked as C's** (`lugha-c-entry`), so the passes that move struct results behind a pointer and split struct parameters leave them alone, as they leave callbacks' entries; a `Point { x: f64, y: f64 }` result came back swapped without it.
12. **`--shared` on musl is a command-line error (exit 2),** not E0338: there is no place in the program to point at.
13. **The runtime and `[link]` C files are compiled `-fPIC`** on every system but Windows (harmless in executables, needed in a library); the cache rebuilds them once.
14. **Windows' import library is named after the DLL** (`words.lib`), not the build's temporary object.
15. **Three files split, with the user's agreement:** `src/check/modules/entries.rs` (the checker's entry points, as `modules/mod.rs` reached 500 lines), and `src/driver/linking.rs` (the build-and-link step, as `pipeline.rs` was at 521 on main).
16. **A Windows DLL is tested under Wine** with a host zig builds, and every other target's `--shared` build is checked to link.
