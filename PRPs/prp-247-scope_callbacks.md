## FEATURE: Callbacks registered against a scope: a C callback with user data and no result runs as a task in the scope that registered it, unless marked `inline`; a panic in an inline callback fails that scope instead of ending the program; events after the scope's body ends are dropped and its captures let go of; milestone 215, the second of three PRPs for callbacks and thread-bound values.

**Status:** proposed 2026-11-20.

## OBJECTIVE
A callback may wait, and the program's own loop drives the C library:

```
import std.task;

extern type Window pinned;
extern fun window_new(title: string): Window;
extern fun window_set_text(w: Window, text: string);
extern fun window_poll(w: Window): bool;      // handles pending events; false once closed
extern fun on_click(w: Window, cb: fun(Window, data), user: data, free: release);
extern fun on_draw(w: Window, cb: fun(Window, data) inline, user: data, free: release);

fun fetch(n: i64): string {
    task.sleep(200);
    "page {n}"
}

fun main(console: Console) {
    let r = scope ui pinned {
        let win = window_new("demo");
        on_click(win, fun(w: Window) {
            window_set_text(w, fetch(1));     // a task in `ui`: it may wait
        });
        on_draw(win, fun(w: Window) {
            window_set_text(w, "drawn");      // inline: runs inside C's call
        });
        while window_poll(win) {
            task.yield_now();                 // queued callbacks run here
        }
    };
    r.unwrap();
    console.println("closed");
}
```

A panic in an inline callback reaches the scope, not C:

```
error: scope `ui` failed: panic: not yet
```

