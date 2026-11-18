## FEATURE: Actors in scopes: `s.spawn(Counter { … })` starts an actor in a scope, which stops it once the scope's tasks are done and reports its failure; `join` on an actor goes; bounded mailboxes, 1,024 by default or `actor Logger mailbox 10_000 { … }`; milestone 201.

**Status:** proposed 2026-11-18.

## OBJECTIVE
```
actor Tally mailbox 4 {
    count: i64,
    fun add(mut self, n: i64) { self.count += n; }
    fun total(self): i64 = self.count;
}

fun work(t: Tally, n: i64) { t.add(n); }

fun main(console: Console) {
    let r = scope s {
        let tally = s.spawn(Tally { count: 0 });   // an actor, in the scope
        let _ = scope w {
            for i in 1..11 {
                w.spawn(work(tally, i));            // senders wait while 4 are queued
            }
        };
        tally.total()
    };
    console.println(r.unwrap());                    // 55
}
```
Once `s`'s body and tasks are done, `s` stops `tally` (after the messages already queued) and waits for it to end; then the scope ends. A call to a stopped actor still panics with ``actor `Tally` has stopped``.

## CONTEXT
- **Draft v18 §4.5** rules 1 (an actor starts in a scope and stops when the scope ends) [ACT-01], 2 (bounded mailboxes; the local half: a caller waits while full) [ACT-02], 6 (`stop()` and `is()` stay, `join()` goes: the scope reports failures) [ACT-04]. Rules 4–5 (generic actors, actors as interface values) are PRP-234 (the user's split); the remote half of rule 2 (`Rejected`) is distribution (phase 9).
- **Spec today:** §4 and §6 on actors (bare `spawn Counter { … }`, unbounded mailboxes, `join(): Option<string>`), §9's runtime table.
- **Code:** `src/check/actors.rs`, `src/check/scopes.rs` (`spawn_in`), `src/codegen/actors.rs`, `runtime/actors.c`, `runtime/scopes.c` (a scope's end, failure and cancellation), the parser's actor declaration.
- **Builds on:** PRP-227 (scopes, cancellation), PRP-230 (policies), PRP-231 (deadlines, Ctrl-C), PRP-232 (deadlock reports name actors).
- **Decisions:** none open.

## DESIGN

### Starting and stopping
1. **`s.spawn(Counter { count: 0 })`** (or a call giving the state, as today) starts an actor in scope `s` and gives its handle. Bare `spawn Counter { … }` is E0474, with help naming `s.spawn(…)` and `main`'s root scope.
2. **A scope stops its actors once its body and tasks are done:** each gets a `stop` after the messages already queued, and the scope waits until every one has ended. So an actor outlives every task that may call it.
3. **Leaving early, a failure, a deadline or Ctrl-C** cancel a scope's actors with its tasks: an idle actor ends at once; one inside a handler is cancelled there as a task is (its `defer`s run). Callers waiting on it, and messages still queued, get ``actor `Counter` has stopped``.
4. **An actor's panic fails its scope** as a task's does (`Failure.task` naming the actor's type, `.Panic(message)`), and under `independent` it is reported and the rest run on. The callers of a panicked actor still panic in turn (``actor `Counter` panicked``), as today.
5. **`join()` on an actor is gone** (E0415, with help: the actor's scope reports how it ended). `stop()` and `is()` stay. Handles stay first-class: they may be stored, sent and returned, unlike `Scope` and `Task`, and a call after the actor ended panics.

### Bounded mailboxes
6. **A mailbox holds 1,024 messages** unless the declaration says otherwise: `actor Logger mailbox 10_000 { … }`, a positive integer literal after the name (E0439 for anything else). A call or a cast to a full mailbox waits for room, a pause point (cancellable). `stop` never waits for room: it is queued past the limit, so a scope can always stop its actors.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The scope stops its actors after its tasks, not with its body:** tasks may still be calling them; stopping them first would make those calls panic.
2. **`stop` ignores the mailbox's limit**, so stopping never waits.
3. **Actors don't count toward `limit n`:** an actor runs until its scope ends, and would hold a slot the whole time.
4. **A cancelled actor's callers see `has stopped`**, not a new message: from outside, the actor stopped.
5. **The mailbox size is a literal** written in the declaration: it is part of the actor's type, like a field's type, not a value computed at run time.
6. **No new error codes:** a bare `spawn` of an actor is E0474 (a task outside a scope already is), `join` E0415, a bad `mailbox` E0439.
7. **`Failure.task` names an actor by its type** (`Counter`), as a task is named by its function.

## REQUIREMENTS

### Must Do
- `s.spawn(state)` for actors; E0474 for bare `spawn`; scopes stopping, cancelling and reporting their actors; `join` removed; `mailbox n`, 1,024 by default.
- Migrate every program using actors: `tests/programs`, the programs inside `tests/*.rs`, the spec's §10 actors program, error pages.
- Spec §4–§6, §9, "Changes since v16", §11 row 201; CHANGELOG (Breaking); E0474's and E0439's pages.

### Must NOT Do
- Generic actors and actors as interface values: PRP-234.
- `Rejected` for remote mailboxes: distribution.

## TESTS
- [ ] Checker unit tests: `s.spawn` of an actor's state; E0474 for bare `spawn`; `join` E0415; `mailbox` (a literal, positive; E0439 otherwise).
- [ ] `tests/programs/concurrency/actors/`: an actor stopped by its scope after its tasks; a panicking actor failing its scope (and under `independent`); a scope cancelled while an actor waits in a handler, its callers seeing `has stopped`; a full mailbox making a sender wait; `stop` past a full mailbox; the root scope stopping its actors when `main` returns.
- [ ] Every migrated program keeps its output, or its new expected output where `join` went.
- [ ] Targeted: `--lib`, `programs`, `leaks`, `spec`, `explain`, `comments`, `tasks_runtime`, and the test files whose embedded programs use actors.

## ACCEPTANCE
- [ ] Actors start in scopes, stop with them, fail them, and their mailboxes are bounded.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [ ] Spec, CHANGELOG and error pages updated.
