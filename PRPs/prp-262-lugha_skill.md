## FEATURE: The Lugha skill: a `skill/` folder of readable pages (an Agent Skill an assistant can load, and the base for courses) built into `lughac`, read with `lughac skill`, checked by a test that compiles every example. `lughac spec` is removed. First batch: the command, the test, `SKILL.md` and concepts 1–5; milestone 230.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
$ lughac skill                      # SKILL.md: what Lugha is, how to work in it, the page index
$ lughac skill functions            # one page
$ lughac skill --list               # every page in syllabus order, with its summary
concepts/01-values-and-types   Numbers, bool, char and string; let and let mut.
concepts/02-functions          …
$ lughac skill --search "let mut"   # page:line: text
concepts/01-values-and-types:42: let mut n = 0;
$ lughac skill --export ~/skills    # writes ~/skills/lugha/, ready to zip and upload to Claude
$ lughac spec
error: unrecognized subcommand 'spec'   (exit 2)
```

## CONTEXT
- **Asked for by:** the user (2026-11-20): "retire the command and instead create a /skill directory with human readable instructions which can be uploaded to claude or still an llm can search over using `lughac skill`… this will also help when designing lugha courses… there is no reaching out to the spec."
- **Research:**
  - Agent Skills format (agentskills.io, Claude's skill best practices):
    - a folder named as its `name`, with `SKILL.md` and frontmatter `name` (≤ 64 chars, lowercase and hyphens) and `description` (≤ 1024 chars, what it is and when to use it);
    - three-layer loading;
    - `SKILL.md` ≤ 500 lines;
    - every other file linked from `SKILL.md` directly, one level deep;
    - files over 300 lines get a table of contents.
  - Exercism's concept syllabus: a concept taught once, with prerequisites and small exercises.
- **Code:**
  - `src/lib.rs` (`SPEC`);
  - `src/driver/cli.rs` and `mod.rs` (`Command::Spec`, `spec()`);
  - `tests/common/spec_programs.rs`, `src/check/interfaces.rs` and `src/driver/lsp/signature.rs` (tests reading `SPEC`);
  - `README.md`.
- **Decisions:**
  - Core decisions 3 and 18 say the spec "doubles as the LLM-ready bundle" and rule out "a condensed second copy". The user's request supersedes both on this point.
  - DECISIONS.md gains a resolved entry: the skill is the bundle for readers and models; the spec stays the project's rulebook and the source of §10's acceptance tests.
- **Security:** `--export` writes files. It refuses to overwrite an existing `lugha/` folder, and writes nothing outside it. No `unsafe`, and no new dependency.

## DESIGN
1. **The folder**, `skill/` in the repository, exported as `lugha/`:
   ```
   SKILL.md          frontmatter; what Lugha is; how to work (write → `lughac check
                     --diagnostics=json` → `lughac explain CODE` → `lughac fmt` → `lughac test`);
                     the rules that most often trip a newcomer; which page to read when
   concepts/NN-name.md   the syllabus, in teaching order (14 planned, below)
   library/name.md       the API, one file per area
   tasks/name.md         how-tos built from the concepts
   examples/name.la      complete programs
   ```
   Concepts, in order:
   1. values and types;
   2. functions;
   3. structs and enums;
   4. match and patterns;
   5. errors;
   6. collections;
   7. interfaces;
   8. generics;
   9. iterators;
   10. capabilities;
   11. tasks and scopes;
   12. channels and actors;
   13. modules and packages;
   14. C interop.
2. **Every concept page has one shape:**
   - Frontmatter: `title`, `prerequisites` (concept names), and `summary` (one line, shown by `--list`).
   - Sections: `## The idea`, `## Example`, `## Rules`, `## Mistakes` (each with the error code seen), `## Exercises`.
   - Library and task pages have the frontmatter and their own sections.
   - Pages never mention the spec, milestones or versions. They stand alone.
3. **`lughac skill`**, with the pages built into the binary so they match that compiler:
   - `lughac skill` prints `SKILL.md`.
   - `lughac skill <page>` prints a page.
     - It is named by its path without `.md` (`concepts/02-functions`) or by its last part without the number (`functions`).
     - An unknown name exits 2 with the closest names.
   - `--list` prints each page's path and summary, in folder order: concepts, library, tasks, examples.
   - `--search <text>` prints every line containing `text` (case-insensitive) as `page:line: text`. It exits 1 when nothing matches, as `grep` does.
   - `--export <dir>` writes `<dir>/lugha/` with every file. It is exit 2 when `<dir>/lugha` exists.
   - A broken pipe is not a failure.
4. **`lughac spec` is removed**, and so is `lugha::SPEC`:
   - The tests that read it include the spec file themselves.
   - The language server's tests no longer depend on it.
5. **The test (`tests/skill.rs`):**
   - Every ```` ```lugha ```` block in every page, and every `examples/*.la`, builds.
   - A block followed by ```` ```output ```` runs, and its stdout must match.
   - A block marked ```` ```lugha error E0xxx ```` must fail with that code.
   - Every page has valid frontmatter, every prerequisite names a concept that comes earlier, `SKILL.md` is ≤ 500 lines and links every page, and `name` matches the exported folder.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Arguments, not subcommands** (`--list`, not `list`), so a page can never clash with a command name.
2. **Error codes stay with `lughac explain`**, which `SKILL.md` points to. One home for each code's explanation.
3. **The spec stays in `docs/`**, unchanged: it is still the rulebook, and §10 is still tested. It is no longer shipped in the binary.
4. **Delivery in batches**, each a PRP:
   - this one: the command, the test, `SKILL.md`, and concepts 1–5;
   - the next ones: concepts 6–10, then 11–14, then `library/`, then `tasks/` and `examples/`.
5. **Search is plain line matching:** no index and no ranking. Small, predictable, and easy for a model to read.

## REQUIREMENTS
### Must Do
- Design 1–5; concepts 1–5 written in full.
- DECISIONS.md: the new entry, with 3 and 18 noted as superseded on this point.
- README's command table; CLAUDE.md's commands (`lughac skill`, not `spec`).
- Spec: §11 row 230, a "Changes since v17" bullet (**Breaking:** `lughac spec` is gone), and the CLI table.
- CHANGELOG.
### Must NOT Do
- Change the language.
- Copy the error pages into the skill.
- Link to the spec from the skill.

## TESTS
- [x] `tests/skill.rs`: design 5 over every page.
- [x] `tests/cli.rs` (or a new `tests/skill_cli.rs`):
  - [x] `lughac skill` and `lughac skill functions`;
  - [x] an unknown page (exit 2, a suggestion);
  - [x] `--list` order;
  - [x] `--search` (a hit, and a miss exiting 1);
  - [x] `--export` (writes `lugha/SKILL.md`, and refuses an existing folder);
  - [x] `lughac spec` (exit 2).
- [x] Targeted: `--lib`, `skill`, the CLI file, `spec`, `lsp`, `comments`.

## ACCEPTANCE
- [x] `lughac skill --export` gives a folder that follows the Agent Skills format.
- [x] Every example in the skill compiles and prints what the page says.
- [x] `lughac spec` is gone, and nothing reads `lugha::SPEC`.
- [x] `cargo fmt --check` and clippy clean; the targeted tests pass.
