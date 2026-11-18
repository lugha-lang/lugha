## FEATURE: An allocator for many threads: small blocks belong to the worker that carved them, a block freed on another thread goes back to its owner with one atomic push, and a C stress test frees blocks across real threads; milestone 209.

**Status:** proposed 2026-11-18.

## OBJECTIVE
Nothing changes for a program: same output, same speed within noise. What changes is that `lugha_rt_alloc` and freeing are safe from several threads at once, which the workers PRP needs before any task runs on a second thread:

```
$ cargo test --test rc_alloc_threads
test eight_threads_free_each_others_blocks ... ok      (also under ThreadSanitizer)
```

## CONTEXT
- **Research:** `docs/research/many-cores.md` §2 "The allocator", §3 item 2, §4's second PRP.
- **Draft v18 §3.1** rule 14 [MEM-14]: generated code allocates only through `lugha_rt_alloc`, backed by an allocator built for many threads. `memory n` (rule 11) is phase 6's last PRP.
- **Built on** PRP-240 (milestone 208): the `Worker` record (`runtime/lugha_rt.c`), the owner check.
- **Code:** `runtime/rc_alloc.c` (all of it), `runtime/rc.c` (`rc_made`/`rc_freed`, the `LUGHA_RC_REPORT=2` list), `runtime/lugha_rt.c` (`worker`), `src/link/mod.rs`.
- **Today:** 16-byte size classes up to 272 bytes, carved from 64 KiB chunks of one reserved region, a chunk's first word its class. Each class has one global free list. Larger blocks, and small ones once the region is full, come from `calloc`. Memory is never given back to the system.
- **Security:** no `unsafe`, no new dependency, nothing on CLAUDE.md's list.
- **Decisions:** none open.

## DESIGN
1. **A chunk has an owner.** Its header (still the first 16 bytes) holds its class and the `Worker` that carved it. Each worker keeps its own free list and carve pointer per class: today's globals, moved into the `Worker` record. Allocating stays exactly today's fast path: pop the list, else carve, else take a chunk.
2. **Freeing checks the owner.**
   - A block whose chunk belongs to the running worker goes on that worker's list, as today.
   - Any other block is pushed onto its owner's **remote list** for that class, one compare-and-swap.
   - The owner takes a whole remote list with one swap when its own list for the class runs dry, before carving.

   The owner is the chunk's, never the shared bit: a stolen task frees its own unshared blocks on another worker, and that is a remote free.
3. **Chunks come from the region with one atomic add.** The region stays one reservation for the whole process.
4. **`worker` becomes a thread-local pointer**, set for the main thread at start. Each worker thread, and the stress test's threads, sets its own (`lugha_rt_worker_enter`). The runtime's C reads it through local-exec TLS: one `fs`-relative load on x86-64, `tpidr_el0` on ARM64.

   This PRP doesn't move tasks between threads. The rule that a read of `worker` must not be cached across a task switch comes with the workers PRP, along with the hot values generated code reads.
5. **Counts for the report** (`rc_made`, `rc_freed`) move into the `Worker` record and are summed at exit. The `LUGHA_RC_REPORT=2` list of every object, a debugging aid, takes a mutex.
6. **Large blocks** (over 272 bytes, or once the region is full) stay with `calloc` and `free`, which are already safe across threads.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Remote lists per owner and class, not mimalloc's per-page lists.** Today's fast path and layout stay as they are, the change is about a hundred lines, and freeing is one compare-and-swap. Per-page lists exist mainly to give empty pages back to the system, which Lugha doesn't do today. That stays a later change.
2. **The owner is a worker, reached through a thread-local pointer.** A helper thread never makes Lugha objects. Callbacks from C's own threads (a later PRP) will need a heap for a thread that isn't a worker, and that PRP decides how.
3. **No vendored allocator.** Per the research and the project's rule, mimalloc only serves as a benchmark baseline (`LD_PRELOAD`), measured once and recorded.
4. **The stress test is C against the runtime,** with real `pthread`s and, as a second build, ThreadSanitizer (available with the system `cc`). GenMC isn't installed here, so model checking waits.

## REQUIREMENTS

### Must Do
- Design items 1–6; `lugha_rt_worker_enter` in `RUNTIME_SYMBOLS` only if generated code calls it (it won't: the C entry and the test harness do).
- Spec §7 (memory: blocks owned per worker, remote frees), §9 (the runtime's allocator), "Changes since v16", §11 row 209; CHANGELOG; CLAUDE.md's stack line; `bench/RESULTS.md`.

### Must NOT Do
- Start workers or move tasks between threads (the workers PRP).
- Give memory back to the system, or add `memory n` (later PRPs).
- Change size classes, the header, or generated code.

## TESTS
- [ ] `tests/rc_alloc_threads.rs` (new): a C harness built against `lugha_rt.c`, run both plainly and with `-fsanitize=thread`:
  - [ ] 8 threads, each a worker, allocate blocks of every class, hand them to the others through mutex-guarded queues, and free what they receive; every block keeps a written pattern until freed, and the report's made and freed counts agree at the end.
  - [ ] A thread whose own list is empty takes back the blocks others freed for it, before carving a new chunk (checked through the chunk count).
  - [ ] Blocks over 272 bytes cross threads through `calloc` and `free`.
- [ ] `tests/tasks_runtime.rs`, `tests/leaks.rs`, `tests/rc_check.rs` and `programs` pass unchanged (one thread: every free is local).
- [ ] `python3 bench/run.py`-style interleaved runs against milestone 208 (binarytrees above all), plus binarytrees under mimalloc via `LD_PRELOAD` once, recorded.
- [ ] Targeted: `--lib`, `programs`, `leaks`, `rc_check`, `tasks_runtime`, `waiting_runtime`, `helpers`, `codegen`, `spec`, `arm`.

## ACCEPTANCE
- [ ] The stress test passes, with and without ThreadSanitizer.
- [ ] Benchmarks within noise of milestone 208, or the difference recorded and accepted.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Spec, CHANGELOG, CLAUDE.md and `bench/RESULTS.md` are updated.
