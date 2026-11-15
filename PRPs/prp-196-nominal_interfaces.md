## FEATURE: Nominal interfaces: a type implements an interface only through `impl I for T`; milestone 164, the first of v18's Phase 1.

**Status:** proposed 2026-11-15. Approved 2026-11-15.

## OBJECTIVE
```lugha
interface Shape {
    fun area(self): f64;
    fun describe(self): string = "a shape of area {self.area()}";
}

interface Named { fun area(self): f64; }

struct Circle { r: f64 }

impl Circle {
    fun area(self): f64 = 0.0;                 // the type's own method
}

impl Shape for Circle {
    fun area(self): f64 = 3.14159 * self.r * self.r;
}

fun total<T: Shape>(xs: T[]): f64 { … }       // bound: only types with `impl Shape for …`

fun main() {
    let c = Circle { r: 2.0 };
    let shapes: Shape[] = [c];                 // conversion needs the impl
    println(c.area());                         // 0: the type's own
    println(Shape.area(c));                    // 12.56636: the interface's
    println(c.describe());                     // a default, through the impl
}
```

A struct with a matching `area` and no `impl Shape for Circle` no longer satisfies `Shape`: `error[E0422]: `Circle` doesn't implement `Shape``, with help `add `impl Shape for Circle { … }``.

