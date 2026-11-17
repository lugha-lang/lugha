## FEATURE: Function types are interfaces: every function value has a type of its own (a struct of its captures) implementing its signature's interface `fun(A): R`, so `<F: fun(A): R>` keeps the exact function and `interface Job: fun(), Other {}` names a kind of function; milestone 196.

**Status:** proposed 2026-11-17.

## OBJECTIVE
```
interface Pricing: fun(i64): i64 {}         // a kind of function, as an interface

fun twice<F: fun(i64): i64>(f: F, x: i64): i64 = f(f(x));   // the exact function: a direct call

fun total(rule: Pricing, xs: i64[]): i64 {
    let mut sum = 0;
    for x of xs { sum += rule(x); }
    sum
}

fun main(console: Console) {
    let rate = 3;
    let triple = fun(x: i64): i64 = x * rate;     // its own type: a struct { rate: i64 }
    console.println(twice(triple, 2));            // 18: twice<that closure>, triple called directly
    console.println(total(triple, [1, 2]));       // 9: converted to the interface `Pricing`
    let f: fun(i64): i64 = triple;                // and to `fun(i64): i64`, as today
    console.println(f(5));                        // 15
}
```
Every program that uses function values today means the same and builds unchanged.

## CONTEXT
- **Decision 224** (DECISIONS.md, resolved with the user): function types are interfaces; a function value is a struct of its captures. It prepares the sendable rule (PRP-229), where a function value is sendable when its captures are, with no special case.
- **Spec v18:** §3 (types: `fun(A): R`), §4 (function values, closures, interfaces, generics and bounds, the one implicit conversion), §6 (calls), §8 (closure and instance symbols). **Draft:** §1.1 rules 8–10 (interfaces, conversion), §4.1 rule 3 (closures and sendability, next PRP).
- **Today:** `Type::Fun(params, ret)` is the only function type. A closure literal, a named function used as a value and a variable all have it. At run time a function value is a `{ code, env }` pair: the environment is a counted record of the captures (a named function's is null), and a call is indirect (`codegen/fun_values.rs`, `codegen/closures.rs`). Interface values are `{ table, box }` (`codegen/interfaces.rs`), and bounds name interfaces only (`ast::Bound`).

## DESIGN

### Types
1. **A function value's own type.** Each closure literal and each named function used as a value has a type of its own: an unnamed struct whose fields are the captures (none for a named function), implementing its signature's interface. It prints as `fun(i64): i64 {closure at 7:18}` or `fun(i64): i64 {triple}` in diagnostics.
2. **`fun(A): R` is an interface,** with one method: the call. A value of type `fun(A): R` is one function, its own type hidden. Nothing changes in how it's written or called.
3. **Conversion:** a function value converts to `fun(A): R`, and to any interface that requires it, where one is expected: the one implicit conversion, value to interface (spec §4).
4. **Where function values meet, they keep working as today:** two function values of one signature but different own types, meeting in `if`/`match` branches, array or map elements, or a reassigned `let mut`, give `fun(A): R`. A `let mut` initialised with a function value has type `fun(A): R`, so `f = other` still works. Interface values in general don't do this (`[Circle {…}, Square {…}]` needs a written type); functions do, because every existing program relies on it.

### Bounds and interfaces
5. **`fun(A): R` as a bound:** `<F: fun(A): R>`, alone or with others (`<F: fun(A): R + Eq>`, though no function is `Eq`). The argument's own type becomes `F`: one instance per function passed, and the call `f(x)` inside is a direct call of that function's body.
6. **`fun(A): R` as a required interface:** `interface Job: fun(), Named {}`. A value of such an interface is callable (`job()`) and has the other interfaces' methods. A function value implements it only through its signature: `Job` with other required interfaces is met by no function value until `with` on closures exists (not in this PRP), but structs may `impl Job` by implementing those interfaces and the call (rule 7).
7. **A struct may implement `fun(A): R`** by hand: `impl fun(i64): i64 for Scale { fun call(self, x: i64): i64 = x * self.k; }`, which makes a `Scale` callable and convertible to `fun(i64): i64`. The method is named `call` (prelude-reserved for this).

### Run time and codegen
8. **A function value's own type is represented as today's pair** `{ code, env }`: no new allocation, and converting it to `fun(A): R` is free. Where its own type is known (rule 5, or a local), a call is a direct call of its body with the environment; through `fun(A): R`, an indirect call, as today.
9. **A value of an interface requiring `fun(A): R`** is an interface value: `{ table, box }`, the table's call slot the function's body; a function value boxes its pair. A plain `fun(A): R` stays a pair, not a table and box.
10. **Symbols:** an own type prints in instance names as its closure symbol (`twice<lugha_closure_main.0>`, `twice<triple>`), within the existing `lugha_fn_` instance scheme: no new symbol kind.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Function values meet as `fun(A): R`** (rule 4), and a `let mut` initialised with one is typed `fun(A): R`: no existing program changes. Rust makes each mix an error; that would break today's programs for no safety gain.
2. **The call method is `call`**, so a struct can implement a function interface by hand (rule 7). It costs nothing and makes "a function is an interface" literally true.
3. **Representation stays a pair** for own types and `fun(A): R` alike (rule 8); only interfaces that *require* `fun(A): R` use tables. Calls through known own types become direct, so generic code over closures gets faster, and nothing gets slower.
4. **Printed names** of own types say where they come from (`{closure at 7:18}`, `{triple}`), so errors point at the function.
5. **No new error codes expected:** misuse reuses E0422 (doesn't implement), E0403/E0402 (type mismatch). If one turns out needed, it takes the next free number (E0475).

## REQUIREMENTS

### Must Do
- Own types for closure literals and named function values; conversion to `fun(A): R` and to interfaces requiring it; meeting rule 4.
- `fun(A): R` in bounds, in interface parents, and implementable by structs (`call`).
- Direct calls through known own types; interface values requiring `fun(A): R` callable.
- Spec §3, §4, §6, §8 and "Changes since v16"; §11 row 196; CHANGELOG.

### Must NOT Do
- `Sendable`, `auto` interfaces, the sendable rule: PRP-229.
- `with` on closure literals; functions that are `Eq`, `Key` or `Ord`.
- Changing a function value's run-time representation or what any existing program prints.

## TESTS
- [ ] Checker unit tests: a closure's own type and its conversion; `<F: fun(i64): i64>` infers `F` from a closure and from a named function; branches, arrays and `let mut` meeting as `fun(A): R`; `interface Job: fun()` values callable; `impl fun(i64): i64 for S`; E0422 for a non-function passed to `<F: fun(i64): i64>`.
- [ ] Codegen tests: one instance per function passed; a direct call inside it; `fun(A): R` calls still indirect.
- [ ] `tests/programs/functions/as_interfaces/`: the objective program; a struct implementing `fun(i64): i64`; an interface requiring a function; generic code over closures capturing different things; errors.
- [ ] Every existing program unchanged: `programs`, `leaks` (closures freed as before), `codegen`, `spec`, `--lib`.

## ACCEPTANCE
- [ ] The objective program prints `18`, `9`, `15`.
- [ ] Existing programs build and print exactly as before.
- [ ] `cargo test --lib` and the touched integration tests pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated where they change.
