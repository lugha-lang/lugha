## FEATURE: `Eq` and `Key`: equality and map keys asked for, not given; interfaces that extend interfaces; `Hasher`; milestone 167, in v18's Phase 1.

**Status:** proposed 2026-11-15. Approved 2026-11-15.

## OBJECTIVE
```lugha
struct Point { x: i64, y: i64 } with Key        // `==`, and a map key (Key implies Eq)

struct Reading { at: i64, value: f64 } with Eq  // `==` only: f64 isn't Key

struct Email { address: string }

impl Eq for Email {
    fun eq(self, other: Email): bool = self.address.to_lower() == other.address.to_lower();
}

impl Key for Email {
    fun hash(self, mut h: Hasher) { h.add(self.address.to_lower()); }
}

struct Note { text: string }                     // neither

fun main() {
    let seen = #{Point { x: 1, y: 2 }: "a"};
    println(seen[Point { x: 1, y: 2 }]);         // a
    println(Reading { at: 1, value: 0.5 } == Reading { at: 1, value: 0.5 });   // true
    let users = #{Email { address: "Ann@x.com" }: 1};
    println(users.contains(Email { address: "ann@X.COM" }));                   // true
    println(1.5 == 1.5);                         // true: floats are Eq
    // Note { text: "a" } == Note { text: "a" }   E0422: `Note` doesn't implement `Eq`
    // #{Reading { at: 1, value: 0.5 }: 1}        E0422: `Reading` doesn't implement `Key`
    // let s: Set<f64> = #{};                     E0422: `f64` doesn't implement `Key`
}

@test
fun email_is_a_good_key() {
    assert_key_laws(Email { address: "Ann@x.com" }, Email { address: "ann@X.COM" });
}
```

## CONTEXT
- Draft v18 §1.2 rules 1–6 and 9 [EQ-01, EQ-02, EQ-03, EQ-04, EQ-05, EQ-06, EQ-07, EQ-10, EQ-11, HSH-07]; INT-14 (nothing derived without asking). Rules 7–8 (`Ord`) are PRP-200; hash seeding [HSH-01–06] is the runtime phase.
- Built on: PRP-196 (`impl I for T`), PRP-197 (`with`, which exempted `Eq` until now), PRP-198 (`mut` parameters, for `mut h: Hasher`).
- v18 spec §4 "Equality" (milestone 144), "Derivable interfaces", "Maps and sets" (E0441), "Interfaces" (declarations).
- Code: `src/prelude/eq.la`, `src/prelude/maps.la`; `check/derive.rs` (`EQ`, `implies_eq`), `check/derive_meets.rs` (own `eq`/`hash`, E0454), `check/derive_with.rs` (the `Eq` exemption), `check/maps.rs` (E0441), `check/interfaces.rs`, `check/bounds.rs`, `check/impls.rs`; `codegen/derive_pairs.rs`, `codegen/equality.rs`, `codegen/hashing.rs`.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Interfaces may extend interfaces:** `interface Key: Eq { … }`, several allowed (`interface I: J, K`).
   - Implementing `I` for a type requires it to implement `J` too (E0422 at the `impl` or `with`, help naming `J`).
   - A bound `T: I` also gives `J`'s items, and `T: I` meets a bound `T: J`.
   - `with I` derives `I`'s super-interfaces too, unless the type implements one by hand: so `with Key` is `with Eq, Key` [EQ-03].
   - An extending interface can't be a value type in this PRP (new **E0464**). `Key` and `Eq` are bound-only anyway, since they use `Self`; values of extending interfaces come with `Sendable`'s phase [INT-07].
2. **`Eq` and `Key` are asked for**, like every other derivable interface since PRP-197: PRP-197's `Eq` exemption ends.
   - The prelude's types stay automatic: `Option`, `Result`, arrays, tuples, `Map`, `Set`, `()`, and extern handles by address.
   - Numbers, `bool`, `char` and `string` are `Eq` and `Key`; `f32` and `f64` are `Eq` only.
   - `Ref`, tasks, channels, actor handles, functions and interface values are neither.
   - `==` on a type that isn't `Eq` is E0422, with help to add `with Eq` [EQ-06].
3. **Hand-written** `impl Eq for T` and `impl Key for T`, as for any derivable interface. PRP-196's E0459 let only the prelude's name-matched interfaces through; `Eq` and `Key` move to blocks. A type's own `eq` and `hash` methods no longer count, so **E0454** (one without the other) and **E0441** (float keys) retire [EQ-04, EQ-05]: their cases are now "doesn't implement `Key`".
4. **`Hasher`** is a prelude struct holding its state:
   - `h.add(x)` for any `x: Key` [HSH-07], `Hasher.new()` and `h.finish(): u64`.
   - It is FNV-1a for now: unseeded, the same hashes as today. The keyed, seeded hash replaces its insides in the runtime phase, with no change to programs.
   - Blocks fold fields in with the existing `derive.fold`, threading a `Hasher` value, so no new intrinsic is needed.
