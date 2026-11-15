## FEATURE: Deriving on request: `with I, J` and `impl I for T = derive;`; milestone 165, the second of v18's Phase 1.

**Status:** proposed 2026-11-15. Waiting for approval.

## OBJECTIVE
```lugha
import std.json;

struct Site { name: string, url: string } with json.Json

enum Status { Up(i64), Down(string) } with json.Json

struct Secret { key: string }        // no `with`: not JSON

struct Point { x: i64, y: i64 }

impl json.Json for Point {           // written by hand instead of derived
    fun to_json(self): json.Value = json.Value.Text("({self.x}, {self.y})");
    fun from_json(v: json.Value, path: string): Result<Point, json.DecodeError> = …;
}

fun main() {
    println(json.stringify(json.encode(Site { name: "a", url: "b" })));
    println(json.stringify(json.encode([Status.Up(200), Status.Down("dns")])));
    println(json.stringify(json.encode(Point { x: 1, y: 2 })));
    // json.encode(Secret { key: "k" })  →  E0422: `Secret` doesn't implement `json.Json`
}
```

A type meets a derivable interface only when it asks: `with I` on its declaration, `impl I for T = derive;`, or a hand-written `impl I for T { … }`. Nothing is derived without asking.

## CONTEXT
- Draft v18 §1.1 rules 11–15 [INT-10, INT-11, INT-12, INT-13, INT-14]; Part 7 Phase 1. v18 spec §4 "Derivable interfaces" (v17 milestones 141–144), §3 grammar, §4 "Implementing one" (milestone 164).
- Ledger: every line used is `approved`; nothing open.
- Code: `parser/decl.rs`, `parser/item.rs` (struct and enum declarations), `ast/` (`StructDecl`, `EnumDecl`, `ImplDecl`); `check/derive.rs` (`Derives`, `target`), `check/derive_meets.rs` (meeting a derivable interface, E0451), `check/impls.rs` (blocks, `nominal`), `check/satisfy.rs`; `codegen/derive.rs`; formatter; the language server's symbols, hover and definition.
- Built on PRP-196: `impl I for T` blocks, the `impls` table, E0459/E0461.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **What becomes opt-in.** A derive block for a *kind* (`derive I for struct`, `derive I for enum`) is a template a struct or enum uses only through `with I` or `= derive`. A block for a *type* (`derive Json for i64`, `derive<T: Json> Json for T[]`, `for Option<T>`, `for Map<K, V>`) is that built-in type's implementation and stays as it is, as do `for tuple` and `for handle`: built-in types, tuples and extern handles have no declaration to write `with` on. So `Site[]` is `Json` because `Site` is, and `(Site, i64)` because both parts are.
2. **`Eq` waits for PRP-198.** Every struct and enum stays `Eq` without asking until `Eq` and `Key` split there; until then `with Eq` is accepted and checked (each field must be `Eq`) but changes nothing, so programs can write it early. Every other derivable interface, `std.json`'s `Json` and a program's own, is opt-in from this PRP.
3. **Hand-written implementations are `impl I for T { … }`** [INT-12], which PRP-196's E0459 refused for derivable interfaces and now allows. It defines every item without a default; derive blocks never fill in a missing one. A type's own method named like an item (v17's way to customise one item) no longer counts. `std.json`'s `Json` has a hidden item, `$missing` (what a struct's absent field decodes to), that a program can't name; it gets a default body in the interface, the error every type but `Option` gives, so a hand-written block leaves it out and `Option`'s block keeps its own. So a derive block may now leave out an item with a default, as an `impl` may.
4. **Errors.** Listing an interface with no block for the type's kind (no `for struct` block for a struct; `Display`, never derivable) is **E0462**, naming the kinds it has blocks for. A field or payload whose type doesn't meet the interface is **E0451** at the declaration, naming the field (today it is reported at the first use). Deriving and hand-writing one interface for one type is **E0463**. Listing one twice (`with Json, Json`) is E0461.
5. **Generic types derive conditionally** [INT-13]: `struct Pair<T> { a: T, b: T } with Json` checks only fields that don't mention a parameter at the declaration; `Pair<X>` meets `Json` where `X` does, checked at each use (E0451 there, as today), and a parameter that appears in no field needs nothing.
6. **Where:** `with` follows the closing `}` of a struct or enum declaration, its interfaces named as in a bound (`with json.Json`, `with Show`). `impl I for T = derive;` may stand wherever `impl I for T { … }` may [INT-02]. `with` is a contextual word, not reserved. Actors don't take `with` in this PRP (rule 11 names structs and enums); the Draft's Part 6, which also names actors, should say the same, and I'll propose that edit with this PRP.
7. **Codegen is unchanged:** a derived interface is reached through its block exactly as today; only who may reach it changes.
8. **Migration by hand, in this PRP**, as before: programs, std modules and spec examples that encode, decode or otherwise use a derivable interface on their own structs and enums get `with`.

## REQUIREMENTS

### Must Do
- Parse `with I, J` after struct and enum declarations and `impl I for T = derive;`; AST fields; formatter; language server hover and go-to-definition on the listed names.
- Checker: derived interfaces recorded per type; meeting a derivable interface needs `with`, `= derive` or a hand-written block (decisions 1–3); E0462, E0451 at the declaration, E0463, E0461; conditional generics (decision 5).
- Allow `impl I for T { … }` for derivable interfaces (decision 3).
- Migrate every affected `.la` file, std module and spec example; output unchanged.
- Docs: `docs/errors/E0462.md`, `E0463.md`; E0451, E0459, E0422 pages updated; the v18 spec (§3 grammar, §4 derivable interfaces, §11 row 165, "Changes since v17"); CHANGELOG with a **Breaking** entry.

### Must NOT Do
- `Eq`/`Key`/`Ord` changes (PRP-198), printing or `Debug` (PRP-200), per-field derive options such as `@json(skip)` (deferred, DEF-13).
- `with` on actors; derive blocks for built-in types made opt-in (decision 1).
- A grace period or warning for types that relied on automatic deriving [PRI-03].

## DESIGN
- **New diagnostics:** E0462 "`Show` can't be derived for an enum: it has derive blocks for `struct` and `tuple` only"; E0463 "`Point` both derives and implements `json.Json`", pointing at both, help "remove `json.Json` from `with`, or the `impl`". E0451 gains a declaration form: "`Job` can't derive `Show`: field `run` is a `fun(i64): i64`, which isn't `Show`".
- **No runtime, ABI or symbol change.**
- **Security:** none: no external input, `unsafe`, dependency, `git` or process changes.

## TESTS
- [ ] `tests/programs/derive/with/`: the OBJECTIVE program; `with` on a struct, an enum and a generic struct used at two arguments; `impl I for T = derive;` in the type's and the interface's module; a program's own derivable interface through `with`; arrays and tuples of derived types.
- [ ] Errors, one `.la` + `.stderr` each: a derivable interface used without `with` (E0422); E0462 for an enum where only `for struct` exists and for `Display`; E0451 at the declaration; E0463; `with Json, Json` (E0461); a generic instance whose argument isn't `Json` (E0451 at the use).
- [ ] Unit tests in `check/` for each new diagnostic and for `with Eq` being accepted; parser and formatter tests for both new forms.
- [ ] `--test programs`, `--test spec`, `--test explain`, the `json` and LSP tests the change touches.

## ACCEPTANCE
- [ ] The OBJECTIVE program prints the three JSON lines; uncommenting the last line gives E0422 with help to add `with json.Json`.
- [ ] Every migrated program, std test and spec example passes with unchanged output.
- [ ] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every `pub` item documented; v18 spec, CHANGELOG and TODO updated.
