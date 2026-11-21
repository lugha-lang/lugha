## FEATURE: Channel and handle cycles are freed by counting: an actor lets go of its state when it ends, and a channel nobody can receive from lets go of what it holds, so a ring of actors and a channel carrying its own sender no longer leak; milestone 243.

**Status:** approved 2026-11-21.

## OBJECTIVE
```
struct Msg { reply: Sender<Msg> }

actor Node {
    next: Option<Node>,
    fun link(mut self, n: Node) { self.next = Some(n); }
}

fun main(console: Console) {
    for i in 0..100 {
        let r = scope s {
            let (tx, rx) = Chan<Msg>.unbounded();
            tx.send(Msg { reply: tx });            // the channel holds its own sender
            let a = s.spawn(Node { next: None });
            let b = s.spawn(Node { next: None });
            a.link(b);
            b.link(a);                              // a ring of actors
        };
    }
    console.println("done");
}
```
```
$ LUGHA_RC_REPORT=1 lughac run cycles.la
done
rc: … 0 live
```
Today both leak: 100 channels, and every actor of every ring.

## CONTEXT
- **The user's choices** (2026-11-21):
  - the next item after `Weak` (the order collector, then `Weak`, then channel and handle cycles);
  - once no `Receiver` is left, `send` drops the value and `try_send` gives `.Closed(v)`.
- **Ledger and Draft:**
  - MEM-06 (cycles form only through `Ref`, channels and handles);
  - MEM-07 ("a scope's end breaks the handle cycles it created"), which this PRP meets differently and the ledger records: an actor ends when its scope stops it, and letting go of its state then breaks the cycle;
  - Draft §3.1 rules 6–7.
- **Spec:**
  - §4: channels (`send`, `try_send`, the two ends), actors;
  - §6: deadlocks;
  - §7: what isn't freed, and the `Not freed` line;
  - §8: the receiving end's layout.
- **Code:**
  - `runtime/actors.c`: `actor_ended_locked` marks the state's contents shared, and `rc_actor_visit` releases the state only when the last handle goes;
  - `runtime/chans.c`, `select.c`, `rings.c`: `LughaChan`, send, recv, try, close, select arms;
  - `src/codegen/channels.rs`: `Chan.new` hands the same pointer out as both ends, and `chan_method`/`chan_receive`;
  - `select.rs` and `iterators.rs` (`for v of rx`).
- **Facts found in discovery:**
  - A stopped actor never runs again: a call on it panics with "has stopped". Its state is still kept until the last handle goes, which is what keeps a ring alive.
  - A `Task` can't be stored, sent or returned, so task handles can't form a cycle.
  - A `Sender` and a `Receiver` are the same object, so a buffered value holding `tx` holds the whole channel.
  - Anything sent is marked shared, so the per-task collector stops at it.
- **Security:** none of CLAUDE.md's list. It is runtime memory and concurrency code: a mistake frees live memory or races. No `unsafe` Rust, and no new dependency.
- **Decisions:** none open.

## DESIGN
1. **An actor lets go of its state when it ends.**
   - However it ends (`stop`, its scope stopping or cancelling it, a panic), the actor takes its state out of its handle and releases it on its own task, after giving up its lock.
   - Nothing can run on the state again: calls panic with "has stopped", as before.
   - The handle's visit releases a state only if one is still there (an actor that never ended, at the report's exit).
   - Marking the state's contents shared at the end is no longer needed, since it is released by its own task.
   - A ring of actors, each holding the next one's handle, then falls apart when its scope stops them.
2. **The receiving end is its own object.**
   - `Chan<T>.new(n)`, `.unbounded()` and `Ring<T>.new(n)` give a `Receiver<T>` that points to a small counted object, the *receiving end* `{ chan }`. The `Sender` is still the channel itself.
   - Copies of a `Receiver` share the one end, so counting stays ordinary and `rx.is(other)` still compares ends of one channel.
   - Generated code loads the channel from the end where a receive needs it: `recv`, `try_recv`, `for v of rx`, a `select` receive arm.
   - Marking a `Receiver` shared marks its channel through the end's visit.
