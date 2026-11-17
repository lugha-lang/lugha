## FEATURE: The memory prototype, last step: count elimination where the measurements point, a size-class allocator under `--memory=rc`, fib measured with every new per-call cost (PRF-09), and the written decision between reference counting and plan B; milestone 192.

**Status:** built 2026-11-17, awaiting review.

## OBJECTIVE
```
$ python3 bench/run.py --lang lugha,lugha-rc,lugha-rc-calls,c,rust,go
| binarytrees | 1.084s | ≤ 1.19s | …            # PRF-02: at most 10% slower than the collector
$ LUGHA_MEMORY=rc lughac run bench/measure/pauses.la 16
longest pause: … ms                            # PRF-04, under each model
```
`bench/RESULTS.md` ends with the decision written up for the user: every §7.2 target the prototype can measure, met or missed under reference counting, what the misses would cost to close, and a recommendation (reference counting, or plan B). DECISIONS.md gets the question as `open` for the user to resolve. Nothing changes without `--memory=rc`.

## CONTEXT
- Draft v18 §3.1 [MEM-03, MEM-14, MEM-15], §7.1 phase 3 ("it decides between reference counting and plan B"), §7.2 [PRF-01..04, PRF-07, PRF-08, PRF-09].
- PRP-222 (m190) and PRP-223 (m191) built counting behind the flag. Measured at m191 (median of 5, `-O2`):

  | | Boehm | rc | target |
  |---|---|---|---|
  | binarytrees time | 1.084 s | 2.428 s | ≤ 1.19 s (PRF-02) |
  | binarytrees memory | 70.3 MB | 25.9 MB | ≤ 35 MB ✓ |
  | wordfreq time | 0.533 s | 0.575 s | ≤ 0.59 s ✓ |
  | wordfreq memory | 81.6 MB | 138.4 MB | no worse ✗ (aim 47 MB) |
  | fib, fannkuch, mandelbrot, nbody | | equal | within 3% ✓ |
  | chanping | 0.285 s | 0.277 s | within 1.5× of Go (2.80 s) ✓ |

- **Where binarytrees' time goes** (discovery; `perf` isn't allowed on this machine, so a C model of the same objects):
  - glibc `calloc`/`free` with the 16-byte header: 1.35 s, already over the target;
  - the same work on size-class free lists: 0.49 s;
  - Lugha under rc: 2.49 s. The other ~1.1 s is counting: per node, two `lugha_rt_rc_set_drop` calls, nine calls releasing statement temporaries that were moved out (zeroed, then released at run time), and the tail retaining the new node's children and then releasing the temporary holding them.
- **Where wordfreq's memory goes:** two million strings from `split`, each carrying the 16-byte header plus malloc's chunk overhead, and `picked` (two million pointers) held to the end of `main` though its last use is `join`.
- **Decided with the user:**
  - A single-thread size-class allocator is built under rc now: plan B would get one too, so measuring rc on glibc's malloc would decide on the wrong thing. The many-threads allocator (MEM-14) stays in phase 6.
  - Once a model is chosen, a value returned from inside `for … of mut` through a type's `index` is its own copy and doesn't see the element handed back later (rc's behaviour). Nothing changes under Boehm in this PRP; the switch-over PRP updates `interfaces/read_modify_write/loops.la`.

## DESIGN

### The allocator (rc only)
- `runtime/rc_alloc.c`, a new fragment: free lists for sizes up to 256 bytes in 16-byte classes, carved from 64 KB chunks taken from `malloc`; anything bigger goes to `malloc` directly.
- A small object is header + contents rounded up to its class. The header stays two words:
  - a small object's class comes from its chunk (chunks are 64 KB aligned, and a chunk's first word holds its class);
  - a large object's block gets a size word before the header, in place of the overhead malloc adds anyway.
