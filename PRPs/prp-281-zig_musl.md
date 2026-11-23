## FEATURE: Foreign targets build through `zig cc`, and Linux musl (static executables) is a target — milestone 2xx (next free row)

## OBJECTIVE

```bash
$ lughac build hello.la --target=x86_64-unknown-linux-musl -o hello
$ file hello
hello: ELF 64-bit LSB executable, x86-64, statically linked
$ ./hello                       # runs on any Linux: Alpine, Debian 10, a FROM scratch image
hi

$ lughac build hello.la --target=aarch64-unknown-linux-musl -o hello-arm
$ qemu-aarch64 ./hello-arm      # static: no ARM sysroot needed
hi

$ lughac build hello.la         # this machine: cc, exactly as today
$ lughac build tls.la --target=x86_64-unknown-linux-musl
error[E0334]: `std.tls` can't be built for x86_64-unknown-linux-musl
```

Without zig: `lughac build hello.la --target=x86_64-unknown-linux-musl` stops with "building for x86_64-unknown-linux-musl needs zig 0.17.0 or newer (https://ziglang.org/download), or set $LUGHA_CC", exit 2.

## CONTEXT

- Read first: `src/link/compiler.rs` (the build's compiler, `$LUGHA_CC`), `src/link/runtime.rs` (compiling the runtime), `src/link/mod.rs` (`args`, `link_with`), `src/link/errors.rs`, `src/link/object_cache.rs`, `src/driver/pipeline.rs:279` (`build`: picks the compiler per target), `src/driver/cli.rs` (`TargetArg`), `src/driver/mod.rs:97` (`--cpu` checks), `src/codegen/targets.rs`, `src/codegen/emit.rs:218` (`target_machine`), `tests/arm.rs`.
- Creates: `src/link/zig.rs` (the target table, finding and checking zig), `docs/errors/E0334.md`, `docs/errors/E0335.md`, `tests/musl.rs`.
- Implements: the v18 Draft, Part 9, rules 9.1.1–9.1.5 and step 1 of 9.3 [PRT-02, PRT-03, PRT-05, PRT-06, PRT-07]. Changes the spec's `--target` row (§9 CLI table, line ~1785) and §8's "the C compiler" paragraph.
- Research: `docs/research/cross-platform.md` §2.
- Decisions: v18 ledger PRT-01–07 (approved). None open.

## REQUIREMENTS

### Must Do

1. **Targets.** `Target` gains `X86_64LinuxMusl` and `Aarch64LinuxMusl`; `--target` accepts `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` beside the two gnu triples. Codegen uses the LLVM triple as given.
2. **Native or foreign.** A target is *native* when it is the machine lughac itself was built for (its architecture, Linux, glibc: read from `cfg!` at compile time). The default `--target` becomes the native one (today it's always x86-64 gnu; identical on x86-64 machines).
3. **The compiler.** `$LUGHA_CC` when set, as today, for every target. Otherwise native → `cc`; foreign → `zig cc -target <zig triple> -mcpu=baseline`. The zig triple comes from a fixed table in `src/link/zig.rs`:

   | `--target` | zig `-target` |
   |---|---|
   | `x86_64-unknown-linux-gnu` | `x86_64-linux-gnu.2.28` |
   | `aarch64-unknown-linux-gnu` | `aarch64-linux-gnu.2.28` |
   | `x86_64-unknown-linux-musl` | `x86_64-linux-musl` |
   | `aarch64-unknown-linux-musl` | `aarch64-linux-musl` |

   `aarch64-linux-gnu-gcc` stops being a default (breaking: set `$LUGHA_CC=aarch64-linux-gnu-gcc` to keep it).
4. **Checking zig.** Before the first compile of a foreign build without `$LUGHA_CC`, lughac runs `zig version` once: missing → `LinkError::ZigNotFound(target)`; older than 0.17.0 or unparseable → `LinkError::ZigTooOld { found, target }`. Both internal errors (exit 2), messages as in OBJECTIVE. `$LUGHA_CC` skips the check (the user chose their compiler).
5. **zig's cache.** Every zig run gets `ZIG_GLOBAL_CACHE_DIR=<lughac cache root>/zig` (and `ZIG_LOCAL_CACHE_DIR` the same), so lughac's cache holds everything it builds and a read-only `HOME` still works. Without a cache root, zig's own default.
6. **Linking through zig.** Every zig link adds `-lunwind`: zig links no unwinder for C programs (no `libgcc_s`), and panics unwind (`_Unwind_Resume`, `_Unwind_ForcedUnwind`, `__gcc_personality_v0`). A musl target also adds `-static`. `-lm` stays (zig takes it as libc).
7. **Foreign `std.tls` / `std.crypto`.** A foreign build of a program importing either is E0334 at the import, before codegen: the host's OpenSSL is never linked into another platform's program.
8. **Foreign `pkg-config`.** A foreign build whose `[link]` tables use `pkg-config` is E0335 at that line, before `pkg-config` runs: its answers describe the host's libraries. `c` files, `libs` and `paths` still work (compiled and linked by zig for the target).
9. **`--cpu`.** Allowed for any x86-64 target, gnu or musl (codegen only, as today: the runtime is baseline); `--cpu` with an aarch64 target stays exit 2.
10. **Tests skip without their tools**, as `tests/arm.rs` does: musl tests need `zig`; the aarch64 ones also `qemu-aarch64`.
11. Docs: spec (§8, §9's `--target`, §11 row), CHANGELOG (Breaking: the aarch64 default compiler; the default target is the native one), CLAUDE.md (COMMANDS `--target` list, STACK's "The C compiler", end-to-end test needs), TODO (remove step 1), the two error pages.

### Must NOT Do

- No wrapper scripts and no rewriting of flags: lughac writes zig's command line itself and never emits flags zig rejects.
- No zig crate or vendored zig: zig is a program on `PATH` [PRT-02].
- No `run`/`test --target` (step 2), no macOS/Windows triples (steps 4–5), no runtime split (step 3).
- No change to native builds' commands, flags or output.
- No runtime source changes, unless musl needs one outside `lockdown.c` and `process.c`; a needed change in either of those stops the work for your review.

## DESIGN

**Checked before writing this PRP** (zig 0.17.1, today's lughac through `$LUGHA_CC`, no code changes): the runtime compiles for musl unchanged. Linking fails only on the unwinder, for musl and glibc alike, and works with `-lunwind`. `div_zero`, `fan_out`, `bank`, `stack_overflow`, `deadlock` and `maps` built for `x86_64-linux-musl` (run natively) and `aarch64-linux-musl` (under `qemu-aarch64`) match their expected stdout, stderr and exit codes; `aarch64-linux-gnu.2.28` links and runs under `qemu-aarch64 -L /usr/aarch64-linux-gnu`.

**Calls (user's review: changes how the C compiler runs, `src/link/`).**
- zig runs as `Command::new("zig").args(["cc", "-target", <table>, "-mcpu=baseline", …])`: an argument list, no shell, the program found on `PATH` as `cc` is. Every argument is lughac's own or a path it made; nothing from the program's text reaches the command except `[link]`'s `c`, `libs`, `paths`, which already reach `cc` under the same validation (E0329–E0331).
- `zig version` is run with no arguments; its output is parsed as `MAJOR.MINOR.PATCH[-…]`, anything else is `ZigTooOld` with what it printed (trimmed, at most 80 characters).
- Two environment variables are set on zig's commands (requirement 5), naming a folder under lughac's own cache root.
- No `unsafe`, no new crate.

**Object cache.** The key already includes the compiler's words and its `--version`; zig's words now include `-target …`, so each target's runtime is cached separately. For zig, `--version` is replaced by the `zig version` output already read.

**Diagnostics** (next free E03xx):
- E0334 "`std.tls` can't be built for <target>" (or `std.crypto`), at the import; help: "the C library it needs, OpenSSL, is linked from this machine, which only fits builds for this machine; build without `--target`, or on the target platform".
- E0335 "`pkg-config` can't be used for <target>", at the `pkg-config` line of `[link]`; help: "its answers describe this machine's libraries; name the target's libraries with `libs` and `paths`, or build without `--target`".

**Errors.** `LinkError` gains `ZigNotFound(String)` and `ZigTooOld { found: String, target: String }`, both internal (no diagnostic, exit 2), like `CcNotFound`.

**Judgement calls (for review).**
- glibc 2.28 for foreign gnu builds: the oldest zig defaults to, so an ARM build runs on Debian 10 / Ubuntu 20.04 and later. If the runtime calls something newer, the tests show it and the PRP is updated before going on.
- The runtime is compiled `-mcpu=baseline` for every zig target, as `cc` compiles it today (no `-march`).
- The default target becomes the native one: needed so lughac built on an ARM machine uses `cc`, not zig, for its own builds.
- `$LUGHA_CC` overrides foreign targets too, with no zig check: it's the user's explicit choice.
- Foreign `pkg-config` is refused rather than tried: never the host's libraries for another platform, the rule E0334 states for OpenSSL.
- Two new error codes rather than one: they point at different lines (an import, a `[link]` line) with different fixes.

## TESTS

- [ ] `src/link/zig.rs` unit tests: the table maps each target; `parse_version` accepts `0.17.0`, `0.17.1`, `0.18.0-dev.123+abc`, refuses `0.16.0`, `garbage`, empty; `is_native` for the build machine's own triple only.
- [ ] `src/link/compiler.rs`: `for_target` gives `cc` for native, `zig cc -target … -mcpu=baseline` for foreign, `$LUGHA_CC`'s words for both when set.
- [ ] `src/link/mod.rs`: `args` for a zig musl link has `-static -lunwind`, a zig gnu link `-lunwind`; a native `cc` link is unchanged.
- [ ] `tests/musl.rs` (skips without zig): `tests/programs/` cases (panics with unwinding, green threads, actors, maps, formatting, stack overflow) built for `x86_64-unknown-linux-musl`, run natively, match their `.stdout`/`.exit`/`.stderr`; the executable is static (no `PT_INTERP` program header); the same cases for `aarch64-unknown-linux-musl` under `qemu-aarch64` (skip without it).
- [ ] `tests/musl.rs`: missing zig (`PATH` without it) → exit 2 and the message; a fake `zig` printing `0.16.0` → exit 2, "0.17.0 or newer"; `$LUGHA_CC` set → no zig check.
- [ ] `tests/cli.rs`: E0334 for `import std.tls` and `std.crypto` with `--target=x86_64-unknown-linux-musl`; E0335 for a `[link]` `pkg-config` line; native builds unchanged.
- [ ] `tests/arm.rs`: builds through zig (no `aarch64-linux-gnu-gcc` needed); skips without zig or qemu.
- [ ] `tests/explain.rs` covers E0334 and E0335's pages.

## ACCEPTANCE

- [ ] OBJECTIVE's commands behave as shown (zig 0.17 installed: `pip install ziglang` or ziglang.org's tarball).
- [ ] Native builds produce the same commands as before (unit test on `args` and the compiler).
- [ ] `cargo test --lib`, `--test musl`, `--test arm`, `--test cli`, `--test explain` pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- [ ] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated; `lughac` version bumped to the next `0.18.P`.
