# TODO — lugha

What's next. Shipped work is in CHANGELOG.md (and the spec's §11 milestone table); questions that need a decision go in DECISIONS.md. Every feature below needs a PRP before code. Done items are removed, not ticked.

---

## Known bugs and limits

BUGS.md holds the bugs understood but not fixed, with the fixes weighed. The ones found writing the Lugha skill (milestones 230–237) are fixed; CLAUDE.md's "Known issues" lists the specified behaviour that looks like a bug.

## Language and library

- [ ] **JSON, later:** per-field options (renaming, skipping, defaults), an `@json` attribute, base64 bytes.
- [ ] **Boxes and bytes** (v16 §12): editing a box in place (`r.with`); a bounds-checked byte buffer.
- [ ] **`Proc`:** streaming pipes to a running child and timeouts (found with oyster). Needs the user's security review.
- [ ] **Parsing values back from text.** Also module paths in printed type names, so two modules' `Point`s print differently.
- [ ] **One-element tuples.**
- [ ] **`.len()`** as a method beside the `.len` field.
- [ ] **Format specifiers chosen at run time.**
- [ ] **Literal patterns on `f64`.**
- [ ] **Warnings:** an irrefutable `if let`.
- [ ] **Small values stored inline in interface values** (an optimisation only).
- [ ] **The stack check: measure and optimise** (after PRP-237): fib is 35% slower and fannkuch 4% (target 3%). Skip the check in functions whose frames are small and that call nothing (as Go does), and move it past a function's early returns (shrink-wrapping: fib's base cases then check nothing).
- [ ] **Maps: measure and optimise** (after PRP-236, user's call): SipHash-1-3 made `wordfreq` about 18% slower (target 10%). End a string with a `0xff` byte in a buffered byte stream, as Rust does, instead of a length word (about 4 rounds for a short word instead of 6); then a SwissTable-style table (groups of control bytes), where most of the gap to Rust's 0.15 s lies.
- [ ] **Packages, more:** a registry; `ssh` URLs; branches or commits as requirements; two versions of one package in a build; full version ranges and pre-release versions.
- [ ] **Imports, more:** re-export chains (`m.v.w.X`), a `--package` flag, package-level (not `public`) re-exports.
- [ ] **Listing a git package's `extern` functions at `lughac update`.** Needs a security review, like PRP-079.
- [ ] **Checking the program's own `[package]` version,** e.g. against a minimum `lughac` version.
- Ruled out by the user (session 127): full Unicode `char` classification and case tables (§11's exclusion stands).

## Concurrency (v17)

- [ ] Built-in supervisors (restart policies), links/monitors; a stack overflow inside an actor ending only that actor. (Generic actors, actors as interface values, bounded mailboxes and actors on several threads shipped in milestones 201–210.)
- [ ] **Guest mode for loop-owning frameworks** (after PRP-247, the user's call: later, alongside macOS): a wake fd and "run the tasks ready on this thread, then return", so `gtk_main()`, Qt's `exec()` and Cocoa's run loop deliver queued callbacks (the Draft, §4.8).
- [ ] **Cancelling's lock, only if measured:** a scope's own spawn and finish bookkeeping has its own lock (PRP-245), but failing, cancelling, deadlines and shields still share the scopes lock. If a program shows it contended, move them to the scope locks, parent before child, as Go's `context` does.
- [ ] **One task spawning many, on one worker** (after PRP-245, the user's call to ship with the miss): `bench/measure/scaling` spawn_many 1.41 s against 1.33 s at milestone 212. Its tasks are all queued before any finishes, so the stack cache never gets one back, and each finished stack past the cache still gives its pages back (`madvise`, about 40% of a spawn); without that call it ran in 0.78 s. Ideas: keep every finished stack's pages until its worker parks (pool included), at the cost of holding a burst's memory while the program stays busy; a scope lock taken in name only on one thread, as the scopes lock was.
- [ ] **The `park` false deadlock under ThreadSanitizer, rarely** (`tests/scheduler_stress.rs`): milestone 214's fix took it from about 1 run in 100 to none in 600, but it came back once during PRP-250's run (none in 200 more runs then). A timer racing the last worker's deadlock check, still; find it with the harness looping the case under ThreadSanitizer.
- [ ] **chanping: measure and optimise** (after PRP-243, the user's call to ship with the miss): 0.29–0.38 s against 0.18 s at milestone 209; on one worker 0.36 s since PRP-244, whose channel locks are real atomics even then (the shared lock was free on one thread). Where it goes, from a C harness: re-reading the thread pointer on every runtime access (a task may move threads, so a cached thread-local address would be wrong), the lock's sections on each send and receive, the run-next exchanges. Ideas: pass the worker down explicitly through the hot paths so it is read once per call; a channel lock per channel (next PRP) so a ping-pong takes no shared lock; checking whether the compiler keeps a thread-local address across calls only where it can't (asserted by a test on the runtime's assembly).
- [ ] **Waiting: measure and optimise** (after PRP-240): the parking handoff made `chanping`, two tasks ping-ponging over a channel, about 13% slower (0.154 s to 0.176 s), and the thread-local `Worker` record (PRP-241) another 3%; the other benchmarks didn't move. Most of it is the extra state checks on every wait and wake (`is_waiting`'s two compares, `park_finish` at each switch), measured in a C harness against the runtime; try numbering the wait states so one compare tests both, and completing the park inside the switch itself. Measure again once several workers run, where the compare-and-swaps come back.
- [ ] **Helper threads: quick calls inline.** A small file read through a helper costs about 21 µs against 3 µs inline (`bench/RESULTS.md`): try each call in place first and hand it over only when the kernel says it would wait (`openat2`'s `RESOLVE_CACHED`, `preadv2`'s `RWF_NOWAIT`), and measure again.

## Portability — one platform at a time

Done: portable x86-64 binaries (`--cpu`), `$LUGHA_CC`, Linux on ARM64 with AAPCS64 struct passing (milestones 130–134); foreign targets through `zig cc` and Linux musl (milestone 247, Draft Part 9 step 1); `run`/`test --target` under qemu, ARM and musl in CI (milestone 248, step 2). The rest is v18's Draft, Part 9.

- [ ] **`[build]` in `lugha.toml`** (`cpu`, target): not in the Draft's Part 9 yet.
- Step 3 (C1) is done: the OS layer and the runtime's folders (milestone 249), sockets at the operation level (milestone 250).
- [ ] **C2 — Mac runtime, the rest** (hello world runs: milestone 251; helper threads, pipes, signals and limits checked against XNU: milestone 253; folders confined: milestone 254; lockdown, sockets and processes: milestone 255; the program suite on CI's Mac: milestone 256): Intel Macs (`os/darwin/x86_64.c`), and last, `run`/`test` for a Mac target (lughac itself on a Mac). TLS and crypto for a Mac build: milestone 257; certificates checked by the Mac's own verifier: milestone 258.
- [ ] **C3 — Mac calling rules and linking:** Apple's AArch64 differences (variadics), Mach-O, signing; built and tested on a Mac runner.
- [ ] **D1–D3 — Windows:** runtime (files, processes, env, stack overflow), sockets, Windows x64 calling rules, `.exe` linking with MinGW first. Large.
- [ ] **E — WebAssembly (wasm32-wasi):** decisions first (tasks with no threads, or none; which capabilities and std modules exist). Large.

## Compiler internals

- [ ] **MIR playground** (`docs/research/mir.md` §5): an experimental stage beside codegen, `--emit=mir` and an interpreter checked against compiled programs; not needed for performance yet (§4). Integers and control flow first.

## Editor support and tools

- [ ] **Expression error nodes** — keep a broken statement with a hole, so `foo(p).` on the broken line completes too.
- [ ] **Lexer recovery** — tokens up to an unclosed string, so a fresh `"` without auto-closing doesn't lose the tree.
- [ ] Format Selection (the statements or items a selection touches).
- [ ] Formatter: break long binary chains and method chains; `fmt --check` in CI once the repo's own `.la` files are formatted (moves panic lines).
- [ ] Later quick fixes: did-you-mean for any undefined name (E0301), an import for a user module.
- [ ] **Go to Implementations** — from an interface method to every type's.
- [ ] **A read-only view of bundled sources** (`std`, the prelude) for hover and definitions that land there.
- [ ] **Bundling** the extension (vsce warns about 231 files; needs a bundler such as esbuild as a dev dependency).
- [ ] **Publishing** the extension to the Marketplace and Open VSX (publisher accounts, a token as a CI secret).
- [ ] Tests in the editor, later: a Debug profile (needs debug info), test items from unsaved text, UTF-16 columns from `lughac test --list`.
- [ ] **Outline and breadcrumbs** (`textDocument/documentSymbol`).
- [ ] **Go to Symbol in Workspace** (`workspace/symbol`, `Ctrl+T`).
- [ ] **Highlight uses** (`textDocument/documentHighlight`).
- [ ] **Inlay hints** (`textDocument/inlayHint`) — a `let`'s inferred type and parameter names at calls.
- [ ] **Iterator helpers as methods:** `it.map(f)` and the other helpers of any `Iter` still come from a name table (`check/iter_helpers.rs`); make them `Iter`'s default methods once interface defaults can take their own type parameters.
- [ ] **`min`/`max` on floats:** `math.min`, `math.max` and `math.clamp` take `T: Ord`, which floats aren't since milestone 168; give floats their own (`math.fmin`/`fmax` following IEEE `minNum`, or `T: Float` versions), decided in a PRP.
- [ ] **Imported enum qualifiers:** in `Msg.Launched` with `Msg` imported (`import m { Msg };`) the checker records no definition for `Msg`, so Go to Definition, References and semantic tokens miss it (found building milestone 154).
- [ ] **Run from the editor** — a "▶ Run" code lens running `lughac run`; needs a security review (it starts programs).
- [ ] **Debugging** — breakpoints and stepping through a Debug Adapter; needs debug info (DWARF) from codegen first. Large.
- [ ] **Call hierarchy** (`callHierarchy/*`).
- [ ] **Go to Type Definition** (`textDocument/typeDefinition`).
- [ ] **Smart expand selection** (`textDocument/selectionRange`).
- [ ] **Folding by syntax** (`textDocument/foldingRange`).
- [ ] **A `lughac doc` tool** (HTML from doc comments).

## Ideas — noted, not scheduled

- **Decorators:** user-written `@name` attributes that wrap a function, as Python's do (`@logged`, `@memo`); `@` is already reserved for them. Needs a way to wrap any signature (variadic generics or compile-time rewriting). Also `@test(skip)`-style arguments.
- **`@cfg` on items** (DECISIONS 208): code for one OS or architecture, every target checked; with portability stage C1.
