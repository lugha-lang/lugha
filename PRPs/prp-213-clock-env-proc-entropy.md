## FEATURE: Capabilities, second step: `Clock`, `Env`, `Proc` and `Entropy`, taken by `main` and tests like `Console` and `Fs`; the ambient functions of `std.time`, `std.env`, `std.process`, `std.random` and `std.crypto` that reach them are removed; milestone 181, in v18's Phase 2.

**Status:** proposed 2026-11-16. Approved 2026-11-16.

## OBJECTIVE
```lugha
import std.random;
import std.crypto;

fun main(console: Console, clock: Clock, env: Env, proc: Proc, entropy: Entropy): i32 {
    let start = clock.monotonic_ms();
    console.println(clock.now_ms() > 0);                       // true
    console.println(env.get("HOME").is_some());                // true
    match proc.run("echo", ["hi"]) {
        .Ok(out) => console.print(out.stdout),                 // hi
        .Err(e) => console.eprintln(e),
    }
    console.println(entropy.bytes(16).len);                    // 16
    let mut rng = random.from_entropy(entropy);
    console.println(rng.below(6) < 6);                         // true
    match crypto.ed25519_keypair(entropy) {                   // its secret drawn from `entropy`
        .Ok((_, key)) => console.println(key.len),             // 32
        .Err(e) => console.eprintln(e),
    }
    console.println(clock.monotonic_ms() >= start);            // true
    0
}
```
`time.now_ms()`, `env.get(…)`, `process.run(…)`, `random.from_os()` and `crypto.random_bytes(…)` no longer exist.

## CONTEXT
- Draft v18 §2.1 [CAP-01, CAP-02, CAP-03, CAP-04, CAP-07, CAP-11, CAP-13, CAP-14, CAP-17]: wall-clock time, environment variables, other processes and operating-system randomness each need a capability; seeded generators and scheduling time (`sleep`) don't; the working directory can't be changed; the prelude declares the interfaces with their core methods and the runtime supplies the real ones; higher-level libraries take capabilities as parameters.
- Follows PRP-212 (`Console`, `Fs`, `Dir`, `main`'s and tests' parameters by type, `ENTRY_CAPABILITIES`).
- Measured: `std.time` (`now_ms`, `monotonic_ms`, `local_offset`: 24 uses), `std.env` (`get`, `current_dir`, `set_current_dir`: 74), `std.process` (`run`, `run_with`, `call`: 47), `random.from_os` (12), `crypto.random_bytes` and the two `…_keypair` functions (27); most in their own modules' tests.
- Code: `src/prelude/capabilities.la`, `src/check/entry.rs`, `src/check/impls.rs`, `src/check/actors.rs`, `src/check/iterators.rs`, `src/std/{time,env,process,random,crypto}.la`, the codegen entry (`codegen/lower.rs`, `codegen/test_main.rs`).

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The interfaces**, in the prelude beside `Console` and `Fs`:
   ```lugha
   interface Clock {
       fun now_ms(self): i64;                              // wall clock, ms since 1970 UTC
       fun monotonic_ms(self): i64;                        // never goes back; for intervals
       fun local_offset(self, ms: i64): Result<i64, string>; // the system zone's offset, seconds east
   }
   interface Env { fun get(self, name: string): Option<string>; }
   interface Proc {
       fun run(self, cmd: string, args: string[]): Result<ProcessOutput, string>;
       fun run_with(self, cmd: string, args: string[], input: string): Result<ProcessOutput, string>;
       fun call(self, cmd: string, args: string[]): Result<i32, string>;
   }
   struct ProcessOutput { code: i32, stdout: string, stderr: string } with Eq
   interface Entropy {
       fun bytes(self, n: i64): u8[];                      // panics for n < 0
       fun next_u64(self): u64;
   }
   ```
   Each method behaves exactly as the std function it replaces (same messages, no shell, `PATH` lookup, UTF-8 checks). `monotonic_ms` belongs to `Clock`, not to scheduling time: it observes real time, so a fake clock must be able to answer it.
