## FEATURE: A per-task cycle collector for `Ref` boxes: groups of boxes that hold only each other are found by trial deletion and freed, so `Ref` cycles, closures stored in a box they capture included, no longer leak; milestone 240.

**Status:** approved 2026-11-21.

## OBJECTIVE
```
struct Node { name: string, next: Option<Ref<Node>> }

fun make() {
    let a = Ref.new(Node { name: "a", next: None });
    let b = Ref.new(Node { name: "b", next: Some(a) });
    a.update(fun(n) = Node { name: n.name, next: Some(b) });   // a ⇄ b
}

fun main(console: Console) {
    for i in 0..1000 { make(); }
    console.println("done");
}
```
```
$ LUGHA_RC_REPORT=1 lughac run cycles.la
done
rc: 3002 allocated, 3002 freed, 0 live; 1000 cycles freed (2000 boxes)
```
Today it ends with `2000 live`. Programs that make no cycle-prone boxes run exactly as before.

## CONTEXT
- **Ledger and Draft:**
  - MEM-06: cycles form only through `Ref`, channels and handles; a small per-task collector, in budgeted slices at deterministic points.
  - MEM-08: a report of freed cycles with creation sites, under `lughac test`.
  - Draft §3.1 rules 6 and 8.
  - This PRP covers MEM-06 for `Ref` and MEM-08's report. `Weak<T>` (MEM-05) and MEM-08's lint follow as their own PRP; channel and handle cycles (MEM-06's rest, MEM-07) after that.
- **Spec:**
  - §7 Memory model;
  - §11's out-of-scope list, whose line "a cycle collector, for `Ref` boxes that hold each other" was added in error (the Draft plans it) and is removed;
  - CLAUDE.md's known issue "`Ref` boxes that hold each other in a cycle are never freed".
- **Research:** the user's discovery of 2026-11-21:
  - Bacon & Rajan's synchronous trial deletion;
  - Nim's ORC (the closest design: type filter, root buffer, adaptive threshold);
  - PHP's adaptive threshold;
  - CPython's 3.14 incremental collector, reverted after garbage outran its budget;
  - Firefox's work-sliced collector;
  - Pony for cross-task cycles (later).
- **Code:**
  - `runtime/rc.c` (the `(visit, count)` header, `RC_SHARED`, `lugha_rt_rc_slow`, `lugha_rt_rc_free` and its queue from milestone 239, `lugha_rt_rc_mark`'s walk over `visit(object, RC_MARK)`);
  - `runtime/rc_check.c` (the owner check's tagged counts, which a cell's tag must live beside);
  - `runtime/tasks.c` (`Fiber`, `lugha_rt_task_preempt`);
  - `src/codegen/refs.rs` (`Ref.new`), `rc.rs` (the inline `lugha_rc_inc`/`lugha_rc_dec`), `rc_objects.rs` (generated visits).
  - New: `runtime/cycles.c`.
- **Facts measured in discovery:**
  - A `Ref` cycle (all its objects), a closure in a box it captures (all) and a channel holding its own sender (all) leak today.
  - A `Ref` never crosses tasks, and a shared object can never reach a `Ref` (sendability rules it out), so every `Ref` cycle lies inside one task.
- **Security:** none of CLAUDE.md's list, but it is the runtime's memory management: a mistake frees live memory. No `unsafe` Rust, and no new dependency.
- **Decisions:** none open.

## DESIGN
1. **Cycle-prone boxes, by type.**
   - A `Ref<T>` is cycle-prone when a value of type `T` can reach a `Ref`, through fields, payloads, elements, `Option`s, tuples, maps and sets.
   - So is one whose `T` can reach a function value or an interface value that doesn't require `Sendable`, since either may hide a `Ref`. (A `Sendable` value can't hold one.)
   - It is decided per concrete type, at each generic instance, by codegen.
   - `Ref<i64>`, `Ref<string[]>` and `Ref<Map<string, i64>>` aren't cycle-prone, and their code is unchanged.
2. **A cell's count.**
   - A cycle-prone box is allocated with a tagged count, beside the owner check's tags. Its retains and releases therefore leave the inline fast path for `lugha_rt_rc_slow`, and other objects' fast paths don't change.
   - The tag also carries the collector's colour and whether the box is a suspect.
3. **Suspects.**
   - A cell whose count drops but stays above zero joins its task's suspect list (on the `Fiber`), once.
   - The list keeps each entry's position with the box, so a suspect freed by ordinary counting leaves the list at once (no dangling entry).
4. **Collection: synchronous trial deletion** (Bacon and Rajan), run by the task itself. Each phase runs over every suspect before the next:
   1. **mark gray:** for each suspect, subtract one from each object it reaches, walking through any local object;
   2. **scan:** what stays above zero is held from outside, so it and what it reaches get their counts back;
   3. **collect white:** what reached zero is freed through the ordinary free path.

   Walking reuses each object's `visit`: in a collector walk, the runtime's handler for a child does the phase's work. So no generated code changes.

   The walk stops at shared objects, which can't lead back to a `Ref`, and at constants. Only the task's own, non-atomic counts are touched, and no lock is taken.
5. **When.**
   - At the task's next scheduling point (`lugha_rt_task_preempt`, a wait or a yield) once its suspect list reaches a threshold; never in the middle of an expression.
   - The threshold starts at 256 suspects and adapts by each run's yield:
     - when a run frees at least half of what it visits, the next threshold is two-thirds of this one (at least 64);
     - otherwise it is half again larger (at most 100 000).
   - A task's remaining suspects are collected when it ends.
   - Every trigger counts suspects, never time, so a deterministic run collects at the same points every time.
6. **Nothing a program can see.**
   - Freeing runs no Lugha code, so output and exit codes are unchanged.
   - `LUGHA_RC_CHECK`'s owner check keeps working, and `LUGHA_RC_POISON`'s tombstones apply to collected objects too.
7. **The report.**
   - `LUGHA_RC_REPORT=1`'s line adds `N cycles freed (M boxes)` when a run freed any.
   - `LUGHA_RC_REPORT=2` also lists where each collected box was made (`main.la:4:13`, from `Ref.new`'s place, which codegen passes for cycle-prone boxes only, kept in the report's side table).
   - Under `lughac test`, a test whose run freed cycles gets a note line with the sites. It isn't a failure.

## DECISIONS MADE IN DISCOVERY (the user chose the order; these are mine, for review)
1. **Synchronous trial deletion per task, not incremental.** A run is bounded by its suspects and what they reach, and the adaptive threshold keeps it small. Slicing one collection across mutations, as only Firefox does, would need a write barrier. CPython's reverted incremental collector shows the risk of a budget falling behind. Slices come later only if a measured pause asks for them.
2. **The count's tag, not a new header word,** so the 16-byte header and every pointer stay as they are.
3. **Closures and non-`Sendable` interface values count as cycle-prone,** since the type can't show what they hold. That is conservative; it never misses a cycle.
4. **Stop at shared objects:** safe because nothing shared reaches a `Ref`, and it keeps the collector inside its task, so no locking is needed.
5. **Creation sites only in the report's side table,** so normal runs pay nothing for them.

## REQUIREMENTS
### Must Do
- Design 1–7.
- **Benchmarks:** `python3 bench/run.py --check` shows no regression for the programs without cycle-prone boxes, and the `Ref`-heavy ones are measured and stated.
- **Spec:**
  - §7 (the collector: what is collected, when, what it can't see);
  - §11 (remove the out-of-scope line; row 240);
  - a "Changes since v17" bullet.
- **CLAUDE.md:** the known issue becomes "channels holding their own senders, and handles in a cycle, are never freed (no cross-task collector yet)".
- **The skill:** `Ref`'s cycle note, in structs-and-enums or functions.
- CHANGELOG.
### Must NOT Do
- Collect cycles through channels, actors or task handles (a later PRP).
- Add `Weak<T>`.
- Change any count operation for objects that aren't cycle-prone boxes.
- Run Lugha code during collection.

## TESTS
- [ ] `tests/programs/runtime/cycles/` (each run with `LUGHA_RC_REPORT=1`, expecting `0 live` and the cycles line):
  - [ ] two boxes;
  - [ ] a ring of 1 000 boxes;
  - [ ] a closure stored in a box it captures;
  - [ ] a tree with parent back-links;
  - [ ] a cycle holding strings and arrays;
  - [ ] a cycle still held from outside, which isn't freed and reads correctly afterwards;
  - [ ] a cycle left when a task ends, which is freed then;
  - [ ] a cycle holding a channel end or an actor handle (shared objects inside are let go of, not traversed).
- [ ] `--lib`, a cycle-prone check per type:
  - [ ] `Ref<i64>`, `Ref<string[]>`, `Ref<Option<Ref<i64>>>`, a struct reaching `Ref`, `Ref<fun()>`, `Ref<Shape>`, and a `Sendable` interface;
  - [ ] codegen: `Ref<i64>` keeps the inline count, and `Ref<Node>` takes the tagged one.
- [ ] `tests/rc_cycles.rs`:
  - [ ] `LUGHA_RC_REPORT=2` lists creation sites;
  - [ ] `LUGHA_RC_POISON=1` with collection finds no use after free;
  - [ ] the same output and collection points under `LUGHA_SCHED=deterministic` across runs;
  - [ ] the owner check (`LUGHA_RC_CHECK=1`) with cycles.
- [ ] Targeted: `--lib`, `programs`, `rc_cycles`, and the existing rc tests (`grep -l LUGHA_RC tests/*.rs`), `workers`, `spec`, `skill`, `comments`; `bench --check`.

## ACCEPTANCE
- [ ] Every `Ref` cycle a task drops is freed, at the latest when the task ends.
- [ ] No live object is freed (poisoned runs clean).
- [ ] Programs without cycle-prone boxes run the same code as before.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
