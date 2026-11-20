## FEATURE: The Lugha skill's `library/` pages, first batch: text, collections, console and files, and numbers, time and randomness; each an API reference with checked examples, and a test that every public item of the modules it covers is on its page; milestone 233.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
$ lughac skill --list
…
library/text                 string and char methods, formatting, parsing numbers, bytes and UTF-8.
library/collections          Every method of arrays, Map, Set and iterators.
library/console-and-files    Console, Fs, Dir and File: every method, its errors, and confinement.
library/numbers-time-random  Number methods, std.math, Clock, std.time, std.random, Entropy and std.task.
$ lughac skill text
```

## CONTEXT
- **Follows** PRP-262 to PRP-264 (decision 225). The concept pages teach; the library pages are where a reader or a model looks up an exact method.
- **Sources:**
  - the prelude (`src/prelude/*.la`), whose `/** */` comments describe each item;
  - std (`src/std/math.la`, `random.la`, `time.la`, `task.la`);
  - the checker's built-in methods (`len`, `push`, `pop`, `clear`, the iterator helpers).
- **The second batch** (a later PRP): `net`, `http`, `tls-and-crypto`, `json-and-toml`.
- **Decisions:** none open.

## DESIGN
1. **Four pages in `library/`, one shape:**
   - frontmatter `title` and `summary` (and `modules`, listing the std modules the page covers, if any);
   - `## Overview`: what the area is and how to get it (`import std.math;`, a capability);
   - `## Reference`: tables of every item, one row each, `` `signature` `` | what it does, including what it panics or errs on;
   - `## Examples`: checked examples of the common uses.

   A page over 300 lines starts with a contents list, as the Agent Skills guide asks.
2. **The pages:**
   1. **text:**
      - `string`: `len`, `char_count`, `chars`, `bytes`, `lines`, `split`, `trim`, `contains`, `starts_with`, `ends_with`, `slice`, `to_bytes`, `+`, comparison;
      - `char`: `is_digit`, `is_alphabetic`, `is_upper`, `is_lower`, `is_whitespace`, `to_upper`, `to_lower`;
      - interpolation and format specifiers;
      - `to_string`, `parse_i64`, `parse_u64`, `parse_f64`;
      - `string_from_utf8`, `include_str`.
   2. **collections:**
      - arrays: the built-ins, the helpers, `sort`, `sort_by`, `join`, `+`;
      - `Map` and `Set`: every method, the literal forms, `FromEntries` and `FromItems`;
      - iterators: every source and helper, and `Iter<T>`;
      - tuples.
   3. **console-and-files:**
      - `Console`, `Fs`, `Dir` and `File`: every method with its result and errors;
      - confinement and narrowing (`read_only`, `dir`, `stdout_only`);
      - `include_bytes`.
   4. **numbers-time-random:**
      - the number types' ranges and methods (`truncate`, `round`, `saturate`, `try_into`, `wrapping_*`, `kb`/`mb`/`gb`);
      - `std.math`;
      - `Clock` and `std.time`;
      - `std.random` and `Entropy`;
      - `std.task` (`sleep`, `yield_now`).
3. **`SKILL.md`:** a "Library" table after the concepts, saying when to read each page.
4. **`tests/skill.rs` gains:**
   - Library pages have `title`, `summary`, `## Overview`, `## Reference` and `## Examples`, and every page over 300 lines has a contents list.
   - **Coverage:** for each module a page lists in `modules`, every `public` function, type and method in that module's source is named on the page.
   - **The prelude:** every method in its `impl` blocks for `string`, `char`, `T[]`, `Map`, `Set`, `Option` and `Result`, and every item of the capability interfaces, is named on the page that covers it. The test keeps a small table from each prelude file and type to its page.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Coverage is checked by name, signatures by examples.** Names come from the source, so a missing item fails the test. A signature in a table is checked only where an example uses it. Checking every table row by compiling it would need a second language inside the tables.
2. **Hand-written, not generated:** each row's text is written for a reader. The source's doc comments are where the facts come from, but the pages don't copy them.
3. **Two batches:** this one covers what every program uses (the prelude and the small std modules); the network and data formats follow.
4. **`library/` pages don't use the concept shape:** they are for looking things up, not for learning in order, so they have no prerequisites or exercises.

## REQUIREMENTS
### Must Do
- Design 1–4.
- Spec: §11 row 233, a "Changes since v17" bullet, and the skill paragraph (library pages and their checks).
- CHANGELOG.
### Must NOT Do
- Change the compiler, the prelude or std.
- Mention the spec, milestones or versions in the pages.

## TESTS
- [x] `tests/skill.rs`:
  - [x] every example runs or checks as marked;
  - [x] the library page shape;
  - [x] coverage of the std modules named in `modules` and of the prelude's mapped items;
  - [x] contents lists on long pages.
- [x] Targeted: `skill`, `comments`, `--lib skill`.

## ACCEPTANCE
- [x] Every public item of `std.math`, `std.random`, `std.time` and `std.task`, and every prelude method on the covered types, appears on its page.
- [x] Every example is checked.
- [x] `cargo fmt --check` and clippy clean; the targeted tests pass.
