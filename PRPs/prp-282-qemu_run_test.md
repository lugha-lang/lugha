## FEATURE: `lughac run` and `lughac test` take `--target`, running programs for another CPU under qemu; ARM and musl tested in CI — milestone 248

## OBJECTIVE

```bash
$ lughac run hello.la --target=aarch64-unknown-linux-musl     # built by zig, run under qemu-aarch64
hi
$ lughac test tests/ --target=aarch64-unknown-linux-gnu       # each @test built for ARM, run under qemu
test math.adds ... ok
test math.panics ... ok
2 passed
$ lughac run hello.la --target=x86_64-unknown-linux-musl      # same CPU: runs directly, no qemu
hi
```

Without qemu: `lughac run hello.la --target=aarch64-unknown-linux-musl` stops with "running a program built for aarch64-unknown-linux-musl needs qemu-aarch64 (Ubuntu: `sudo apt install qemu-user`)", exit 2.

In CI, the ARM and musl tests run (zig and qemu installed); a missing tool there fails the run instead of skipping.

## CONTEXT

- Read first: `src/driver/pipeline.rs:369` (`run`: builds, then starts the program), `src/driver/test.rs:341` (`run_one`: starts one test's executable), `src/driver/test_build.rs` (builds test executables for `Target::native()`), `src/driver/cli.rs` (`Run`, `Test`, `TargetArg`), `src/driver/mod.rs:97` (the `--cpu` check), `tests/arm.rs`, `tests/musl.rs`, `.github/workflows/ci.yml`.
- Creates: `src/driver/emulate.rs` (how a target's executable is started on this machine).
- Implements: the v18 Draft, Part 9, 9.3 step 2 [PRT-04]. Changes the spec's `--target` row (§9 CLI table) from "On `build` only" to `build`, `run` and `test`, and the `run`/`test` rows.
- Decisions: v18 ledger PRT-01–07, PRT-14 (approved); none open. `[build]` in `lugha.toml` (TODO B3) is not in the Draft's step 2 and stays on TODO.

## REQUIREMENTS

### Must Do

1. **`--target` on `run` and `test`**, the same four values as `build`, defaulting to the machine lughac runs on. Building follows PRP-281 (zig for a foreign target, `$LUGHA_CC` over both). `--cpu` with an ARM64 target is exit 2 on `run` and `test` too, as on `build`.
2. **Starting the executable** (`src/driver/emulate.rs`): a target whose CPU is this machine's runs directly, as today (musl included: static executables need nothing). A target for another CPU runs as `qemu-<arch> <exe> <args…>` (`qemu-aarch64`, `qemu-x86_64`), qemu found on `PATH`.
3. **glibc under qemu.** A foreign-CPU glibc executable needs that CPU's C library for qemu to load. When `QEMU_LD_PREFIX` is unset, lughac sets it to `/usr/<arch>-linux-gnu` if that folder exists (Ubuntu's `libc6-arm64-cross`); if it doesn't, the run stops before starting with exit 2: "running a program built for aarch64-unknown-linux-gnu needs an ARM64 C library for qemu: install libc6-arm64-cross, set QEMU_LD_PREFIX, or build for aarch64-unknown-linux-musl". A set `QEMU_LD_PREFIX` is left as it is.
4. **No qemu** → exit 2 with the message in OBJECTIVE, before building (so a missing tool is found in a second, not after zig's work).
5. **Exit codes and signals pass through**: qemu exits with the program's code and re-raises its fatal signal, so `run`'s 128 + N rule and `test`'s outcomes hold unchanged.
6. **`lughac test --target`**: test executables are built for the target and each started as in 2–3, with the same `LUGHA_TEST`, `LUGHA_TEST_DIR`, `LUGHA_SCHED`, folder and timeout as today.
7. **CI** (`.github/workflows/ci.yml`): install zig 0.17.1 from ziglang.org's tarball, checked against its published SHA-256 (pinned in the workflow) before unpacking; drop `gcc-aarch64-linux-gnu` and `libc6-dev-arm64-cross` (zig replaced them), keep `qemu-user` and add `libc6-arm64-cross` (qemu's ARM64 glibc); set `LUGHA_CROSS_TESTS=required`, which makes `tests/arm.rs` and `tests/musl.rs` fail instead of skipping when a tool is missing.
8. Tests: see TESTS. Docs: spec (`--target` row; `run` and `test` rows; §11 row 248), CHANGELOG, CLAUDE.md (COMMANDS: `--target` on `run` and `test`), TODO (remove B3's `run`/`test` and CI parts, keep `[build]`).

### Must NOT Do

- No `[build]` table in `lugha.toml` (not planned in the Draft's step 2).
- No macOS or Windows runners (steps 4–5), no `binfmt_misc` setup, no running through Docker.
- No change to how native executables are started.
- No downloading of qemu or of C libraries by lughac.

## DESIGN

**Calls (user's review: lughac starts a new program).** `qemu-<arch>` runs from `PATH` with an argument list (`[exe, args…]`), never a shell, as the built program runs today; the only arguments are the executable lughac just built and the user's own `-- args`. One environment variable may be set, `QEMU_LD_PREFIX`, to a fixed system folder that exists. lughac downloads nothing.

**Under qemu, process lockdown doesn't apply.** qemu-user doesn't pass a program's seccomp filter or Landlock rules to the kernel; checked on this machine: `LUGHA_LOCKDOWN=report` says `seccomp not applied; landlock not applied` under `qemu-aarch64` (natively: both applied). That is the specified behaviour, each part skipped quietly where the kernel side isn't there, and `LUGHA_LOCKDOWN=report` says so. Lugha's own capability checks are unaffected. The spec's `run`/`test` rows say it: emulated runs are for testing, without the kernel's limits.

**`emulate.rs`**:
```rust
/// How to start an executable built for `target` on this machine.
pub(super) struct Runner { program: Option<OsString>, envs: Vec<(OsString, OsString)> }
pub(super) fn runner(target: Target) -> Result<Runner, String>;   // the checks of 2–4
impl Runner { pub(super) fn command(&self, exe: &Path) -> Command; } // `exe` itself, or `qemu-… exe`
```
`runner` takes `PATH` lookup and the folder check through parameters in a private function, so unit tests use fakes. Its error is an internal error (exit 2) with the message.

**`LUGHA_CROSS_TESTS`** is read only by the two test files, never by lughac.

**Judgement calls (for review).**
- Same CPU runs directly, even across C libraries: a static musl executable runs on a glibc machine. A glibc executable on a musl machine (lughac built for musl) may not; that machine isn't supported as a host today.
- `QEMU_LD_PREFIX` defaulting to `/usr/<arch>-linux-gnu`: Debian and Ubuntu's cross C libraries live there; a set value always wins.
- The qemu check comes before building: it's instant, zig's first build isn't.
- A CI variable rather than unconditional failure: on a developer's machine the tests still skip with a note, as all of lughac's tool-dependent tests do.
- The `test` command's existing `--cpu` (x86-64 levels) gets the ARM64 refusal; `run` already shares `build`'s options.

## TESTS

- [ ] `src/driver/emulate.rs` unit tests: native CPU → no program; foreign CPU → `qemu-aarch64` with `[exe, args]`; musl foreign → no `QEMU_LD_PREFIX`; glibc foreign with the folder → `QEMU_LD_PREFIX=/usr/aarch64-linux-gnu`; glibc foreign without it → the C library message; `QEMU_LD_PREFIX` set → kept; no qemu → the qemu message.
- [ ] `tests/arm.rs`: `lughac run --target=aarch64-unknown-linux-gnu` of a printing program gives its output and exit code; a panicking one exits 101 with the panic text; `lughac test --target=aarch64-unknown-linux-gnu` on a file with a passing and a failing test reports one of each; `PATH` without qemu → exit 2 and the message.
- [ ] `tests/musl.rs`: `lughac run --target=x86_64-unknown-linux-musl` runs directly (works with no qemu on `PATH`); `lughac run --target=aarch64-unknown-linux-musl` runs under qemu; `--cpu=x86-64-v3` with `--target=aarch64-…` on `run` and `test` → exit 2.
- [ ] Both files: with `LUGHA_CROSS_TESTS=required` and a tool missing, the check fails instead of skipping (unit-tested on the check function with a fake tool list).

## ACCEPTANCE

- [ ] OBJECTIVE's commands behave as shown on this machine (zig 0.17.1, qemu-user, libc6-arm64-cross).
- [ ] Native `run` and `test` start programs exactly as before.
- [ ] `cargo test --lib`, `--test arm`, `--test musl`, `--test test_runner`, `--test cli` pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] The CI workflow installs and checks zig and sets `LUGHA_CROSS_TESTS=required` (it runs on the next push to `main`; merges don't wait for it).
- [ ] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated.
