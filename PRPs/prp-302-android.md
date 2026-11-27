## FEATURE: Android command-line programs, `aarch64-linux-android` and `x86_64-linux-android` — milestone 283

## OBJECTIVE

A Lugha program builds for Android and runs there as a command-line program, from `adb shell` or a terminal app such as Termux (its Play Store build included), with what a Linux build has: tasks, panics, folders, sockets, name lookups through the phone's own resolver (Private DNS, VPNs), processes, local time, lockdown, TLS and crypto.

```sh
export ANDROID_NDK_HOME=~/Android/ndk/<version>
lughac build app.la --target aarch64-linux-android     # app: a PIE executable for Android 10+
adb push app /data/local/tmp/ && adb shell /data/local/tmp/app
# Today: error: unknown target
```

This is stage 1 of two the user chose: stage 2 (PRP-303, later) builds a Lugha library an Android app's Kotlin or Java code calls. Both stand on what this one ports.

🔒 **Security-sensitive:** lughac starts a new compiler, the NDK's `clang` (`src/link/`); process lockdown runs on a new system and can do less there (`runtime/os/linux/lockdown.c`); TLS trusts a new certificate store. This needs your review before any code.

## CONTEXT

- **Read first:**
  - `src/codegen/targets.rs`, `src/driver/cli.rs`, `src/link/compiler.rs` (`for_target`), `src/link/zig.rs` (as the model), `src/link/runtime_parts.rs` (`System`), `src/link/openssl.rs`;
  - `runtime/os/linux/` (`os.c`, `lockdown.c`, `dir_resolve.c`), `runtime/os/darwin/dir_resolve.c` (the folder walk), `runtime/os/posix/process.c`, `runtime/openssl/tls.c` (where trust is loaded);
  - `tests/openssl.sh`, `tests/windows_arm64.rs` (a build-only test file), CI's `build` job and `probe.yml`.
- **Spec:** the Draft's Part 9 gains a step: Android, before WebAssembly. A valid program means the same on Android (PRT-01). Lockdown "is applied where the kernel allows", and what a program promises where the kernel can't lock it down is "decided with that OS's port, with security review" (9.2 rule 5, PRT-11): here.
- **Decisions:** the user chose (2026-11-27) command-line programs and libraries for apps (stages 1 and 2), not full apps; and Android's own C library through the NDK, not static musl. Recorded as decision 229 when this ships. Nothing open blocks it.

## RESEARCH (summary; sources are AOSP's own files, on its main branch)

- **Why bionic, not musl:** apps targeting Android 10+ can't run files from their own folders. The Play Store Termux runs every program through Android's dynamic loader (`/system/bin/linker64`), which can't run a static executable; Termux's docs name zig's musl builds as not working. A PIE linked against bionic runs everywhere: `adb shell`, either Termux, an app.
- **Name lookups:** bionic's `getaddrinfo` asks the system resolver (netd, over `/dev/socket/dnsproxyd`), which both `adb shell` and apps may reach. The runtime already looks names up through `getaddrinfo` on a helper thread, so this comes free.
- **Local time:** bionic's `localtime_r` reads `TZ`, else the `persist.sys.timezone` property, and Android's own tz database. The runtime already goes through `localtime_r`: free.
- **The app sandbox:** an app (Termux) and everything it starts run under Android's seccomp filter, which **kills** the process (SIGSYS) on a call outside its list, rather than failing it. On arm64 the list allows `seccomp`, `prctl`, `futex`, `getrandom`, `clone3`, `epoll_*`; it doesn't allow `landlock_*`, `openat2`, `fchmodat2` or `io_uring_*`. `adb shell` has no such filter.
- **Certificates:** in `/apex/com.android.conscrypt/cacerts` (Android 14+, when not empty), else `/system/etc/security/cacerts`: PEM files named by OpenSSL's *old* subject hash, so OpenSSL's hashed-folder lookup never finds them; each file must be loaded.
- **Thread-local storage:** bionic has ELF TLS from Android 10 (API 29); below that, compilers emulate it through calls. The runtime reads its `worker` and `current` thread-locals on every switch.
- **16 KB pages:** devices from Android 15 may use 16 KB pages; every LOAD segment must be aligned to at least 16 KB.
- **CI:** GitHub's Linux runners have KVM, so an x86-64 Android emulator runs there at full speed. An ARM64 Android image on x86-64 is full emulation, too slow.
- **Rust**, for comparison: `aarch64-linux-android` is Tier 2 and builds with the NDK's clang as its linker, found through the NDK's location (`cargo-ndk`); no static musl route.

