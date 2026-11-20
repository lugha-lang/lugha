## FEATURE: The Lugha skill's second batch: concepts 6–10 (collections, interfaces, generics, iterators, capabilities), with `SKILL.md`'s index extended; milestone 231.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
$ lughac skill --list
…
concepts/06-collections    Arrays, maps and sets: building, reading, changing and looping over them.
concepts/07-interfaces     Shared behaviour: interface declarations, impl blocks, interface values and bounds.
concepts/08-generics       Type parameters on functions and types, with bounds that say what a T can do.
concepts/09-iterators      for … of over anything with impl Iter, and the helpers map, filter, take, collect.
concepts/10-capabilities   Console, Fs, Dir, Net, Clock, Env, Proc, Entropy: what main is given, and narrowing it.
$ lughac skill capabilities
```

## CONTEXT
- **Follows** PRP-262 (decision 225), whose layout, page shape and tests this batch uses unchanged.
- **Sources for the pages' facts:** the compiler, and the prelude and std code. Every claim is checked by `tests/skill.rs` running the examples, and prose claims outside an example are checked with scratch programs before writing them down.
- **Decisions:** none open.

## DESIGN
1. **Five concept pages**, each in the shape `tests/skill.rs` enforces: frontmatter, The idea, Example, Rules, Mistakes (with real error codes, each a `lugha error` block), Exercises.
   1. **06 collections:**
      - arrays (`[1, 2]`, `[x; n]`, `push`, `pop`, `len`, indexing and its panic, slices, `sort`, `map`/`filter`/`fold`, `join`, `contains`);
      - `Map<K, V>` and `Set<T>` (`Map { "a": 1 }`, `insert`, `get`, `remove`, `contains`, looping, `with Key` for your own keys);
      - copy on assignment.
   2. **07 interfaces:**
      - `interface` with required and default methods;
      - `impl I for T` (one home: the interface's module or the type's);
      - interface values (`let s: Shape = Circle { … }`, arrays of them);
      - parents (`interface A: B + C`);
      - `Display`, `Error` and the operator interfaces (`impl Add for V`).
   3. **08 generics:**
      - `fun f<T>(…)`, bounds `<T: Ord>`, several with `+`;
      - generic structs and enums, `impl<T> Stack<T>`;
      - type arguments inferred or written (`f<i64>(…)`, `Stack<string>.new()`).
   4. **09 iterators:**
      - `impl Iter<T> for X { fun next(mut self): Option<T> }`;
      - `for … of` over arrays, strings (bytes; `chars()` for characters), maps, sets and iterators;
      - the helpers (`map`, `filter`, `take`, `skip`, `enumerate`, `zip`, `chain`, `collect`);
      - `Iter<T>` values and bounds.
   5. **10 capabilities:**
      - why `main` takes them;
      - `Console` (print, `read_line`'s `Result<Option<string>, string>`);
      - `Fs` and `Dir` (`fs.cwd()`, `read_file`, `write_file`, confinement);
      - `Net`, `Clock`, `Env`, `Proc` and `Entropy` in brief;
      - passing them to functions;
      - narrowing (`console.stdout_only()`, `dir.read_only()`, `net.only(…)`);
      - writing your own fake for tests.
2. **`SKILL.md`:** five more rows in the page table.
3. **Examples that need the outside world** (files, network, stdin) are written so the test can run them anyway: a file example writes then reads in its own working folder, and network and stdin examples are checked, not run.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **One concept per page, in this order:** collections before interfaces (arrays are used everywhere), generics after interfaces (bounds are interfaces), iterators after generics (`Iter<T>` is generic), and capabilities last of the batch (each is an interface).
2. **Library detail stays brief here:** the concept pages show the common methods; the full API lists belong to the `library/` pages of a later batch.
3. **No new test machinery:** the existing block markers suffice.

## REQUIREMENTS
### Must Do
- Design 1–3.
- Spec: §11 row 231 and a "Changes since v17" bullet.
- CHANGELOG.
### Must NOT Do
- Change the compiler, the command or the test's rules.
- Mention the spec, milestones or versions in the pages.

## TESTS
- [x] `tests/skill.rs`, unchanged: every new example builds and prints what its page says, every mistake fails with its code, and the prerequisites run in order.
- [x] Targeted: `skill`, `comments`.

## ACCEPTANCE
- [x] `lughac skill --list` shows concepts 1–10 in order.
- [x] Every example in the new pages is checked by the test.
- [x] `cargo fmt --check` and clippy clean; the targeted tests pass.
