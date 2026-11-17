## FEATURE: The sendable rule: a value crossing to another task must be `Sendable`, an `auto` prelude interface worked out from a type's parts; so a `Ref` never reaches two tasks; milestone 197.

**Status:** proposed 2026-11-17.

## OBJECTIVE
```
struct Job { id: i64, tags: string[] }               // sendable: its parts are
struct Shared { hits: Ref<i64> }                     // not: a Ref

fun work(job: Job): i64 = job.id;

fun main(console: Console) {
    let hits = Ref.new(0);
    let r = scope s {
        s.spawn(work(Job { id: 1, tags: [] }));                 // fine
        s.spawn(fun() { hits.set(1); }());
        //      ^^^^^^^^^^^^^^^^^^^^^^^^^ error[E0475]: this call can't cross to another task:
        //      it captures `hits`, a `Ref<i64>`, which two tasks would share
    };
    let c: Chan<Shared> = Chan.new();
    //     ^^^^^^^^^^^ error[E0475]: `Shared` isn't sendable: `.hits` is a `Ref<i64>`
}
```
Data races become impossible by construction: the only shared, changeable thing, `Ref`, can't reach a second task. State that tasks share lives in an actor or travels through channels.

## CONTEXT
- **Draft v18 §4.1 rule 3** [COR-03]: arguments and captures of a spawned call, channel elements, actor message arguments and replies must be sendable, worked out from a type's parts; not `Ref`, nor closures capturing one, nor interface values unless their interface requires `Sendable`. **§1.1 rule 9** [INT-07]: `interface I: Sendable`. **§3 rule 14** [CAP-14]: capabilities are sendable. **§4.4 rule 9** [CHN-09]: a channel's element type must be sendable; channels are.
- **Decision 224** (with the user): `Sendable` is an interface with derive blocks, marked `auto` so they apply to every type whose parts allow; a function value is sendable when its own type is, from its captures (PRP-228 gives function values their own types).
- **Today:** only actors are checked (`check/actors.rs`, `may_share`, E0439): an actor's fields, message parameters and results may hold no `Ref`, function or interface value (capabilities excepted). Tasks and channels share anything.
- **Derive machinery:** `check/derive_meets.rs` (`unmet_derived`) walks a type's parts against an interface's derive blocks and names the first part that fails; `check/derive.rs` holds the blocks.

## DESIGN

### `Sendable`
1. **The prelude declares `auto interface Sendable {}`** and its derive blocks: one for each number type, `bool`, `char`, `string` and `()`; `derive<T: Sendable> Sendable for T[]`, `Option<T>` and `Chan<T>`; and `struct`, `enum` and `tuple` blocks. No block covers `Ref<T>`, so nothing holding one is sendable.
2. **`auto`** means the blocks apply to every type whose parts meet them, with no `with`. It is written only in the prelude (one auto interface exists). `impl Sendable for T` and `with Sendable` are E0476: sendability is worked out, never claimed.
3. **What else is sendable:**
   - an actor handle and a C handle (`extern type`): one task owns an actor's state, and C handles carry no Lugha values;
   - an interface value whose interface requires `Sendable`;
   - a function's own type when its captures are (a named function captures nothing);
   - a type parameter bounded `T: Sendable`.
4. **What isn't:** `Ref`, a plain `fun(…)` value (which function, and what it captured, is unknown), an interface value whose interface doesn't require `Sendable`, `Scope` and `Task` (already E0472).
5. **Interfaces that require `Sendable`:** the capabilities and what they open (`Console`, `Fs`, `Dir`, `File`, `Clock`, `Env`, `Proc`, `Entropy`, `Net`, `Listener`, `Conn`, `Udp`), so they can be passed to tasks [CAP-14]; and `Error`, since a task's error crosses to its scope. An implementation must then be sendable: `impl Conn for Pipe` is refused if `Pipe` holds a `Ref` (E0475 at the block).
6. **Converting to such an interface** checks the value's type: a struct or enum from its fields, a function value from its captures. A plain `fun(…)` value converts to none of them.

