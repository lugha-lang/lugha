## FEATURE: Callbacks on threads C made: a `direct` callback runs on C's thread (on a Lugha stack the thread gets on its first callback), a `spawned` one becomes a task on the workers; neither can make C's thread wait; milestone 217, the first of two PRPs for C's own threads (then `Ring<T>` and the allocation lint).

**Status:** proposed 2026-11-20.

## OBJECTIVE
A C library calling back from a thread of its own works:

```
import std.task;

extern fun sensor_start(cb: direct fun(f64, data), user: data);      // C calls cb on its own thread
extern fun mqtt_on_message(cb: spawned fun(string, data), user: data);

fun main(console: Console) {
    let (tx, rx) = Chan<f64>.unbounded();
    let r = scope s {
        sensor_start(fun(x: f64) { tx.send(x * 2.0); });   // on C's thread, now; never waits
        mqtt_on_message(fun(m: string) {                     // a task on the workers
            task.sleep(10);
            console.println("got {m}");
        });
        for v of rx { if v > 100.0 { break; } }
    };
}
```

Waiting on C's thread is a panic, reported by the registering scope:

```
task `callback` panicked: a direct callback can't wait on a thread C made: … declare it `spawned fun(…)`
```

## CONTEXT
- **Spec:** the Draft, §4.8 rule 4 (CBK-04: `direct` runs on C's thread, sendable data only; `spawned` is delivered to the workers); spec §7 ("A callback must be called on a thread the program runs Lugha on"), §8 (callbacks), §9.
- **Research (discovery, with sources):**
  - Every runtime with a scheduler attaches a record to C's thread on its first call and keeps it. Go binds an extra M per C thread through a pthread key, released by its destructor when the thread exits (it guesses the C stack's size); JNI has `AttachCurrentThread`/`DetachCurrentThread`; Python has `PyGILState_Ensure`. Erlang and Node instead only accept messages from such threads.
  - The real-time rules (Bencina, "time waits for nothing"): never allocate, lock or wait on such a thread.
- **The user's calls:** two PRPs (this, then `Ring<T>` and the lint); behaviour must be visible in the syntax, which `direct`/`spawned` already are (milestone 216).
- **Findings in the code:**
  - Every thread has a worker record in thread-local storage, empty on C's threads; `lugha_rt_cb_enter` would take such a thread for `main`'s.
  - Small blocks are carved from chunks owned by the allocating thread, and a block freed elsewhere is pushed back to its owner. A C thread's record dies with the thread, so it must never own chunks.
  - The stack check assumes every stack Lugha code runs on is aligned to its size; C's stacks aren't.
- **Code:**
  - Runtime: `runtime/callbacks.c` (entries, the record), `stacks.c` (`lugha_rt_on_stack`, the pool), `rc_alloc.c` (allocation), `runq.c` (where a fiber made ready from C's thread goes), `waiting.c` (waiting), `parking.c` (the deadlock check).
  - Codegen: `codegen/callbacks.rs` (`c_body`), `codegen/c_stack.rs` (the thunk rewrite reused for the entry's stack switch).
- **Decisions:** none open.
- **Security:** no `unsafe`; no dependency; the C thread's record is runtime-internal.

## DESIGN
1. **Recognising C's threads:** the main thread, the workers and the helpers mark themselves when they start. A callback entry on an unmarked thread is on a thread C made.
2. **The thread's record, on its first callback (Go's approach):**
   - The thread's worker record is marked foreign and gets a fiber of its own (no task) and a Lugha stack from the pool.
   - A pthread key's destructor gives both back when the thread exits.
   - The thread is never a worker: nothing runs there but its callbacks.
3. **The stack switch:**
   - Each callback entry becomes a C-ABI shell that calls its body through `lugha_rt_cb_run(body, frame)`, built by the same rewrite that sends C calls to the C stack (`c_stack.rs`), in reverse.
   - On the program's own threads, `lugha_rt_cb_run` calls the body in place, as today. On C's thread it switches to that thread's Lugha stack and back.
4. **A `direct` callback on C's thread:**
   - It runs there and can't wait: anything that would park (a full channel's `send`, `recv`, `join`, `task.sleep`, an actor call) panics with `a direct callback can't wait on a thread C made`. The panic is reported as milestone 215 reports a direct callback's panic: C gets a zero value, and a task in the registering scope reports it by the scope's policy.
   - Operations that don't wait run as usual: an unbounded channel's `send`, arithmetic, reading captures.
   - `lugha_rt_pin_here` does nothing there, since C's thread is the handle's.
   - What it captures is shared with C's thread, so it must be `Sendable` (E0475, as for `spawned`): a `direct` callback's captures are now checked too.
