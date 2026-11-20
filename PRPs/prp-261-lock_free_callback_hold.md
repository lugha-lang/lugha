## FEATURE: A `direct` or `realtime` callback with `data` checks that its scope still runs and holds its captures without taking the callbacks lock, so a real-time call takes no lock at all; milestone 229.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
extern fun audio_start(cb: realtime fun(MutView<f32>, data), u: data);

scope {
    audio_start(fill);   // C's audio thread calls `fill` 48 000 times a second
    …
}                        // the scope ends while C may be mid-call: the captures are let go of
                         // by whichever of the two finishes last, and later calls are dropped
```
Nothing a program writes changes. Today each call with `data` takes the runtime's one callbacks lock (`lugha_rt_cb_hold`), so an audio thread can wait behind another thread registering a callback or ending a scope. After this PRP it never does.

## CONTEXT
- **Found by:** the PRP-258 hand-over (2026-11-20), the item left optional there; ledger CBK-04 and CBK-09 (a `realtime` call allocates nothing and takes no lock).
- **Spec:**
  - §8 "`realtime fun(…)`" (L1718);
  - "Threads C made" (L1719: "taking no lock" for the first call only);
  - §8 "The scope's end".
- **Code:**
  - `runtime/callbacks.c`:
    - `CbRecord`, `cb_lock`;
    - `lugha_rt_cb_hold` (takes the lock and checks `r->scope`, then retains `env`);
    - `callbacks_stop` and `lugha_rt_cb_release` (take `env` out under the lock and release it);
  - `src/codegen/callbacks.rs` (the direct entry: `hold_callback`, then the call, then `rc_release` of the held `env`).
- **Security:** none of CLAUDE.md's list. No `unsafe`, and no new dependency.
- **Decisions:** none open.

## DESIGN
1. **A word per record.** `CbRecord` gains one atomic `uses` word: the count of calls between their check and their retain, plus a `STOPPED` bit and a `DROPPED` bit.
2. **`lugha_rt_cb_hold`, without the lock:**
   1. It adds one to `uses`.
   2. If `STOPPED` is set, it takes the one back and returns false; C's call is dropped as now.
   3. Otherwise it retains `env` and hands it out, then takes the one back.
   4. The retained `env` keeps the captures alive for the rest of the call, as today.
3. **The scope's end** (`callbacks_stop`) and **C's `release`**, still under the lock for the scope's list:
   - They set `STOPPED`.
   - Whoever leaves `uses` at zero with `STOPPED` set lets go of the record's own hold on `env`. That is the stopper when no call is between check and retain, else the last such call. Exactly one of them wins a compare-and-swap that sets `DROPPED`.
4. **Unchanged:**
   - `spawned` callbacks (`lugha_rt_cb_queue`) and a panic's report (`lugha_rt_cb_caught`) still take the lock: they start a task in the scope, which waits anyway.
   - Codegen is unchanged.
5. **Spec:** "`realtime fun(…)`" says a call takes no lock, `data` or not. The only time it frees anything is when its scope ended during the call and it held the captures last, as already said ("dropping the last holder…").

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Every `direct` callback with `data` uses it,** not only `realtime` ones: one way to check and hold, and it's cheaper for all of them.
2. **Last one out, not waiting:** the scope's end never spins waiting for C's calls to finish. A wait would stall a task on C's timing, and a C library that ends a scope from inside its own callback would deadlock.
3. **C's `release` while C is still calling the same callback** stays C's error, as today: the record is freed then.

## REQUIREMENTS
### Must Do
- Design 1–5.
- Spec: "`realtime fun(…)`", §11 row 229, and a "Changes since v17" bullet.
- CHANGELOG.
### Must NOT Do
- Change what any callback does, when it is dropped, or codegen.
- Make `spawned` callbacks or `cb_pin` lock-free.

## TESTS
- [x] `tests/foreign_threads.rs`:
  - [x] Four C threads call a `direct` callback with `data` 100 000 times each, while the registering scope ends part-way.
  - [x] No crash; the calls after the end are dropped; the count of calls that ran is at most the total.
  - [x] With `LUGHA_RC_REPORT=1`, nothing is left live.
  - [x] Run 20 times.
- [x] `tests/foreign_threads.rs`: the same with a `realtime` callback.
- [x] Targeted: `scope_callbacks`, `foreign_threads`, `callbacks`, `rings`, `spec`, `comments`.

## ACCEPTANCE
- [x] `lugha_rt_cb_hold` takes no lock.
- [x] The captures are let go of exactly once, whichever side finishes last.
- [x] `cargo fmt --check` and clippy clean; the targeted tests pass.