### Where values cross
7. **`s.spawn(call)`:** every argument, everything the call captures (a closure literal's captures included) and the call's result (what `join` hands back) must be sendable.
8. **Channels:** the element type of a channel made (`Chan.new`, `Chan.buffered`) or written (`Chan<T>`) must be sendable.
9. **Actors:** an actor's fields, each message's parameters and its result: the sendable rule replaces E0439's own sharing rule, so capabilities, `: Sendable` interfaces and sendable functions may now cross, as everywhere.
10. **Generic code:** `T` crossing needs `T: Sendable`; the help says so.

### Errors
- **E0475** — a value that isn't sendable crosses to another task. The message names the path to the first part that fails (`.hits is a Ref<i64>`, a capture by name) and where it crosses (spawned call, channel, actor). Help: keep the state in an actor or send copies through a channel; for `T`, add `T: Sendable`; for a `fun(…)` parameter, take an interface that requires `Sendable`.
- **E0476** — `impl Sendable`, `with Sendable`, or `auto` outside the prelude.
- E0439 keeps its other uses (an actor's state outside `spawn`, `mut` parameters, `stop`); its sharing part moves to E0475. **E0438 stays**: `s.spawn(bump(mut c))` would silently change the task's own copy.

### Library and programs
- **`std.http`:** `interface Handler: fun(Request): Response, Sendable {}`; `serve` and its kin take a `Handler`. `http.serve(net, addr, handle)` is unchanged for a named function or a closure capturing sendable values.
- **Programs that share a `Ref` between tasks** are rewritten to a channel or an actor (about eight end-to-end programs, and the TLS-over-pipes test's fake `Conn`, whose `closed` flag is a `Ref`).

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **`Sendable` reuses the derive machinery** (blocks plus field walking), with `auto` skipping the `with`; the user-visible rule is the Draft's "worked out from a type's parts".
2. **`auto` is prelude-only.** One auto interface is needed; opening it to programs waits for a second use.
3. **`Error` requires `Sendable`**, since a task's `Err` becomes its scope's `Failure.Error`. An error type holding a `Ref` stops compiling; no bundled one does.
4. **A task's result is checked too** (rule 7). The Draft lists arguments and captures; the result crosses back the same way.
5. **C handles and actor handles are sendable**; only `pinned` (thread-bound) C types, which don't exist yet, won't be.
6. **Two new codes**, E0475 and E0476; E0439's sharing part retired into E0475 (its page updated, the code kept for the rest).
7. **E0438 kept**, against the Draft's "replaced": a `mut` argument to a spawned call is a silent no-op, not a race, and deserves its error.

## REQUIREMENTS

### Must Do
- `auto interface Sendable {}` with prelude derive blocks; the checks at every crossing in rules 7–9; conversions in rule 6; capabilities and `Error` requiring `Sendable`; `T: Sendable` bounds.
- E0475 and E0476 with pages; E0439's page updated.
- `std.http`'s `Handler`; every program sharing a `Ref` across tasks rewritten.
- Spec §3, §4, §6, §9 and "Changes since v16" (Breaking); §11 row 197; CHANGELOG; CLAUDE.md if a rule changes.

### Must NOT Do
- Channel splitting, `Ring<T>`, `Weak`, `Wire`, `pinned` types: their own PRPs.
- Several OS threads: the rule prepares for them; tasks stay on one thread.

## TESTS
- [ ] Checker unit tests: each sendable and non-sendable kind in rules 1–4; crossings 7–9 with the path in E0475; function values by captures; `T: Sendable`; E0476 for `impl`, `with` and `auto`.
- [ ] `tests/programs/concurrency/sendable/`: the objective's errors; sharing through an actor and a channel instead; a `Handler` closure; a fake `Conn` (sendable) passed to a task.
- [ ] Rewritten programs print what they printed.
- [ ] Targeted: `--lib`, `programs`, `leaks`, `spec`, `http`, `net`, `tls`, `tls_keys`, `tls_over_conn`, `udp`, `explain`, `comments`.

## ACCEPTANCE
- [ ] No program can make a `Ref` reachable from two tasks; every crossing is checked.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [ ] Spec, CHANGELOG, error pages updated.