## REQUIREMENTS

### Must Do

**The targets**
- `--target aarch64-linux-android` and `--target x86_64-linux-android`: `Target::Aarch64Android`, `Target::X86_64Android`, the system `Android`. LLVM's triple carries the API level, `aarch64-unknown-linux-android29`, so codegen uses ELF TLS; objects are position-independent (already, `RelocMode::PIC`).
- `run` and `test` refused for an Android target: "runs only on Android: build it with `lughac build`, then run it with `adb shell`", as for macOS and Windows.

**The compiler: the NDK** (`src/link/ndk.rs`, new, beside `zig.rs`)
- The NDK is found through `$ANDROID_NDK_HOME`, else `$ANDROID_NDK_ROOT`: a folder holding `toolchains/llvm/prebuilt/<host>/bin/clang`. Without one, the build fails with a message naming both variables and where to get the NDK (as zig's missing message does). `$LUGHA_CC` still overrides it.
- Compiles and links with that `clang --target=<arch>-linux-android29`, plus `-Wl,-z,max-page-size=16384`. The NDK's clang links its own libunwind, which the runtime's panics use. Its `--version` line goes into the object cache's key, as zig's does.
- Never a shell; the NDK's path is used as given, as `$LUGHA_CC`'s words are.

**The runtime** (Linux's, with Android's differences, each under `__ANDROID__`)
- `runtime_parts.rs`: `System::Android` joins `os/linux/` and `os/posix/`, but the folder walk instead of `openat2`.
- **Folders:** `openat2` would kill an app's program, so Android confines a `Dir` with the walk macOS uses, moved from `os/darwin/dir_resolve.c` to `os/posix/dir_walk.c` and shared, its few differences (the `Dir`'s own open flags, where a folder is now) per system. A mode is set through the walk's handle (`fchmod`), never `fchmodat2`.
- **Lockdown:** seccomp as on Linux (allowed in apps, so it stacks on Android's own). **Never Landlock** on Android: probing it kills an app's program. So files aren't confined by the kernel there; the `Dir` walk still keeps every `Dir` inside its folder, as on macOS. `LUGHA_LOCKDOWN=report` says "files not limited", as macOS's does.
- **TLS:** on Android a client trusts every certificate in the system store (the APEX folder when it isn't empty, else `/system/etc/security/cacerts`), each file read and added; not `SSL_CTX_set_default_verify_paths`.
- Nothing else changes: sockets, epoll, futexes, `posix_spawnp` (no shell, `PATH` without the current folder), `getrandom`, signals and stack overflow are bionic's as they are glibc's.

**OpenSSL**
- `tests/openssl.sh aarch64-linux-android <folder>` (and x86-64): the same pinned 3.5.4 and checksum, configured as OpenSSL's own `android-arm64` / `android-x86_64` with the NDK, API 29, static. `LUGHA_OPENSSL_AARCH64_LINUX_ANDROID` names it.

**Tests and CI**
- Unit: triples, system, `run` refused, the NDK's paths and arguments, the missing-NDK message.
- `tests/android.rs` (new, as `tests/windows_arm64.rs`): with an NDK, hello, tasks, a panic, folders, sockets and a TLS case build for both targets; each is an ELF for its CPU, PIE, interpreter `/system/bin/linker64`, LOAD segments aligned to 16 KB; the runtime compiles with `-Wall -Wextra` without a warning. Without an NDK they skip; with `LUGHA_CROSS_TESTS=required`, as CI sets, they fail.
- CI's `build` job gets the NDK (a pinned release, its SHA-256 checked, cached) and builds `tests/android/` cases and the program suite for `x86_64-linux-android`.
- A new CI job, **Android (x86-64)**, on `ubuntu-latest` with KVM: an API 29 x86-64 emulator, the cases pushed and run with `adb shell` against their `.stdout`/`.exit`, then the suite against `tests/android/known.txt`. ARM64 Android is built on CI, not run.
- `probe.yml` gains `system android` (the x86-64 emulator).

**Docs:** the Draft's Part 9 order (the Android step), the spec's targets paragraph, `--target` row and §11 row 283; DECISIONS 229; CHANGELOG; TODO; CLAUDE.md's targets, `--target` list and test-requirements line.

### Must NOT Do

- No static musl Android build, and no `-android` alias for the musl targets.
- No Landlock, `openat2`, `fchmodat2` or `io_uring` call on Android, ever: in an app each kills the program.
- No resolver, time-zone or property code of our own: bionic's.
- No APK, no JNI, no shared library: stage 2.
- No 32-bit ARM (`armv7`): Lugha's runtime is 64-bit only.
- No change on any other target, beyond moving the folder walk (macOS keeps its behaviour byte for byte).

## DESIGN

- **Files:**
  - `src/codegen/targets.rs`, `src/driver/cli.rs`, `src/link/ndk.rs` (new), `src/link/compiler.rs` (`for_target` picks the NDK for Android), `src/link/runtime_parts.rs`, `src/link/openssl.rs` (the variable's name);
  - `runtime/os/posix/dir_walk.c` (moved from `os/darwin/dir_resolve.c`), `runtime/os/darwin/dir_resolve.c` and `runtime/os/linux/` (what stays per system), `runtime/os/linux/lockdown.c` (no Landlock on Android), `runtime/openssl/tls.c` (Android's store);
  - `tests/openssl.sh`, `tests/ndk.sh` (new: fetches and checks the pinned NDK, for CI and anyone without one), `tests/android.rs`, `tests/android/`, `.github/workflows/ci.yml`, `.github/workflows/probe.yml`.
- **No new crate, no `unsafe` Rust.**
- **What a reviewer should check hardest:**
  - lughac runs the NDK's `clang` from the path the environment names, with arguments from a list, never a shell;
  - no call on Android can be one the app filter kills: Landlock, `openat2`, `fchmodat2`, `io_uring`;
  - what lockdown promises on Android: seccomp (sockets without `Net`, processes without `Proc`) yes; files no, as on macOS;
  - TLS trusts the system's store and nothing else: no user-added certificates, no fallback to accepting.

## TESTS

- [ ] Unit (`targets.rs`): both triples, LLVM's with `29`, `Arch`, `System::Android`, not native, not Windows.
- [ ] Unit (`ndk.rs`): `$ANDROID_NDK_HOME` before `$ANDROID_NDK_ROOT`; the clang path for the host; `--target=…29` and the page-size argument; a folder without clang, and no variable at all, give the missing-NDK error naming both.
- [ ] Unit (`compiler.rs`): an Android target picks the NDK; `$LUGHA_CC` still wins.
- [ ] Unit (`runtime_parts.rs`): Android's parts are Linux's and POSIX's with `dir_walk.c` and without `linux/dir_resolve.c`; macOS's set changes only by the move.
- [ ] `run`/`test` with an Android target: the refusal message, exit 2.
- [ ] `tests/android.rs`: the cases build for both CPUs, each PIE, `linker64`, 16 KB-aligned; the runtime builds without a warning.
- [ ] macOS: CI's macOS job unchanged (the folder walk moved).
- [ ] CI's Android (x86-64) job: `tests/android/` cases (folders confined, a socket echo, a name lookup of `localhost`, a process, local time, lockdown's report, a TLS client trusting the system store) and the suite less the known list.

## ACCEPTANCE

- [ ] CI's Android (x86-64) job passes; ARM64 Android builds on CI.
- [ ] An ARM64 build runs from `adb shell` on a phone (the user's, by hand, when one is to hand).
- [ ] `cargo fmt --check`, clippy and the touched tests are clean; each file at or under 500 lines.

## DECIDED HERE (for review)

1. **Android 10 (API 29) is the oldest supported:** it brings ELF TLS (the runtime's thread-locals read directly, not through emulation calls), and `posix_spawn` and `getrandom`, which the runtime uses, came in 9. Devices older than 2019 are left out.
2. **The names are Rust's and LLVM's, `aarch64-linux-android` and `x86_64-linux-android`,** without the `unknown` vendor, as every Android toolchain spells them; the API level is lughac's to add, never the user's to write.
3. **x86-64 too,** though phones are ARM64: it's what the emulator runs at full speed, so CI can run every case and the suite on Android itself.
4. **The NDK through `$ANDROID_NDK_HOME`/`$ANDROID_NDK_ROOT` only,** the variables Android's own tools and Rust's `cargo-ndk` use; no search of `~/Android` or the SDK's folders, so which NDK builds a program is always visible.
5. **No Landlock on Android at all,** rather than probing for it: a probe in an app kills the program, and a program can't tell whether it runs in an app. Files stay confined by the `Dir` walk, not the kernel, as on macOS.
6. **The folder walk is shared, not copied:** macOS's walk moves to `os/posix/` for both systems, so a fix to it reaches both.
7. **TLS trusts the system store only,** not user-added certificates (`/data/misc/keychain/cacerts-added`), as a program run from a shell isn't the user's browser; adding them later is a small change if wanted.
8. **ARM64 Android isn't run on CI:** emulating it on x86-64 is too slow to be worth it. The runtime code is the same for both CPUs except the switch, which Linux's ARM64 builds already test under qemu.

## MADE WHILE BUILDING (for review)

9. **The NDK is r29 (29.0.14206865),** pinned in `tests/ndk.sh` by the SHA-256 of the zip Google's repository lists (its SHA-1 checked too when it was first fetched). r30 had just come out; r29 has had longer in use. Its `clang` links its own libunwind for C, so panics unwind with no extra argument.
10. **The per-system parts of the folder walk stay per system:** `os/posix/dir_walk.c` holds only the walk; `os/darwin/dir_resolve.c` and the new `os/android/dir_resolve.c` each define how a folder on the way is opened, how a `Dir` is opened, and where a folder is now. On Android the walk opens folders `O_PATH | O_DIRECTORY`: with `O_NOFOLLOW`, `O_PATH` alone would open a link itself rather than refuse it.
11. **Each system lists what it leaves out** (`System::left_out`), instead of every part outside its folder, since Linux leaves out the walk and Android leaves out Linux's `openat2` file.
12. **`src/link/mod.rs`'s tests moved to `src/link/mod_tests.rs`** (the user's choice), as it reached 507 lines.
13. **Android's certificate loading lives in `openssl/tls_trust.c`** beside the other systems' trust code; `tls.c` only calls it.
14. **CI runs `tests/android.rs` in the build job,** which has the NDK, and the four test jobs leave it out (`-E 'not binary(android)'`), rather than restoring a 2 GB NDK four more times. OpenSSL is built for x86-64 Android only, for the emulator's TLS case and the suite's.
15. **`tests/android_run.sh`** pushes the built cases, the suite and the test folders to the emulator, runs each case through `adb shell` (which keeps a program's exit and its stdout and stderr apart), checks lockdown's report, and runs `tests/suite.sh` on the device itself with Android's own shell tools.
16. **The cases** (`tests/android/`) are the portable ones from `tests/macos/` and `tests/windows/` (folders, confinement and the race, tasks, unwinding, panics, sockets, processes, lockdown), plus `lookup` (the system's resolver) and `local_time` (bionic's zone), and `openssl/roots` (a TLS connection to a public host trusting the system store).
