## FEATURE: Channels with two ends: `Chan<T>.new(n)` giving a `Sender<T>` and a `Receiver<T>`, a required capacity, `unbounded()`, only senders closing, receivers as iterators, `try_send`/`try_recv` as enums, `s.merge(receivers)`, and deadlock reports naming where channels were made; milestone 200.

**Status:** proposed 2026-11-18.

## OBJECTIVE
```
fun produce(jobs: Sender<string>, paths: string[]) {
    for p of paths { jobs.send(p); }
    jobs.close();                                // only a Sender closes
}

fun main(console: Console) {
    let (jobs, queue) = Chan<string>.new(4);     // the capacity is required
    let r = scope s {
        s.spawn(produce(jobs, ["a", "b", "c"]));
        for p of queue {                         // until closed and empty
            console.println(p);
        }
    };
    match jobs.try_send("late") {
        .Sent => console.println("sent"),
        .Full(v) => console.println("full: {v}"),
        .Closed(v) => console.println("closed: {v}"),   // the value comes back
    }
}
```
prints `a`, `b`, `c`, `closed: late`.

## CONTEXT
- **Draft v18 §4.4** rules 1–9 and 11 [CHN-01…CHN-09, CHN-11]; Part 6's §4 notes on channels. Rules 10 (broadcast, `Watch<T>`: library types) and 12 (`Ring<T>`, for callbacks on foreign threads, Draft §4.8) are later phases.
- **Spec today:** §4's channel table (`Chan.new()` unbuffered, `Chan.buffered(n)`, `try_recv` as an `Option`, any holder closing), §5's `select`, §6's green threads and deadlock, §9's runtime table.
- **Code:** `Type::Chan` (`src/check/types.rs`), `src/check/channels.rs`, `src/check/builtins.rs` (the methods), `src/check/select.rs`, `src/check/sendable.rs`, `src/codegen/channels.rs`, `runtime/chans.c`, `runtime/tasks.c` (`deadlock`).
- **Builds on:** PRP-229's sendable rule (a channel is sendable when what it carries is) and PRP-227's scopes (`merge` runs in a scope).
- **Decisions:** none open.

## DESIGN

### The two ends
1. **`Chan<T>.new(n)`** returns `(Sender<T>, Receiver<T>)`, two handles on one channel holding up to `n` values; `0` hands each value over directly (today's `Chan.new()`). `n` is an `i64`; a negative `n` panics. **`Chan<T>.unbounded()`** has no limit. `T` comes from the type argument, or from the expected type (`let (tx, rx): (Sender<i64>, Receiver<i64>) = Chan.new(1);`), else E0412 as today.
2. **`Sender<T>`:** `send(v)` (waits while full; on a closed channel it panics), `try_send(v): TrySend<T>`, `close()` (closing twice panics, as today), `is(other)`.
3. **`Receiver<T>`:** `recv(): Option<T>` (`None` only once closed and empty), `try_recv(): TryRecv<T>`, `is(other)`, and it implements `Iter<T>` (`next` is `recv`), so `for x of rx` and the iterator helpers work. `close` on a receiver is E0415, with help: only a `Sender` closes.
4. **`TryRecv<T>`** is `.Ready(T) | .Empty | .Closed`; **`TrySend<T>`** is `.Sent | .Full(T) | .Closed(T)`, the value given back. Both are prelude enums.
5. **`Chan` is no longer a type**, only the maker: `Chan<T>` written as a type is E0305 (unknown type) with help naming `Sender<T>` and `Receiver<T>`; `Chan.buffered(n)` is E0415 and `Chan.new()` without a capacity E0405, each with help showing `Chan<T>.new(n)`.
6. **`select`** keeps its arms on the ends: `m = rx.recv()`, `tx.send(v)`, `after(ms)`, `default`. The pick among ready arms stays fair and seeded.
7. **Sendable:** both ends are sendable when `T` is; `Chan<T>.new` needs `T: Sendable` (E0475, as today for `Chan<T>`).

### `s.merge(receivers)`
8. **`s.merge(rs: Receiver<T>[]): Receiver<T>`** on a `Scope`: one forwarding task per receiver, started in `s`, sends what it receives on a new channel of capacity 0; when every input is closed and drained, the merged channel closes. Written in the prelude in Lugha (a `done` channel counts the forwarders down). The forwarders end with the scope like any task.

### Diagnostics at run time
9. **A channel records where it was made** (file, line, column). **A deadlock report** lists, after `panic: deadlock: every green thread is waiting`, each task waiting on a channel with that place: ``  `worker` waits to receive on the channel made at main.la:3:20`` (`to send`, `in a select`; other waits — `join`, a full scope's `limit` — are named as such). The first line stays as today, so existing tests matching it still pass.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **One runtime object, two handle types:** the checker's `Type::Chan` becomes `Type::Sender` and `Type::Receiver`; codegen and the runtime keep one `LughaChan`, and both ends point at it.
2. **A negative capacity panics at run time** (`limit n` does the same), not a checker error: the value is often computed.
3. **`unbounded()` grows its ring by doubling**; nothing else about a buffered channel changes.
4. **`TrySend` isn't must-use** (E0467 stays for `Result`): `let _ =` would be the only thing it forced, and dropping `.Full(v)` is a visible choice in a `match`.
5. **`merge` makes a capacity-0 channel**, so backpressure reaches every input; a buffer is the caller's to add.
6. **Old forms are errors with help, not silent aliases:** `Chan<T>` as a type, `Chan.new()`, `Chan.buffered(n)`, `close` on a receiver. Programs migrate: every `.la` in `tests/programs`, the programs inside `tests/*.rs`, `bench/programs/chanping`, the spec's §10 programs, and the prelude and std.
7. **No new error codes:** each misuse fits E0415 (no such method), E0305 (unknown type) or E0405 (wrong number of arguments), with help.

## REQUIREMENTS

### Must Do
- `Chan<T>.new(n)`, `Chan<T>.unbounded()`, `Sender<T>`, `Receiver<T>`, `TrySend<T>`, `TryRecv<T>`; `for x of rx`; `select` on the ends; `s.merge`.
- Deadlock reports naming the channels' places.
- Migrate every program and test using channels; spec §4–§6, §9, §10, "Changes since v16", §11 row 200; CHANGELOG (Breaking).

### Must NOT Do
- `Ring<T>`, broadcast channels, `Watch<T>`: later phases (Draft §4.8, library types).
- Closing automatically when the last sender goes (the Draft rejects it).
- Actors' bounded mailboxes: the next PRP.

## TESTS
- [ ] Checker unit tests: the ends' types and methods; `T` from the type argument and from the expected type; `close` on a receiver, `Chan<T>` as a type, `Chan.buffered`, `Chan.new()` without a capacity (each its error and help); sendable.
- [ ] `tests/programs/concurrency/channels/`: capacity 0 and n; `unbounded`; `for x of rx` ending at close; `try_send` (`.Sent`, `.Full(v)`, `.Closed(v)`) and `try_recv` (`.Ready`, `.Empty`, `.Closed`); `select` on ends; `merge` of three producers; a negative capacity's panic; a deadlock report naming a channel.
- [ ] Every migrated program keeps its output.
- [ ] Targeted: `--lib`, `programs`, `leaks`, `spec`, `explain`, `comments`, `tasks_runtime`, `snippets`, `net`, `udp`, `tls_over_conn`, `fmt_corpus`.

## ACCEPTANCE
- [ ] The objective program prints as shown; every channel program migrated and passing.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [ ] Spec, CHANGELOG and error pages updated.
