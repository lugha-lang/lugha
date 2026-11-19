## FEATURE: One spelling for a generic `impl` (`impl<T: Num> Grid<T>`, INT-19) and `+` between an interface's parents (`interface Store: Eq + Sendable`, INT-20); milestone 222.

**Status:** approved 2026-11-20 (the user's "do the ledger's decisions in one swoop": built back to back, merged once local checks pass).

## OBJECTIVE
```
struct Grid<T> { cells: T[] }

impl<T> Grid<T> {                       // was: impl Grid<T>
    fun size(self): i64 = self.cells.len;
}
impl<T: Num> Grid<T> {                  // was: impl Grid<T: Num>
    fun total(self): T { … }
}
impl<T: Display> Show for Grid<T> { … } // was: impl Show for Grid<T: Display>

interface Store: Eq + Sendable { … }    // was: interface Store: Eq, Sendable
```

```
error[E0214]: a generic `impl` names its type parameters after `impl`
  --> grid.la:3:6
   |
 3 | impl Grid<T> {
   |      ^^^^^^^
   = help: write `impl<T> Grid<T>`
```

```
error[E0215]: an interface's parents are joined with `+`
  --> store.la:1:20
   |
 1 | interface Store: Eq, Sendable {
   |                    ^
   = help: write `interface Store: Eq + Sendable`
```

## CONTEXT
- **Ledger:** INT-19 and INT-20 (2026-11-20, the v18 spec review). PRI-02, and the user's rule that two spellings of one construct never coexist.
- **Spec:** §3 `impl_decl` and `interface_decl`; §4 "Generic impl blocks", "Interfaces extending interfaces", the derive and operator sections that show `impl X<T…>`.
- **Code:**
  - `src/parser/decl.rs` `impl_decl`, `named_ahead`; `src/parser/generic.rs` `impl_params`; `src/parser/interface.rs` `interface_decl`; `src/formatter/items.rs`.
  - The checker already takes both of today's forms. `ImplDecl.target` holds a type written after `impl<…>`, and `module`/`ty`/`params` hold a struct's, enum's, actor's or extern type's block. It refuses `impl<T> Grid<T>` for a program's type today (E0466 or E0459).
- **Decisions:** none open.

## DESIGN
1. **Parsing.** After `impl`, its optional `<…>` and an optional `I for`, the type is read as any type, except that a bare name with no arguments (`impl Grid {`, `impl f64 {`, `impl geo.W {`) stays a name, as today.
2. **Normalising, once the file is parsed.**
   - A block whose type is a named type `N<A, …>` gets the struct form (`ty` = `N`, `params` = the `impl<…>` ones) when its arguments are exactly its parameters, in order, each a bare name. `N` must be a program type: qualified (`m.N`); or not a built-in generic name (`Option`, `Result`, `Map`, `Set`, `Ref`, `Task`, `Chan`, `Ring`, `Sender`, `Receiver`, `RingSender`, `View`, `MutView`); or declared or named-imported by the file.
   - So the checker sees what it already handles, and `Self` means the same.
3. **E0214.** A named program type with arguments that aren't exactly the `impl<…>` parameters is refused, which covers the old `impl Grid<T>` and `impl Grid<T: Num>`. The help writes the long form from the written names. A block for a built-in generic type is unaffected (`impl Show for Option<Point>`, `impl<T> Option<T>`).
4. **E0215.** An interface's parents are joined with `+`; a `,` there is E0215, with the `+` spelling as help. `with` keeps `,`.
5. **Formatter:** prints `impl<T: B> Grid<T>` and `: A + B`.
6. **Migration:** every `.la` file (the prelude, std, `tests/programs`), every inline program in `tests/*.rs` and `src/**/*.rs`, the error pages and the spec are rewritten by a script, checked by the targeted tests.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Normalise in the parser, not the checker.**
   - The checker's many passes read `ty`/`params`/`target` directly.
   - Built-in generic names are spelled out in the parser beside `Option`'s existing special case.
   - A file that declares or imports its own `Map` still gets the struct form.
2. **Two new codes, E0214 and E0215,** rather than E0201, as `#{…}` got E0213: an old spelling deserves a page that says what replaced it.
3. **The checker's leftover path for `impl Show for Option<Point>` written as a struct's block** (`builtin_target` without a target) goes, since such a block now always arrives as a target.

## REQUIREMENTS
### Must Do
- Design 1–6; E0214 and E0215 with `docs/errors/` pages; spec §3 grammar (`impl_decl`: `"impl" [ type_params ] [ bound "for" ] type`; `interface_decl` with `+`), §4 examples, §11 row 222, "Changes since v17" (Breaking); CHANGELOG.
### Must NOT Do
- Change what any block means, or `with`'s separator.

## TESTS
- [ ] Parser unit tests: the long form for a program type gives the struct form; built-in targets stay targets; E0214 for `impl Grid<T>`, `impl Grid<T: Num>` and `impl Show for Grid<T>`; E0215 for `interface I: A, B`; `+` parses.
- [ ] Formatter: round-trips both.
- [ ] `tests/programs/interfaces/one_spelling/`: a program using the long form for its own type, a generic interface impl and `+` parents (stdout); E0214 and E0215 (stderr).
- [ ] Targeted: `--lib`, `programs`, `spec`, `explain`, `comments`, `formatter`, and each `tests/*.rs` whose inline programs changed.

## ACCEPTANCE
- [ ] Only `impl<…> T` spells a generic block; only `+` joins an interface's parents.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
