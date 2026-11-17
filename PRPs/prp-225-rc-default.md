## FEATURE: Reference counting becomes the only memory model: the Boehm collector, `--memory` and the collector's code paths are removed, and what the prototype left uncovered (C callbacks, folder and hasher handles, `-O0` frame size) is covered; milestone 193.

**Status:** proposed 2026-11-17.

## OBJECTIVE
```
$ lughac run prog.la                 # counted, always: no collector linked
$ LUGHA_RC_REPORT=1 lughac run prog.la
…
rc: 4120 allocated, 4120 freed, 0 live
$ lughac build --memory=gc prog.la
error: unexpected argument '--memory' found      # exit 2
```
Every end-to-end program prints what it printed under the collector, except `interfaces/read_modify_write/loops.la` (decided: a returned value doesn't see a later hand-back). Every program that ends normally frees everything, except the listed `Ref` cycles and a panicking actor's frames. `third_party/bdwgc/` is gone.

## CONTEXT
- Decision 223 (2026-11-17): reference counting is the memory model. Draft v18 §3.1 rules 1–4 [MEM-01..04], §7.1 phase 3.
- Milestones 190–192 built counting behind `--memory=rc`; it covers every program except:
  - C callbacks, refused (`--memory=rc doesn't support C callbacks yet`);
  - `Ref` cycles (`types/shared_boxes/linked.la`);
  - a panicking actor's frames (`concurrency/actors/supervisor.la`);
  - two `-O0` stack-depth programs (`runtime/slot_sharing/popover_depth.la`, `runtime/small_frames/deep_match.la`), whose recursive frames grow under counting;
  - `Dir` handles and SHA-256 hashers, which the collector's finalizers closed and freed (`runtime/dir.c`, `runtime/crypto.c`); under counting they stay until exit.
- **What depends on the collector:**
  - `src/link/gc.rs` (compiles bdwgc; its object is linked into every program) and `third_party/bdwgc/` (1.8 MB, vendored);
  - `#ifdef LUGHA_RC` branches across `runtime/` (≈80 lines), fibers' stack scanning in `tasks.c`, callbacks' uncollectable pins;
  - copy-on-write's sticky shared flag (the top bit of an array's `cap`) in codegen and the runtime;
  - `Memory::Gc` and `--memory` / `LUGHA_MEMORY` (`src/driver/cli.rs`, `pipeline.rs`, `codegen/emit.rs`);
  - tests compiling the runtime with the collector's headers (`tests/codegen.rs`, `tests/tasks_runtime.rs`); `bench/`'s `lugha-rc` column.