- Freed small objects go back on their class's list. Memory isn't returned to the system (the prototype's programs don't need it; phase 6's allocator does).
- `LUGHA_RC_POISON` keeps working: poisoned objects are never put back on a list.
- One OS thread today, so the lists are plain globals; the file says so.
- **Allocation takes the visit:** `lugha_rt_rc_alloc(size, visit)` sets the header in the same call, replacing `lugha_rt_alloc` + `lugha_rt_rc_set_drop` under rc.

### Count elimination
1. **Moved temporaries aren't released at all:** when codegen knows statically that a temporary was moved out (into a constructor, a store, a return), it emits neither the zeroing store nor the release. The run-time zero-and-release stays only where a move happens on some paths and not others.
2. **A function's tail moves its value out:** a fresh tail value is returned as it is, not retained and then released with its statement.
3. **Bindings are released at their last use,** not at the end of their block, using the liveness analysis moves already use (milestone 149). Programs can't observe it (MEM-03: counts aren't observable; nothing is closed when its holder goes, MEM-12).
4. **Releases of types holding no counted value in the variant taken are skipped inline:** a release of an enum checks the tag inline and calls the release glue only for a variant holding a counted value (`Leaf` costs a compare, not a call).
- Each step is measured on its own; RESULTS.md records what each gained. If a step gains nothing it is dropped, not kept for symmetry.

### PRF-09: fib with every new per-call cost
- `LUGHA_CALL_COSTS=1` (read by lughac like `LUGHA_MEMORY`, documented only in `bench/README.md`) adds to every Lugha function what phases 4 and 6 will add, so the fib target is checked against all of them together:
  - **the stack check at entry:** the stack pointer compared with a limit in a global, with a cold call on the far side;
  - **the countdown:** a global budget decremented at entry and at loop back-edges, with a cold call that resets it at zero;
  - **cleanup paths for unwinding:** in a function holding counted values, calls become `invoke`s whose landing pad releases them and resumes. Nothing ever unwinds (a stub personality function that is never called); the point is the hot-path cost of having the paths.
- `bench/run.py` gains `lugha-rc-calls` (rc with `LUGHA_CALL_COSTS=1`).

### PRF-04: pauses
- `bench/measure/pauses.la`: keeps a large tree alive while repeatedly building and dropping smaller ones, timing each iteration with the `Clock` capability, and prints the longest. Run under both models; recorded in RESULTS.md.
- A measure of the two memory models, not a cross-language benchmark, so it lives outside `bench/programs/` and has no C, Rust or Go versions (PRF-07 covers benchmarks compared across languages).

