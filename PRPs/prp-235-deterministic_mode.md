## FEATURE: Deterministic mode and virtual time for tests: `lughac test` runs plain tests on virtual time that jumps to the next timer when every task waits, and `--shuffle=SEED` reorders the ready queue from a seed so a race shows up and repeats; milestone 203, the first of phase 6.

**Status:** approved 2026-11-18; built 2026-11-18 (milestone 203).

## OBJECTIVE
```
import std.task;

fun retry(tries: i64): i64 {
    for i in 0..tries {
        task.sleep(30_000);                      // thirty seconds, each time
    }
    tries
}

@test
fun retries_three_times() {
    assert_eq(retry(3), 3);                      // runs instantly: virtual time
}

@test
fun two_writers() {
    let (tx, rx) = Chan<string>.new(2);
    let r = scope s {
        s.spawn(tx.send("a"));
        s.spawn(tx.send("b"));
    };
    assert_eq(rx.recv(), Some("a"));             // holds in FIFO order, not under every shuffle
}
```
```
$ lughac test
test retries_three_times ... ok                  (in milliseconds, not 90 s)
test two_writers ... ok
$ lughac test --shuffle=7
test retries_three_times ... ok
test two_writers ... FAILED (seed 7)
$ lughac test --shuffle=7 --filter two_writers   (fails the same way, every time)
```

## CONTEXT
- **Draft v18 §4.6** rules 10 (timers on scheduling time, not `Clock`) [SCH-10], 11 (normal, deterministic and shuffle modes; `lughac test` deterministic by default; `--shuffle=SEED`) [SCH-11], 13 (determinism is a mode) [SCH-14]. Rule 12 (helper threads' results at fixed points) waits for helper threads (PRP 4 of this phase); the hash seed's part (§3.2 rule 5) waits for hash seeding (PRP 2).
- **Phase 6 order** (the user's choice): this safety net first, then hash seeding, stacks, helper threads, many cores, callbacks, memory, extras.
- **Code:** `runtime/tasks.c` (`now_ns`, sleepers, the ready queue, `schedule`), `runtime/net.c` (`idle_wait`), `runtime/chans.c` (`select`'s pick), `runtime/scope_time.c`, `src/driver/test.rs` (the runner, at 501 lines), `src/driver/cli.rs`.
- **Decisions:** none open.

## DESIGN

### The modes
1. **Normal** (as today): real monotonic time, the ready queue first in, first out. `lughac run`, `lughac build`'s programs and integration tests run this way.
2. **Deterministic:** plain `@test`s, by default. Time is virtual: it starts at 0 and moves only when every task waits and no I/O is ready (one poll that doesn't wait): then it jumps to the earliest timer (a sleep, an `after`, a deadline, a shield's end, a socket timeout), with no real waiting. Everything else is as in normal mode, which on one thread is already repeatable.
3. **Shuffle** (`lughac test --shuffle=SEED`, deterministic plus): each time the scheduler picks the next ready task, it picks one at random among the ready ones, from a generator seeded with `SEED`; `select`'s pick among ready arms uses the same generator. One seed gives one order, so a failure repeats.

### What sees virtual time
4. **Every timer the runtime keeps:** `task.sleep`, `select`'s `after`, scope deadlines (`within`, and `until` in the same clock), `s.remaining()`, shields, socket and UDP timeouts. A test's `Clock` is the fake one, stopped at 2000-01-01, as before: timers use scheduling time, not `Clock`.

### Running it
5. **The runner says the mode** through the environment of each test's process (`LUGHA_SCHED=deterministic` or `shuffle:SEED`), which the runtime reads at start; any other value, or none, is normal mode.
6. **`lughac test --shuffle=SEED`** (a `u64`; `--shuffle` alone draws one and prints `shuffle seed N` first) runs every plain test shuffled; a failing one's line says `FAILED (seed N)`. Integration tests never shuffle.
7. **`--timeout S`** stays wall-clock time: virtual time can't hang a test, but a busy loop still can.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Plain tests deterministic, integration tests normal:** an integration test talks to the real world (real servers, real programs), whose time is real.
2. **Virtual time jumps only when no I/O is ready:** a plain test's sockets are loopback, with both ends in the process, so when every task waits and nothing is ready, nothing will arrive; time can move.
3. **Virtual time starts at 0**, so `until t` in a test means `t` ms after the program started.
4. **The mode travels in `LUGHA_SCHED`**, like `LUGHA_TEST`: the runner sets it; a program run by hand gets normal mode unless someone sets it on purpose.
5. **`--shuffle` without a seed draws one from the clock** and prints it, so a lucky failure can be repeated.
6. **`lughac run` gets no mode flag yet:** the Draft asks for it in tests; a flag on `run` can follow when it is wanted.
7. **`src/driver/test.rs` splits** (it is at 501 lines): how a finished test is reported (`failure`, `json_result` and the line printing, about 120 lines) moves to `src/driver/test_report.rs`.

8. **Made during the build:** the shuffle's generator is seeded through splitmix64, so nearby seeds give unrelated orders (seeding it directly made seeds 1–6 all pick the same way); the scheduling modes are named `MODE_*` in the runtime, since `SCHED_*` belongs to the C library's `sched.h`.

## REQUIREMENTS

### Must Do
- The three modes in the runtime; virtual time for every timer; the seeded pick.
- `lughac test --shuffle[=SEED]`; plain tests deterministic; `(seed N)` on failures; the JSON output carrying the seed.
- Spec §6 (green threads, tests) and §9 (the runtime's time), "Changes since v16", §11 row 203; CHANGELOG; CLAUDE.md's `lughac test` line.

### Must NOT Do
- Several workers, helper threads, the hash seed: later PRPs of this phase.
- A mode for `lughac run`.

## TESTS
- [x] `tests/test_runner.rs` (or a new `tests/test_modes.rs`): a plain test sleeping minutes finishes in well under a second; deadlines and `after` fire in virtual time; an integration test still sleeps really; `--shuffle=SEED` fails a test that depends on FIFO order, the same way twice, naming the seed; `--shuffle` alone prints a seed; a test without races passes under several seeds.
- [x] The C harness (`tests/tasks_runtime.rs`): virtual time jumping to the next sleeper; a seeded pick repeating.
- [x] Targeted: `--lib`, `test_runner`, `test_groups`, `test_capabilities`, `tasks_runtime`, `programs`, `spec`, `comments`.

## ACCEPTANCE
- [x] The objective's tests behave as shown.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [x] Spec, CHANGELOG and CLAUDE.md updated.