- **Decided with the user:** `Ref` cycles leak until the cycle collector and `Weak` (phase 6). The spec lists it as a known issue, and `linked.la` stays on the leak test's known list.
- Security review needed: `src/link/` changes (the collector's compile and its object leave the link line).

## DESIGN

### Removal
- **Gone:** `third_party/bdwgc/`, `src/link/gc.rs` and its cached object, the `gc.o` on the link line, `-isystem` for its headers. Also every `#ifndef LUGHA_RC` branch and the `GC_*` shims; the runtime is compiled without `-DLUGHA_RC` (the code is just there).
- **`--memory` and `LUGHA_MEMORY` are removed** (breaking). `LUGHA_CALL_COSTS=1` stays, the benchmarks' tool: `Memory` becomes a plain `call_costs: bool` passed the same way.
- **Copy-on-write by count only:** the sticky shared flag and its codegen go. An array's `cap` is its capacity.
- **Codegen:** every `if self.rc()` becomes unconditional, and the collector's `else` paths go.

### Covering the rest
- **C callbacks** keep today's rule: a closure handed to C lives until the program exits or C calls the extern's `release`. `lugha_rt_cb_pin` takes a holder on the closure's environment (marked shared, since C may hand it to any thread later). `lugha_rt_cb_release` lets it go. This is today's behaviour, not `Pinned<T>` (MEM-10, phase 6).
- **Folder and hasher handles become counted:**
  - a new internal handle type for runtime-made resources (the `state` of `Dir` and `Sha256`), counted like a string;
  - the runtime makes them with `lugha_rt_rc_alloc` and a visit that closes the descriptor or frees the OpenSSL context;
  - so they go when their last holder does, as the finalizers did. `CPtr` for C's own pointers stays uncounted;
  - a `Dir` open that fails with `EMFILE` no longer collects to free descriptors (`dir_reclaim`): there is nothing left unreachable to find.
- **`-O0` frames:** counting's temporaries and result slots use the same per-block slot sharing other temporaries use (`frames.rs`), not a slot each, so recursive frames are no larger than under the collector.
- **Fibers** become runtime memory, freed when the fiber ends; nothing scans stacks.

### Tests
- `tests/rc_programs.rs` becomes **`tests/leaks.rs`**: every program run with `LUGHA_RC_POISON=1` and `LUGHA_RC_REPORT=1` must end with `0 live`. Its known list: `linked.la` (`Ref` cycle, phase 6) and `supervisor.la` (a panic doesn't unwind, phase 4). `tests/programs.rs` itself is unchanged and now runs everything counted.
- `loops.la`'s expected output changes to the decided behaviour.
- The callback programs (`tests/programs/c/callbacks/`) run, and their closures go when released.

### Docs
- **Spec:**
  - §7 says reference counting where it described the collector;
  - the prototype paragraph becomes the rules;
  - known issue: `Ref` cycles leak, and so does a panicking actor's stack;
  - "Changes since v16", §11 row 193.
- **CLAUDE.md:**
  - the stack entry (Boehm out, the allocator in);
  - architecture rule 7 (why allocation goes through `lugha_rt_alloc`: every object gets its header);
  - known issues (an integer looking like a pointer no longer keeps anything alive; `Ref` cycles leak instead);
  - the `--memory` command line.
- **README and bench:** `lugha-rc` is now `lugha`, and `lugha-rc-calls` becomes `lugha-calls`; the binarytrees comment drops Boehm.
- **CHANGELOG** gets a Breaking entry.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **`--memory` is removed, not kept as a no-op.** One model, so the flag can't mean anything, and silently accepting `gc` would mislead.
2. **Callbacks keep today's lifetime rule** (until exit, or C's `release`), as a held count. `Pinned<T>` waits for phase 6.
3. **Runtime resources get a counted handle type** and are closed or freed at their last holder, as the finalizers did. Nothing user-visible changes; MEM-12's warning for open files is phase 6, and files are closed explicitly already.
4. **Counting's slots share stack space per block,** the rule other temporaries follow, rather than the two `-O0` programs being dropped or shrunk.
5. **The leak test runs every program** (as `rc_programs` does), renamed for what it checks now.
6. **`LUGHA_CALL_COSTS` stays** for the benchmarks; `Memory` shrinks to that flag.

## REQUIREMENTS

### Must Do
- Remove bdwgc, `src/link/gc.rs`, `--memory`, `LUGHA_MEMORY`, the collector's runtime and codegen paths, and the sticky shared flag.
- Callbacks, counted runtime handles, shared slots, fibers as runtime memory.
- `tests/leaks.rs`; `loops.la` updated; every other program unchanged.
- Spec, CLAUDE.md, README, bench, CHANGELOG.

### Must NOT Do
- The cycle collector, `Weak`, `Pinned<T>`, deferred freeing, warnings for unclosed files, a multi-threaded allocator: phase 6.
- Unwinding (phase 4).

## TESTS
- [ ] `tests/programs.rs` passes with everything counted; `loops.la` updated.
- [ ] `tests/leaks.rs`: every program frees everything, but the two known ones.
- [ ] A callback program: the closure runs, and after `release` its environment is freed (`0 live`).
- [ ] `std.crypto` and folder programs: a hasher and a `Dir` dropped are freed (`0 live`); opening and dropping many folders never runs out of descriptors (`tests/programs/capabilities/…/many_dirs.la`, new).
- [ ] The two `-O0` programs pass.
- [ ] `tests/codegen.rs` (runtime compiles without warnings), `tests/tasks_runtime.rs`, `tests/arm.rs` (cross build), `tests/spec.rs`, `--lib`, `bench`, `comments`.
- [ ] CLI: `--memory` is an unknown argument (exit 2).

## ACCEPTANCE
- [ ] No collector is compiled or linked; `third_party/bdwgc/` is gone.
- [ ] Every covered program runs the same, and frees everything.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Every `pub` item documented; spec, CLAUDE.md, CHANGELOG and bench updated.