### The write-up
- `bench/RESULTS.md`, a final section: every measurable §7.2 target under rc after this PRP's work, against the target and the collector; what the prototype can't measure yet (parallel binarytrees, idle task size: phase 6's runtime); and a recommendation, with the reasons.
- DECISIONS.md: "Memory model: reference counting or plan B" as `open`. The user resolves it; the next PRP acts on it (rc becomes the default and Boehm goes, or the prototype is removed for plan B).

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Free lists in 16-byte classes up to 256 bytes, 64 KB chunks, never returned to the system:** the smallest allocator that removes malloc from the measurement.
2. **The class comes from the chunk, not the header:** the header stays two words. Shrinking it to one (PRP-222 noted the second word's cost) is measured against wordfreq's memory only if wordfreq still misses after steps 1–3.
3. **Release at last use** counts as count elimination: it uses the same liveness as moves, and nothing observable changes.
4. **The call-cost simulation is an environment variable, not a CLI flag:** it's a measuring tool, not something programs are built with.
5. **Landing pads with a never-called personality** model unwinding's hot-path cost without building unwinding (phase 4).
6. **`pauses` isn't a cross-language benchmark** and has only a Lugha version.
7. **Targets the prototype can't measure** (parallel efficiency, idle task size) are listed as unmeasured, not estimated.

## REQUIREMENTS

### Must Do
- The size-class allocator and `lugha_rt_rc_alloc(size, visit)`, rc only.
- Count elimination steps 1–4, each measured.
- `LUGHA_CALL_COSTS` and `lugha-rc-calls`; `bench/measure/pauses.la`.
- The write-up in RESULTS.md and the open DECISIONS.md entry.

### Must NOT Do
- Change anything without `--memory=rc` (or `LUGHA_CALL_COSTS`).
- Make rc the default or remove Boehm: the user decides first, then the next PRP.
- Build unwinding, the countdown's yielding, stack growth, the many-threads allocator, deferred freeing (MEM-09), `Weak` or the cycle collector.

## TESTS
- [ ] `src/codegen/rc_tests.rs`: a moved temporary has no release; a fresh tail value isn't retained; a binding is released after its last use, before later statements; an enum release checks the tag inline; rc allocation is one call taking the visit.
- [ ] Runtime: `tests/programs/runtime/rc_alloc/` (sizes on both sides of 256 bytes, reuse after free, poisoning) under rc, through `tests/rc_programs.rs`, which runs every program under rc.
- [ ] `tests/rc_programs.rs` stays green: every covered program prints the same under rc and frees everything.
- [ ] Call costs: a unit test that `LUGHA_CALL_COSTS` adds the entry check, the countdown and `invoke`s; fib's output is the same with it.
- [ ] Targeted runs: `--lib`, `programs`, `rc_programs`, `codegen`, `bench`; CI the rest.

## ACCEPTANCE
- [ ] Every covered program runs the same under `--memory=rc`, with nothing leaked beyond the listed cycles.
- [ ] Without the flag nothing changes.
- [ ] Every benchmark measured under `lugha`, `lugha-rc` and `lugha-rc-calls`, pauses under both models, recorded with the decision write-up.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Every `pub` item documented; spec prototype note, §11 row 192, CHANGELOG, `bench/README.md` updated.

## CHANGES DURING THE BUILD
- **Two changes outside `--memory=rc`, approved with the user mid-build:**
  - **Enum payloads by `getelementptr`:** the split pass reached a payload's later words through `ptrtoint`/`inttoptr`, which kept LLVM from turning those slots into registers. A struct view (no new `unsafe`) fixes it. binarytrees under the collector went from 1.18 s / 72 MB to 0.66 s / 38 MB, and most of rc's dead releases folded away with it.
  - **The runtime is compiled at `-O2`:** it had no optimization flag. chanping went from 0.28 s to 0.11 s under either model; rc's wordfreq from 0.63 s to 0.44 s.
  - Both move the baseline: RESULTS.md compares rc with the collector as it is now, and gives the Draft's baseline too.
- **Zeroing a reused block** uses fixed-size stores falling through from its class: a `memset` of a size known only at run time was a libc call on every allocation.
- **Step 4 (enum releases checking the tag inline) was dropped:** after the payload fix LLVM already folds those calls, so it had nothing left to gain.
- **Release at last use covers `let` locals** declared by a plain name; pattern bindings still go at their block's end. A local is let go after the statement in its own block holding its last mention, unless that is the block's last statement, the mention is in the tail or a `defer`, or the name is declared again before then.
- **The header stays two words.** wordfreq still misses, but the size-class counts show why: its two million short strings take 32 or 48 bytes each, against 16 or 32 under the collector. A one-word header with 8-byte classes would take them to 24 or 32 (about 105 MB peak, estimated from the class counts), still above the collector. Only a smaller string representation would close the gap, so it is in the write-up rather than built.
- **The call-cost simulation has no landing pads.** Turning calls into `invoke`s would mean teaching the passes that rewrite calls after lowering (struct returns, split parameters) about `invoke`, which is unwinding's own work. fib, PRF-09's measure, holds no counted value, so it would have no cleanup path anyway. The simulation is the stack check and the countdown; `LUGHA_CALL_COSTS` is read by the driver into a third `Memory` value, `RcCallCosts`.
- **`pauses`** replaces one of 256 live trees per round and counts rounds the millisecond clock sees take 2 ms or more.
- **Allocator test:** `tests/rc_programs.rs` also runs `tests/programs/runtime/rc_alloc/` without poisoning, so freed blocks are really reused.