5. **A `spawned` callback from C's thread:**
   - It starts its task in the registering scope, through the shared queue, waking an idle worker.
   - It may wait, as on any worker.
   - One taking a pinned handle can't (the handle's thread is C's): it panics, reported by the scope.
6. **Allocation on C's thread** uses the system allocator (`calloc`/`free`, the path large objects already take), so no block outlives its thread's record. Freeing such blocks elsewhere is already allowed.
7. **The deadlock check:** while any thread C made holds a record (it has called back and not exited), a program whose tasks all wait isn't declared deadlocked: C may still deliver.
8. **The spec:** §7's "must be called on a thread the program runs Lugha on" is replaced by these rules.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The record lives until the thread exits,** not attached and detached per call: Go switched from per-call to per-thread because per-call cost syscalls; audio callbacks run every few milliseconds.
2. **A real Lugha stack per C thread,** from the pool, instead of guessing C's stack bounds as Go does: the stack check stays exact.
3. **`direct` callbacks' captures must be `Sendable`, always.** Whether a call will come from C's thread isn't known when compiling, and the rule must be visible: a `direct` callback may run on another thread, so it shares what it captures. This is a breaking change: a `direct` callback capturing a `Ref` is E0475.
4. **System allocation on C's thread** rather than chunks handed over at thread exit: simpler, and the lint (next PRP) tells real-time code to avoid allocating there anyway.
5. **Waiting panics rather than blocking C's thread:** blocking an audio thread is the failure the rules forbid; a panic is visible and reported.
6. **No deadlock while C's threads live:** a false "deadlock" would end a program waiting for sensor data; a real deadlock in such a program waits instead, which `LUGHA_RC_REPORT`-style tooling can't tell apart anyway.

## REQUIREMENTS

### Must Do
- Design items 1–8.
- **Breaking:** `direct` callbacks' captures checked for `Sendable` (design decision 3); CHANGELOG under Breaking.
- Spec §7, §8, §9 (`lugha_rt_cb_run`), "Changes since v16", §11 row 217; the Draft §4.8; CHANGELOG; TODO (the "callbacks on C's own threads" item narrows to `Ring<T>` and the lint).

### Must NOT Do
- `Ring<T>`, the allocation lint: the next PRP.
- Make C's threads workers, or run tasks on them.

## TESTS
- [ ] `tests/foreign_threads.rs` (a C file starting pthreads, `[link]`), on one and four workers:
  - [ ] A `direct` callback called 10,000 times from a C thread sends each value on an unbounded channel; `main` receives them all, in order.
  - [ ] A `spawned` callback from C's thread runs as a task that sleeps and sends; every call arrives.
  - [ ] A `direct` callback that would wait (a full bounded channel) panics; C gets 0 and goes on; the scope fails with the message.
  - [ ] A panic in a `direct` callback on C's thread: C gets 0; the scope reports it.
  - [ ] Recursion 10,000 deep inside a callback on C's thread works (the Lugha stack), and an overflow there is a `stack overflow` panic, not a crash.
  - [ ] 200 short-lived C threads, each calling once, then exiting: every call arrives, the program ends, `LUGHA_RC_REPORT` shows nothing left.
  - [ ] Strings built on C's thread and dropped on a worker (system allocation): nothing left, no crash.
  - [ ] `main` waiting on a channel only C's thread feeds, after a 100 ms pause in C: no false deadlock.
- [ ] `tests/programs/c/callbacks/`: E0475 for a `direct` callback capturing a `Ref`.
- [ ] `tests/callbacks.rs`, `scope_callbacks.rs`, `pinned.rs`, `blocking_externs.rs`, `workers.rs`, `programs`, `leaks`, `rc_check`, unchanged except for captures newly checked.
- [ ] Benchmarks: the seven within 3% of milestone 216 (the allocator's one more check on its fast path); a callback-heavy measure (C calling a `direct` callback 1,000,000 times on a worker) within 5% of milestone 216, for the entry's extra call.
- [ ] Targeted: `--lib`, `programs`, `callbacks`, `scope_callbacks`, `foreign_threads`, `pinned`, `c_strings`, `c_memory`, `c_arrays`, `extern_structs`, `arm`, `blocking_externs`, `leaks`, `rc_check`, `rc_alloc_threads`, `workers`, `explain`, `spec`, `codegen`, `comments`.

## ACCEPTANCE
- [ ] C libraries calling back from their own threads work, with `direct` and `spawned` callbacks, on one and four workers, without leaks or false deadlocks.
- [ ] Nothing a `direct` callback does on C's thread can make that thread wait.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass; benchmarks within their bounds.
- [ ] Spec, the Draft, CHANGELOG (Breaking), TODO updated.
