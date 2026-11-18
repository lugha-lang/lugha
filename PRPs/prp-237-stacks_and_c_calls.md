## FEATURE: Stacks and C calls: task stacks become large lazy reservations carved from pooled regions with no guard pages, a check at each function's entry raises a `stack overflow` panic that unwinds, C code runs on its own large stack, and `main` is a task on a pooled stack; milestone 205.

**Status:** proposed 2026-11-18.

## OBJECTIVE
```
fun depth(n: i64): i64 = if n == 0 { 0 } else { 1 + depth(n - 1) };

fun main(console: Console) {
    defer console.println("unwound");
    let r = scope s {
        for i in 0..100_000 {
            s.spawn(depth(10));                  // a hundred thousand tasks: no ceiling
        }
    };
    console.println(r.is_ok());
    depth(1_000_000_000);                         // panic: stack overflow, and `main` unwinds
}
```
prints `true`, then `panic: stack overflow at …` on stderr and `unwound`, and exits 101. Today the hundred thousand tasks hit the kernel's mapping limit (about 30,000), and an overflow ends the program from a signal handler without unwinding.

## CONTEXT
- **Draft v18 §4.7** rules 1 (C never runs on a task's stack: every `extern` call switches to the worker's 8 MiB C stack) [STK-01], 2 (a task's stack is an 8 MiB reservation carved from pooled regions, touched pages only, returned before reuse, never moving) [STK-02], 3 (no guard pages: an entry check raises a `stack overflow` panic that unwinds, with room kept for the unwinding) [STK-03], 4 (smaller reservations on a small address space) [STK-04], 6 (`main` is a task on a pooled stack) [STK-06]. Rule 5 (`scope s stack n`, a project default) is PRP 8 of this phase; §4.8's callback rules are PRP 6.
- **Draft §7.2** targets: fib, fannkuch, mandelbrot, nbody within 3% [PRF-01]. The earlier simulation of the entry check (`LUGHA_CALL_COSTS`, with the countdown, through `llvm.frameaddress`) cost fib 40%.
- **Code:** `runtime/tasks.c` (the fiber, the stack switch's assembly; at 489 lines), `runtime/spawn.c`, `runtime/stack.c` (the `SIGSEGV` handler), `runtime/callbacks.c`, `src/codegen/call_costs.rs` (the simulated check), `src/codegen/c_calls.rs` and `externs.rs` (extern calls), `src/codegen/function.rs` (`c_main`), `src/codegen/test_main.rs`.
- **Security:** nothing on CLAUDE.md's list: no `unsafe`, nothing about starting programs (`runtime/process.c` is untouched), nothing about `git`, `[link]` or lockdown.
- **Decisions:** none open.

## DESIGN

### Task stacks
1. **A task's stack is an 8 MiB reservation** (`MAP_NORESERVE`), carved from regions of 64 stacks (512 MiB of address space each, one mapping), so a program's mappings stay few however many tasks it runs. Only the pages a task touches use memory; when a task ends, its stack's pages are returned (`madvise(MADV_DONTNEED)`) and the stack goes back to the pool. Stacks never move.
2. **No guard pages:** every Lugha function's entry compares the stack pointer, read directly (`llvm.read_register` of `rsp` or `sp`, not the frame address), with the running task's limit, a runtime global the scheduler sets at each switch: the stack's bottom plus a reserve of 256 KiB. Below it, `lugha_rt_stack_low` panics with `stack overflow` at the function's location. While that panic unwinds, the limit drops to the bottom plus 64 KiB, so the frames' cleanups have the reserve to run in; a second overflow there ends the program, as a panic while unwinding does.
3. **A small address space** (an `mmap` of a region failing) halves the stack size, down to 1 MiB, before giving up with `panic: out of memory`.
4. **`main` is a task:** the C `main` lughac emits starts `main` (or a test) on a pooled stack through the runtime (`lugha_rt_main`) and waits for it; the process's own stack becomes the C stack.

### C calls
5. **Every `extern` call runs on the C stack:** codegen emits, per extern called, a small thunk (`lugha_c_<name>`) that takes the arguments from a frame on the task's stack, calls the extern and stores its result; the call site runs the thunk through `lugha_rt_c_call(thunk, frame)`, which switches to the C stack (8 MiB, the process's own) and back, in assembly beside the task switch. A call made while already on the C stack (from a callback) calls the thunk directly.
6. **A callback runs on the C stack, as it is called:** Lugha code there gets the C stack's limit. It may not wait (a channel, `join`, `sleep`, a socket): waiting would let another task run on the same stack, so the runtime panics with ``panic: a C callback can't wait: start a task instead`` (the Draft's rule, ahead of PRP 6's queued callbacks).
7. **The runtime's own functions** (`lugha_rt_*`, OpenSSL through `std.tls`) still run on the task's stack, which is large and lazy now: they are the runtime, not the program's C.

### What stays
8. **The `SIGSEGV` handler** stays for the C stack's overflow (the kernel's guard below it), reporting `panic: stack overflow` without unwinding, as today.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Regions of 64 stacks** (512 MiB of addresses): the mapping count stays near tasks ÷ 64; on a 47-bit address space that is room for millions of tasks.
2. **A 256 KiB reserve, 64 KiB of it kept for unwinding:** the runtime's functions and the unwinder run below the check.
3. **Reading the stack pointer, not the frame address,** keeps functions without a frame pointer; the check is a load, a compare and a branch, as Go's is.
4. **The entry check is measured** (fib, fannkuch, mandelbrot, nbody, against `main`, interleaved as for PRP-236) and recorded; a miss of the 3% target is recorded with the optimisations it calls for (leaf functions with small frames skipping the check, as Go does), the user's way with PRP-236's miss.
5. **A callback that would wait panics now**, rather than corrupting the C stack: the safe rule before PRP 6 gives callbacks their full shape.
6. **The C harness (`tests/tasks_runtime.rs`) keeps `main` on the process's stack** (the runtime still accepts a `main` fiber without a pooled stack), so it needs no `lugha_rt_main`.

