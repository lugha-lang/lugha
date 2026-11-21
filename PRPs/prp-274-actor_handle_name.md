## FEATURE: An actor's name means its handle everywhere, inside its body too, and `Self` means its state there, so an actor can hold, take and send handles of its own type; milestone 242.

**Status:** approved 2026-11-21.

## OBJECTIVE
```
actor Holder {
    out: Option<Sender<Holder>>,                 // a channel of Holder handles
    fun keep(mut self, s: Sender<Holder>) { self.out = Some(s); }
    fun new(): Self = Holder { out: None };     // Self: the state
    fun has_out(self): bool = self.out.is_some();
}

fun main(console: Console, root: Scope) {
    let h = root.spawn(Holder.new());
    let (tx, rx) = Chan<Holder>.unbounded();
    h.keep(tx);
    console.println(h.has_out());               // true
    h.stop();
}
```
Today this is rejected with `expected Sender<Holder>, found Sender<Holder>`, because inside the body `Holder` means the state and the handle type can't be written there at all.

## CONTEXT
- **The user's question** (2026-11-21): naming an actor's handle inside its body. They chose "the name means the handle everywhere, `Self` the state" after comparing it with `Self.Handle` and a generic `Handle<T>`.
- **Spec:**
  - §3: "Inside the body `Self`, and the actor's own name, mean its state" changes.
  - §4: actors, and actors as interface values.
  - §10, if an example writes the name inside a body. None does today.
- **Decisions:** DECISIONS.md entry 175 (actors, milestone 92) says "the name is the handle type outside, the state inside". This PRP changes that, and the entry gets a note.
- **Code:**
  - `src/parser/self_type.rs`: `Self` in an `impl` is rewritten to the type's name, so later stages can't tell the two apart.
  - `src/check/actors.rs`: `as_handle`, whose `in_actor` exception makes the name mean the state inside.
  - `src/check/generics.rs`: named and applied types go through `as_handle`.
  - `src/check/methods.rs` and `structs.rs` set `in_actor`.
  - `src/check/errors/types.rs`: the state-or-handle help added by the last fix.
- **What uses the old meaning:** `tests/programs/concurrency/actors/counter.la` (`fun new(start: i64): Counter = …`) and `state_or_handle.la` (the error case). Nothing in `skill/`, the spec's examples, `src/std` or `src/prelude`.
- **Security:** none of CLAUDE.md's list.
- **Decisions:** none open.

## DESIGN
1. **The name is the handle.**
   - A type written with the actor's name means its handle everywhere, inside its body and its `impl` blocks too: fields, parameters, results, type arguments (`Sender<Holder>`, `Holder[]`, `Option<Holder>`) and `let` annotations.
   - For a generic actor, `Cache<K, V>` inside its body is the handle of that instance.
2. **`Self` is the state.**
   - Inside an actor's body, and inside an `impl I for SomeActor` block, `Self` means the state, as `self` is the state.
   - The parser keeps `Self` distinct in an actor's declaration, instead of rewriting it to the name, and the checker resolves it to the state.
   - Elsewhere `Self` keeps its meaning: the type in a struct's or enum's `impl`, the implementing type in an interface.
3. **Literals and calls don't change.**
   - A struct literal `Holder { … }` builds the state, inside or outside, as `spawn` needs.
   - `Holder.new()` calls the actor's function without `self`, as before.
4. **The handle inside.**
   - A message may take, return and store handles of its own actor, and they cross as any handle does: they are `Sendable`, and other actors' handles are already allowed.
   - A handle field in the state is a pointer, so a state may hold handles of its own type (no E0307).
   - Calling a message on a handle of its own actor from inside a message waits like any call. Calling itself that way deadlocks, as calling any actor that is waiting on you does today (spec §6).
5. **The error's help.**
   - The mismatch help from the last fix now explains `Self` against the name: "inside `actor Holder`, `Self` is its state and `Holder` a handle to it".
   - Both still print as `Holder`, so the help is still what tells them apart.
6. **Breaking.**
   - A type written with the actor's own name inside its body meant the state and now means the handle. Code that meant the state writes `Self`: `fun new(start: i64): Self = Counter { count: start };`.
   - Where the old meaning no longer fits, the error is E0403 with the help above, pointing at `Self`.

## DECISIONS MADE IN DISCOVERY (the user chose the meaning; these are mine, for review)
1. **`Self` in `impl I for SomeActor` is the state too,** like the body. Its methods run inside the actor on the state, so one rule covers every place an actor's messages are written.
2. **The parser marks `Self` in actor declarations** rather than the checker guessing from the name. Rewriting `Self` to the name is exactly what loses the distinction.
3. **No way to get one's own handle (`self`'s handle) here.** An actor gets a handle of its own type when one is passed to it, as in the objective. A built-in for "my own handle" (Erlang's `self()`) is a separate question, left for later if asked.
4. **No new error code.** A wrong meaning shows as a type mismatch, E0403, with the help.

## REQUIREMENTS
### Must Do
- Design 1–6.
- **Update:** `counter.la` to `Self`, and `state_or_handle.la` (it becomes a program that runs, plus a new reject case for `Self` against the handle).
- **Spec:**
  - §3's sentence on names inside the body;
  - §4 (actors: what the name and `Self` mean, with the objective's example);
  - §11 row 242;
  - a Changes bullet marked **Breaking**.
- **Elsewhere:**
  - DECISIONS.md entry 175 gets a note on the change;
  - the skill's channels-and-actors page states the rule;
  - CHANGELOG.
### Must NOT Do
- Move an actor's methods out of its body.
- Add a way for an actor to get its own handle.
- Change what `Self` means outside actors.

## TESTS
- [ ] `--lib`, in `check/actors.rs`:
  - [ ] the name inside a body types as the handle (field, parameter, result, `Sender<Holder>`), and `Self` as the state;
  - [ ] a generic actor's `Cache<K, V>` inside is the instance's handle;
  - [ ] in `impl I for SomeActor`, `Self` is the state;
  - [ ] a struct literal of the actor's name is still the state;
  - [ ] the old meaning (`fun new(): Holder = Holder { … }`) is E0403 with the help naming `Self`.
- [ ] `--lib`: the parser keeps `Self` in an actor declaration, and still rewrites it in a struct's `impl`.
- [ ] `tests/programs/concurrency/actors/`:
  - [ ] the objective (`own_handles.la`);
  - [ ] an actor keeping a list of handles of its own type and messaging them;
  - [ ] `counter.la` updated;
  - [ ] a reject case for `Self` where a handle is expected.
- [ ] Targeted: `--lib`, `programs`, `rc_check` (the concurrency programs under the owner check), `spec`, `skill`, `comments`.

## ACCEPTANCE
- [ ] The objective runs as shown.
- [ ] Inside and outside an actor, its name means the same type.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
