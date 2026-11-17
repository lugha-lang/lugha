## FEATURE: How a scope runs its tasks: `independent` and `independent(f)`, `limit n`, `scope<E>`, and failures that name their task (`Failure` becomes a struct with `task` and `kind`); milestone 198.

**Status:** approved 2026-11-18; built.

## OBJECTIVE
```
fun handle(conn: Conn) { … }                       // may panic on bad input

fun serve(listener: Listener): Result<(), Failure> {
    scope conns independent limit 1000 {           // a failing connection doesn't stop the rest
        loop {
            match listener.accept() {
                .Ok(conn) => { conns.spawn(handle(conn)); }   // waits while 1000 run
                .Err(_) => {}
            }
        }
    }
}
// stderr, for each failure: task `handle` failed: panic: bad header at app.la:12:9

fun sync(net: Net): Result<(), Failure<SyncError>> {
    scope<SyncError> s {
        s.spawn(download(net));                    // Result<_, SyncError>
        s.spawn(save());                           // Result<_, SyncError>
    }
}

match sync(net) {
    .Ok(_) => {},
    .Err(f) => match f.kind {
        .Error(e) => console.println("{f.task} failed: {e}"),     // e: SyncError
        .Panic(m) => console.println("{f.task} panicked: {m}"),
    },
}
```

## CONTEXT
- **Draft v18 §4.2** rule 4 (a failure records which task failed) [SCO-04], rule 5 (`scope<E>`) [OPN-05], rule 8 (`independent`) [SCO-06], rule 10 (header words) [OPN-07], rule 12 (`limit n`) [SCO-09], rule 14 (`name(text)`) [SCO-11]. Phase 5's plan (PRP-227): this is the first half of scope settings; deadlines, `shield` and Ctrl-C are PRP-231.
- **Asked of the user:** a `Failure` keeps the task's name as a struct, `Failure { task, kind }` (chosen over a second payload in each variant, or the name only in the printed text).
- **Today (m195–197):** `Failure` is `.Error(Error)` or `.Panic(string)`; every scope is all-or-nothing; `t.name()` is stored and never read; a root-scope task's `Err` prints only "a task in the root scope failed". A scoped task's panic ends at its fiber's start, where the runtime records it (`runtime/scopes.c`, `scope_task_unwound`).

## DESIGN

### `Failure`
1. **`struct Failure<E> { task: string, kind: FailureKind<E> }`** and **`enum FailureKind<E> { Error(E), Panic(string) }`** in the prelude; `Failure` written alone means `Failure<Error>`, as `Task` alone means `Task<()>`. `Failure` implements `Display` (`task `fetch` failed: connection reset`, `task `fetch` panicked: boom at …`) and `Error`, so `?` still works. PRP-231 adds `.Deadline` to `FailureKind`.
2. **`task`** is the task's name: what `t.name(text)` set, else the called function's name (`work` for `s.spawn(work(x))`, `area` for `s.spawn(p.area())`, `closure` for a closure called on the spot). A failure of the body itself names the scope's: `scope s` gives `s`.

### Header words
3. **`scope [<E>] name word… { body }`**, the words in any order, each at most once (E0477 for a repeat or an unknown word): `independent`, `independent(f)` and `limit n` here; `within`, `until` later. The words are contextual, as `scope` is.

### `independent`
4. **A task's failure doesn't fail the scope**: the other tasks and the body run on, and the failure goes to the scope's handler as it happens, in the failing task, after it has unwound. `independent` alone prints it to standard error (`task `handle` failed: …`); `independent(f)` calls `f`, a `fun(Failure<E>)` that must be sendable (it runs in the scope's tasks; E0475 otherwise).
5. **The scope's value** is `Ok` with the body's, unless the body itself fails, which still cancels the tasks and fails the scope. `join` on a failed task now sees `Stopped.Failed` or `.Panicked`, since nothing cancels the joiner.

### `limit n`
6. **`s.spawn` waits while `n` of the scope's tasks are running** (a pause point: cancellable). `n` is an `i64` evaluated once as the scope starts; below 1 panics.

### `scope<E>`
7. **`scope<E> s { … }` has type `Result<T, Failure<E>>`**, and `s` has type `Scope<E>`. Every task started in it that returns a `Result` must have an error that is `E` or converts to it, as `?` converts (E0478 at the spawn otherwise); its `Failure.kind` is `.Error` of that converted value. A task returning no `Result` can only panic. `E` must be sendable (it crosses to the scope).
8. **`Scope` alone means `Scope<Error>`**: the plain `scope s`, and the root scope.

### Runtime and codegen
9. **A scoped task's entry catches its own unwind**, as a scope's body does: a landing pad around the call takes the panic over from the runtime, so the failure is built in Lugha (its `Failure` value, its name) and handed to the scope: failing an all-or-nothing scope, or calling an independent scope's handler. A cancellation ends the task quietly. The fiber's end no longer handles a scoped task's panic.
10. **The root scope's failures print in full** through the same path: `task `work` failed: <the error>`, then exit 101 as before.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The split:** this PRP is the policies; PRP-231 is time and shutdown (`within`, `until`, `remaining`, `shield`, Ctrl-C), building on these header words.
2. **The handler runs in the failing task** (rule 4), so failures are reported as they happen even while the body is busy (a server's accept loop). That makes `f` cross to tasks, hence sendable.
3. **A body failure ends even an independent scope**: the body is the scope's own code, not one of its tasks.
4. **Default task names are the called function's simple name**; `closure` for a closure literal; the scope's name for its body.
5. **`Failure` alone is `Failure<Error>`, `Scope` alone `Scope<Error>`**, special-cased like `Task`; general defaults for type parameters aren't needed for this.
6. **Error codes:** E0477 (a header word repeated or unknown), E0478 (a task's error that doesn't convert to the scope's `E`).
7. **Catching in the task entry** replaces the end-of-stack path for scoped tasks: one way failures are handled, in generated code, where the `Failure` value can be built.

## REQUIREMENTS

### Must Do
- `Failure`/`FailureKind` as above, `Display` and `Error`; task names (`name`, defaults); the root's full report.
- Header words with `independent`, `independent(f)`, `limit n`; `scope<E>` with `Scope<E>` and the conversion check.
- Rewrite every match on the old `Failure` (tests, the spec's §10 scopes program, `std.http`'s `serve_on`).
- Spec §3–§6, §9, "Changes since v16" (Breaking: `Failure` is a struct); §11 row 198; CHANGELOG; error pages.

### Must NOT Do
- Deadlines, `shield`, Ctrl-C, `.Deadline`, `.Shutdown`: PRP-231. `background`, `memory n`, `stack n`: later phases.
- Library combinators (first success, …): the Draft leaves them to libraries.

## TESTS
- [x] Checker unit tests: header words and E0477; `scope<E>` types and E0478; `Failure`/`Scope` defaults; a handler's type and sendability.
- [x] `tests/programs/concurrency/scope_policies/`: an independent scope surviving failures (printed, and to a handler); `join` seeing `.Failed` and `.Panicked`; `limit` keeping at most n running; `scope<E>` with a converting error; task names (default, `name()`, body); a root task's `Err` printed in full.
- [x] Rewritten matches print what they printed.
- [x] Targeted: `--lib`, `programs`, `leaks`, `spec`, `http`, `explain`, `comments`, `codegen`.

## ACCEPTANCE
- [x] An independent scope reports every failure and keeps running; `limit` holds; `scope<E>` types its failures.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [x] Spec, CHANGELOG, error pages updated.