## REQUIREMENTS

### Must Do
- Pooled lazy task stacks; the entry check and `stack overflow` panics that unwind; `main` on a pooled stack; extern calls on the C stack; callbacks there, unable to wait.
- `runtime/tasks.c` stays under 500 lines: stacks move to a new `runtime/stacks.c`.
- The measurements, recorded in the PRP and `bench/RESULTS.md`.
- Spec §6 (green threads, stacks), §7 (memory), §8 (C calls, callbacks), §9 (the runtime's start, the C `main`), "Changes since v16", §11 row 205; CHANGELOG; CLAUDE.md's known issue about stack overflows not unwinding.

### Must NOT Do
- `scope s stack n`, a project default (PRP 8); queued or `inline` callbacks, `Ring<T>` (PRP 6); several workers (PRP 5).

## TESTS
- [ ] `tests/programs/control/unwinding/`: an overflow in `main` and in a task unwinding with their `defer`s run (exit 101, and a scoped task's overflow failing its scope); a deep but legal recursion (a million frames) succeeding.
- [ ] `tests/programs/concurrency/`: a hundred thousand tasks in one scope.
- [ ] `tests/programs/c_interop/` (or the extern tests): an extern doing deep recursion in C on the C stack; a callback calling back into Lugha, which calls an extern again; a callback that would wait panicking with the message.
- [ ] `tests/tasks_runtime.rs` unchanged in behaviour; the `stack_overflow` known-leak entry updated if the program now unwinds.
- [ ] Targeted: `--lib`, `programs`, `leaks`, `spec`, `tasks_runtime`, `test_modes`, the extern and callback tests, `arm` (ARM64 builds).

## ACCEPTANCE
- [ ] Overflows unwind; task count is bounded by memory, not mappings; C runs on its own stack.
- [ ] The benchmarks measured and recorded.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [ ] Spec, CHANGELOG and CLAUDE.md updated.
