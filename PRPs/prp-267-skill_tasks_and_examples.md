## FEATURE: The Lugha skill's `tasks/` how-tos and `examples/` programs, which complete its layout: five task pages and five complete programs, all built and run by the tests; milestone 235.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
$ lughac skill --list
…
tasks/command-line-tool     A tool that reads arguments, files and stdin, and exits with a code.
tasks/tcp-server            A TCP server: a task per connection, shared state in an actor, clean shutdown.
tasks/http-service          A JSON API with std.http: routes, decoding bodies, errors as statuses.
tasks/wrap-a-c-library      Linking C code, declaring externs, and a safe Lugha API over them.
tasks/testing               @test functions, fakes for capabilities, and lughac test.
examples/word-count.la      Counts lines, words and characters of files or stdin.
…
$ lughac skill word-count     # prints the program
```

## CONTEXT
- **Follows** PRP-262 to PRP-266 (decision 225): the planned layout's last two folders.
- The concept pages teach one idea at a time; a task page shows how they combine to do one job, end to end; an example is a whole program to read or adapt.
- **Decisions:** none open.

## DESIGN
1. **`tasks/NAME.md`, five pages:**
   - command-line-tool;
   - tcp-server;
   - http-service;
   - wrap-a-c-library;
   - testing.

   Each page has:
   - frontmatter `title` and `summary`, and `uses` (the concept and library pages it builds on, by short name);
   - `## Goal`;
   - `## Steps`, numbered, each explaining one piece;
   - `## The whole program`;
   - `## Going further`.
2. **Steps show pieces of the whole program:** a ```` ```lugha part ```` block must appear, word for word, inside the page's whole program. So a step's code is real, checked code, and can't drift from the program it explains.
3. **Files beside the program,** generalising `lugha file NAME`: a block whose info is `LANG file NAME` (`c file glue.c`, `toml file lugha.toml`) is written as `NAME` beside the next example. This lets wrap-a-c-library link real C, and the test runs it.
4. **Tests run too:** ```` ```lugha test ```` builds the block and runs `lughac test` on it, which must pass. The testing page uses it.
5. **`examples/NAME.la`, five programs:**
   - word-count (files and stdin);
   - kv-server (TCP and an actor, client in-process);
   - notes-api (HTTP and JSON over loopback);
   - parallel-sum (scopes and channels);
   - config (TOML, `Dir` and `Env`).

   Each program:
   - starts with `//` lines saying what it does and how to run it;
   - ends with an `// Output:` comment block holding exactly what it prints.

   The test builds each, runs it in a fresh folder with no stdin and no arguments, and compares.
6. **Commands:**
   - An example is named like a page: `examples/word-count` or `word-count`.
   - `--list` shows its first comment line as its summary.
   - `--export` writes the `.la` files beside the pages.
7. **`SKILL.md`:** a "Tasks" table and an "Examples" table, saying when to read each.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **`lugha part` must be a substring of the whole program:** it checks the step code without making each step a program of its own.
2. **Expected output lives in the example file** (`// Output:`), not a `.stdout` file beside it: one file to read or upload per example, and the comment documents the output for the reader too.
3. **Examples run with no outside input:** servers include their client, and file and stdin tools make or read their own input, so the test needs no network, files or terminal.
4. **`uses` lists pages by short name** (`tasks-and-scopes`, `net`), checked to exist, so a model on a task page knows what to read next.

## REQUIREMENTS
### Must Do
- Design 1–7.
- Spec: §11 row 235, a "Changes since v17" bullet, the skill paragraph (tasks, examples, the new block markers).
- CHANGELOG.
### Must NOT Do
- Change the language or std.
- Need the network, a terminal or files outside the test folder.

## TESTS
- [x] `tests/skill.rs` (or a new `tests/skill_tasks.rs` if `skill.rs` would pass 500 lines):
  - [x] every task page's shape;
  - [x] each `uses` names a page;
  - [x] every `lugha part` appears in its page's whole program;
  - [x] `LANG file` blocks written beside the next example;
  - [x] `lugha test` blocks pass `lughac test`.
- [x] Every `examples/*.la` builds, runs and prints exactly its `// Output:` block.
- [x] `--lib skill`: examples found by name, and their summary taken from the first comment.
- [x] Targeted: `skill`, `skill_library`, the new test file, `comments`.

## ACCEPTANCE
- [x] `lughac skill --list` shows the overview, 14 concepts, 9 library pages, 5 tasks and 5 examples.
- [x] Every line of code in the skill is compiled by the tests.
- [x] `cargo fmt --check` and clippy clean; the targeted tests pass.
