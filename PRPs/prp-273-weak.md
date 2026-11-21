## FEATURE: `Weak<T>`, a reference to a `Ref<T>` that doesn't keep its box alive, and a note under `lughac test` for each test whose run the cycle collector had to clean up; milestone 241.

**Status:** approved 2026-11-21.

## OBJECTIVE
```
struct TreeNode {
    name: string,
    kids: Ref<TreeNode>[],
    parent: Option<Weak<TreeNode>>,      // doesn't keep the parent alive
}

fun add(parent: Ref<TreeNode>, name: string): Ref<TreeNode> {
    let kid = Ref.new(TreeNode { name: name, kids: [], parent: Some(Weak.new(parent)) });
    parent.update(fun(p) {
        let mut kids = p.kids;
        kids.push(kid);
        TreeNode { name: p.name, kids: kids, parent: p.parent }
    });
    kid
}

fun parent_name(node: Ref<TreeNode>): string = match node.get().parent {
    .Some(w) => match w.get() {
        .Some(p) => p.get().name,
        .None => "(gone)",
    },
    .None => "(root)",
};

fun main(console: Console) {
    let root = Ref.new(TreeNode { name: "root", kids: [], parent: None });
    let leaf = add(root, "leaf");
    console.println(parent_name(leaf));    // root
}
```
```
$ LUGHA_RC_REPORT=1 lughac run tree.la
root
rc: N allocated, N freed, 0 live
```
There's no "cycles freed" here, because the tree never formed a cycle: the parent link is weak. And under `lughac test`, a passing test that left cycles behind for the collector says where they were made:
```
test tree_builds ... ok
  note: 200 cycles freed (400 boxes), made at tree.la:12:15, tree.la:20:9; a `Weak` back-link avoids them
```