2. **`std.env` and `std.process` are removed**: everything they did is now a method. `std.time` keeps its pure calendar (`Civil`, `civil`, `from_civil`, `TimeError`); `local_offset` moves to `Clock` and returns its message as a `string` (the prelude can't name `std.time`'s `TimeError`).
3. **The working directory:** `set_current_dir` is gone [CAP-11]. `current_dir` becomes `Dir.path(): Result<string, string>`, the folder's absolute path (`fs.cwd().path()`), the one new `Dir` method here.
4. **`std.random`:** `from_os()` becomes `from_entropy(entropy: Entropy): Rng`, seeded from `entropy.next_u64()`; `seeded` is unchanged (not a capability [CAP-03]).
5. **`std.crypto`:** `random_bytes(n)` is removed (`entropy.bytes(n)`); `ed25519_keypair(entropy)` and `x25519_keypair(entropy)` take the capability and draw their 32-byte secret from it, so a fake `Entropy` gives reproducible keys. `hpke_seal` and TLS keep OpenSSL's own generator for their one-time keys: feeding them a capability's bytes needs a change to the C crypto code, which is security-reviewed work for its own PRP. Listed under Must NOT Do.
6. **Entropy's real implementation** builds `bytes(n)` from the runtime's 64-bit kernel read (`getrandom`, as `random.from_os` used): no runtime change. One system call per 8 bytes is slow for megabytes but fine for keys and seeds; a bulk read can come later.
7. **`main` and tests** take any of the six capabilities by type, as in PRP-212 (E0304 and E0434 messages list them). Tests get the real ones for now; the runner's fakes, and keeping `Proc` to integration tests, are the next PRP [CAP-13].
8. **Sendable, nominal, visible by bare name**, like `Console`: they join the same lists (implemented only by `impl`, allowed in actors). `ProcessOutput` is an ordinary prelude struct; a module's own `ProcessOutput` hides it.
9. **A removed std function** is the ordinary "no such function in the module" error; the CHANGELOG's Breaking entry lists each replacement. No new error code.
10. **Migration** in this repository only, by hand and script as in PRP-212: tests of `std.env`'s `set_current_dir` are deleted with the function; tests of the rest move to the methods.

## REQUIREMENTS

### Must Do
- Prelude: the four interfaces, `ProcessOutput`, `Dir.path`, their real implementations (`$Clock`, `$Env`, `$Proc`, `$Entropy`, `$clock()` …) over the existing `$os` built-ins.
- Checker: the four as entry capabilities for `main` and tests; nominal and sendable; E0304/E0434 messages updated.
- Codegen: the C entry and the test entry build them (already table-driven by `ENTRY_CAPABILITIES`).
- std: remove `std.env`, `std.process`, `time.now_ms`/`monotonic_ms`/`local_offset`, `random.from_os`, `crypto.random_bytes`; keypairs take `Entropy`.
- Spec: §2.1-style capability paragraph extended, std sections updated, §11 row 181, Changes, Breaking. CHANGELOG. Error pages that use the removed functions.

### Must NOT Do
- `Net` (with `std.net`, `std.tls`, `std.http`) and the rest of `std.fs` onto `Dir`: the next capability PRPs.
- `Signals`: it gives a `Receiver` (Phase 5).
- The test runner's fakes and `@test(integration)`; narrowing; confinement; process lockdown.
- Any change to `runtime/process.c` or the C crypto code (security review): `Proc` and the keypairs call what exists.

## TESTS
- [x] `tests/programs/capabilities/clock_env_proc_entropy/`: the objective; parameters in another order; a fake `Clock` and `Entropy` passed to a helper (reproducible key from fixed bytes); E0304 for a repeated `Clock`.
- [x] Unit (`check/entry.rs`): `main` accepts each new capability, rejects a repeat.
- [x] The existing `std.time`, `std.random`, `std.crypto`, env and process tests, migrated; the removed functions' tests deleted.
- [x] Full `cargo test` (the rule change touches programs inline in `tests/*.rs`).

## ACCEPTANCE
- [x] The objective prints `true`, `true`, `hi`, `16`, `true`, `32`, `true` and exits 0.
- [x] No std or prelude function reaches the clock, the environment, other programs or kernel randomness except through a capability, apart from `hpke_seal` and TLS (item 5), `std.fs` and `std.net` (later PRPs).
- [x] `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean
- [x] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated where they change

## CHANGES DURING THE BUILD
- **`Entropy.next_u64`**, not `u64`: `u64` is a type name and can't name a method.
- **`ProcessOutput` by bare name** needed two lists widened: type annotations (`prelude_type`) and struct literals (so a fake `Proc` can build one).
- **The §10 "Changing directory" program** became "The working folder" (`fs.cwd().path()`, paths relative to it); `tests/programs/std/chdir/` was deleted with `set_current_dir`, and the env and process programs moved to `tests/programs/capabilities/clock_env_proc_entropy/`.
- **Left in place, unused:** the `$os.chdir`, `$os.crypto_seed` and `$os.crypto_random` built-ins and their runtime functions. Removing them touches `src/link/`'s symbol list and the C crypto code, both security-reviewed; a cleanup for that review.
- **The VS Code grammar fixture** imported `std.env`; it imports `std.math` now (version 0.34.0).