5. **Maps and sets need `Key`** of their keys and elements, wherever a `Map<K, V>` or `Set<T>` type is formed or written (E0422 with the path to the part that isn't `Key`).
6. **`assert_key_laws(a, b)`** is a prelude function [EQ-07]. It panics if `a != a`, or if `a == b` but their hashes differ, and names which. It reports the prelude's line until `@caller_location` arrives with the printing PRP.
7. **`T: Eq` stays written as today**, and `Num` still implies `Eq`. `T: Key` is an ordinary interface bound.
8. **Migration by hand, in this PRP:** every program, std module and spec example that compares its own structs or enums, or keys a map or set with them, gets `with Eq` or `with Key`. Own `eq`/`hash` methods become impls. Outputs stay the same.

## REQUIREMENTS

### Must Do
- Parse `interface I: J, K { … }`; the formatter and language server show it.
- Checker: decisions 1–7; E0464; E0441 and E0454 retired.
- Prelude: `eq.la` split into `Eq` and `Key` blocks (`Key` for every built-in but floats), `Hasher`, `assert_key_laws`; `maps.la` hashes through `Hasher`.
- Codegen: `==` through a hand-written `impl Eq` (`eq@Eq`) as through a derived one; hashing through `Key`.
- Migrate every affected `.la` file, std module, spec example and error page.
- Docs: `docs/errors/E0464.md`; E0441 and E0454 pages retired; E0422, E0451 and E0459 pages updated; v18 spec (§4 equality, maps and sets, interfaces, derivable interfaces; §11 row 167; "Changes since v17"); CHANGELOG with a **Breaking** entry; TODO.

### Must NOT Do
- `Ord`, `with Ord`, the tuple ordering block, float sorting (PRP-200); built-in types implementing a program's interfaces (its own PRP after that).
- Seeded hashing (runtime phase); `@caller_location` (the printing PRP).
- A grace period for types that compared without asking [PRI-03].

## CHANGES DURING THE BUILD (for review)
1. **A map or set key that mentions a type parameter is checked where the map is used**, not where the type is written: a struct's parameters can't have bounds, so `struct Index<K> { m: Map<K, i64> }` couldn't otherwise be written. Every map operation asks `K: Key`, so nothing gets through.
2. **Float keys say why:** E0451 "`f64` can't be a key: it is a float", with the `NaN != NaN` help, rather than listing every type that has a `Key` block. A written `Set<f64>` with a literal reports at both, the annotation and the literal.
3. **`Eq` and `Key` resolve like `Iter` and `Display`:** a module's own `Eq` or `Key` means its own; `Hasher` is a prelude type like `Map`; `assert_key_laws` falls back to the prelude like `range`.
4. **E0422's examples in OBJECTIVE:** a float key is E0451 (a part that no block covers), as other derivable interfaces report, not E0422.
5. **The example compares emails ignoring spaces** (`trim`): strings have no `to_lower`.
6. **Fixed on the way:** an associated call on a prelude type (`Hasher.new()`) was taken for a prelude helper called on a value and crashed codegen; `==` on a struct that asks for `Eq` with a function field reported E0404 after the declaration's E0451.

## DESIGN
- **New diagnostic:** E0464 "`Key` extends `Eq`, so it can't be a type: use it as a bound". Implementing `Key` without `Eq` is E0422 at the block: "`Email` implements `Key` but not `Eq`, which `Key` extends".
- **Retired:** E0441, E0454 (pages say so; codes never reused).
- **No ABI or symbol change** beyond `Hasher`'s methods, which are ordinary prelude methods.
- **Performance:** maps hash through `Hasher` methods that inline to today's arithmetic; `bench/wordfreq` is checked before and after (expected within noise).
- **Security:** none: no external input, `unsafe`, dependency, `git` or process changes. Hashes stay unseeded until the runtime phase, as today.

## TESTS
- [ ] `tests/programs/interfaces/eq_key/`: the OBJECTIVE; a struct, an enum and a generic struct with `with Key` used as map keys and set elements; `with Eq` alone; hand-written `Eq` and `Key`; floats compare but can't be keys; an extending interface of a program's own (`interface Named: Show`) used as a bound.
- [ ] Errors, one `.la` + `.stderr` each: `==` without `Eq`; a map key without `Key`; `Set<f64>`; `impl Key` without `Eq`; an extending interface as a type (E0464); `assert_key_laws` catching unequal hashes (exit 101).
- [ ] Unit tests: extending interfaces (bounds, items, implying), `Hasher`, the retired codes no longer reported.
- [ ] `cargo test --lib`, `--test programs`, `--test spec`, `--test explain`, and the integration tests that compare or key structs; `bench/wordfreq` before and after.

## ACCEPTANCE
- [ ] The OBJECTIVE prints `a`, `true`, `true`, `true`; each commented line gives its error; the test passes under `lughac test`.
- [ ] Every migrated program, std test and spec example passes with unchanged output.
- [ ] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every `pub` item documented; spec, CHANGELOG, TODO updated.
