## FEATURE: A cancel that lands just before a task parks is no longer missed: a parking task looks at its cancellation once its wait is published, so the canceller and the waiter can't both miss each other.

**Status:** approved 2026-11-21.

## OBJECTIVE
A task waiting on a channel, an actor's room, a `join` or a scope's `limit n` checks for cancellation (`lugha_rt_cancel_point`), takes the object's lock, queues itself and parks (`block`). A cancel from another worker in between stores `cancelled`, looks at the task, sees it not yet waiting, and wakes nothing; the task then parks with nothing left to wake it: it waits until something else does, or forever (a deadlock report, or a scope that never ends).

Afterwards the waiter's cancellable waits always end with `CANCELLED` when a cancel was stored before or during their parking.

## CONTEXT
- **The user's request** (2026-11-21): "okay let's go", for this item first (TODO.md: "A cancel just before a wait parks may be missed", since milestone 212, found by reading the code).
- **Spec:** §4/§6 cancellation: a waiting task that may be cut short is woken by a cancel at once.
- **Code:**
  - `runtime/scopes.c`, `cancel_fiber`: stores `cancelled` (release), then `if (is_waiting(f) && f->cancellable && !shielded(f)) wake(f, CANCELLED)`.
  - `runtime/waiting.c`, `block`: stores `PARKING` (release) and switches away; never looks at `cancelled` again.
  - Release and acquire alone don't order a store before a later load of another word, so each side can read the other's old value: the classic store-buffering miss.
- **Security:** none of CLAUDE.md's list. Runtime concurrency: a mistake here hangs a task or wakes one twice.
- **Decisions:** none open.

## DESIGN
1. **The waiter re-checks.** `block`, once it has stored `PARKING`, puts a sequentially consistent fence and reads its `cancelled`. If set, the wait is cancellable and not shielded, it claims its own wait for `CANCELLED` (the same compare-and-swap a waker uses, so a waker racing it wins or loses cleanly) and doesn't switch away: `switch_away` already returns at once for a fiber no longer waiting, letting go of the locks it parked holding. The caller sees `CANCELLED` as from any cancel.
2. **The canceller fences too.** `cancel_fiber` puts a sequentially consistent fence between storing `cancelled` and reading whether the task waits. With a fence on each side, at least one of them sees the other: the canceller sees `PARKING` and wakes, or the waiter sees `cancelled` and doesn't park.
3. **One thread, no fence:** before a second worker starts (`rt_locking` false) nothing races, and the fences are skipped, as `fired`'s atomics already are.
4. **Waits that aren't cancellable don't change** (an actor call's answer, a scope waiting for its tasks): the re-check asks `cancellable`, as the canceller does.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Fences, not a lock:** the canceller can't take the object's lock (it doesn't know which object), and a scopes-lock handshake on every park would cost every wait; two fences cost almost nothing beside a context switch.
2. **One fix in `block`,** not at each wait site: every cancellable wait parks through `block`, so channels, `select`, actors, `join`, `limit n`, sleeps and helper jobs are covered at once.
3. **The test drives the window by hand:** the race is a few instructions wide, so the harness cancels a task itself between its cancel point and its parking, which reproduces the miss every time.

## REQUIREMENTS
### Must Do
- Design 1–4.
- **Elsewhere:** TODO.md loses the item; CHANGELOG (Fixed).
### Must NOT Do
- Change what a cancellation does once it is seen, or which waits are cancellable.

## TESTS
- [ ] **`tests/scheduler_stress.rs`**, a new harness case: a task, past its cancel point, is cancelled (by `cancel_fiber` on itself, standing for another worker), then queues on a channel and parks as `recv` does; `block` returns `CANCELLED` at once. Before the fix it deadlocks (exit 101). Also under ThreadSanitizer, as the file's other cases run.
- [ ] **Targeted:** `scheduler_stress`, `workers`, `programs` (the concurrency cases), `rc_check`, `leaks`.

## ACCEPTANCE
- [ ] The new case passes, plain and under ThreadSanitizer.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
