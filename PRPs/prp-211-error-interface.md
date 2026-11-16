## FEATURE: Interfaces that extend others are value types (E0464 retired), and the prelude's `Error: Display`; milestone 179, in v18's Phase 1.

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
struct ParseError { text: string }
impl Display for ParseError { fun show(self): string = "not a number: {self.text}"; }
impl Error for ParseError {}

fun parse(s: string): Result<i64, ParseError> {
    match parse_i64(s) {
        .Some(n) => Ok(n),
        .None => Err(ParseError { text: s }),
    }
}

fun total(a: string, b: string): Result<i64, Error> {
    let x = parse(a)?;                      // a ParseError becomes an Error
    let y = check(b)?;                      // so does a string
    Ok(x + y)
}

fun check(s: string): Result<i64, string> = if s == "" { Err("empty") } else { Ok(s.len) };

interface Named { fun name(self): string; }
interface Greeter: Named { fun greet(self): string = "hello, {self.name()}"; }
struct Ann {}
impl Named for Ann { fun name(self): string = "Ann"; }
impl Greeter for Ann {}

fun main() {
    match total("4", "") {
        .Ok(n) => println(n),
        .Err(e) => println("failed: {e}"),  // Error is Display: prints `failed: empty`
    }
    match total("x", "ab") {
        .Ok(n) => println(n),
        .Err(e) => println(e.show()),       // not a number: x
    }
    let g: Greeter = Ann {};                // was E0464
    println(g.greet());                     // hello, Ann
    println(g.name());                      // Named's item, through Greeter's table
    let n: Named = g;                       // a Greeter value is a Named value
    println(n.name());
}
```

## CONTEXT
- Draft v18 §1.7 rule 4 [OPN-10]: "The `Error` interface requires `Display`", so any error can be shown with `"{e}"`. Part 4 builds on it: `Failure`, `Stopped` and `CallError` implement `Error`, and `?` goes into `Result<_, Error>` [SCO-04, SCO-11, SCO-13].
- Decided with the user: lift E0464 for every interface, not only `Error`.
- Today: an interface may extend others (`interface Greeter: Named`) and be a bound, but not a value type (E0464, whose page says "not yet"). There is no prelude `Error`: programs declare their own (one test program does, and the spec's §10 errors program).
- `?` already converts a concrete error into an interface error it satisfies (`Checked.try_conversions`), and an interface value into another interface whose items it declares (`codegen/upcast.rs`).
- std: `DecodeError` and `TimeError` have `Display`; `CryptoError`, `JsonError` and `TomlError` don't; most of std returns `Result<_, string>`.
- Code: `check/iface_values.rs` (E0464), `check/applied.rs` (`items`), `check/iface_calls.rs`, `check/iface_tables.rs` (`InterfaceShape`, `conversion_uses`), `check/satisfy.rs`, `codegen/upcast.rs`, `codegen/dispatch.rs`, `src/prelude/`.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **An extending interface is a value type.** An `I` value's table holds the items of every interface `I` extends, transitively, each once, ancestors first in declaration order, then `I`'s own. Calls on an `I` value reach any of them (`g.name()` above); a default of an ancestor fills its slot as today.
2. **Converting to it** needs the type to implement `I`, which already needs its parents (E0422 otherwise, as today).
3. **An `I` value is a value of each ancestor `J`:** it converts where a `J` is expected (`let n: Named = g;`), meets a bound `T: J`, and `?` converts `Result<_, I>` into `Result<_, J>`.
4. **The prelude's `Error`:** `interface Error: Display {}`, no items of its own, nominal (met only by `impl Error for T`), visible by bare name in every module like `Display`; a module's own `Error` hides it, so programs that declare one keep working.
5. **`impl Error for string`** in the prelude, so the many `Result<_, string>` functions convert with `?` into `Result<_, Error>`.
6. **std's error types implement `Error`:** `DecodeError` and `TimeError` (which have `Display`), and `CryptoError`, `JsonError` and `TomlError`, which get a `Display` from the messages they already carry.
7. **`"{e}"`, `to_string(e)` and `e.show()`** on an `Error` value use its type's `Display` through the table; `e.debug()` stays the debug form.
8. **E0464 is retired:** never reported, its page kept to say so, the number not reused.
9. **Not breaking** for programs, except one whose interface is named `Error` and is then implemented for `string` or a std type expecting the prelude's (none in the repository).

## REQUIREMENTS

### Must Do
- Checker: decisions 1–3 (items of a value type, calls, conversions, bounds, `?`); E0464 no longer reported.
- Codegen: tables with the ancestors' items; upcasts to an ancestor.
- Prelude: `Error`, `impl Error for string`. std: decision 6.
- Docs: E0464's page (retired); v18 spec (§4 interfaces and interface values, `?`, printing, the prelude list, std's error types, the error table, §11 row 179, "Changes since v17"); CHANGELOG; TODO if it mentions it.

### Must NOT Do
- `Failure`, `Stopped`, `CallError` (Part 4 and 5).
- Downcasting an `Error` value beyond today's type patterns (`match e { p: ParseError => … }` works as for any interface value).
- Changing which interfaces may be values for other reasons (`Self` in an item, E0421-style rules stay).

## TESTS
- [ ] `tests/programs/interfaces/extending_values/`: the OBJECTIVE (runs); a three-level chain with defaults at each level; an ancestor value from a descendant; a type pattern on an `Error` value; `?` from a std error.
- [ ] Checker unit tests: calls through ancestors, conversions both ways (a `Named` isn't a `Greeter`: E0403), bounds.
- [ ] Codegen unit test: a table's slot order.
- [ ] The full `cargo test`.

## ACCEPTANCE
- [ ] The OBJECTIVE prints `failed: empty`, `not a number: x`, `hello, Ann`, `Ann`, `Ann`.
- [ ] The full `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Spec, CHANGELOG and the error pages updated.
