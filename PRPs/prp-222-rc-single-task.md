## FEATURE: The memory prototype, first step: reference counting for single-task programs behind `lughac build --memory=rc` (Boehm stays the default): a count on every heap object, copies as count increments, values freed when their last holder goes, copy-on-write by count, moves on last use; measured against the benchmarks; milestone 190, the first of v18's Phase 3.

**Status:** proposed 2026-11-16.

## OBJECTIVE
```
$ lughac run --memory=rc bench/programs/binarytrees/binarytrees.la 16
stretch 17 262143
…
$ LUGHA_RC_REPORT=1 lughac run --memory=rc tests/programs/collections/…/x.la
…
rc: 1834551 allocated, 1834551 freed, 0 live          # nothing leaked, nothing freed twice
```
Every end-to-end program that uses no task, channel, actor or C callback prints the same under `--memory=rc` as under Boehm, and frees everything it allocated. The benchmarks run under both, and the results are recorded beside the baseline.

## CONTEXT
- Draft v18 §3.1 [MEM-01, MEM-03, MEM-04, MEM-13, MEM-14, MEM-15], §7.2 [PRF-01..03, PRF-07..09], ordering [ORD-03]: reference counting replaces Boehm, conditional on the performance targets; plan B is a heap per task. Phase 3 is a prototype that decides between them, built before unwinding.
- Decided with the user: **the prototype is built in the compiler, behind a flag**: if the targets are met it becomes the real implementation and the flag goes; if not, it is dropped for plan B.
- Measured before any change (PRF-07): `bench/RESULTS.md`'s new baseline, saved this session on the same i5-4308U as October's; Phases 1–2 left every benchmark within noise of October.
- The codegen survey (this session): strings, arrays, `Ref`, tasks, channels and actors are pointers to collected memory; structs, tuples and enums are values, an enum's payload raw words (pointers found only through its tag), a self-referencing payload boxed (`construct.rs`); function values `{code, env}`; interface values `{table, box}` with a box-copy slot in the table; string literals constant globals. Copies are centralised in `src/codegen/copy.rs` (deep copy today, arrays marked shared, `lugha_copy_<T>` for structs); `defer`'s machinery (`run_defers`) already runs code at every block exit, `return`, `break`/`continue` and failing `?`; temporaries aren't tracked; `frames.rs` shares stack slots of disjoint lifetimes.

## THE WHOLE PROTOTYPE (three PRPs)
1. **This one:** single-task reference counting for every value kind; tests proving no leaks and no double frees; the benchmarks measured.
2. **Next:** tasks, channels and actors (marking on send, atomic counts for shared objects, MEM-02), C callbacks and `Pinned` (MEM-10), the cycle collector for `Ref` (MEM-06) and `Weak` (MEM-05); a channel benchmark.
3. **Last:** count elimination where the measurements point, the PRF-09 fib measurement with the other per-call costs simulated, and the decision written up for the user: reference counting (the flag becomes the default and Boehm goes) or plan B.

## DESIGN

### Objects
- Every object `lugha_rt_alloc` makes under `--memory=rc` has an 8-byte header before it: its count. The pointer the program holds still points at the object, so every layout and every runtime function reading an object is unchanged. The memory comes from the C library's `malloc` (MEM-14's multi-threaded allocator is a later choice; one task needs none).
- **Immortal objects:** string literals, `include_bytes` arrays and other constants get a header holding a count that increments and decrements never touch (the top bit set), so they are never freed.
- `lugha_rt_retain(p)` adds one; `lugha_rt_release(p, drop)` takes one and, at zero, calls `drop(p)` (which releases what the object holds) and frees it. Codegen emits both inline (a load, an add, a store, a branch) and calls the runtime only to free.

