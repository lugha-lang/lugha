## FEATURE: Generic actors (`actor Cache<K: Key + Sendable, V: Sendable> { … }`) and actors as interface values (`impl Store for UserStore { … }`, whose methods are messages, so a handle converts to `Store`); milestone 202.

**Status:** approved 2026-11-18; built 2026-11-18 (milestone 202).

## OBJECTIVE
```
actor Cache<K: Key + Sendable, V: Sendable> {
    entries: Map<K, V>,
    fun get(self, k: K): Option<V> = self.entries.get(k);
    fun put(mut self, k: K, v: V) { self.entries[k] = v; }
}

interface Store {
    fun find(self, id: string): Option<string>;
}

actor UserStore { users: Map<string, string> }

impl Store for UserStore {
    fun find(self, id: string): Option<string> = self.users.get(id);
}

fun greet(console: Console, store: Store, id: string) {
    console.println(store.find(id).debug());
}

fun main(console: Console, root: Scope) {
    let c = root.spawn(Cache { entries: Map<string, i64> {} });   // a Cache<string, i64>
    c.put("a", 1);
    console.println(c.get("a").debug());                           // Some(1)
    let users = root.spawn(UserStore { users: Map { "ann": "Ann" } });
    greet(console, users, "ann");                                  // Some("Ann"), a message
}
```

## CONTEXT
- **Draft v18 §4.5** rules 4 (generic actors, each instance its own message type) and 5 (`impl I for SomeActor`: the handle implements `I`, the impl's methods are message handlers, calls through `I` are messages) [OPN-04]. The rest of §4.5 was PRP-233; `Remote<T>` (rule 5's second half) is distribution.
- **Spec today:** §4's actors (E0439 for type parameters; a handle is `Type::Actor(key)`, no arguments), §5's interfaces and conversions, §9's tables.
- **Code:** `src/check/actors.rs`, `src/check/types.rs` (`Type::Actor`), the conversion and `impl` checks (`check/impls*`, `check/satisfy.rs`, `Checked.conversions`), `src/codegen/actors.rs` (handlers, message building), `src/codegen/interfaces.rs` (tables), `src/codegen/instances.rs` (generic instances).
- **Asked of the user:** a generic actor writes `Sendable` on its parameters, as other generic code does (E0475 otherwise).
- **Decisions:** none open.

## DESIGN

### Generic actors
1. **`actor Cache<K: Key + Sendable, V: Sendable> { … }`**: an actor may have type parameters with bounds, as a struct may. Each must be `Sendable` (written; E0475 with help to add it), since everything the actor holds and exchanges crosses to its task.
2. **The handle's type is `Cache<string, i64>`**, its arguments those of the state it was spawned from (inferred from the literal or the call making it, or written). Each instance is its own type with its own messages: its handlers are generated per instance, as a generic struct's methods are.
3. **An actor's methods still can't have type parameters of their own** (E0439): a message is one fixed shape per instance.

### Actors as interface values
4. **`impl I for UserStore { … }`** written for an actor makes its handle implement `I`. The block's methods are the actor's: they run inside it on its state, as messages, and follow its rules (no `mut` parameter, E0439; what crosses must be sendable, E0475). They may also be called on the handle directly.
5. **A handle converts to `I`** where an `I` is expected (a parameter, a `let x: I`, an array of `I`), and satisfies `T: I` bounds. A call through the `I` value is a message: a method with a result waits for it, one without is queued, as on the handle.
6. **`I`'s default methods**, when the block doesn't write them, run inside the actor too, on its state, as a message: through `I`, every call reaches the actor the same way.
7. **Generic actors may implement interfaces** (`impl Source<V> for Cache<K: Key + Sendable, V: Sendable>`, written as for any generic type), each instance's handle getting its own table.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Methods of an actor's `impl I` block are ordinary messages too**, callable on the handle without going through `I`: they are the actor's methods.
2. **Interface defaults run inside the actor**, not in the caller: a default would otherwise call the other methods as separate messages, and could interleave with other callers between them.
3. **Every instance of a generic actor shares the declaration's mailbox size.**
4. **`mut self` methods of `I` are allowed on actors** and need no mutable place through `I` either, as on a handle.
5. **No new error codes:** E0439 and E0475 cover the rules, E0422 a handle converted to an interface its actor doesn't implement.

6. **Made during the build:**
   - A handle meets an interface exactly when its actor's state does: the `impl` block is the state's, and the satisfaction check reads it through the state.
   - Every instance the handle needs of an interface's items (through a conversion or a `T: I` bound) is the state's instance: a default instantiated for the handle would treat the state as a handle inside the actor, and both print alike.
   - A call through a bound on an actor finds the state's block method in codegen, as for a struct, else the default.
   - A slot sent from a table carries no call site: a call through `I` to an actor that has ended panics without a location.
   - Found, not fixed (it exists on `main` too): a written generic type whose argument must meet `Key`, `S<string>` for `struct S<K: Key>`, reports that `string` doesn't satisfy `Key`; spawned states infer their arguments, so actors meet it only when written.

## REQUIREMENTS

### Must Do
- Type parameters on actors; `Type::Actor` with arguments through checker and codegen; handlers per instance.
- `impl I for Actor`; conversions of handles to `I`; bounds `T: I` met by a handle; calls through `I` as messages; defaults inside the actor; generic actors implementing interfaces.
- Spec §4–§5, §9, "Changes since v16", §11 row 202; CHANGELOG (Added); E0439's page (type parameters allowed now; method type parameters still not).

### Must NOT Do
- `Remote<T>` and its `Result`-only interfaces: distribution.
- Type parameters on an actor's methods.

## TESTS
- [x] Checker unit tests: a generic actor's handle type, inferred and written; E0475 without `Sendable`; E0439 for a method's type parameters; `impl I for Actor` meeting `I`; a handle converting to `I` and meeting `T: I`; an `impl` method with a `mut` parameter (E0439).
- [x] `tests/programs/concurrency/actors/`: a generic cache used at two instances; a handle passed as an interface value, called through it (a result, a cast, a `mut self` method, a default method); an array of interface values mixing an actor and a struct; a generic actor implementing a generic interface.
- [x] Targeted: `--lib`, `programs`, `leaks`, `spec`, `explain`, `comments`.

## ACCEPTANCE
- [x] The objective program prints `Some(1)` and `Some("Ann")`.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [x] Spec, CHANGELOG and E0439's page updated.
