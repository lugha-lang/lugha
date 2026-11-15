## FEATURE: A discarded `Result` is an error; `let _ = e;` discards one on purpose; milestone 175, in v18's Phase 1.

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
import std.fs;

fun save(text: string): Result<(), string> {
    if text == "" {
        return Err("nothing to save");
    }
    Ok(())
}

fun main() {
    save("draft");                       // E0467: this `Result` is discarded
    let _ = save("draft");               // a deliberate discard
    match save("") {
        Ok(()) => println("saved"),
        Err(e) => println("not saved: {e}"),
    }
    defer {
        let _ = fs.remove_file("tmp.txt");   // a discard inside `defer`'s braces
    };
}
```

## CONTEXT
- Draft v18 §1.4 rule 1 [IMP-01]: "Discarding a value of type `Result` is a compile error. `let _ = e;` discards one deliberately."
- Today `let _ = e;` already parses and checks, and E0456 already rejects `defer e;` on a `Result`, whose page shows a discard as `e;` inside braces: that fix becomes `let _ = e;`.
- Measured: no test program, std module or spec example discards a `Result` today; they all use `?`, `match` or `let`.
- Code: `check/stmt.rs` (expression statements), `docs/errors/E0456.md`.

## DECISIONS MADE IN DISCOVERY (mine, for review; nothing here needed the user)
1. **What discards:** an expression statement whose type is the prelude's `Result`, at any type arguments: a call, a method call, an `if` or `match` used as a statement whose branches give one, anywhere a statement goes (functions, methods, closures, blocks, loops, `defer`'s braces).
   - A block's tail (no `;`) is its value, not a discard. `x?` gives the `Ok` value, so it isn't one either.
2. **Only `Result`**, as the draft says. `Option` and anything holding a `Result` (`Result[]`, a struct) can still be dropped.
3. **Through generics:** a `T` that some instance makes a `Result` isn't checked. The rule reads the types as written, so `fun run<T>(f: fun(): T) { f(); }` is fine. It would need the error at each instance, which the checker doesn't report.
4. **The deliberate discard** is `let _ = e;`, which already works. There is no other form (`_ = e;` stays E0201).
5. **Codes:** a new **E0467**, "this `Result` is discarded", with help: handle it with `?` or `match`, or write `let _ = …;` to drop it on purpose. E0456's page changes its fix to `let _ = …;`.
6. **Breaking** for programs that discard a `Result`; none of the repository's do.

## REQUIREMENTS

### Must Do
- Checker: E0467 for decision 1; nothing else changes.
- Docs: the E0467 page; E0456's fix; v18 spec (§6 statements, the error table, §11 row 175, "Changes since v17", Breaking); CHANGELOG.

### Must NOT Do
- The rest of §1.4 (patterns with a leading dot, lossless `as`).
- A `#[must_use]`-like attribute for other types.
- Warnings instead of an error.

## TESTS
- [ ] `tests/programs/errors/discarded_result/`: the OBJECTIVE minus its errors (runs), and each discard form as an E0467 case (a call, a method call, an `if` and a `match` as statements, inside a closure, a loop and `defer`'s braces).
- [ ] Unit tests: each form; `let _ =`, a tail and `?` stay fine; `Option` and a `T` aren't errors.
- [ ] The full `cargo test`, since programs inside the Rust tests may discard one.

## ACCEPTANCE
- [ ] The OBJECTIVE gives E0467 at its first line only, and runs once that line goes.
- [ ] The full `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Spec, CHANGELOG and the error pages updated.
