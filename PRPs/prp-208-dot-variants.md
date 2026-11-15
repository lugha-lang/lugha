## FEATURE: Enum variants in patterns take a leading dot, `.Some(x)`, `.None`, `.Circle(r)`; a bare name in a pattern always binds; milestone 176, in v18's Phase 1.

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
enum Shape { Circle(f64), Rect(f64, f64), Empty }

fun area(s: Option<Shape>): f64 {
    match s {
        .Some(.Circle(r)) => 3.14 * r * r,
        .Some(.Rect(w, h)) => w * h,
        .Some(.Empty) | .None => 0.0,
    }
}

fun main() {
    let shapes = [Some(Shape.Circle(1.0)), Some(Shape.Rect(2.0, 3.0)), None];
    for s of shapes {
        println(area(s));
    }
    if let .Some(.Rect(w, _)) = shapes[1] {
        println(w);
    }
    let .Some(first) = shapes[0] else { return; };
    match first {
        .Circle(_) => println("round"),
        other => println("not round: {other}"),   // a bare name binds, at the top level too
    }
    match shapes[2] {
        None => println("none"),                   // E0468: `None` is a variant of `Option<Shape>`: write `.None`
        _ => {}
    }
}
```

## CONTEXT
- Draft v18 §1.4 rule 2 [IMP-02, OPN-09]: "Every enum variant in a pattern is written with a leading dot, the prelude's included: `.Some(x)`, `.None`, `.Ok(v)`, `.Err(e)`, `.Circle(r)`. A bare name in a pattern is always a new binding."
- Today (spec §4 `match`): a bare name at the top level must be a variant; in a payload slot it is a variant if the slot's type has one by that name, else a binding. So the meaning of `x` depends on the types, and a top-level binding can only be written `mut x`.
- Patterns appear in `match`, `if let`, `while let` and `let … else`; all share `parser/pattern.rs` and `check/patterns.rs`.
- Measured: about 730 arms in `.la` files (tests 460, std 220, prelude 45), 95 in the spec, a few in docs/errors pages and Rust tests' inline programs.
- Code: `parser/pattern.rs`, `parser/conditional.rs`, `ast/pattern.rs`, `check/patterns.rs`, `check/or_patterns.rs`, `check/exhaustive*` (missing-case text), `codegen/{patterns,matching,or_patterns,hoist_scan}.rs`, `formatter/leaves.rs`, `parser/sexp`.

## DECISIONS MADE IN DISCOVERY (mine, for review; nothing here needed the user)
1. **Syntax:** `.Name` and `.Name(p, …)` are variant patterns, anywhere a pattern goes (top level, payload slots, tuples, or-patterns; `match`, `if let`, `while let`, `let … else`). The enum's name is never written; the type comes from the matched value, as today. `.Shape.Circle` isn't a form.
2. **A bare name binds, everywhere,** the top level included: `other => …` is a catch-all that binds the whole value (a later arm is W0102, as after `_`). `mut x` keeps working.
3. **Two errors keep the old form from changing meaning silently:**
   - `Name(…)` without the dot is a parse error, **E0212** "a variant in a pattern takes a leading dot", help `.Name(…)`.
   - A bare binding whose name is a variant of the type it binds (`None` against an `Option`, `Empty` in a `Shape` slot) is **E0468**, same message and help. Without it, every unmigrated `None =>` would become a catch-all. A binding named like some *other* type's variant is fine.
4. **Expressions don't change:** `Shape.Circle(1.0)`, `Some(x)`, `None` and `Ok(v)` as values stay as they are; the draft's rule is about patterns.
5. **Messages print the dot:** E0417's missing cases read `.Some(.Rect(_, _))`, `.None`; W0102 and E0416 quote patterns as written.
6. **Tooling:** the formatter prints the dot; hover and go-to-definition on `.Circle` work as on `Circle` today (the name's span excludes the dot). No LSP completion after `.` in a pattern in this PRP; the VS Code grammar doesn't colour variants, so it doesn't change.
7. **Migration:** every pattern in the repository (tests, std, prelude, spec, error pages, bench, editor samples, Rust tests' inline programs) gets its dots, driven by the two new errors' spans from `--diagnostics=json`, so nothing is rewritten by guesswork.
8. **Breaking** for every program that matches on an enum; listed under Breaking.

## REQUIREMENTS

### Must Do
- AST: a `Binding(Ident)` pattern for a bare name; `Variant` only for the dotted form.
- Parser: `.Name[(…)]`; bare `Name(` is E0212.
- Checker: the new binding rule and E0468; exhaustiveness and W0102 unchanged in meaning; messages with dots.
- Codegen, formatter, sexp printer: follow the AST change.
- Docs: pages for E0212 and E0468; v18 spec (grammar, §4 `match` and its examples, every pattern in the spec, the error table, §11 row 176, "Changes since v17", Breaking); CHANGELOG.

### Must NOT Do
- The rest of §1.4 (lossless `as`).
- `.Variant` as an expression (`let s: Shape = .Circle(1.0)`).
- Completions after `.` in patterns.

## TESTS
- [ ] `tests/programs/patterns/dot_variants/`: the OBJECTIVE minus its error (runs), E0212 and E0468 cases, a top-level binding arm with W0102 after it.
- [ ] Parser unit tests: dotted forms nest; `Name(` is E0212; bare names are bindings.
- [ ] Checker unit tests: each form; E0468 at top level and in a slot; a binding named like another type's variant is fine; E0417 prints dots.
- [ ] Formatter round-trip.
- [ ] The full `cargo test`, since the change reaches every program with a `match`.

## ACCEPTANCE
- [ ] The OBJECTIVE gives E0468 at the `None =>` arm only, and runs once that line is `.None`.
- [ ] The full `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Spec, CHANGELOG and the error pages updated.
