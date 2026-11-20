## FEATURE: The Lugha skill's third batch: concepts 11–14 (tasks and scopes, channels and actors, modules and packages, C interop), with `SKILL.md`'s index extended and the test taught multi-file examples; milestone 232.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
$ lughac skill --list
…
concepts/11-tasks-and-scopes       Running work at the same time: scope, spawn, join, cancellation, deadlines and failures.
concepts/12-channels-and-actors    Tasks talking: channels, select, and actors that own their state.
concepts/13-modules-and-packages   Files as modules, import and visibility, lugha.toml, dependencies and tests.
concepts/14-c-interop              Calling C: extern functions and types, linking libraries, callbacks and the ffi grant.
```

## CONTEXT
- **Follows** PRP-262 and PRP-263 (decision 225): the same page shape and tests.
- **Sources:** the compiler, the prelude and std, and `tests/programs/` for working multi-file and C cases. Claims outside an example are checked with scratch programs before they are written down.
- **Decisions:** none open.

## DESIGN
1. **Four concept pages** in the enforced shape.
   1. **11 tasks and scopes:**
      - `scope s { … }` and its `Result`;
      - `s.spawn(call)`, `Task<T>` and `join()`;
      - all-or-nothing versus `independent`;
      - `cancel`, `within` deadlines and `shield`;
      - `Failure`, `Sendable`, `task.sleep`/`yield_now`;
      - `main`'s root `Scope`.
   2. **12 channels and actors:**
      - `Chan<T>.new(n)` and `unbounded()`, with `Sender`/`Receiver`;
      - `send`, `recv` and `for … of rx`;
      - closing by dropping senders;
      - `select`;
      - `actor` declarations, spawning one into a scope, calls and `stop()`;
      - when to use which (an actor for shared state, a channel for a stream).
   3. **13 modules and packages:**
      - a file is a module, and `import geometry;`;
      - `pub`/`private` and named imports;
      - `lugha.toml` (name, dependencies from git, `allow = ["ffi"]`, `[link]`), `lughac init` and `lughac update`;
      - `@test` functions and `lughac test`;
      - `std` modules (`import std.json;`).
   4. **14 C interop:**
      - `extern fun` with C types;
      - strings and `Option<string>` from C;
      - `extern type` handles and `extern struct`;
      - `[link]` in `lugha.toml`;
      - `blocking` externs;
      - callbacks (`direct`, `spawned`, `realtime`) with `data`;
      - views of C's memory;
      - the `ffi` grant for packages;
      - what C may not do (keep Lugha pointers).
2. **`SKILL.md`:** four more rows.
3. **`tests/skill.rs` learns multi-file examples:** a block marked ```` ```lugha file NAME.la ```` is written as `NAME.la` beside the next `lugha` block's `main.la`, and is not checked alone. This is needed for modules. `toml` blocks are shown, not checked, as `sh` and `text` are.
4. **C examples use libc only** (`puts`, `strlen`, `sqrt`, `abs`), which every program links. They need no C file, so the test runs them.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Multi-file examples are real files the test builds,** not prose: the one way to keep the modules page honest.
2. **Git dependencies are shown but not run:** the test must not need the network. The `lugha.toml` blocks are reference text, with `lughac update`'s behaviour described.
3. **Callback, handle and library examples are type-checked, not run:** libc has no callback a Lugha callback can take as written (`qsort` passes pointers), and a library needs its C side.
4. **This completes the concepts.** `library/`, `tasks/` and `examples/` follow as their own PRPs.

## REQUIREMENTS
### Must Do
- Design 1–4.
- Spec: §11 row 232, a "Changes since v17" bullet, and the skill paragraph's block markers (`lugha file`, `toml`).
- CHANGELOG.
### Must NOT Do
- Change the compiler or the command.
- Mention the spec, milestones or versions in the pages.

## TESTS
- [x] `tests/skill.rs`:
  - [x] every new example builds and prints what its page says, including the multi-file modules example and the libc examples;
  - [x] a `lugha file` block with no `lugha` block after it fails the test.
- [x] Targeted: `skill`, `comments`.

## ACCEPTANCE
- [x] `lughac skill --list` shows concepts 1–14 in order.
- [x] Every example in the new pages is checked: run, type-checked, or written beside one that is.
- [x] `cargo fmt --check` and clippy clean; the targeted tests pass.
