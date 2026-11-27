## FEATURE: Windows on ARM64, `aarch64-pc-windows-gnu` — milestone 275

## OBJECTIVE

Every Lugha program that builds for Windows on x86-64 builds for Windows on ARM64 too (Surface Pro X and later, Snapdragon X laptops, Windows on Apple Silicon virtual machines), as a native ARM64 `.exe`, with the same behaviour: tasks, panics, stack overflow, folders, sockets, processes, lockdown, `extern struct`s by value, TLS and crypto.

```sh
lughac build hello.la --target aarch64-pc-windows-gnu   # hello.exe, PE32+ ARM64
# Today: error: unknown target
```

Windows on ARM already runs today's x86-64 builds through its emulator (Prism), slower and with x64's frames. This target is what runs natively.

🔒 **Security-sensitive:** the build gains a target lughac starts zig for (`src/link/`), and lockdown runs on a new CPU (`runtime/os/windows/lockdown.c` is unchanged, but it's what a program relies on). This needs your review before any code.

## CONTEXT

- **Read first:**
  - `runtime/os/windows/x86_64.c` (the switch, the TEB's stack bounds, the first frame, `lugha_rt_on_stack`) and `runtime/os/darwin/aarch64.c` (the AArch64 switch this one follows);
  - `src/codegen/targets.rs`, `src/link/zig.rs` (`zig_triple`, `link_args`), `src/link/runtime_parts.rs` (`WINDOWS_ONLY`);
  - `src/codegen/c_calls.rs` (`c_sig`: AArch64 is decided before Windows) and `c_abi_arm.rs`;
  - `tests/openssl.sh`, `tests/windows.rs`, CI's `build` and `windows` jobs.
- **What exists:** everything in `runtime/os/windows/` but `x86_64.c` is written against Windows' API, not a CPU. The overflow handler, sockets, processes, lockdown and trust read no register. Codegen already has AArch64 (Linux, macOS) and Windows' SEH personality.
- **Follows:** PRP-292 (Windows on x86-64) step for step; TODO's D1–D3, "ARM64 Windows".

## RESEARCH (summary)

- **zig 0.17.1** builds `aarch64-windows-gnu` (MinGW-w64's headers and import libraries, UCRT) and links its libunwind there, `__gcc_personality_seh0` included: a test `.exe` linked and is `PE32+ … ARM64`. LLVM's `aarch64-pc-windows-gnu` emits ARM64 SEH unwind data.
- **Rust** names this `aarch64-pc-windows-gnullvm` (its `-gnu` means GCC's toolchain, which has no ARM64 Windows). LLVM, clang and zig call it `aarch64-pc-windows-gnu` / `aarch64-windows-gnu`, the name we already follow for x86-64.
- **Windows ARM64's calling rules** are AAPCS64's for non-variadic functions: x0–x7 and v0–v7, HFAs in SIMD registers, composites over 16 bytes by reference, results through x8. Only variadic calls differ (floats in integer registers), and Lugha's `extern fn`s are never variadic. x18 holds the TEB and is the system's; callee-saved are x19–x29, x30 and d8–d15, as on Linux.
- **The TEB** is the same 64-bit layout on ARM64: `StackBase` +0x08, `StackLimit` +0x10, `DeallocationStack` +0x1478, reached through x18 instead of gs.
- **OpenSSL:** 3.5.4 (our pinned LTS) has no MinGW ARM64 target; 3.6.0 added `mingwarm64`. 3.5.4's `arm-xlate.pl` already speaks the `win64` flavour (MSVC's ARM64 build uses it), so 3.6's target definition, given to 3.5.4 with `--config`, keeps the assembly (AES, SHA and bignum with ARMv8's crypto instructions, constant-time) rather than `no-asm`'s C tables.
- **CI:** GitHub's `windows-11-arm` runners are free for public repositories, as this one is. Wine on an x86-64 Linux can't run ARM64 `.exe`s, so locally the cases only build.

## REQUIREMENTS

### Must Do

**The target**
- `--target aarch64-pc-windows-gnu`: `Target::Aarch64Windows`, `Arch::Aarch64`, `is_windows()`. zig's name is `aarch64-windows-gnu`; its link arguments are x86-64 Windows' (the same import libraries).
- Everything `is_windows()` decides today follows: `.exe` names, the SEH personality, no `probe-stack` attribute (Windows' own `__chkstk` probes), `run` and `test` refused with "runs only on Windows: build it with `lughac build`".
- `extern struct`s by value follow AAPCS64, as on Linux and macOS: `c_sig` already decides by CPU before system.

**The runtime** (`runtime/os/windows/aarch64.c`, ≈170 lines, joined beside `x86_64.c`, each in its own `#if`)
- The switch saves and restores x19–x30, d8–d15 and FPCR, and the TEB's three stack bounds through x18, as x86-64's does through gs. x18 itself is never written.
- `lugha_rt_task_boot`, a new task's first entry, with ARM64 SEH unwind data ending the walk there, as x86-64's zero return address does.
- `lugha_rt_on_stack` with its frame pointer kept on the old stack and the TEB's bounds set for the new one, as x86-64's.
- `cpu_first_frame` lays out the first switch's frame.
- `os.c`'s check becomes `__x86_64__` or `__aarch64__`.

**OpenSSL**
- `tests/openssl.sh aarch64-pc-windows-gnu <folder>`: the same pinned 3.5.4 and SHA-256 check, configured as `mingwarm64` from a target definition the script writes (3.6.0's, word for word). `LUGHA_OPENSSL_AARCH64_PC_WINDOWS_GNU` names it.

**Tests and CI**
- Unit: the triple, family, zig name and link arguments; `.exe` naming; `run` refused.
- `tests/windows.rs`: hello, a task case, a panic case and the `extern_structs` shapes build for ARM64 Windows, each an ARM64 PE (machine `0xAA64`). Without Wine for ARM64, they build only.
- CI's `build` job builds the Windows cases, the program suite and ARM64 Windows' OpenSSL (cached as x86-64's is), as `windows-arm64-programs`.
- CI's `windows` job becomes a matrix: x64 on `windows-latest`, ARM64 on `windows-11-arm`. Each runs the cases (lockdown's report and the TLS trust cases included) and the suite against `tests/windows/known.txt`. A case failing only on ARM64 gets its own line naming why. If that grows past a few, it gets a file of its own.
- The `extern_structs` shapes against zig's C run on the ARM64 runner too: AAPCS64 checked against Windows' C, not only Linux's.

**Docs:** the spec's targets paragraph and §11 row 275; CHANGELOG; TODO (D1–D3 done); CLAUDE.md's targets line and `--target` list.

### Must NOT Do

- No `-gnullvm` spelling or alias: one name per target, LLVM's.
- No MSVC (`-windows-msvc`): MinGW through zig, as x86-64.
- No x18 written by the runtime, and no assumption that x18 is free in generated code (LLVM reserves it on Windows).
- No change on any other target, beyond CI's Windows job becoming a matrix.

## DESIGN

- **Files:**
  - `src/codegen/targets.rs`, `src/driver/cli.rs`, `src/link/zig.rs`, `src/link/runtime_parts.rs` (`WINDOWS_ONLY` 13);
  - `runtime/os/windows/aarch64.c` (new), `runtime/os/windows/os.c` (the CPU check);
  - `tests/openssl.sh`, `tests/windows.rs`, `.github/workflows/ci.yml`.
- **No new crate, no `unsafe` Rust.**
- **What a reviewer should check hardest:**
  - x18 is only read;
  - the TEB bounds are saved before and restored after the stack moves, as x86-64's;
  - the boot frame's unwind data ends the walk, so a panic in a task can't unwind into stale memory;
  - OpenSSL's target definition is 3.6.0's, unchanged.

## TESTS

- [ ] Unit: `Target::Aarch64Windows` triple `aarch64-pc-windows-gnu`, `Arch::Aarch64`, `is_windows`, not native; zig name `aarch64-windows-gnu`; link args as x86-64 Windows'.
- [ ] Unit: `default_output` gives `.exe`; `run` refused with the Windows message.
- [ ] `tests/windows.rs`: four cases build for ARM64 Windows as ARM64 PEs.
- [ ] CI's ARM64 Windows runner: the Windows cases, lockdown's report, the TLS trust cases (all seven lines), the struct shapes, and the program suite less the known list.
- [ ] x86-64 Windows: `--test windows` under Wine unchanged.

## ACCEPTANCE

- [ ] CI's ARM64 Windows job passes as x86-64's does.
- [ ] `cargo fmt --check`, clippy and the touched tests are clean; each C file at or under 500 lines.

## DECIDED HERE (for review)

1. **The name is `aarch64-pc-windows-gnu`,** as LLVM, clang and zig spell it and as our x86-64 one is; Rust's `-gnullvm` isn't followed, since its `-gnu` means GCC.
2. **AAPCS64 for structs, unchanged,** since Windows ARM64 differs only for variadic calls, which Lugha has none of. The struct shapes run against Windows' own C on CI to prove it.
3. **OpenSSL stays 3.5.4 (LTS),** given 3.6.0's `mingwarm64` definition through `--config`, rather than moving every target to 3.6 (not LTS), or building ARM64 Windows' without assembly (slower, and C table-based AES isn't constant-time).
4. **Tested on CI's ARM64 runner only:** Wine on x86-64 Linux can't run ARM64 PEs. Locally, the cases build and their machine type is checked.
5. **One Windows job as a matrix,** not a second copy, so x64 and ARM64 always run the same steps.

## MADE WHILE BUILDING (for review)

6. **The ARM64 tests are a file of their own,** `tests/windows_arm64.rs`, as `tests/windows.rs` is near the line limit and runs under Wine, which this one never does. It builds five Windows cases, the struct shapes at -O0 and -O2, and (with `LUGHA_OPENSSL_AARCH64_PC_WINDOWS_GNU`) the TLS cases, checking each is an ARM64 PE, and compiles the runtime with `-Wall -Wextra` without a warning.
7. **The struct shapes reach the ARM64 runner as cases:** with `LUGHA_CASES_OUT` set, the test keeps each build and what it must print, and CI's build job puts them beside the Windows cases.
8. **The boot frame's unwind data is real:** `lugha_rt_task_boot` clears x30 (`.seh_nop`), then saves x29 and x30 (both zero) with a frame record, so the unwinder reads a zero return address there and stops. `llvm-readobj --unwind` shows `nop`, `stp x29, x30, [sp, #-16]!`, `mov fp, sp`.
9. **The whole program suite (475 cases) builds for ARM64 Windows here;** nothing new is known to fail until CI's ARM64 runner says so.
