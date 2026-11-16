## FEATURE: The memory prototype, second step: reference counting across tasks behind `--memory=rc`: tasks, channels and actors counted, values marked shared (with atomic counts) when they cross to another task, and a channel benchmark; milestone 191.

**Status:** built 2026-11-16, awaiting review.

## OBJECTIVE
```
$ LUGHA_RC_REPORT=1 LUGHA_RC_POISON=1 lughac run --memory=rc tests/programs/concurrency/channels/pipeline.la
…                                              # what it prints under the collector
rc: 4120 allocated, 4120 freed, 0 live
$ python3 bench/run.py chanping --lang lugha,lugha-rc,c,rust,go
```
Every end-to-end program with tasks, channels or actors runs the same under `--memory=rc` as under the collector, frees everything (cycles through `Ref` boxes, channels and handles aside), and never touches freed memory. Only C callbacks are still refused. A channel ping-pong benchmark measures what a send costs.

## CONTEXT
- Draft v18 §3.1 [MEM-01, MEM-02, MEM-04, MEM-15], §7.1 phase 3 ("reference counting with moves on last use and marking on send"), §7.2 [PRF-05 channel ping-pong, PRF-07, PRF-08].
- **A correction to PRP-222's outline.** Its part 2 named callbacks and `Pinned` (MEM-10), the cycle collector (MEM-06) and `Weak` (MEM-05). The Draft puts those in phase 6, with the multi-threaded runtime (§7.1). Phase 3 covers rules 1–4 and 15 only, so they stay out: building them now would build a feature before the Draft plans it.
- PRP-222 (milestone 190) built single-task counting. The prototype refuses programs using tasks, channels, actors or callbacks (`CodegenError::Unsupported`).
- **The runtime today:** green threads on one OS thread (`runtime/tasks.c`, `spawn.c`).
  - `chans.c` copies a value's bytes into a ring buffer or straight to a waiting receiver.
  - `spawn.c` and `actors.c` keep tasks, results, actors and messages in collected memory.
  - Codegen copies a spawned call's arguments and a joined result (`tasks.rs`), and actor arguments (`actors.rs`).
- **Decided with the user:** `runtime/process.c`'s scratch memory changes in this PRP (reviewed in the report). `argv` is freed after `posix_spawnp` returns (it copies), the two capture buffers after their bytes are copied into Lugha arrays, and on every failure path. Argument building, the spawn itself and lockdown don't change, and under Boehm nothing changes.

## DESIGN

### Shared counts (MEM-02)
- **The count word gains a shared bit**, bit 62. The header stays 16 bytes.
  - A shared object's count changes atomically (`__atomic` add and subtract, acquire-release). Freeing it follows an acquire fence.
  - The inline fast path keeps its shape, one unsigned compare: `1 ≤ count < 2^62` is plain and local. Shared, immortal and freed counts go to the runtime.
  - On one OS thread the atomics are uncontended. The point is to measure what they cost on the paths that use them.
- **Marking:** `lugha_rt_rc_mark(object)` sets the bit on an object and everything reachable from it that is still local, in one walk. Objects already shared are skipped, with what they hold.
  - The walk uses a work list, not recursion, so a million-node list doesn't overflow the stack.
- **Where values are marked:** whatever crosses to another task, at the crossing:
  - a value sent on a channel (`send`, a `select` send arm);
  - a spawned call's arguments and captures;
  - an actor message's arguments, and a call's result;
  - a task's result when `join` takes it.

### One function per object, two operations
- The header's drop becomes a **visit function**, `visit(object, op)`. `op` is either "release what you hold" (freeing) or "mark what you hold" (marking).
- Codegen's per-type functions (`lugha_rc_drop_box_<T>`, `_arr_<T>`, environments) take the operation, and marking glue joins retain and release glue. `lugha_rt_rc_drop_pointers` does both for arrays of pointers.
- The Draft's later cycle collector (phase 6) needs exactly this traversal, so it is built once.

### Tasks, channels and actors as counted objects
- `Chan<T>`, `Task<T>` and an actor's handle become counted types. Copies retain them and their last holder frees them.
- **Channels:** a send moves the sender's holder into the channel, and a receive moves it to the receiver as a fresh value.
  - A channel's visit releases the values still buffered, through its element's glue (set by `Chan.new`).
  - A `select` send arm that doesn't fire releases the value it prepared.
- **Tasks:** the running task holds its handle until it finishes. The result is stored once, with its holder.
  - `join` hands out the result marked and retained.
  - The task's visit releases a result nobody took.
- **Actors:** a running actor holds its own handle, and its state, until it ends.
  - A message holds its arguments until its handler runs; its memory is then freed.
  - A call's result block is freed once the caller has taken the result.
- **Runtime-internal records** (messages, result blocks, fibers) move to `rt_scratch` memory, freed where their life ends.

