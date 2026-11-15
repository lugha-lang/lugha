## FEATURE: Built-in types implement interfaces: `impl I for f64`, `impl I for V[]`, and generic `impl<T: B> I for T[]`; milestone 169, in v18's Phase 1.

**Status:** proposed 2026-11-15.

## OBJECTIVE
```lugha
interface Show {
    fun show(self): string;
}

impl Show for f64 {
    fun show(self): string = "{self} m";
}

impl Show for string {
    fun show(self): string = "\"{self}\"";
}

impl<T: Show> Show for T[] {                       // every array of a `Show` type
    fun show(self): string = "[" + self.map(fun(x: T): string = x.show()).join(", ") + "]";
}

impl<T: Show> Show for Option<T> {
    fun show(self): string = match self {
        Some(x) => "some " + x.show(),
        None => "none",
    };
}

struct Point { x: f64, y: f64 }

impl Show for Point {
    fun show(self): string = "({self.x}, {self.y})";
}

fun all<T: Show>(xs: T[]): string = xs.show();

fun main() {
    println(2.5.show());                                       // 2.5 m
    println(Show.show("hi"));                                  // "hi"
    println(all([[1.0, 2.0], [3.0]]));                         // [[1.0 m, 2.0 m], [3.0 m]]
    println(Some(Point { x: 1.0, y: 2.0 }).show());            // some (1.0, 2.0)
    let items: Show[] = [1.5, "a", Point { x: 0.0, y: 0.0 }];   // interface values
    for i of items {
        println(i.show());
    }
    println(3.cmp(4) == Ordering.Less);                        // true: the prelude's blocks count too
    // impl Show for bool { … } in another module       E0459: only `Show`'s module
    // impl Eq for f64[] { … }                           E0459: the prelude decides built-in types' `Eq`, `Key` and `Ord`
}
```

## CONTEXT
- Draft v18 §1.1 rules 1–2, 7, 8, 10 [INT-01, INT-02, INT-15, INT-16, INT-08]; §1.7 rule 5 [PRE-03] (the prelude's own `impl<T> T[]` blocks, a later PRP, share this PRP's syntax).
- Built on PRP-196 (nominal interfaces, E0459–E0461), PRP-197 (derive blocks on request), PRP-199 and PRP-200 (`Eq`, `Key`, `Ord` as derivable interfaces with blocks for built-in types).
- v18 spec §4 "Interfaces" (implementing one, using one as a bound, interface values), "Breaking" in "Changes since v17" ("a number, string or array converted to a program's interface is E0422 until built-in types can implement interfaces").
- Code: `parser/decl.rs` (`impl_decl`, which takes a type name after `for`), `parser` `derive<T: B> I for T[]` (the model for `impl<T>`); `check/impls.rs` (`register_impl`, E0459, `report_not_implemented`'s "can't implement an interface yet"); `check/derive.rs` (`Covers::Type` blocks, hidden owners, instantiation); `check/methods.rs`, `check/method_calls.rs` (`x.m()` on a built-in receiver); `codegen/interfaces.rs` (`table`, `box_as`).
- What works today (checked): a `derive Show for f64` block in `Show`'s module already satisfies `T: Show` for `f64` and `f64[]`; `2.5.show()` is E0415, `Show.show(3.5)` and `let v: Show = 4.5;` are E0422.

## DECISIONS MADE IN DISCOVERY (mine, for review; the scope, generic blocks included, is the user's)
1. **Which types:** a block may be for any type that isn't a struct, enum, actor or extern type (those keep today's form): the numbers, `bool`, `char`, `string`, `()`, `CPtr`, arrays, tuples, `Option`, `Result`, `Map` and `Set`, at concrete arguments (`f64[]`, `Option<Point>`, `(string, i64)`) or the block's parameters (`T[]`, `Option<T>`, `(A, B)`, `Map<K, V>`). Not function types, interface types, `Ref`, tasks, channels, actor handles, or a bare parameter (`impl<T> I for T`, a blanket impl): E0459, each with its reason.
2. **Syntax:** `impl<T: B, …> I for Type { … }`, the parameters after `impl` as `derive<T: Eq> Eq for T[]` writes them; every parameter must appear in the type (E0459 otherwise, as an unconstrained parameter has no type to come from). A struct's or enum's block keeps its parameters after its name (`impl Source<T> for Stack<T: Ord>`); `impl<T>` before a struct or enum is E0459, with help showing that form. `impl I for T = derive;` stays for structs and enums only.
3. **Where** [INT-02, INT-16]: in the module that declares the interface, or in the module that declares a program type the block's type is built from: `impl Show for Point[]` and `impl Show for Option<Point>` in `Point`'s module; `impl Show for (Point, Line)` in either's. A block over only built-in types and parameters (`impl Show for f64`, `impl<T: Show> Show for T[]`) is only in the interface's module. E0459 otherwise.
4. **The prelude decides `Eq`, `Key` and `Ord` for built-in types:** a program's `impl Eq`/`Key`/`Ord` for a built-in type, `Point[]` included, is E0459. Arrays of `Eq` types are already `Eq`, and floats and arrays stay out of `Ord`.
5. **One block per type:** a block whose type overlaps another block of the same interface (two `impl Show for f64`, or `impl<T: Show> Show for T[]` with `impl Show for Point[]`, or either with a derive block covering the same type) is E0461, naming the first. No specialization: the overlap is checked by unifying the two types, bounds ignored.
6. **Uses, for every way a built-in type implements an interface (an `impl` block or a derive block):**
   - `T: I` bounds, as derive blocks already allow.
   - `x.item()` on a built-in value: a built-in method of the same name wins (`s.len`, `xs.map`), as a type's own method does [INT-15]; `I.item(x)` reaches the interface's.
   - Conversion to an interface value wherever one is expected [INT-08]: `let s: Show = 2.5;`, `Show[]` literals, arguments, returns.
   - So the prelude's blocks count: `3.cmp(4)`, `"a".eq("b")` and `let k: Ordering = 1.cmp(2)` work.