## CONTEXT
- **Ledger and Draft:**
  - MEM-05 (`Weak<T>`), Draft §3.1 rule 5;
  - MEM-08: its report part, left unbuilt by PRP-272, is done here. Its lint is **dropped** (the user's call, 2026-11-21): with the collector freeing `Ref` cycles, a lint for "types that can form cycles without a `Weak`" would fire on every singly linked list and graph. The ledger records it.
  - COR-03 (sendability: `Weak` isn't sendable) and DST-01 (never `Wire`).
- **Spec:**
  - §4 (shared boxes: `Weak` beside `Ref`, its whole API, printing);
  - §7 (the box layout, the weak cell, and the collector);
  - §8 (the box's extra word);
  - §9 (`lughac test`'s note).
- **Code:**
  - `src/check/refs.rs` (`Ref.new` and its predeclared name);
  - `check/generics.rs` and `check/enums.rs` (the predeclared names);
  - the sendability, `Eq` and constant checks (where `Ref` is refused);
  - `src/codegen/refs.rs`, `rc_objects.rs` (`rc_box_drop`), `cycles.rs`;
  - the debug printer;
  - `runtime/cycles.c` (sites), `runtime/spawn.c` (the last collection);
  - `src/driver/test.rs`, `test_report.rs`, `test_json.rs`.
- **Research** (other languages):
  - Rust's `Rc::downgrade`/`Weak::upgrade` (weak count in the allocation);
  - Swift's `weak var` (zeroing, side table);
  - Python's `weakref.ref(obj)()`;
  - Java's `WeakReference.get()`;
  - Objective-C's zeroing weak (side table);
  - Nim (no weak type, `{.cursor.}` instead).
  - All read the referent back as "maybe gone". The constructor-and-`get` shape is Java's and Python's.
- **Security:** none of CLAUDE.md's list. It is memory management: a mistake would leave a weak cell pointing at a freed box. No `unsafe` Rust, and no new dependency.
- **Decisions:** none open.

## DESIGN
1. **The type and its API.** `Weak<T>` is predeclared like `Ref<T>`, unless a module declares its own `Weak`.

   | Call | Result |
   | --- | --- |
   | `Weak.new(r: Ref<T>): Weak<T>` | a weak reference to `r`'s box |
   | `w.get(): Option<Ref<T>>` | the box, held again, or `.None` once it has been freed |

   Copying a `Weak` copies the reference; every copy sees the box go at once.

   The rest follows `Ref`'s rules, with the same codes and no new ones:
   - a field is E0410 and an index E0411;
   - any other method, or `Weak.f`, is E0415;
   - `==` is E0404, with help "`w.get()` gives the box to compare with `is`";
   - a type holding a `Weak` isn't `Eq` (E0419);
   - it isn't `Sendable` (E0475);
   - it is never a constant (E0445) and never `Wire`.

   A type may hold itself through a `Weak`, as through a `Ref`.

2. **Printing.** `Weak(…)`, whether the box is there or not. It never prints the contents, so a back-link doesn't print its whole parent.
3. **The weak cell.**
   - A `Weak` is a pointer to a small counted object, its box's *weak cell*, `{ box }`.
   - In a program that uses `Weak` anywhere, every `Ref` box has one more word after its contents, which points to its weak cell or is null. Programs that never use `Weak` keep their boxes as they are.
   - `Weak.new(r)` makes the box's cell on first use. The box holds it once, and each `Weak` holds it once.
   - When the box is freed, by its count or by the collector, its drop sets the cell's `box` to null and lets go of the cell. `w.get()` reads `box`: null gives `.None`; otherwise it retains the box and gives `.Some`.
   - Copying, storing and dropping a `Weak` is ordinary counting of the cell, so no other codegen path changes. The cell holds nothing counted.
4. **The collector.**
   - A `Weak` never makes a box cycle-prone, since the cell holds no `Ref`.
   - A box's walk reaches its cell, which reaches nothing. So a parent linked by `Weak` adds no cycle, and a collected box clears its cell like any other.
5. **The note under `lughac test`** (MEM-08's report).
   - A test binary records where each cell was made, as `LUGHA_RC_REPORT=2` does, and collects its remaining suspects at exit.
   - When the collector freed any cycles, the program writes one line to stderr: `lugha: cycles <n> <boxes> <file>:<line>:<col> …`.
   - `lughac test` turns that line into a note under the test's `ok` (or failure) line, as in the objective. The note is never a failure.
   - `--json` adds `"cycles": {"count": n, "boxes": m, "sites": ["file:line:col", …]}` to that test's object. Readers that don't know the field ignore it, so the VS Code extension needs no change.
   - Outside `lughac test`, nothing is recorded and no line is printed.

## DECISIONS MADE IN DISCOVERY (the user chose to drop the lint; these are mine, for review)
1. **`Weak.new(r)` and `w.get()`,** not `r.weak()` / `w.upgrade()`.
   - `Ref` is made with `Ref.new`, so `Weak.new` reads alike and leaves `Ref`'s closed API alone.
   - `get` is Java's name, and its `Option` result shows the box may be gone.
2. **A weak cell, not Rust's weak count.**
   - A count kept in the box would need its own retain and release wherever a `Weak` is copied, and a freed box's block kept until the last `Weak` goes.
   - A separate cell is counted like any object: only `Weak.new`, `get` and the box's drop are new code.
3. **The extra word for every box, only in programs that use `Weak`,** rather than per type.
   - The same layout for every `T` avoids the generic-instance question (is this `T` ever weakly referenced?).
   - It costs one word per box only where `Weak` is used.
4. **Printing `Weak(…)` always:** printing contents through a back-link would print the parent's whole tree.
5. **The note's channel is a stderr line** the runner strips and reads, like the panic line it already parses. So the test binary needs no new file or pipe.

## REQUIREMENTS
### Must Do
- Design 1–5.
- **Spec:**
  - §4 (`Weak` after `Ref`);
  - §7 and §8 (layout, cell, collector);
  - §9 (the test note);
  - §11 row 241;
  - a Changes bullet.
- **Ledger:** MEM-08's lint marked dropped, with the reason.
- **The skill:** `Weak` in the functions page's box note (and wherever `Ref` is taught), with the tree example; the testing task page mentions the note.
- **CHANGELOG.**
### Must NOT Do
- Add the MEM-08 lint.
- Change box layout or counting in programs that don't use `Weak`.
- Let a `Weak` reach another task.
- Change the VS Code extension.

## TESTS
- [ ] **`--lib` (checker):**
  - [ ] `Weak.new(r)` types `Weak<T>`, and `w.get()` types `Option<Ref<T>>`;
  - [ ] `Weak.new(1)` is E0403, `Weak.make(r)` E0415, `w.x` E0410, `w == v` E0404;
  - [ ] a struct with a `Weak` isn't `Eq` (E0419);
  - [ ] spawning with one is E0475, and a constant is E0445;
  - [ ] a module's own `Weak` shadows the predeclared one.
- [ ] **`--lib` (codegen):**
  - [ ] boxes have no extra word without `Weak`, and have it with one;
  - [ ] `cycle_prone(Weak<Node>)` is false.
- [ ] **`tests/programs/types/weak_refs/`:**
  - [ ] the objective's tree;
  - [ ] `get` after the box is freed is `.None`;
  - [ ] several `Weak`s to one box, all `.None` after it goes;
  - [ ] a `Weak` kept in a cache map;
  - [ ] an observer list of `Weak`s that skips gone observers;
  - [ ] a box freed by the collector clears its `Weak`s;
  - [ ] printing.
  - Each runs poisoned and leak-free through `tests/leaks.rs`.
- [ ] **`tests/programs/types/weak_refs/` rejects:** E0475, E0404 and E0410 cases.
- [ ] **`tests/test_runner.rs`** (or the existing `lughac test` file):
  - [ ] a test leaving `Ref` cycles gets the note with its sites;
  - [ ] a test without cycles gets none;
  - [ ] `--json` has the `cycles` field;
  - [ ] a failing test still shows its failure and the note.
- [ ] **Targeted:** `--lib`, `programs`, `leaks`, `rc_cycles`, the `lughac test` tests, `spec`, `skill`, `comments`; `bench --check`.

## ACCEPTANCE
- [ ] The objective runs as shown, and a tree with `Weak` parent links frees by counting alone (no cycles freed).
- [ ] A `Weak` gives `.None` once its box is gone, never a freed box (poisoned runs clean).
- [ ] `lughac test` notes the cycles a test left, with their sites.
- [ ] Programs without `Weak` compile to the same code as before.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
