# BUGS — lugha

Known bugs that are understood but not fixed: what happens, why, and the ways to fix each one that have been weighed. When one is fixed, its entry moves to CHANGELOG.

---

## A channel holding its own receiver is never freed

**Status:** open, analysed 2026-11-22. Spec §7 ("Not freed") and CLAUDE.md's known issues state it.

### What happens

```
struct Msg { inbox: Receiver<Msg> }

fun leak() {
    let (tx, rx) = Chan<Msg>.unbounded();
    tx.send(Msg { inbox: rx });    // the channel now holds its own receiver
}                                  // tx and rx go out of scope: nothing is freed
```

`LUGHA_RC_REPORT=1` counts the channel, its receiving end and the message as live at exit, every time `leak` runs. The same happens through several channels: channel A buffering a `Receiver` of B while B buffers a `Receiver` of A.

### Why

```
1. after Chan.new                    counts
   tx ──► [chan]                     chan: 2 (tx, end)
   rx ──► [end] ──► [chan]           end:  1 (rx)

2. after tx.send(Msg { inbox: rx })
   tx ──► [chan] buffer: [Msg ─┐]    chan: 2 (tx, end)
   rx ──► [end] ◄──────────────┘     end:  2 (rx, the buffered Msg)
           └──► [chan]

3. leak() returns: tx and rx let go
          [chan] buffer: [Msg ─┐]    chan: 1 (end)
          [end] ◄──────────────┘     end:  1 (the buffered Msg)
           └──► [chan]
```

The channel and its end hold each other, so neither count reaches zero. Since milestone 243 a channel lets go of what it buffers once its last receiver goes, which frees a channel holding its own *sender*; here the buffer holds the receiver, so the last receiver never goes.

The cycle collector (milestone 240) frees groups that hold only each other, but only `Ref` boxes inside one task: it stops at shared objects, and a channel and its end are shared.

### Why the collector can't just walk shared objects while tasks run

Shared objects never change in place (copy-on-write writes in place only with one holder, and a `Ref` never crosses tasks), and a channel's buffer can be read under its lock. But references to shared objects are still copied and dropped by running tasks, and trial deletion reads each count at a different moment:

```
M holds tx; M sits in channel C's buffer; task T also holds a copy of M.

collector                              task T (on another worker)
---------                              ------
reads C's count: 2 (held by M, end)
                                       let t = m.tx;   // copies tx: C's count 3
                                       drops m         // M is still buffered
reads M's count: 1 (held by the buffer)
subtracts holds from inside the group:
   C: 2 − 2 = 0, M: 1 − 1 = 0
→ frees C and M                         T still holds t: a use after free
```

So a collection over shared objects needs a moment when no Lugha code runs anywhere. (Bacon and Rajan's concurrent cycle collector solves this with extra validation passes; it is far more machinery than the problem warrants here.)

Only channels need it: every other shared value is built once and never changed in place, so it can only point at older objects and can't be part of a cycle. Actors let go of their state when they end (milestone 243), and a `Task` can't be stored, so their handles can't leak in a cycle either.

### Ways to fix it

1. **Collect when every worker is idle (recommended).** Channels whose element type can reach a `Receiver` are registered as cycle-prone when made (a compile-time fact, like a `Ref` cell's). When the scheduler finds every worker parked (where it already decides whether to report a deadlock), and at exit under `LUGHA_RC_REPORT`, it runs trial deletion over the registered channels, their receiving ends and what they buffer, the walk stopping at actor handles. Nothing else runs then, so the counts are consistent. Cost: none while the program runs; a program that never pauses keeps its leaked channels until it does. Most programs pause whenever every task waits on the network or a timer.
   - C threads: a `direct` callback runs Lugha code on a thread C made. The collection must not overlap one: callbacks in flight are counted, and a callback entering during a collection waits for it to end (short). `realtime` callbacks change no object counts (they hold their captures through their record's counter) and need not wait.
2. **Stop the world at safe points.** The collector sets a flag; every worker stops at its next preemption point (a function's entry or a loop's head); it collects; they go on. It can run at any time, but every worker and every C thread in a callback must reach a safe point, and all tasks pause for the collection.
3. **Leave it, documented.** The pattern is avoidable: send a `Sender` for replies, never the channel's own `Receiver`.

### Tests a fix needs

- The leak above in a loop, under `LUGHA_RC_POISON=1 LUGHA_RC_REPORT=1`: `0 live` (in `tests/programs/concurrency/channels/`, which `tests/leaks.rs` runs).
- Two channels each buffering the other's receiver.
- A channel whose buffered receiver is also held by a running task: not freed, and still usable.
- With option 1: a program that never goes idle keeps the cycle until exit, where the report's collection frees it.
- `tests/scheduler_stress.rs` with ThreadSanitizer: collections racing direct callbacks from a thread C made.