### Still out
- **C callbacks stay refused** under `--memory=rc` (`CodegenError::Unsupported`). They need `Pinned`, which comes in phase 6.
- **Cycles leak**, through `Ref` boxes, channels holding their own handles, or tasks and actors holding each other. Leaking programs are listed in `tests/rc_programs.rs`.
- **Unchanged:** the scheduler, `select`'s semantics, determinism and every program's output.

### Measuring
- `bench/programs/chanping/`: two tasks bounce an integer over two unbuffered channels N times, then print the last value.
- Versions in Lugha, C (two threads and condition variables), Rust (`std::sync::mpsc`) and Go (channels), per PRF-07. Node has no channels and shows `-`.
- `bench/RESULTS.md` gets a new entry: every benchmark under `lugha` and `lugha-rc`, with the marking in place.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The scope follows the Draft:** marking on send now; `Weak`, the cycle collector and `Pinned` in phase 6 (see CONTEXT).
2. **A shared bit in the count word**, not a second header word: the header stays two words, and the fast path stays one compare.
3. **One visit function with an operation**, replacing the drop: one traversal per type, for freeing now and marking now, and for the cycle collector later.
4. **A work list for marking:** marking a deep structure mustn't depend on stack depth.
5. **Real atomics on one OS thread:** the measurement is of their cost, which the decision needs; skipping them would flatter reference counting.
6. **A joined result is marked at `join`,** where it crosses back. One left in a task nobody joins is never marked.
7. **The benchmark is `chanping`,** sized for about half a second under Lugha.

## REQUIREMENTS

### Must Do
- The shared bit, atomic slow paths, `lugha_rt_rc_mark` with a work list.
- Visit functions with an operation, across codegen and the runtime.
- Counted `Chan`, `Task` and actor handles; marking at every crossing; channel, task and actor lifetimes; `select` send arms that don't fire.
- `process.c`'s scratch memory (approved), and the other runtime files' internal records.
- `chanping` in Lugha, C, Rust and Go, and a results entry.
- The tasks, channels and actors refusal lifted, and callbacks still refused.

### Must NOT Do
- Change anything without `--memory=rc`.
- `Weak`, the cycle collector, `Pinned`, scope memory limits, a multi-threaded allocator: phase 6.
- Decide between reference counting and plan B (the next PRP).

## TESTS
- [ ] `tests/rc_programs.rs`: the concurrency programs join; only callbacks are uncovered; cycles are listed as known.
- [ ] Unit tests (codegen): a send, a spawn's arguments and a joined result call `lugha_rt_rc_mark`; a channel's visit releases buffered values; the inline fast path stays one compare.
- [ ] End to end (`tests/programs/concurrency/rc_sharing/`): a structure sent to another task and dropped by both ends; a channel dropped with values still buffered; an actor stopped with messages queued; a task never joined. Each prints the same under the collector and reaches `0 live` under rc.
- [ ] The benchmarks, `chanping` included: the same output in every language.
- [ ] Targeted runs: `--lib`, `programs`, `rc_programs`, `codegen`; CI the rest.

## ACCEPTANCE
- [ ] Every covered program runs the same under `--memory=rc`, with nothing leaked beyond the listed cycles and no use after free.
- [ ] Without the flag nothing changes.
- [ ] `chanping` and the rest are measured under both, and recorded.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Every `pub` item is documented. The spec's prototype note, §11 row 191, CHANGELOG and `bench/README.md` are updated.

## CHANGES DURING THE BUILD
- **Unfinished threads let go when `main` returns** (`lugha_rt_rc_stop_threads`): a thread still running at the end (an idle actor, a `forever` loop) held its environment and task, so every such program reported live objects. What only its stack holds still stays: stopping doesn't unwind.
- **A panicking actor keeps what its frames held** (`concurrency/actors/supervisor.la`, listed as a known gap): the panic ends its thread without unwinding, which is phase 4's.
- **`tests/rc_programs.rs` no longer scans sources** for what the prototype doesn't cover: lughac's refusal (exit 2) marks such programs, and the test skips them. A program importing a module that uses C callbacks is caught the same way.
- **The runtime records** (message records, a task's result block, a channel's ring) are runtime memory freed with their owner. A call's result block is freed by the caller once it has the value (`lugha_rt_rc_block_done`).
- **`chanping` sends an `i64`**, so it measures channels and task switches, PRF-05's target. A counted value bounced back and forth would mostly measure copy-on-write: the receiving binding and its copy hold the value twice, under the collector as under counting. C and Rust hand off between OS threads, so they are 50 to 100 times slower than Lugha's green threads, and the size is set so C stays well inside the 60-second limit.
- **`rc.rs`'s tests moved to `rc_tests.rs`** to keep the file under 500 lines.
- **Targeted runs:** `--lib`, `programs`, `codegen`, `rc_programs`, `bench`; CI the rest.
