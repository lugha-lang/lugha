## FEATURE: Unwinding: a panic runs the pending `defer`s and releases what every frame of its task holds, through unwind tables; a panic in an actor ends the actor after unwinding, one anywhere else ends the program after unwinding its own task; milestone 194.

**Status:** proposed 2026-11-17.

## OBJECTIVE
```
fun save(dir: Dir) {
    let f = dir.create("out.txt").unwrap();
    defer { f.close(); println("closed"); };
    let xs: i64[] = [];
    println(xs[3]);                       // panics
}
```
```
panic: index out of bounds: the length is 0 but the index is 3 at main.la:5:15
closed
$ echo $?
101
```
The message comes first, then each pending `defer` runs, innermost first, and every counted value the task's frames held is released. `LUGHA_RC_REPORT=1` shows `0 live` for a program that panics, as for one that doesn't (the known exceptions aside). The supervisor program's actor leaks nothing.

## CONTEXT
- Draft v18 §4.3 rules 7–8 [CAN-07, CAN-08], §7.1 phase 4 ("Unwinding: panics run defers and fail their scope"). Replaces §6's rule that a panic doesn't run pending defers (`docs/decisions/defer.md`'s no-unwinding design) and the actor rule that a panicking actor's green thread is abandoned.
- Decision 223: reference counting is the memory model; its cleanup paths are what unwinding runs.
- **Today** (`runtime/actors.c` `panic_end`, `runtime/lugha_rt.c` `panic_with`): a panic prints its message and exits 101, or, in an actor, marks the actor panicked, wakes its callers and joiners, and abandons its green thread with everything its frames held.
- **Decided with the user:**
  - **Unwind tables**, not a flag checked after calls: calls that have something to clean up become `invoke`s whose landing pads run it, driven by the system unwinder (libgcc's), so the normal path costs nothing.
  - **A panic in a `spawn`ed task ends the program** once that task's stack has unwound: until scopes exist (phase 5) every task belongs to the root scope. Other tasks aren't unwound; cancelling them is phase 5's.
- Security review needed: `src/link/` (the link line gains the unwinder library), `runtime/` changes to how a green thread's stack starts (the boot frame gets unwind information marking it the outermost frame).

## DESIGN

### What a panic does
1. It prints `panic: <message> at <place>` as today, flushing stdout first.
2. Its task's stack unwinds, frame by frame, to the task's start. Each Lugha frame runs its cleanup for the call that was in progress: the pending `defer`s of every block it is in, innermost first, last first, exactly as a `return` there would, then it releases every counted value the frame holds (temporaries, locals, owned parameters), as `rc_exit_function` does.
3. Then, as today but after unwinding:
   - in `main`'s task, the program releases `main`'s arguments and the constants (as after a normal return) and exits 101;
   - in a `spawn`ed task, the program exits 101 (decided);
   - in an actor, the actor is marked panicked, its callers and joiners are told, and its green thread ends (`panic_end` as today), with nothing left held.
- **A panic during unwinding** (inside a `defer`, or a release reaching a panicking visit) prints its message and ends the program at once with 101, without unwinding further: the state is unknown.
- **Not unwound:** stack overflow (the guard page's signal handler can't unwind the faulting frame; it ends the program as today), `exit(n)` (it ends the program on purpose; the Draft's rule is about panics), and C frames.

### C frames (rule 7)
- A panic never unwinds through C a `[link]` library compiled: a Lugha callback C called is the boundary. A panic in a callback unwinds the callback's Lugha frames, then ends the program (101), as a callback panic does today.
- The runtime's own C frames (a `lugha_rt_*` function that panics, an actor handler's caller, a task's start) are compiled with unwind tables and hold nothing to clean up, so unwinding passes through them to the task's start. A runtime function that panicked midway may leave runtime-internal memory behind; those panics (closed channels, deadlock) end the program or actor anyway.

### Mechanism
- **Landing pads:** a call is an `invoke` only where its frame has something to clean up at that point (a counted slot, an owned parameter or a pending `defer`); every other call stays a plain `call`, so a function like `fib` is unchanged. Each such call's pad runs that point's cleanup and `resume`s. Pads with the same cleanup (same pending `defer`s, same statement) are shared.
- **Personality:** `__gcc_personality_v0`, libgcc's personality for cleanup-only frames (what C's `-fexceptions` cleanups use). Lugha never catches: every landing pad is a cleanup.
- **Raising:** `lugha_rt_panic` (and every runtime panic) starts a forced unwind (`_Unwind_ForcedUnwind`) whose stop function ends it at the task's start: `main`'s C entry or a green thread's entry, each of which set a resume point first. The unwinder finds every frame's cleanup through the tables; frames without any are passed.
- **Tables:** every Lugha function gets unwind tables (`uwtable`), and none is `nounwind`. The runtime is compiled with `-fexceptions`/unwind tables. A green thread's first frame is marked outermost in its unwind information, so the unwinder stops there instead of walking into whatever lies past the stack.
- **The passes that rewrite calls after lowering** (struct results through a pointer, split parameters, big structs, frame slot sharing) learn `invoke`: they rewrite one keeping its two destinations, and treat its unwind edge as a successor.
- **Linking:** programs link libgcc's unwinder (`-lgcc_s`, which every glibc system has, as Rust programs do). ARM64 builds use `aarch64-linux-gnu-gcc`'s.

### Measuring
- `bench/run.py`: every benchmark before and after (`-O2`), and binary sizes: PRF-01/02 still hold. `fib` has no landing pads at all; `binarytrees` and `wordfreq` have some.
- `-O0` frames: `popover_depth.la` and `deep_match.la` must still pass (pads add code, not slots: their slots are the ones the frame already has).

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The message prints before unwinding,** so `defer` output follows it and a defer that hangs can't hide the panic.
2. **A panic while unwinding ends the program at once,** with both messages printed.
3. **Stack overflow, `exit` and C frames don't unwind.** The first can't safely, the second is deliberate, and the third is rule 7.
4. **libgcc's personality and unwinder,** not ones of our own: cleanup-only frames are exactly what `__gcc_personality_v0` serves, and libgcc is on every system lughac supports.
5. **`invoke` only where there is cleanup,** so frames that hold nothing pay nothing, not even in size.
6. **Leak test:** programs that panic join it (their report must say `0 live`), with a known list for any that still hold runtime-internal memory.

## REQUIREMENTS

### Must Do
- Landing pads with `defer`s and releases; `invoke` where there is cleanup; the call-rewriting passes and slot sharing aware of `invoke`.
- The runtime: forced unwinding from every panic, stop points at `main` and every green thread's start, unwind tables, an outermost boot frame; actors' `panic_end` after unwinding; double panics.
- Linking libgcc's unwinder (x86-64 and ARM64).
- `supervisor.la` off the leak test's known list; panicking programs in the leak test.
- Spec §6 (`defer`, panics), §7 (actors, known issues), "Changes since v16", §11 row 194; CLAUDE.md's known issues; CHANGELOG; `docs/decisions/defer.md` marked replaced.

### Must NOT Do
- Scopes, cancellation, `shield`, or anything making one task's panic unwind another task (phase 5).
- Catching a panic in Lugha code (no `catch`, `recover` or `try`).
- Unwinding through C a library compiled.

## TESTS
- [ ] `tests/programs/control/unwinding/` (new): a panic runs pending `defer`s innermost first, in nested blocks and calls; a `defer` not yet reached doesn't run; a loop's `defer` runs once for the iteration that panicked; a panic inside a `defer` while unwinding ends at once with both messages; `exit` still skips `defer`s; a panic in a spawned task runs that task's `defer`s and ends the program; a panicking actor's `defer`s run and its callers see the panic as today.
- [ ] `tests/leaks.rs`: panicking programs report `0 live`; `supervisor.la` off the list.
- [ ] `tests/callbacks.rs`: a panic in a callback unwinds its Lugha frames (its `defer` runs) and ends the program.
- [ ] Codegen unit tests: a call with nothing to clean up stays `call`; one with a pending `defer` or a counted slot is an `invoke` with a cleanup pad; functions are `uwtable` with `__gcc_personality_v0`.
- [ ] The rewrite passes: unit tests that an `invoke` returning a struct, or taking split parameters, is rewritten with both destinations.
- [ ] `tests/arm.rs` (cross build and run under qemu), `tests/tasks_runtime.rs`, `tests/programs.rs` (every panicking program's stderr and exit code unchanged, defers aside), `--lib`, `codegen`, `spec`.

## ACCEPTANCE
- [ ] Panics unwind as above; nothing else changes for programs that don't panic.
- [ ] Benchmarks within PRF-01/02 of milestone 193's numbers; recorded with binary sizes.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Every `pub` item documented; spec, CLAUDE.md, CHANGELOG updated.
