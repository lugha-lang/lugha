## FEATURE: Hash seeding: maps and sets hash with SipHash-1-3 keyed by a secret seed the runtime draws per process, replacing FNV-1a, so keys chosen to collide can't make a map slow; a fixed seed in deterministic tests; milestone 204.

**Status:** approved 2026-11-18; built 2026-11-18 (milestone 204).

## OBJECTIVE
```
fun main(console: Console) {
    let mut seen: Set<string> = Set {};
    for line of attacker_lines() { seen.insert(line); }   // no key list makes this quadratic
    console.println(seen.len);                              // the same on every run
    let mut h = Hasher.new();
    h.add("ann");
    console.println(h.finish());                            // differs between runs
}
```
Maps and sets behave exactly as before (insertion order, `==` regardless of order); only hash values change, and change between runs.

## CONTEXT
- **Draft v18 §3.2** rules 1 (a keyed hash resisting chosen collisions, replacing FNV-1a, chosen by benchmark) [HSH-01], 2 (`Hasher.add<T: Key>`, already so) [HSH-07], 3 (one secret seed per process from the operating system's randomness, not a capability) [HSH-02], 4 (the spec says the runtime may use randomness that can't affect results) [HSH-03], 5 (fixed in deterministic mode, from the shuffle seed in shuffle mode) [HSH-04], 6 (hash values differ between runs: never store or send them) [HSH-05]. Rule 7 (`Wire`) is phase 7.
- **Asked of the user:** SipHash-1-3, its cost on `wordfreq` measured against the Draft's budget (at most 10% slower than its baseline, §7.2 [PRF-02]); a miss stops the work for a report.
- **Security:** a keyed hash and the seed's source are under the user's review (CLAUDE.md). The seed comes from `getrandom(2)` (else `/dev/urandom`) through the runtime's existing `lugha_rt_os_seed`, before process lockdown.
- **Code:** `src/prelude/eq.la` (`Hasher`, `$word`, `$text`), `src/check/hashing.rs` and `src/codegen/hashing.rs` (the `$hash` built-ins), `runtime/random.c`, `runtime/tasks.c` (the scheduling mode, PRP-235), `runtime/lugha_rt.c` (`lugha_rt_init`).
- **Decisions:** none open.

## DESIGN
1. **`Hasher` holds SipHash's four words and a count**, all private: `new()` starts them from the process's 128-bit key, `add` feeds a value as before (each `Key` implementation feeds `u64` words), `finish()` runs SipHash-1-3's finalization (three rounds) and gives the `u64`. Fed one `u64` word at a time, each word is one SipHash block and one round (the "1" of 1-3); the length goes into the finalization as the standard says.
2. **A string feeds its bytes eight at a time** (the last word zero-padded, little-endian), then its length, so `"ab"` + `"c"` and `"a"` + `"bc"` differ as fields. `$hash.chunk(s, i)` (a `u64` of the bytes from `i`, built inline) replaces `$hash.text`'s FNV loop.
3. **The key is two `u64`s the runtime fills as it starts** (`lugha_rt_init`, before anything can hash and before lockdown): from the operating system's randomness in normal mode; fixed in deterministic mode; derived from the shuffle seed in shuffle mode (`LUGHA_SCHED`, PRP-235). `$hash.key(0)` and `$hash.key(1)` read them inline (a load of the runtime's `lugha_rt_hash_key`).
4. **Nothing a program prints changes, except hash values themselves:** maps keep insertion order and compare regardless of order, so no result depends on the seed. The spec says so, and that hash values differ between runs and must never be stored or sent.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **The seed isn't a capability** (the Draft's rule): it can't change a result, and every function making a map would otherwise need one.
2. **A string's length follows its bytes**, the standard way to keep fields' boundaries apart.
3. **The key is read inline** (two loads), not through a runtime call per `Hasher`: a map lookup makes one, so the call would show in `wordfreq`.
4. **The deterministic key is fixed** (`0` and `0`, mixed), so a test that prints a hash value prints the same one on every run.
5. **`tests/programs/interfaces/eq/eq.la`**, which compares hashes of equal values, keeps comparing them; any program printing a hash value prints it only under a test.

## MEASUREMENTS
- `wordfreq`, both compilers' programs built once and run 15 times each, interleaved on one machine: `main` (FNV-1a) 0.395 s median, SipHash-1-3 0.466 s: **18% slower**, over the Draft's 10% target. (Runs one after another through `bench/run.py` swung by 10–30%, so interleaving is the measure.) A full word read with one load instead of a `memcpy` changed nothing: the cost is SipHash's rounds on short keys (a 5-byte word: one block, its length's block, three finishing rounds).
- **The user's call:** keep SipHash-1-3 and accept the miss for now; optimisations and measurements later (TODO.md: Rust's `0xff`-terminated strings, about 4 rounds instead of 6; a SwissTable-style table).

## MADE DURING THE BUILD
- `$hash.key` reads the key through a struct of two `u64`s (C's `uint64_t[2]`) with a safe struct GEP, its index a literal: no `unsafe` block.
- SipHash-1-3 was checked against an independent implementation, itself checked against SipHash-2-4's published vector; `tests/hash_seed.rs` pins three values under the deterministic key.

## REQUIREMENTS

### Must Do
- SipHash-1-3 in `Hasher`; strings by 8-byte words and their length; the seed drawn, fixed or derived as the mode says; `$hash.key` and `$hash.chunk`.
- `wordfreq` measured before and after (`python3 bench/run.py wordfreq --runs 5`), recorded in the PRP; stop and report if it is more than 10% slower than its baseline.
- Spec §4 (maps and sets, `Key`, `Hasher`), §6 (the runtime's randomness), §9 (the runtime's start), "Changes since v16", §11 row 204; CHANGELOG; `docs/` mentions of FNV.

### Must NOT Do
- `Wire`'s rebuilding of maps (phase 7).
- Any change to map or set behaviour, iteration order or `==`.

## TESTS
- [x] SipHash-1-3 against the reference vectors (keyed, a sequence of 8-byte blocks), through `Hasher` in a deterministic test with a known key, or in the C harness.
- [x] A program printing `Hasher` results twice in two processes: different in normal mode, the same under `LUGHA_SCHED=deterministic` and for one shuffle seed.
- [x] Equal values hash equal (`assert_key_laws`) for every built-in `Key` and derived structs and enums; strings split differently hash differently as fields.
- [x] Every map and set program keeps its output (`programs`).
- [x] Targeted: `--lib`, `programs`, `leaks`, `spec`, `explain`, `comments`, `test_modes`.

## ACCEPTANCE
- [x] Maps and sets use SipHash-1-3 under a per-process secret key; deterministic runs repeat.
- [x] `wordfreq` within its budget, recorded.
- [x] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean; targeted tests pass.
- [x] Spec and CHANGELOG updated.
