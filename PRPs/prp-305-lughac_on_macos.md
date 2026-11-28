## FEATURE: lughac on a Mac — milestone 287

## OBJECTIVE

lughac itself builds and runs on macOS (Apple Silicon, 14.4 or newer), from source, as it does on Linux: `build`, `run`, `test`, `check`, `fmt`, `lsp` and the VS Code extension all work there, and a program built on a Mac runs on that Mac.

```sh
brew install llvm@21 rust          # once; Xcode's command-line tools for `cc`
cargo install --path .
lughac run hello.la                # builds with the Mac's own cc, runs here
lughac test                        # tests run as on Linux
lughac build app.la --target x86_64-pc-windows-gnu   # other targets still through zig
```

This is the first of three steps the user chose (2026-11-27): lughac on a Mac from source (this PRP), then on Windows from source, then prebuilt downloads.

🔒 **Security-sensitive:** how lughac starts what it builds (`src/driver/emulate.rs`: a Mac's own programs run directly) and how the C compiler runs (`src/link/`: Apple's `cc` for the Mac's own builds). This needs your review before any code.

## CONTEXT

- **Read first:** `src/codegen/targets.rs` (`Target::native`, only Linux's today), `src/driver/emulate.rs` (`choose`: which targets run here), `src/link/compiler.rs` (`cc` for the native target), `src/link/openssl.rs` (where a native TLS build finds OpenSSL), `src/modules/cache.rs` (the cache's folder), the few `std::os::unix` uses (`pipeline.rs`, `emulate.rs`, `compiler.rs`'s tests), CI's `macos` job, `README.md`'s "Working on lughac".
- **Spec:** section 9's `--target` row (the default target is the machine lughac runs on) and the Draft's Part 9 (lughac on Mac and Windows last).
- **Decisions:** DECISION-005 (LLVM 21, shared library, through inkwell's `llvm21-1-prefer-dynamic`), which this follows on the Mac. Nothing open.

## RESEARCH (summary)

- **LLVM 21 on a Mac:** Homebrew's `llvm@21` (21.1.8, keg-only, bottled for macOS 15, 26 and 27 on Apple Silicon) ships `llvm-config` and `libLLVM.dylib` (137 MB). `llvm-sys` 211 finds it by itself (`brew --prefix llvm@21`) when `LLVM_SYS_211_PREFIX` isn't set, and links it dynamically as on Linux. A binary built so needs Homebrew's `llvm@21` installed to run: fine from source; prebuilt downloads are step 3's question.
- **CI:** GitHub's `macos-15` runner has Apple Clang 17 and Homebrew's `llvm@18` only: the job installs `llvm@21` (a 368 MB bottle, cached).
- **OpenSSL:** macOS has no OpenSSL of its own for programs to link; Homebrew's `openssl@3` lives under `/opt/homebrew/opt/openssl@3`, off the compiler's default paths. Rust's `openssl-sys` looks there by name for the same reason.
- **What lughac assumes of Linux today:** `Target::native()` names only Linux targets; `run`/`test` refuse `aarch64-apple-darwin` ("runs only on a Mac"); a few `std::os::unix` uses, which macOS has too; the cache under `$XDG_CACHE_HOME` or `$HOME/.cache`; tests that need Linux (qemu, seccomp and Landlock's reports, glibc and musl builds, Wine).

## REQUIREMENTS

### Must Do

**The native target**
- On a Mac, `Target::native()` is `aarch64-apple-darwin`: no `--target` builds for the Mac lughac runs on, with its own `cc` (Xcode's), not zig; `-mmacosx-version-min=14.4`, so a program built on a Mac runs on any Mac 14.4 or newer, as zig's builds do.
- `run` and `test` run a Mac build directly on a Mac. Every other target is refused there with its own message (a Linux build: "runs only on Linux"; Windows' as today): qemu-user doesn't exist on macOS.
- `lughac build --target …` for every other target goes through zig as on Linux.

**OpenSSL for the Mac's own builds**
- A TLS or crypto program built on a Mac links the OpenSSL that `LUGHA_OPENSSL_AARCH64_APPLE_DARWIN` names (as a foreign Mac build does), else Homebrew's `openssl@3` at its standard places (`/opt/homebrew/opt/openssl@3`, `/usr/local/opt/openssl@3`); with neither, E0332 says to `brew install openssl@3` or set the variable.

**What else runs there**
- `check`, `fmt`, `lsp`, `update`, `init`, `explain`, `skill`: unchanged, checked on CI's Mac.
- The cache stays under `$XDG_CACHE_HOME/lugha`, else `$HOME/.cache/lugha`, as on Linux.
- Git packages: `git` as on Linux (`modules/git.rs` unchanged; macOS has `/dev/null`).

**Tests and CI**
- The Rust test suite builds and passes on a Mac: tests that need Linux itself (qemu, glibc and musl cross builds, seccomp's and Landlock's reports, Wine) are compiled for Linux only (`#[cfg(target_os = "linux")]`), each with a comment saying why; the rest pass unchanged or say why a Mac differs.
- The program suite runs natively through `cargo test --test programs`, against `tests/macos/known.txt` (the cases a Mac is known to differ on, as the zig-built suite has).
- CI gains a job, **lughac on macOS**: `macos-15`, `brew install llvm@21 openssl@3`, then fmt, clippy, the unit tests and the whole test suite (cargo-nextest, as Linux's). The existing macOS job (programs built on Linux) stays.

**Found on CI's Mac (added with approval, 2026-11-28)**
- A thread C made keeps its worker record on the heap on macOS, and the key destructor that retires it takes the record from the key's value and frees it last: dyld frees thread-local storage before that destructor runs, which left the record dangling in the `workers` list (a wrong `LUGHA_RC_REPORT` count, reads of freed memory).
- A Mac's object files carry the oldest macOS they run on: LLVM's triple `arm64-apple-macosx14.4.0`, so the linker sees the platform (it warned "no platform load command").
- `HOME` is resolved before it bounds the manifest lookup, as the current folder is: a home reached through a symlink (a Mac's `/var`) stopped nothing.
- A lost wake-up (approved 2026-11-28, "fix the race"): the kick meant for the worker waiting in the poll could be taken by another worker's poll that doesn't wait (a kqueue reports an event to one caller only; epoll has the same race, narrower), so the poller slept on through a new timer or a task made ready for it: programs hung with every worker idle (seen in `foreign_threads` and `net` on CI's Mac). A poll that doesn't wait now raises the kick again while a worker waits in the poll.

**Docs:** README's "Working on lughac" (a Mac's steps), CLAUDE.md's build line and stack, the spec's `--target` row (a Mac's default), §11 row 287, CHANGELOG, TODO (the Mac half of "lughac itself on a Mac").

### Must NOT Do

- No Intel Mac (decision 228).
- No prebuilt binary, Homebrew formula or installer: step 3.
- No static LLVM: DECISION-005's shared library, as on Linux.
- No change to how programs behave on a Mac: they are the same programs CI already runs there, built by Apple's `cc` instead of zig.
- No change on Linux.

## DESIGN

- **Files:** `src/codegen/targets.rs` (`native`), `src/link/compiler.rs` (the Mac's native flags), `src/link/openssl.rs` (Homebrew's place), `src/driver/emulate.rs` (a Mac's runs), the tests gated for Linux, `.github/workflows/ci.yml` (the new job), `README.md`.
- **No new crate, no `unsafe` Rust.**
- **What a reviewer should check hardest:** that a Mac runs only its own builds directly, never a Linux ELF; that the OpenSSL places are fixed paths checked for existence, never a program run (no `brew` call); that a test gated for Linux names why.

## TESTS

- [ ] Unit: `Target::native()` per host OS (Linux's unchanged); `choose` on a Mac: a Mac build runs directly, a Linux build is refused, both CPUs.
- [ ] Unit: the Mac's native compiler arguments carry `-mmacosx-version-min=14.4`; the OpenSSL folder chosen from the variable, else Homebrew's places, else E0332.
- [ ] CI's lughac-on-macOS job: fmt, clippy, unit tests, every integration test that runs on a Mac, and the program suite against `tests/macos/known.txt`.
- [ ] Linux's CI unchanged.

## ACCEPTANCE

- [ ] On CI's Mac: `cargo install --path .`, then `lughac run`, `lughac test` and the language server work, and the whole suite passes but its known cases.
- [ ] `cargo fmt --check`, clippy and the touched tests are clean on Linux and on the Mac; each file at or under 500 lines.

## DECIDED HERE (for review)

1. **The Mac's own builds use Apple's `cc`,** as Linux's use the system `cc`: zig stays for other targets. `-mmacosx-version-min=14.4` keeps the promise zig's builds make (`os_sync_wait_on_address` came with 14.4).
2. **OpenSSL from the existing variable, else Homebrew's two standard places,** looked up as paths, as Rust's `openssl-sys` does, rather than running `brew` or `pkg-config`: nothing is run to find it.
3. **The cache stays where Linux keeps it** (`$XDG_CACHE_HOME`, else `~/.cache`), as cargo, rustup and zig use home-folder caches on a Mac too, rather than `~/Library/Caches`.
4. **Linux-only tests are gated, not deleted:** each says what Linux thing it needs.
5. **The Mac's program suite reuses `tests/macos/known.txt`:** the zig-built and the Mac-built programs are the same programs, so they differ from Linux in the same places.
6. **Built on CI's Mac only:** there's no Mac here, so CI's job (and probes) is how this is built and tested.
