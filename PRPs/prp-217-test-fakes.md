## FEATURE: The test runner's capabilities: a test is given a fresh temporary `Dir`, a fixed `Clock`, an empty `Env`, a seeded `Entropy` and a loopback-only `Net`; the real `Fs`, `Proc` and network only in `@test(integration)` tests, run by `lughac test --integration`; milestone 185, in v18's Phase 2.

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
import std.time;

fun stamp(clock: Clock): string {
    let c = time.civil(clock.now_ms(), 0).unwrap();
    "{c.year}-{c.month:02}-{c.day:02}"
}

@test
fun files_land_in_a_fresh_folder(dir: Dir) {
    assert(dir.list(".").unwrap().len == 0);              // empty: made for this test
    dir.write_file("note.txt", "hi").unwrap();
    assert_eq(dir.read_file("note.txt").unwrap(), "hi");
    assert(dir.read_file("/etc/hostname").is_err());      // only paths inside it
}

@test
fun the_date_is_fixed(clock: Clock) {
    assert_eq(stamp(clock), "2000-01-01");
}

@test
fun randomness_repeats(entropy: Entropy) {
    assert_eq(entropy.next_u64(), 16294208416658607535);  // the same every run
}

@test(integration)
fun the_real_world(fs: Fs, proc: Proc) {
    assert(proc.run("true", []).unwrap().code == 0);
}
```
```
$ lughac test t.la
test t.files_land_in_a_fresh_folder ... ok
test t.the_date_is_fixed ... ok
test t.randomness_repeats ... ok
test t.the_real_world ... skipped (integration: run with --integration)
$ lughac test --integration t.la          # all four run
```

## CONTEXT
- Draft v18 §2.1 rule 13 [CAP-13, OPN-06]: "A test asks for capabilities by type, like `main`, and the test runner supplies safe ones: a fresh temporary `Dir`, deleted afterwards; a fake `Clock` at a fixed date; a captured `Console`, shown when the test fails; a seeded `Entropy`; a fake, local `Net`. Real `Fs`, `Net` and `Proc` are given only to `@test(integration)` tests, which run only under `lughac test --integration`."
- Today (PRP-212, PRP-213): a test takes `Console`, `Fs`, `Clock`, `Env`, `Proc`, `Entropy`, `Net` by type and gets the real ones. The runner (`src/driver/test.rs`) runs each test in its own process with stdin closed and stdout and stderr captured, shown only when the test fails: the real `Console` already is the captured one.
- Code: `src/parser/attributes.rs` (`@test`), `src/ast` (the function's test marker), `src/formatter`, `src/check/test_fns.rs` and `src/check/entry.rs` (what a test may take, E0434), `src/codegen/test_main.rs` (the test entry builds the values), `src/driver/test.rs` and `test_build.rs` (the runner, `--integration`), `src/prelude/` (the fakes), the language server's test lenses if they read the marker.
- Measured: one test in the repository takes `Fs` (`tests/test_runner.rs`), none takes `Proc`.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **What a test may take:** `Console`, `Dir`, `Clock`, `Env`, `Entropy` and `Net`, by type, each at most once. `Fs` and `Proc` only in `@test(integration)`; asking for them in a plain test is E0434 with help "mark it `@test(integration)`". `string[]` stays `main`'s only.
2. **The plain test's values:**
   - `Console`: the real one (the runner captures it, as now).
   - `Dir`: a fresh empty folder the runner makes for each test under the system's temp folder and deletes afterwards, passed or failed. Its methods refuse an absolute path or one with a `..` part (`Err("a test's folder takes only paths inside it")`), so a test stays inside it. This is a check on the path's text, not the kernel's: a symbolic link the test itself makes inside the folder can still lead out. Kernel confinement comes with `fs.dir(path)`.
   - `Clock`: fixed at 2000-01-01T00:00:00Z (`now_ms()` 946684800000), `monotonic_ms()` always 0, `local_offset` always `Ok(0)`.
   - `Env`: empty (`get` is always `None`).
   - `Entropy`: a splitmix64 stream from seed 0, the same in every test and every run.
   - `Net`: real sockets on the loopback interface only: `connect`, `listen` and `udp_bind` accept a host of `127.0.0.1`, `::1` or `localhost` (`Err("a test's network is this machine only")` otherwise); `resolve` answers only those names; `interfaces()` is empty.
3. **An integration test's values:** the real `Console`, `Fs`, `Clock`, `Env`, `Proc`, `Entropy` and `Net`, and the same fresh temporary `Dir` if it asks for one. It runs in the folder `lughac test` was started in, as tests do today.
4. **`@test(integration)`** is the only argument `@test` takes; anything else in the parentheses is E0210. The formatter prints it as written.
5. **`lughac test --integration`** runs plain and integration tests; without it an integration test is reported `skipped (integration: run with --integration)` and counts as skipped, in text and JSON. `--filter` applies as before.
6. **How the folder reaches the test:** the runner starts each plain test's process in its fresh folder (its working directory), so the prelude's test `Dir` is the real `Dir` with the path check in front. An integration test asking for a `Dir` gets the same: the runner passes the folder's path in `LUGHA_TEST_DIR`, and the test `Dir` resolves paths under it.
7. **The fakes live in the prelude** (`src/prelude/test_caps.la`), named `$test_dir()`, `$test_clock()`, … beside `$console()`, and are compiled only into test executables (the test entry is their only caller).

## REQUIREMENTS

### Must Do
- Parser, AST, formatter: `@test(integration)`.
- Checker: the two parameter lists; E0434's message and help.
- Codegen: the test entry builds each parameter from the plain or integration list.
- Runner: a fresh folder per test (made before, deleted after, `LUGHA_TEST_DIR`), plain tests run in it; `--integration`; integration tests skipped without it.
- Prelude: the test `Dir`, `Clock`, `Env`, `Entropy` and `Net`.
- Spec (the testing section, capabilities paragraph, E0434, §11 row 185, Changes, Breaking), CHANGELOG, `docs/errors/E0434.md`, CLAUDE.md's `lughac test` line.

### Must NOT Do
- Kernel confinement of the folder (`openat2`, Landlock): `fs.dir(path)`'s PRP, with security review.
- Virtual time or deterministic scheduling (Phase 6): the fixed clock doesn't move, and `task.sleep` still waits.
- A seed option (`--seed`): one fixed seed for now.

## TESTS
- [ ] `tests/programs/tooling/test_capabilities/` (run through `lughac test` from `tests/test_runner.rs`): the objective; the folder is empty at the start of each test and gone afterwards, even when the test fails; absolute and `..` paths refused; the clock, env and entropy values; a loopback server and client inside a test; a non-loopback connect refused.
- [ ] E0434 for `Fs` and `Proc` in a plain test; `@test(integration)` accepted; `@test(other)` is E0210.
- [ ] `--integration` runs integration tests; without it they are skipped, in text and in `--json`.
- [ ] Formatter keeps `@test(integration)`.
- [ ] Full `cargo test`.

## ACCEPTANCE
- [ ] The objective's three plain tests pass and the integration test is skipped; with `--integration` all four pass.
- [ ] A plain test can't reach the real file system outside its folder by path, the network beyond loopback, other programs, the environment or the real clock and randomness.
- [ ] `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated where they change