3. **The last receiver gone.**
   - When the receiving end is freed, its channel becomes *unread*. Under the channel's lock it takes out what it buffers and wakes every waiting sender; after giving up the lock it releases what it took out, then lets go of the channel.
   - So a buffered `Msg { reply: tx }` releases `tx`, and the channel goes.
   - Later, on an unread channel:
     - `send(v)` releases `v` and returns at once;
     - a sender waiting for room wakes and does the same;
     - `try_send(v)` gives `.Closed(v)`;
     - a `select` send arm is ready, and its value is dropped;
     - a ring's send drops its value.
   - None of this can be seen except through `try_send`, since nothing can receive.
4. **What isn't freed.** A channel that holds its own receiver (`tx.send(Msg { inbox: rx })`) keeps the end alive, and so itself. It is the one known issue left: rarer, and it would need a cross-task collector. CLAUDE.md's known issue and spec §7 say so.
5. **Deadlocks.** A sender blocked on a full channel whose receivers are all gone used to wait forever, a deadlock if nothing else ran. Now it goes on. Spec §6 says so.

## DECISIONS MADE IN DISCOVERY (the user chose `send`'s behaviour; these are mine, for review)
1. **Release the state at the actor's end,** rather than MEM-07's "the scope breaks the links".
   - The actor's end is the earliest safe moment.
   - It needs no list of what a scope made.
   - Scopes already stop their actors when they end, so it meets MEM-07's aim too.
2. **A receiving-end object, not a receiver count in the channel.**
   - A count would need its own retain and release wherever a `Receiver` is copied.
   - A separate end is counted like any object (the `Weak` cell's approach), so only the receive operations change.
3. **`try_send` gives `.Closed(v)`** once unread: it returns the value, and the variant already means "nobody will take it".
4. **Task handles are left alone:** they can't be stored, so they can't form a cycle.

## REQUIREMENTS
### Must Do
- Design 1–5.
- **Spec:**
  - §4 (channels: the unread channel and `send`/`try_send` on it; actors: the state let go of at the end);
  - §6 (deadlock note);
  - §7 (cycles through channels and handles, and the one left);
  - §8 (the receiving end);
  - §11 row 243;
  - a Changes bullet.
- **Elsewhere:**
  - the ledger records MEM-07's change;
  - CLAUDE.md's known issue becomes "a channel holding its own receiver is never freed";
  - the skill's channels page mentions sends after the consumer has gone;
  - CHANGELOG.
### Must NOT Do
- A cross-task cycle collector.
- Change what a `Sender` is, or any count operation.
- Change behaviour on a channel that still has a receiver.

## TESTS
- [ ] **`tests/programs/concurrency/actors/`** (`leaks.rs` runs every case poisoned, expecting `0 live`):
  - [ ] a ring of actors in a loop;
  - [ ] a call on a stopped actor still panics with "has stopped";
  - [ ] an actor whose state holds channels and other actors' handles, stopped mid-run.
- [ ] **`tests/programs/concurrency/channels/`:**
  - [ ] a channel holding its own sender, in a loop;
  - [ ] a consumer that breaks early while a producer keeps sending to a bounded channel (it finishes, no deadlock);
  - [ ] `try_send` after the receivers are gone gives `.Closed`;
  - [ ] a `select` send arm on an unread channel;
  - [ ] `Ring<T>` with its receiver gone;
  - [ ] receivers copied, sent to tasks and dropped in any order;
  - [ ] `rx.is(rx2)`.
- [ ] **`--lib`:** codegen gives `Chan.new` two different ends, and a receive loads the channel from the end.
- [ ] **Targeted:** `--lib`, `programs`, `leaks`, `rc_check`, `workers`, `rc_cycles`, `spec`, `skill`, `comments`; `bench --check` (`chanping` is channel-heavy, measured and stated).

## ACCEPTANCE
- [ ] The objective ends with `0 live`.
- [ ] A producer whose consumer has gone never hangs.
- [ ] Poisoned and owner-checked runs are clean.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