7. **Inside a generic block,** `self` has the block's type and the parameters' bounds hold, as in a struct's generic `impl`; a default item of the interface applies to built-in types too.
8. **Codes:** no new ones. E0459 gains the new placement and type reasons; E0461 the overlap; E0422's help for a built-in type stops saying it can't implement one, and names the block to write and where.
9. **Not derivable by this:** an `impl` block for a built-in type doesn't make its interface derivable (no `with I` on structs follows from it); derive blocks stay `derive I for …`.

## REQUIREMENTS

### Must Do
- Parser: `impl<…> I for Type` with any type after `for`; spans for each part.
- Checker: decisions 1–9; blocks for built-in types kept apart from derive blocks' `derivable`, but found by the same lookup for bounds, method calls, qualified calls and conversions.
- Codegen: method calls and qualified calls on built-in receivers to the block's instance; tables `lugha_vt_<type>.<Interface>` for built-in types, generic ones per instance; boxes for built-in values.
- Language server: hover, go to definition and completion of a block's items on built-in receivers, as for structs.
- Docs: E0459, E0461 and E0422 pages; v18 spec (§4 "Implementing one", "Using one as a bound", interface values; the "Breaking" line about converting a number; §11 row 169; "Changes since v17"); CHANGELOG (Added); TODO.

### Must NOT Do
- The prelude's own `impl<T> T[] { … }` methods (PRE-03), inherent blocks for built-in types in programs, or `impl<T>` on structs.
- Blanket impls (`impl<T: A> B for T`), specialization, or negative bounds.
- Operator interfaces through blocks (`impl Mul<V> for f64`, INT-04): `impl f64 { fun mul … }` stays as it is.
- `Display`/`Debug` (a later PRP).

## DESIGN
- **Representation:** a block for a built-in type is recorded like a `Covers::Type` derive block, with a hidden owner `<I>@<tag>` in its module and the block's parameters as its generics, so its items are checked once and instantiated per type; a flag keeps it out of `Derives::derivable`. Symbols follow derive blocks' (`lugha_fn_<module>/<I>@<tag>.item<args>`), within the `lugha_fn_` prefix.
- **Messages:** E0459 "`impl Show for bool` must be written in the module that declares `Show`", "… for `fun(i64): i64`: a function type can't implement an interface", "… `Eq` for `f64[]`: the prelude decides `Eq`, `Key` and `Ord` for built-in types"; E0461 "`Show` is already implemented for `Point[]`" with a note at `impl<T: Show> Show for T[]`.
- **Performance:** none on existing programs: built-in types' prelude operators stay inline; only new calls go through blocks.
- **Security:** none: no external input, `unsafe`, dependency, `git` or process changes.

## TESTS
- [ ] `tests/programs/interfaces/builtin_impls/`: the OBJECTIVE; a block for a number, `string`, `bool`, `()`, a tuple, `Map<string, V>`, `Option<T>`, `Result<T, E>`; `impl Show for Point[]` in `Point`'s module (a directory case with two modules); a default item on a built-in type; `mut self` items on an array; interface values of built-in types in arrays, passed and returned; qualified calls; the prelude's `cmp`/`eq` through method syntax.
- [ ] Errors, one `.la` + `.stderr` each: a block in a third module (E0459); a function type, an interface type, a bare parameter, an unused parameter (E0459); `impl<T>` before a struct (E0459); `impl Eq for f64[]` (E0459); overlapping blocks (E0461); a block missing an item (E0422); a built-in method name winning, reached by `I.item(x)`.
- [ ] Unit tests: the parser's `impl<…>` forms; the checker's placement, overlap and uses; codegen tables for built-in types.
- [ ] `cargo test --lib`, `--test programs`, `--test spec`, `--test explain`, `--test lsp`.

## ACCEPTANCE
- [ ] The OBJECTIVE prints the lines shown; each commented line gives its error.
- [ ] Existing programs pass unchanged.
- [ ] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every `pub` item documented; spec, CHANGELOG, TODO updated.