## CONTEXT
- Draft v18 §1.1 rules 1, 2, 5–8 and 10 [INT-01, INT-02, INT-05, INT-06, INT-08, INT-09, INT-15, INT-16]; Part 7 Phase 1. v17 spec §4 "Interfaces" ("Satisfying one", "Default methods", "Interface values"), §8 symbol names.
- Ledger `docs/decisions/v18-ledger.md`: every line used here is `approved`; nothing open. DECISIONS.md core decision 6 (symbols).
- Code: `parser/item.rs`, `parser/interface.rs`, `ast/mod.rs` (`ImplDecl`); `check/satisfy.rs` (structural `unmet`/`offer`, replaced), `check/interfaces.rs`, `check/defaults.rs`, `check/iface_tables.rs`, `check/iface_calls.rs`, `check/conflicts.rs`, `check/errors/interfaces.rs`; `codegen/` method symbols and interface tables; the formatter and the language server's symbols/hover.
- Programs to migrate: the `.la` files under `tests/programs/` (65 declare an interface; more satisfy one), `src/prelude/`, `src/std/`, the spec's §10 programs and `bench/`'s Lugha sources.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Slicing Phase 1.** This PRP makes interfaces nominal and nothing else. Later PRPs, in order: 197 `with` and deriving (INT-10–14); 198 `Eq`/`Key`/`Ord` (EQ); 199 operators (INT-04); 200 `Debug`/`Display`, printing, the prelude takes over (PRE, OPN-02/10); 201 `.Variant`, must-use `Result`, lossy conversions (IMP); 202 type arguments (TYA); 203 map and set literals (LIT). `Sendable` (INT-07) waits for the sendable rule in Phase 5.
2. **Built-in behaviours stay as they are until their PRP.** The interfaces that the compiler drives by method name today (`Iter`, `Eq`, `Ord`, `Num` and the operator bounds, printing's `show`, `Json`) and every derivable interface (one with `derive` blocks) keep v17's rules here; each becomes nominal in its own PRP (decision 1). Every other interface, user or prelude/std, is nominal from this PRP on. This keeps each step reviewable at the cost of a short mixed period, which only `main` between merges sees.
3. **Generic impls take parameters from the type, as inherent `impl Stack<T>` does:** `impl Source<T> for Stack<T> { … }`, bounds written there (`impl Sorted<T> for Stack<T: Ord>`). The interface's arguments may be concrete (`impl From<string> for Path`) [INT-05]. No separate `impl<T>` list: one way to write it, matching v17's inherent blocks.
4. **Which types can be implemented for:** structs, enums and actors by name; built-in types (`i64`, `string`, `T[]`, `Option<T>`, tuples) in the interface's module [INT-02]; `V[]`, `Option<V>`, `Map<K, V>` etc. also in `V`'s module [INT-16]. Interface values and type parameters can't be the `T` of an impl (no blanket impls): E0459.
5. **What an impl must contain:** every item without a default, each with exactly the interface's signature (`Self` replaced by `T`); it may replace a default. A missing item, an extra item, or a wrong signature is E0422 at the impl, listing each. Associated functions (no `self`) are implemented the same way.
6. **Calling interface methods with a dot.** `c.area()` finds, in order: the type's own method [INT-15]; then the items of interfaces `T` implements that are *visible* at the call (declared in, or imported by, the calling module, or the prelude). Two such interfaces supplying the name is E0460, with help showing both qualified forms [INT-06]. This replaces "the one visible interface" rule for defaults: a default is reached only through an implemented interface, so E0430 is retired and E0415 no longer covers defaults.
7. **Qualified calls** `Shape.area(c)` and `m.Shape.area(c)` (an associated item: `Shape.make()` needs an expected type or E0412; type arguments wait for PRP-202). Qualifying with an interface `T` doesn't implement is E0422.
8. **Where an impl may be written** [INT-02, INT-16]: the module declaring `I`, or the one declaring `T` (or the type `T` is built from). Anywhere else is E0459, naming both modules. Writing the same `impl I for T` twice (at equal interface arguments) is E0461, pointing at both.
9. **Visibility:** an impl has no visibility of its own: it applies wherever both `I` and `T` can be named. Its methods follow the interface's.
10. **Bounds and conversions** check the impl table instead of method shapes: `T: Shape` and `let s: Shape = c` need `impl Shape for C` (or, for a type parameter, a bound that implies it; an interface value satisfies its own interface, and converts to another interface `J` when `I` declares `J`'s items, as in v17, since no impl is involved). Error stays E0422, reworded "doesn't implement", with help to add the impl.
11. **Symbols** (core decision 6): an impl's method is `lugha_fn_<Type>.<Interface>.<name>`, the interface as it prints with its module path and arguments: `lugha_fn_Circle.Shape.area`, `lugha_fn_geo/Circle.geo/Shape.area`, `lugha_fn_Path.From<string>.from`. An inherent method has one `.` after the type, so the two never collide; `$own` copies keep their suffix. New codes take the next unused numbers: E0459, E0460, E0461.
12. **Spec file:** as for v17, this PRP starts `Language v18 Specification.md` as a copy of v17's, marked "in progress", and changes §4 and §8 there; v17's file is frozen from now on. `lughac spec` prints v18's.
13. **Migration by hand, in this PRP.** Every program, prelude and std module that relies on structural satisfaction of a nominal interface gets its `impl I for T`. The migration tool [PRI-05] isn't needed to land this.

## REQUIREMENTS

### Must Do
- Parse `impl I for T { … }`, `impl m.I for T`, `impl I<A> for T<P: B>`; AST: `ImplDecl` gains `interface: Option<…>`. Formatter prints it; the language server shows it in symbols and hover.
- Checker: impl table keyed by (interface, interface arguments, type); E0422 for incomplete or mismatched impls; E0459, E0460, E0461; satisfaction, bounds and conversions through the table for nominal interfaces (decision 2); dot calls and qualified calls (decisions 6–7).
- Codegen: impl methods under decision 11's symbols; interface tables and bound calls use them; defaults instantiated per impl.
- Migrate every affected `.la` file (decision 13); the spec's §10 programs still print and exit as before.
- Docs: `docs/errors/E0459.md`, `E0460.md`, `E0461.md`; E0422, E0415 and E0430 pages updated (E0430 retired); the v18 spec (§4 Interfaces, §8 symbols, §11 row, "Changes since v17"); CHANGELOG (with a **Breaking** list); the Draft's Phase 1 status.

### Must NOT Do
- `with`, deriving changes, `Eq`/`Key`/`Ord`, operators, printing, `Iter`, `Json` (decisions 1–2).
- `Sendable` (needs the sendable rule, Phase 5); blanket impls; `impl<T>` syntax; explicit type arguments.
- Keep a structural fallback, a flag or a warning period for nominal interfaces: v18 is a breaking change [PRI-03].

## CHANGES DURING THE BUILD (for review)
1. **Built-in types can't implement interfaces yet** (narrows decision 4). `impl I for i64`, `string`, `V[]` and so on is E0459 for now: the header parser takes only a type name, and dot calls and dispatch on numbers go through their own built-in paths. Built-in impls arrive with PRP-198, where the prelude needs `impl Key for i64`. Until then an empty `interface Any {}` can't hold numbers or strings (`tests/programs/interfaces/values/any.la` now holds structs and enums).
2. **Symbols** (refines decision 11): a block's method is `lugha_fn_Circle.area@Shape`, not `lugha_fn_Circle.Shape.area`, since a method key is split at its last `.`. The interface is written as in the source, its `.`s as `/`s (`area@geo/Shape`, `from@From<string>`); a block for another module's type adds that module's path (`area@Shape@shapes`). The parser gives the names, so every way of building a program gets them.
3. **Associated functions** (extends decision 6): `Widget.make()` reaches an associated function of an implemented interface in sight when the type has none of its own, like `c.area()` for methods.
4. **Indexing stays by name**, with the other built-in behaviours (decision 2): `a[i]` uses a type's own `index`/`set_index`.
5. **A test was near its stack limit:** `parser::expr::tests::deep_nesting_is_e0206_not_a_stack_overflow` overflowed the 2 MiB test thread after unrelated code moved; it now parses on an 8 MiB thread, the size of `lughac`'s main thread.

## DESIGN
- **New diagnostics:** E0459 "`impl Shape for Circle` must be written in the module that declares `Shape` or the one that declares `Circle`" (also: can't implement for an interface value or type parameter); E0460 "`c.area()` is ambiguous: `Shape` and `Named` both supply `area`", help `Shape.area(c)` / `Named.area(c)`; E0461 "`Shape` is implemented for `Circle` twice". E0422 reworded for nominal interfaces.
- **No runtime or ABI change** beyond the new method symbols; interface tables keep their layout.
- **Security:** none: no external input, `unsafe`, dependency, `git` or process changes.

## TESTS
- [ ] `tests/programs/interfaces/nominal/`: the OBJECTIVE program; impls in the interface's module, the type's module and the element type's module (`impl I for V[]`); a generic impl with bounds; `From<string>` and `From<Bytes>` for one type; defaults replaced and inherited; associated functions; qualified calls through a module.
- [ ] Errors, one `.la` + `.stderr` each: a structurally matching type without an impl (E0422) in a bound, a conversion and a qualified call; an incomplete impl, an extra item, a wrong signature (E0422); an impl in a third module and for a type parameter (E0459); an ambiguous dot call (E0460); a duplicate impl (E0461).
- [ ] Parser and formatter unit tests for the new form; `tests/explain.rs` covers the new pages; `tests/spec.rs` (§10) unchanged output; `tests/comments.rs`.
- [ ] Language server: an impl's methods in document symbols and hover (`tests/lsp*.rs`, the touched file).

## ACCEPTANCE
- [ ] The OBJECTIVE program prints `0`, `12.56636`, `a shape of area 12.56636`; removing the impl gives E0422 with the help.
- [ ] Every migrated program passes unchanged in output; `--test programs` and `--test spec` pass.
- [ ] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every `pub` item documented; v18 spec, CHANGELOG, TODO and CLAUDE.md (`lughac spec` line) updated.
