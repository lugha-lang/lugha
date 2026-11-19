## FEATURE: Three checker rules from the v18 spec review: one module for an `impl` of a type built from several program types (INT-17), interface values converting only along `extends` (INT-18), and every name syntax uses reserved (INT-21); milestone 223.

**Status:** approved 2026-11-20 (the user's "do the ledger's decisions in one swoop": built back to back, merged once local checks pass).

## OBJECTIVE
```
// geo.la declares Point, lines.la declares Line
impl Show for (Point, Line) { … }   // E0459 in geo.la: built from two program
                                    // types, it belongs in `Show`'s module

interface Named { fun name(self): string; }
interface Label { fun name(self): string; }
interface Pet: Named { fun sound(self): string; }
fun greet(n: Named) { … }
fun f(p: Pet, l: Label) {
    greet(p);   // fine: Pet extends Named
    greet(l);   // E0422: `Label` doesn't extend `Named`
}

interface Display { … }             // E0302: `Display` is a built-in name
struct Map { tiles: i64[] }         // fine: syntax doesn't use `Map`
```

## CONTEXT
- **Ledger:** INT-17, INT-18 and INT-21 (2026-11-20). These refine INT-02/INT-16 (where a block may be written), INT-01 (nominal) and E0302.
- **Spec:**
  - §4 "Implementing a built-in type" (the home rule, the claim that two packages can't clash);
  - "Between interfaces" and "As a type argument" (structural today);
  - §2/§6 reserved names (`Option`, `Some`, `None`, `Eq`, `Ord`, `Result`, `Ok`, `Err` today).
- **Code:**
  - `check/builtin_impls.rs` (`program_types`, the module check);
  - `check/upcast.rs` (`undeclared`, `upcasts`), `check/iface_tables.rs`, `check/scopes.rs` (callers of `undeclared`);
  - `check/enums.rs` `BUILTIN_NAMES`, used by `structs.rs`, `handles.rs`, `interfaces.rs`.
- **Decisions:** none open.

## DESIGN
1. **INT-17.**
   - A block's type that names two or more distinct program types (`(Point, Line)`, `Map<Point, Line>`, `Pair<Point, Line>`) may be written only in the interface's module. One that names a single program type keeps its homes (`Point[]`, `Option<Point>`, `(Point, Point)`).
   - An operator's right-hand types keep their home (INT-04).
   - E0459, with the reason "built from several program types: write it in the module that declares `Show`, or wrap them in a struct of your own".
   - This applies equally to a program struct's own generic block over two program types (`impl Show for Pair<Point, Line>` in `Pair`'s module).
2. **INT-18.**
   - `J` converts to `I` only when `J` is `I` or extends it, at any depth, with arguments matching. That holds for conversions, for interface values as type arguments, and for the interface tables built from them.
   - A value whose interface has `I`'s items by name is E0422: "`J` doesn't extend `I`" (the existing `not_extending` diagnostic, which the prelude's nominal interfaces already use).
3. **INT-21.**
   - Reserved, E0302 "is a built-in name": `Option`, `Some`, `None`, `Result`, `Ok`, `Err`, `Eq`, `Ord`, `Display`, `Iter`, `Error`, `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, `FromEntries`, `FromItems`, `Ref`.
   - The check covers the same declarations as today: structs, enums, actors, extern types and interfaces.
   - The prelude may declare them. Every other prelude name stays free.
4. **Migration:** tests and examples declaring their own `Error`, `Iter` or `Ref` are renamed. That includes §10's "Errors as interfaces", whose own `Error` becomes `Report`, as the review suggested. Programs relying on structural interface conversion add `extends`.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **"Distinct program types"** counts types, not modules. `(Point, Line)` with both in `geo.la` still belongs in the interface's module: the rule reads off the type, and a later move of `Line` can't change where the block may live.
2. **No new codes:** E0459 (misplaced block), E0422 (doesn't convert) and E0302 (built-in name) already say these things.
3. **A module's own `Ref` handling** (`check/refs.rs`, a module type shadowing `Ref`) becomes unreachable and is removed with its test; that test is turned into an E0302 case.

## REQUIREMENTS
### Must Do
- Design 1–4.
- Spec: §4's home rule (and its false claim that blocks never clash), "Between interfaces", "As a type argument", the reserved-name lists in §2, §4 and §6, §11 row 223, and a "Changes since v17" bullet marked Breaking.
- CHANGELOG.
### Must NOT Do
- Change where single-type blocks may be written, or operator homes.

## TESTS
- [ ] `--lib`: INT-17 (two program types refused in the type's module and allowed in the interface's; one type and `(Point, Point)` still allowed); INT-18 (extends converts at depth two; same-named items don't; type arguments too); INT-21 (each reserved name as a struct, enum, interface; `Map` and `Console` allowed).
- [ ] `tests/programs/interfaces/nominal_rules/`: one stdout case and an E0459, an E0422 and an E0302 case.
- [ ] Targeted: `--lib`, `programs`, `spec`, `interface_values`, `explain`, `comments`.

## ACCEPTANCE
- [ ] Two libraries can no longer both write one block.
- [ ] No interface value converts by names alone.
- [ ] Exactly the names syntax uses are reserved.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