## CONTEXT
- **Spec:** the Draft, §4.8 rules 2, 3, 5, 6, 7 (CBK-02, 03, 05, 06, 07); spec §7 (closures handed to C), §8 (callbacks, `data`, `release`, arrays in callbacks), §4 (scopes, failure policies, `Sendable`), §6 (cancellation).
- **Research (in discovery, the user's call):** almost every C event library can be driven from a loop the program writes (SDL, GLFW, Win32, X11/Wayland, curl, libuv; on Linux GTK and Qt too). Only loop-owning frameworks (Cocoa, Qt's `exec()`, Android) need the runtime to run as a guest inside them, as gtk-rs, qasync and Swift's `MainActor` do. **The user's choice:** the program drives the loop now; guest mode is a later PRP, recorded in the Draft. No language runs tasks from inside an arbitrary callback, so neither does this.
- **Built on:** PRP-246 (milestone 214): pinned handles and scopes, so a queued callback in a pinned scope runs on the library's thread.
- **The last PRP of the three:** callbacks on C's own threads (`Ring<T>`, `queued`, the allocation lint).
- **Code:**
  - Parser: extern parameters.
  - Checker: `check/callbacks.rs`, `check/sendable.rs`.
  - Codegen: `codegen/callbacks.rs` (entries, trampolines, `lugha_rt_cb_pin`).
  - Runtime: `runtime/callbacks.c`, `unwind.c` (`lugha_rt_unwound_to_c`), `scopes.c` (the scope's end), `spawn.c`.
  - Tests: `tests/callbacks.rs`, `tests/c_strings.rs`, `tests/c_memory.rs`.
- **Decisions:** none open; ledger CBK-02, 03, 05, 06, 07 approved.
- **Security:** callbacks are on CLAUDE.md's list only through C running Lugha code, which this keeps; no `unsafe`, no dependency, no change to the `ffi` permission.

## DESIGN
1. **Which callbacks are queued:** a callback with `data` (C holds a registration: the user data lughac gives it) and no result. Marking its type `inline`, `cb: fun(Window, data) inline`, keeps today's behaviour. These are always inline:
   - a callback with a result (C waits for the value);
   - a callback without `data` (C holds only a code pointer, used during the call: `qsort`, `each`).
2. **The registering scope:** the innermost scope of the task making the extern call (the root scope for `main`'s body). The pinned record (`lugha_rt_cb_pin`) keeps it.
3. **A queued call:**
   - The trampoline copies C's arguments (numbers, handles, copied strings and arrays, structs by value) into a block. It starts a task in the registering scope that calls the closure with them, and returns to C at once.
   - The task is one of the scope's: counted, waited for at its end, its failure handled by the scope's policy, cancelled with it.
   - It starts regardless of `limit n`, since C can't wait for room.
   - A queued callback taking a pinned handle starts pinned to the worker C called from.
   - What it captures crosses to that task, so it must be `Sendable` (E0475, with help suggesting `inline`).
   - A queued callback can't take `Ref<T[]>` (new **E0481**): C's array is written back when an inline call returns, and a queued one returns before it runs.
4. **An inline callback:**
   - Runs as today, in the calling task on the C stack, and can't wait (CBK-02).
   - Cancellation never interrupts it: cancellation points inside it do nothing (CBK-06).
   - **A panic in it** stops at the C boundary: C gets the zero value of its result, and the registering scope fails with the panic as a task's would (CBK-05). If that scope's body has already ended, or for a callback without `data`, the task C called from fails instead.
5. **The scope's end:**
   - Once the scope's body finishes (or is left, or unwinds), its callbacks stop. A call C makes after that is dropped: a queued one isn't started, an inline one returns the zero value without running.
   - The closures' captures are let go of when the scope ends, or when C calls `release`, whichever comes first. The record itself is freed only by `release`, as now.
   - A callback lock (taken before a scope's lock) makes "stopped" and "start a task" exclusive.
6. **The Draft gains guest mode** as a later item: a wake fd and "run the ready tasks pinned here, then return", for loop-owning frameworks.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Only `data` callbacks are queued.** CBK-03 says a callback returning nothing is queued "in the scope it was registered with". A callback without `data` has no registration (C keeps a bare code pointer, and such APIs call it during the call), so it stays inline.
2. **Callbacks stop when the body ends,** not after the scope's tasks finish. A queued call started while the scope waits would otherwise outlive the wait.
3. **Queued callbacks ignore `limit n`:** blocking C until there's room would break CBK-02's promise that C never waits on Lugha.
4. **A panic whose scope is gone** fails the task C called from, rather than ending the program: the nearest scope still running.
5. **E0481** for `Ref<T[]>` in a queued callback, rather than silently not writing back.
6. **`inline` is accepted where it changes nothing** (a callback with a result or without `data`), so a declaration can say what it means without the compiler objecting.

## REQUIREMENTS

### Must Do
- Design items 1–6; E0481 with its `docs/errors/` page; the help on E0475 for a queued capture.
- **Breaking:** `data` callbacks without a result now run later. Existing tests and examples that rely on their running during the call get `inline`. CHANGELOG lists this under Breaking.
- Spec §4, §7, §8 (the callback rules, `inline`, panics, the scope's end, driving a C loop), §9 (new runtime entries), "Changes since v16", §11 row 215; the Draft (guest mode, later); CHANGELOG; TODO; the language server and grammar for `inline` (extension version bump).

### Must NOT Do
- Callbacks on threads C made, `queued`, `Ring<T>`, the allocation lint: the next PRP.
- Guest mode: a later PRP.
- Run tasks from inside a callback.

## TESTS
- [ ] `tests/callbacks.rs` (a C file per case, `[link]`):
  - [ ] A queued callback runs after the extern call returns, as a task in the registering scope, and may wait (`task.sleep`, a channel).
  - [ ] An `inline` one runs during the call; one with a result is inline without the mark; one without `data` is inline.
  - [ ] A queued callback's panic fails its scope by its policy (all-or-nothing, `independent`).
  - [ ] An inline callback's panic: C gets 0, and the scope fails with the message; the program doesn't end.
  - [ ] Cancellation during an inline callback waits until it returns.
  - [ ] After the scope's body ends, C's calls are dropped (counted on the C side), and the captures are let go of (`LUGHA_RC_REPORT`: nothing left) before C's `release`.
  - [ ] A queued callback taking a pinned handle runs on the thread C called from (four workers).
  - [ ] The existing cases, updated with `inline` where they mean "during the call".
- [ ] `tests/programs/c/callbacks/`: E0475 for a queued callback capturing a `Ref`, and none when it is `inline`; E0481; `inline` parsing and formatting.
- [ ] `tests/workers.rs`'s sweep, `tests/programs/`, `leaks`, `rc_check`, `c_strings`, `c_memory`, `extern_structs`, `arm`.
- [ ] Benchmarks: the seven within 3% of milestone 214 (nothing there calls back).
- [ ] Targeted: `--lib`, `programs`, `callbacks`, `c_strings`, `c_memory`, `c_arrays`, `extern_structs`, `arm`, `leaks`, `rc_check`, `workers`, `pinned`, `blocking_externs`, `explain`, `spec`, `codegen`, `comments`, `parser`, `fmt`, `lsp_semantic_tokens`.

## ACCEPTANCE
- [ ] A callback-driven program can wait inside its callbacks, with a loop it drives itself, on one or four workers.
- [ ] An inline callback's panic fails its scope; C never sees an unwind.
- [ ] No callback runs after its scope's body ends, and its captures are freed by then.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass.
- [ ] Spec, the Draft, CHANGELOG (Breaking), TODO and the editor tooling are updated.