### What holding means
- A value of a type with heap parts is **owned** (this code must release it) or **borrowed** (someone else holds it). Locals from `let` own their value; parameters, `for … of` variables and pattern bindings borrow (the `borrowed` flag `Local` already has).
- **A copy is a count increment:** `value_for_store` retains instead of deep-copying, through a generated `lugha_retain_<T>` for structs, tuples and enums (a tag switch for an enum's payload) and the box's table slot for interface values. Strings, arrays and boxes are retained directly.
- **Freeing:** a generated `lugha_drop_<T>` per type releases what a value holds (fields, elements, payload by tag, a closure's environment, an interface's box through a new table slot). An owned local is dropped at every exit of its block, through the same path `defer` uses; a local assigned again drops its old value first.
- **Temporaries:** an expression that makes a new value (a call's result, a concatenation, a format, a literal array or struct) gives an owned temporary. Each is recorded when made and dropped at the end of its statement unless something took it over: a store, a `let`, a return, or an argument handed over on its last use.
- **Moves on last use** (milestone 149's `moves`, `hands`, `keeps`): a moved value transfers its count instead of retaining and releasing.

### Copy-on-write by count
`own`/`own_place` (cow.rs, places.rs) clone an array when its count is above one instead of testing the sticky shared flag; a clone retains each element. Under `--memory=rc` no array is ever marked shared: the count says it.

### The rest of the compiler and runtime
- `frames.rs`'s slot sharing is turned off for locals that own heap values under `--memory=rc` (their lifetime now ends at the block's end, not their last use).
- Runtime objects made in C (strings from formatting and concatenation, arrays from growth and runtime calls, `Dir` handles) get headers through the same `lugha_rt_alloc`; the `Dir` handle's finalizer becomes its drop. Boehm isn't linked under `--memory=rc`.
- `LUGHA_RC_REPORT=1` prints, at exit, how many objects were allocated, freed and still live; `LUGHA_RC_POISON=1` fills freed memory with a pattern and checks counts on release, so a use after free or a double release panics (`panic: double free`) instead of corrupting memory.
- A program using what the next PRP covers (`spawn`, `Chan`, actors, `select`, C callbacks) is refused under `--memory=rc` with a clear message (`lughac: --memory=rc doesn't support tasks yet`, exit 2).

### Measuring
`bench/run.py` gains a `lugha-rc` column (the same programs built with `--memory=rc`). The results go in `bench/RESULTS.md` beside the baseline, with peak memory; nothing is decided yet.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **A header before the object**, not a field inside it: no layout, runtime function or C interop changes.
2. **The C library's `malloc`** for the prototype: one task needs no multi-threaded allocator, and it keeps the measurement about counting, not about an allocator.
3. **Temporaries tracked per statement**, not per expression: the simplest rule that frees everything; tightening it is part 3's count elimination.
4. **The flag on `build`, `run` and `test`** (`--memory=rc`), and an environment variable `LUGHA_MEMORY=rc` for runs that can't pass flags (the language server ignores both: it never builds).
5. **Leak and double-free checks as the test oracle:** every end-to-end program in scope must report `0 live` and survive poisoning.

## REQUIREMENTS

### Must Do
- The flag and the environment variable; `--memory=rc` builds link no Boehm.
- Headers, immortal constants, `lugha_rt_retain`/`release`/`free`, poisoning and the report.
- `lugha_retain_<T>`/`lugha_drop_<T>` for every value kind; copies as retains; drops at block exits and reassignment; temporaries; moves.
- Copy-on-write by count.
- `frames.rs` slot sharing aware of owned locals.
- `bench/run.py`'s `lugha-rc` column and a results entry.

### Must NOT Do
- Change anything when the flag isn't given: Boehm, every layout and every test stay as they are.
- Tasks, channels, actors, callbacks, `Pinned`, `Weak`, the cycle collector (the next PRP).
- Decide between reference counting and plan B (the last PRP).

## TESTS
- [ ] `tests/rc_programs.rs`: every end-to-end program in `tests/programs/` that uses no task, channel, actor or callback, built with `--memory=rc` and `LUGHA_RC_POISON=1 LUGHA_RC_REPORT=1`: the same stdout and exit code as its `.stdout`/`.exit`, and `0 live` (programs refused for using a task are listed as skipped).
- [ ] The six benchmarks under `--memory=rc`: the same output as under Boehm.
- [ ] Unit tests (codegen): the IR of a copy is a retain, of a block's end a drop, of a moved value neither; copy-on-write tests the count.
- [ ] Runtime: an immortal string is never freed; poisoning catches a double release.
- [ ] Targeted runs: `--lib`, `programs` (Boehm unchanged), `rc_programs`, `codegen`; CI the rest.

## ACCEPTANCE
- [ ] Every in-scope end-to-end program runs the same under `--memory=rc`, with nothing leaked and no double free.
- [ ] Without the flag, nothing changes.
- [ ] The benchmarks' `lugha-rc` times and peak memory are recorded beside the baseline.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass
- [ ] Every `pub` item documented; spec (the memory model gains a "prototype" note; §11 row 190), CHANGELOG and CLAUDE.md updated where they change
