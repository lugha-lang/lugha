## FEATURE: Hello world on macOS: `--target=aarch64-apple-darwin` builds a program a Mac runs — milestone 251

## OBJECTIVE

The first, smallest step of portability step 4 (Draft 9.3): prove the whole path to a Mac before building the rest of the macOS runtime.

```bash
# on Linux, with zig 0.17
$ lughac build hello.la --target=aarch64-apple-darwin -o hello
$ file hello
hello: Mach-O 64-bit arm64 executable, … PIE|HAS_TLV_DESCRIPTORS
# on a Mac (Apple Silicon, macOS 14.4 or newer): CI's macOS runner
$ ./hello
hi
```

What this step does **not** promise, each stated where a program meets it:
- **Folders** (`Dir`, `Fs`): every operation is `Err("folders aren't supported on macOS yet")`, failing closed (the user's call), until confinement exists for macOS.
- **Process lockdown**: not applied, as on a Linux kernel without Landlock; `LUGHA_LOCKDOWN=report` says so (the user's call). Lugha's own capability checks hold.
- Intel Macs, `lughac run`/`test` for a Mac target, UDP's truncation length, and the rest of step 4.

## CONTEXT

- Read first: PRP-283 (the layout every file follows), `runtime/os/layer.c`, `runtime/os/linux/os.c`, `runtime/os/linux/aarch64.c`, `runtime/core/lugha_rt.c` (`worker_now`), `src/link/zig.rs`, `src/link/runtime_parts.rs`, `src/link/runtime.rs`, `src/codegen/targets.rs`, `.github/workflows/ci.yml`.
- Probed before writing (zig 0.17.1): the shared runtime compiles for `aarch64-macos` with 8 errors, all listed below; a C program using kqueue with `EVFILT_USER`, `os_sync_wait_on_address`, a pthread key, `getentropy`, `sysctlbyname` and `pthread_get_stackaddr_np` links into a Mach-O arm64 executable (zig's libSystem stub has `os_sync_*`; its headers lack `os/os_sync_wait_on_address.h`, so the runtime declares the two functions).
- Implements: v18 Draft, Part 9, 9.3 step 4's first part; 9.2 rule 4 (CPU × system) [PRT-14]; PRT-11's interim answer for folders and lockdown (the user's decisions above), not its final one.
- Decisions: PRT-02, PRT-06, PRT-14 (approved); PRT-11 deferred (final macOS lockdown and confinement).

## REQUIREMENTS

### Must Do

1. **The target.** `--target=aarch64-apple-darwin` on `build` (Mach-O through LLVM's own triple); zig's `aarch64-macos.14.4-none` (macOS 14.4: `os_sync_wait_on_address`); its link adds neither `-lunwind` nor `-static` (libSystem has the unwinder; macOS has no static executables). `run` and `test` refuse it (exit 2: "a macOS program runs only on a Mac").
2. **Parts per system.** lughac joins `os/linux/*` for a Linux target and `os/darwin/*` for a macOS one; shared and `os/posix/*` parts for both. `RUNTIME_SOURCE` becomes a function of the target's system; the join order is otherwise unchanged.
3. **The running thread's worker moves into the layer** (`os_worker()`): Linux keeps today's thread-pointer offset (one ELF thread-local block per thread at a fixed place); macOS reads a pthread key (its thread-locals are allocated per thread, so no offset holds). `thread_pointer` and `worker_offset` move from the core to `os/linux/`.
4. **`os/darwin/os.c`**: the layer on kqueue (`EVFILT_READ`/`EVFILT_WRITE` one-shot, `EVFILT_USER` for the two signals), `os_sync_wait_on_address`/`os_sync_wake_by_address_any` (declared here), `sysctlbyname("hw.activecpu")`, `getentropy`, `pthread_get_stackaddr_np`/`_stacksize_np`, the worker's pthread key; the socket details (`socket`/`accept` then `fcntl` for non-blocking and close-on-exec, `SO_NOSIGPIPE`, `OS_SEND_FLAGS` 0, `OS_RECV_FLAGS` 0); `#error` for any CPU but ARM64.
5. **`os/darwin/aarch64.c`**: Linux's AArch64 task switch, boot, `on_stack` and first frame in Mach-O form (`_` symbol prefix, no `.type`/`.size`); AAPCS64's rules are Apple's here (x18 is never touched).
6. **`os/darwin/dir.c`** and **`dir_names.c`**: every `lugha_rt_dir_*` and name function lughac's codegen calls, returning the error above. **`os/darwin/lockdown.c`**: `lugha_rt_lockdown` applies nothing and reports `lockdown: not applied on macOS; …` under `LUGHA_LOCKDOWN=report`.
7. **Shared fixes from the probe**: `memrchr` in `io/net.c` becomes a portable loop; in `os/posix/process.c` (🔒), `sigtimedwait` with a zero timeout becomes `sigwait`, which takes the SIGPIPE the code has just found pending without blocking.
8. **CI**: the build job builds `hello.la` (and the two cases below) for `aarch64-apple-darwin` and uploads them; a `macos` job on GitHub's Apple Silicon runner runs them and compares their output.
9. Docs: the spec's `--target` row and §11 row 251, CHANGELOG, TODO (step 4's remaining parts), `CLAUDE.md` (targets).

### Must NOT Do

- No Linux behaviour change (the worker lookup compiles to the same access there; the joined Linux runtime is diffed).
- No confinement or lockdown for macOS beyond the interim answers; no Intel Mac; no running Mac programs on Linux.
- No edit to 🔒 files but the one line in `process.c` (needs the user's review).

## DESIGN

**The probe's 8 errors and their fixes**: `OS_SEND_FLAGS` ×2, `OS_RECV_FLAGS`, `cpu_first_frame` (×2, call and conversion): `os/darwin/`; `memrchr` (×2): shared fix; `sigtimedwait`: `process.c`.

**`os_worker()`** is the one cost-sensitive change: every `worker` access goes through it. On Linux it stays `thread_pointer() + worker_offset` (inline, the same instructions); on macOS `pthread_getspecific`, a few instructions (macOS keeps keys at a fixed slot from its thread pointer). Measured on Linux with the benchmarks; macOS's speed is a later concern.

**Security.** Folders fail closed on macOS; lockdown is reported as not applied, as the spec already allows where the kernel can't; the `process.c` change keeps its meaning (take a pending SIGPIPE before unblocking) and is portable POSIX. The user decided the two interim answers; this line in `process.c` needs their review.

**Judgement calls (for review).** macOS 14.4 as the minimum (needed for `os_sync_wait_on_address`; Sonoma, 2024); ARM64 only; `os_worker()` in the layer rather than macOS-only code in the core; the CI job runs binaries built on Linux, so the Mac runner needs no LLVM or Rust.

## TESTS

- [ ] `src/link/zig.rs`: `aarch64-apple-darwin` → `aarch64-macos.14.4-none`, no `-lunwind`/`-static`.
- [ ] `src/link/mod.rs`: the boundary and joining tests per system (each system's parts joined only for it; every `os/darwin/` file joined for macOS); the macOS source compiles with `zig cc -target aarch64-macos.14.4-none` (skips without zig).
- [ ] `tests/macos.rs` (skips without zig): hello world builds into a Mach-O arm64 executable (magic `0xfeedfacf`, CPU type ARM64); `run`/`test --target=aarch64-apple-darwin` are exit 2.
- [ ] CI's `macos` job: `hello` prints `hi`; a program reading a folder gets the macOS folder error; `LUGHA_LOCKDOWN=report` prints the not-applied line.
- [ ] Linux unchanged: `cargo test --lib`, `--test programs`, `workers`, `scheduler_stress`, `waiting_runtime`, `helpers` and `lockdown` (programs started through `process.c`, the 🔒 change), `arm`, `musl`; the joined Linux runtime diffed (only the moved worker lookup); benchmarks no slower.

## ACCEPTANCE

- [ ] OBJECTIVE's build on Linux, and CI's macOS job green.
- [ ] Tests above pass; `cargo fmt --check`, clippy clean; files ≤ 500 lines.
- [ ] Spec, CHANGELOG, TODO, `CLAUDE.md` updated.
