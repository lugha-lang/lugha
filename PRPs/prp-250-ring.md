## FEATURE: `Ring<T>`: a fixed-size queue a callback fills without ever waiting, locking or allocating, even on a thread C made, read by tasks as an ordinary `Receiver<T>`; milestone 218, the first of the last two §4.8 PRPs (then `realtime fun(…)`).

**Status:** proposed 2026-11-20.

## OBJECTIVE
An audio or sensor callback hands data to tasks without stalling C's thread:

```
extern fun audio_start(cb: direct fun(f32[], data), u: data);

fun main(console: Console) {
    let (samples, incoming) = Ring<f32>.new(4_096);
    let r = scope s {
        audio_start(fun(buf: f32[]) {
            for x of buf {
                match samples.try_send(x) {
                    .Sent => {},
                    .Full(_) | .Closed(_) => {},     // dropped: C's thread never waits
                }
            }
        });
        s.spawn(fun() {
            for x of incoming { meter(x); }          // an ordinary Receiver<f32>
        }());
    };
}
```

## CONTEXT
- **Spec:** the Draft, §4.8 rule 4 (CBK-04: a `direct` callback on C's thread talks to tasks through `Ring<T>`); spec §4 (channels: `Chan<T>`, `Sender`, `Receiver`, `TrySend`, `select`), §6, §9.
- **Research (discovery):**
  - Real-time code passes data out through lock-free FIFOs with one writer and one reader (Bencina, "time waits for nothing"; Rust's `rtrb`, JUCE's `AbstractFifo`, the JACK and PortAudio ring buffers), the only kind that is wait-free.
  - When full, a write is refused (drop newest) or overwrites the oldest; good APIs report the loss.
  - JUCE's reading side polls on a timer because waking another thread from the audio thread can block.
- **The user's calls:**
  - One writer, as in the research.
  - Two PRPs: this, then `realtime fun(…)`, a `direct` callback checked at compile time for allocating and waiting (ledger CBK-04, revised 2026-11-20).
  - Behaviour visible in the syntax.
- **Code:**
  - Runtime: `runtime/chans.c` (`LughaChan`, the receive and `select` paths), `workers.c`/`parking.c`/`net.c` (the poller, what counts as outstanding).
  - Checker: `check/channels.rs` (`Chan<T>.new`), `Type::Chan(End, T)`, `check/sendable.rs`.
  - Codegen: `codegen/channels.rs`.
  - Prelude: `TrySend`.
- **Decisions:** none open.
- **Security:** no `unsafe`; no dependency; one eventfd per program, already the poller's kind.

## DESIGN
1. **`Ring<T>.new(n)`** (`n` at least 1, a panic otherwise) gives `(RingSender<T>, Receiver<T>)`:
   - a fixed ring of `n` slots, made once;
   - the reading end an ordinary `Receiver<T>`: `recv`, `try_recv`, `for x of rx`, `select`, `merge` work unchanged.
   - `T` is written, or comes from the expected type, as for channels.
2. **`RingSender<T>`** has three methods, and no `send`, since it never waits:
   - `try_send(v): TrySend<T>`: `.Sent`, `.Full(v)` or `.Closed(v)`, the value given back, as a channel's `try_send` does;
   - `close()`;
   - `is(other)`.
   It is a type of its own (`Type::Chan(End::Ring, T)`), printed `RingSender<T>`, sendable when `T` is. `send` on it is E0415 with help pointing at `try_send`.
3. **One writer at a time:**
   - `try_send` takes the writing end for its duration with one compare-and-swap, and lets it go with one store. A second `try_send` starting meanwhile, from another thread, panics with `a ring has one writer at a time`.
   - Successive writers on different threads (a `direct` callback run by different workers) are fine.
   - The write itself is a store into a slot and a counter bump: wait-free.
4. **Nothing on the writing side locks, allocates or waits.** The value is copied into its slot (it was marked shared, as anything crossing to a task is).
   - **Waking the reader:** if a reader waits, the writer pushes the ring onto a lock-free list of rings to wake and writes to the poller's eventfd (one non-blocking system call, only when a reader waits).
   - **The poller** takes each ring's lock and hands values to its waiting receivers, as a channel's sender would.
   - While a reader waits on a ring, the poll counts as outstanding, so a parked worker polls, and the deadlock check sees it.
5. **Closing:** after `close()`, `try_send` gives `.Closed(v)`, and readers get what is left, then `None`, as for a channel.
6. **The `try_send` result is ordinary Lugha** (`TrySend<T>`): counting drops is the program's own, visible in the code.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The reading end is a plain `Receiver<T>`,** not a new type: tasks already know how to receive, `select` and `merge`. Only the writing end, which behaves differently, has a type of its own.
2. **`try_send` returns `TrySend<T>`,** the channel's result type. A full ring is visible in the code without a new concept; drop-newest is the only policy (overwrite-oldest would make the reader race the writer for the same slot; a later PRP can add it as a separate method if needed).
3. **"One writer at a time", checked by the runtime,** instead of "one thread": a `direct` callback run on the workers moves between threads from one call to the next, which is safe; overlapping writes aren't, and panic visibly instead of corrupting the ring.
4. **The poller's eventfd for waking** rather than the scheduler's locks, so the writing side stays lock-free on any thread, C's included.
5. **Readers may be several tasks:** reading takes the ring's lock, as a channel's does; only writing is lock-free.

## REQUIREMENTS

### Must Do
- Design items 1–6; the prelude (`RingSender`'s methods, its doc comments for the language server); E0415's help for `send` on a `RingSender`.
- Spec §4 (channels: the ring rows), §6, §9 (the runtime functions), "Changes since v16", §11 row 218; the Draft §4.8; CHANGELOG; TODO (the `Ring<T>` item narrows to `realtime`); the language server's completion knows `RingSender`'s methods.

### Must NOT Do
- `realtime fun(…)` and its compile-time check: the next PRP.
- An overwriting send, several writers at once.

## TESTS
- [ ] `tests/programs/concurrency/rings/` (one worker, exact output):
  - [ ] making a ring, `try_send` until full (`.Full(v)` gives `v` back), receiving in order, `close` then `.Closed(v)` and `None` after what is left, `for x of rx`;
  - [ ] a ring's receiver in `select` beside a channel's;
  - [ ] errors: `send` on a `RingSender` (E0415 with help), `recv` on it (E0415), `Ring<T>.new` without `T` (E0412), a non-sendable `T` (E0475);
  - [ ] `Ring<T>.new(0)` panics.
- [ ] `tests/rings.rs`, a C library with pthreads, on one and four workers:
  - [ ] a `direct` callback on C's thread sending 1,000,000 values into a ring of 1,024 while a task drains it: received plus dropped equals sent, the received in order;
  - [ ] a task waiting on an empty ring, woken by C's thread after a pause (no false deadlock);
  - [ ] two C threads writing one ring at once: a panic, `a ring has one writer at a time`, reported by the scope.
- [ ] `tests/scheduler_stress.rs`, plain and under ThreadSanitizer: the runtime's ring written from a pthread and read by tasks on four workers; every value once, in order, nothing dropped with room.
- [ ] Benchmarks: the seven within 3% of milestone 217; a ring's `try_send` from a callback measured against an unbounded channel's `send`.
- [ ] Targeted: `--lib`, `programs`, `rings`, `foreign_threads`, `callbacks`, `scope_callbacks`, `scheduler_stress`, `workers`, `leaks`, `rc_check`, `explain`, `spec`, `codegen`, `comments`, `lsp_completion`.

## ACCEPTANCE
- [ ] A callback on C's thread fills a ring without waiting, locking or allocating; tasks read it as any receiver.
- [ ] Overlapping writers panic; nothing is lost or duplicated otherwise (ThreadSanitizer clean).
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Spec, the Draft, CHANGELOG, TODO and the language server updated.
