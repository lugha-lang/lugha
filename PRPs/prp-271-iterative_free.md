## FEATURE: Freeing walks a list, not the stack, so a long chain of values (a million-node list) is let go of without overflowing it; milestone 239.

**Status:** approved 2026-11-21.

## OBJECTIVE
```
enum List {
    Nil,
    Cons(i64, List),
}

fun main(console: Console) {
    let mut list = List.Nil;
    for _ in 0..1_000_000 {
        list = List.Cons(1, list);
    }
    console.println("built");
}
```
Today (m238) this ends with a segfault, exit 139, and prints nothing: `built` is lost with the unflushed output. A hundred thousand nodes do the same; ten thousand work. After this change it prints `built` and exits 0.

The same bug turns the spec's own example of a stack overflow, "walking a list of a million nodes recursively" (§9, "Recursion depth"), into exit 139 instead of 101: the panic prints, then unwinding lets go of the list and overflows again.

## CONTEXT
- **Found by:** lugha-dev, drafting its course's Stack overflow step (2026-11-21), in its sandbox at m238. Three programs, a recursive `sum` over a million-node list (expected `panic: stack overflow`, exit 101), the same `sum` as a `while let` loop (expected `1000000`), and the objective's program, all exit 139.
- **Cause:** `lugha_rt_rc_free` (`runtime/rc.c`) calls the object's visit with `RC_RELEASE`; the visit releases what the object holds, and a count reaching zero calls `lugha_rt_rc_free` again. A list frees one C frame per node, on the task's 8 MiB stack (§7, "Task stacks"). Marking already avoids this: `lugha_rt_rc_mark` queues objects on `worker->marking` while a walk is in progress.
- **Spec:**
  - §7 "Counting": no limit on how much one release frees; §3 "Recursive data": "copying a list of any length takes constant time".
  - §9 "Recursion depth": `main`'s overflow "ends the program with exit 101".
  - This PRP adds one sentence to §7 "Counting", gives §11 row 239 and a "Changes since" line.
- **Decisions:** none open bear on it. No new decision: it follows the marking walk's design (milestone 191).
- **Code:** `runtime/rc.c` (`lugha_rt_rc_free`), `runtime/lugha_rt.c` (the worker struct, beside `marking`). Every free goes through `lugha_rt_rc_free`: generated code's inline release calls it at zero (`src/codegen/rc.rs`), and so does the runtime's `rc_change`.

## REQUIREMENTS

### Must Do
- `lugha_rt_rc_free`, called while the same worker is already freeing (inside a visit), marks the object `RC_FREED` and queues it on a per-worker list instead of visiting it. The outermost call visits and frees its object, then each queued one in turn, until the list is empty.
- Every object is still visited exactly once and its block freed after its visit, as today. A double free still dies with `double free`, and a constant is still never freed.
- The list grows like `marking` (doubling, from 64; `out of memory` if `realloc` fails) and is per worker, so no locking.
- `LUGHA_RC_REPORT`'s freed count, `LUGHA_RC_POISON`'s tombstones and `LUGHA_RC_CHECK` behave as today.
- Docs: the spec's §7 "Counting" sentence ("Freeing, like marking, walks a list rather than the stack, so a chain of any length is let go of without recursion"), §11 row 239, "Changes since", CHANGELOG, and the comments on `lugha_rt_rc_free` and the worker field.

### Must NOT Do
- Change generated code, the object header or any visit function: the fix is the runtime's free alone.
- Change when objects are freed (still at the release that drops the count to zero, before that call returns), so `defer`s, `Dir` closing and the report see the same order of events within one release, only without nesting.
- Touch the marking walk, the allocator (`rc_alloc.c`) or cross-thread block return.

## DESIGN
- **Worker fields** (`lugha_rt.c`, next to `marking`): `void **freeing; int64_t freeing_len, freeing_cap; bool releasing;`.
- **`lugha_rt_rc_free`:**
  ```
  check RC_FREED (die) / RC_IMMORTAL (return), as now
  header->count = RC_FREED
  if worker->releasing: push object; return
  worker->releasing = true
  rc_free_one(object)                 // visit RC_RELEASE, freed++, block free
  while freeing_len > 0: rc_free_one(pop)
  worker->releasing = false
  ```
  Depth stays constant for a list: freeing node 1 queues node 2, the loop pops it, its visit queues node 3. A wide object (an array of a million structs) queues its elements as its visit releases them, so the list's size is bounded by the number of objects let go of at once, as `marking`'s is.
- **Panics and unwinding:** a visit never runs Lugha code (it releases counts, closes a `Dir`, frees OpenSSL state), so nothing unwinds out of the loop and `releasing` can't be left set. A `die` (double free, out of memory) ends the process.
- **Threads:** `worker` is the current thread's; a task freeing on one worker drains its list before the release returns, so a task moving to another worker later never leaves work behind.
- **No new diagnostics, no ABI change, no new dependency, nothing security-sensitive** (no `unsafe` in Rust; the C runtime's existing patterns).

## TESTS
- [ ] `tests/programs/runtime/long_chains/`, new cases (each `.la`, `.stdout`, `.exit`):
  - `free_list.la`: the objective's program at a million nodes, prints `built`, exit 0.
  - `loop_over_list.la`: builds a million-node list and sums it in a function with `while let`, prints `1000000`, exit 0.
  - `free_nested_boxes.la`: a million `Ref`s each holding an `Option` of the next, let go of at once, exit 0.
- [ ] `tests/programs/control/unwinding/list_overflow.la`: a recursive `sum` over a million-node list, with a `defer` in `main`: stdout from the `defer`, stderr `panic: stack overflow`, exit 101.
- [ ] `tests/long_chains.rs` (added while implementing): the cases above fail only on a small stack, so this builds each and runs it under `prlimit --as=256 MiB`, where task stacks shrink to 1 MiB (lugha-dev's sandbox limit), comparing stdout, stderr and exit with the case's files; skipped without `prlimit`. Without the limit the same cases exit 0 (measured, at ten million nodes too), so `tests/programs/` alone doesn't catch the bug.
- [ ] `tests/leaks.rs` runs the new cases with poisoning and the report: every object freed.
- [ ] `cargo test --lib`, `--test programs`, `--test leaks`, `--test rc_check`, `--test rc_alloc_threads`.

## ACCEPTANCE
- [ ] The objective's program prints `built` and exits 0; the recursive `sum` over a million nodes exits 101 with `panic: stack overflow`.
- [ ] Every existing `tests/programs/` case prints and exits as before, and `tests/leaks.rs` still finds every object freed.
- [ ] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Spec (§7 sentence, §11 row 239, "Changes since") and CHANGELOG updated.
- [ ] Tried by hand: `cargo run -q -- run` on the objective's program, and lugha-dev's three programs in its drafting sandbox once the user rebuilds it.
