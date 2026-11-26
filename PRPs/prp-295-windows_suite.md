## FEATURE: The program suite on Windows — milestone 265

## OBJECTIVE

CI's Windows job runs every run case in `tests/programs/` on real Windows, as the Mac job does on a Mac.
- The cases are built for `x86_64-pc-windows-gnu` on Linux.
- Each one's stdout, exit code and expected stderr are compared byte for byte.
- `tests/windows/known.txt` names the cases expected to fail there, each with why.
- The job fails on any other failure, and on a known case that passes.

The same run works locally under Wine:

```bash
LUGHA_SUITE=/tmp/winsuite LUGHA_SUITE_TARGET=x86_64-pc-windows-gnu \
    cargo test --test platform_suite -- --ignored
LUGHA_SUITE_RUNNER=wine WINEDEBUG=-all MESA_LOG_LEVEL=error \
    sh tests/suite.sh /tmp/winsuite tests/windows/known.txt
```

**Measured before writing this, under Wine:**
- every one of the 472 run cases builds for Windows, but one: a TLS case, refused with E0334 as on the Mac;
- 452 pass;
- 10 fail only because Wine's Mesa prints a warning on stderr;
- the other 10 are platform differences, the draft known list below.

## CONTEXT

- **Read first:**
  - `tests/macos_suite.rs`, `tests/macos/suite.sh` and `tests/macos/known.txt`: the Mac's version of all this (PRP-289);
  - `.github/workflows/ci.yml`'s build, `macos` and `windows` jobs.
- **Implements:** Draft 9.3 step 5's last line, "a Windows CI runner": the whole suite, not only `tests/windows/`.
- **Follows:** PRP-294 (milestone 262).
- **Next:** processes on Windows, which would empty most of the known list.

## REQUIREMENTS

### Must Do

**One builder and one runner script for both systems,** not a copy per system:
- **`tests/macos_suite.rs` becomes `tests/platform_suite.rs`:**
  - `LUGHA_SUITE=<folder>` and `LUGHA_SUITE_TARGET=<triple>` replace `LUGHA_MACOS_SUITE` and `LUGHA_MACOS_SUITE_TARGET`;
  - the target is required, with no Mac default;
  - a Windows build refused for E0337 (an `extern struct` by value) is skipped with why, as E0334 and E0335 already are;
  - a Windows case's program is `exe.exe`, since Windows starts only names ending `.exe`;
  - the `.pdb` zig writes beside it is deleted: 2.4 MB a case, 1.3 GB for the suite against 248 MB without.
- **`tests/macos/suite.sh` becomes `tests/suite.sh <suite folder> <known list>`:**
  - it runs `exe` or `exe.exe`, whichever the folder holds;
  - it limits each run with `timeout` where the system has it (Linux, Git Bash on Windows), and with perl's `alarm` otherwise (a Mac);
  - it starts each program through `$LUGHA_SUITE_RUNNER` when set (`wine`).
- **The known-list check** (`every_known_difference_names_a_run_case_and_why`) reads both `tests/macos/known.txt` and `tests/windows/known.txt`.

**CI:**
- The build job builds both suites, into `macos/suite` and `windows/suite`.
- The Windows job:
  - sets `core.autocrlf false` before checking out the repository, since the cases read files beside them and compare bytes;
  - runs `sh tests/suite.sh windows/suite tests/windows/known.txt` from the repository's root under Git Bash;
  - sets `MSYS_NO_PATHCONV=1`, so Git Bash doesn't rewrite arguments that look like paths (`/c`, `/dev/null`) on their way to a Windows program.
- The Mac job calls `tests/suite.sh` with the Mac's list.

**`tests/windows/known.txt`, drafted from the Wine run.** CI's first run settles it:

| Case | Why |
| --- | --- |
| `capabilities/clock_env_proc_entropy/bytes.la`, `process.la`, `objective.la` | processes aren't supported on Windows yet |
| `capabilities/dir_confinement/confined.la` | its links are made with `ln` through `Proc`, which Windows refuses (`tests/windows/confined/` covers Windows' own confinement) |
| `std/fs_size/size.la` | its link, FIFO and sparse file are made with `ln`, `mkfifo` and `truncate` through `Proc` |
| `capabilities/clock_env_proc_entropy/env.la` | an absolute path on Windows starts with a drive (`C:\`), not `/` |
| `control/defer/files.la` | it counts open descriptors in `/dev/fd`, which Windows doesn't have |
| `collections/growable_arrays/segv_from_c.la` | a crash in C is a Windows exception, not SIGSEGV's exit 139 |
| `std/calendar/local.la` | Windows' C runtime gives no local time past the year 3000; the case asks for `i64.MAX` |
| `std/math/more.la` | `exp(1.0) == e()` is false under Wine's C runtime; on CI either listed or dropped |

**Docs:**
- the spec's §11 row 265;
- CHANGELOG;
- TODO: the "whole program suite on CI's Windows runner" item goes;
- CLAUDE.md's test line for `platform_suite`.

### Must NOT Do

- **No runtime or compiler change.** A Windows bug the suite finds becomes a known entry or its own fix, chosen with you.
- **Don't make lughac stop writing `.pdb` files.** That's a user-visible choice: debug info for WinDbg against a stray file beside the `.exe`. It goes to TODO as its own question.
- **Don't run the suite under Wine in CI:** CI has real Windows.

## DESIGN

- **No new crates and no `unsafe`.** Nothing reaches the security list: the suite runs our own test programs on CI's runner.
- **The `.pdb` files are deleted by the builder,** not by asking zig not to write them, so lughac's behaviour stays as it is until that question is decided.
- **Sizes:** the builder stays near 280 lines, and the script near 110.

## TESTS

- [ ] **`platform_suite.rs`:**
  - `a_case_folder_mirrors_its_case` (kept);
  - a refused Windows build is skipped with why, E0337 included;
  - every known entry in both lists names a run case and a reason.
- [ ] **Under Wine,** with the commands above: the Windows suite passes, every failure listed.
- [ ] **On Linux:** `LUGHA_SUITE_TARGET=x86_64-unknown-linux-gnu` and `sh tests/suite.sh` with an empty list pass every case. This checks the script's `timeout` path, as the Mac checks the perl one.
- [ ] **CI:** the Mac job still passes, and the Windows job passes with the list as CI's first run settles it.

## ACCEPTANCE

- [ ] The Windows job runs the whole suite and passes.
- [ ] The Mac job is unchanged in what it runs and still passes.
- [ ] `cargo fmt --check`, clippy and `cargo test --test platform_suite` are clean.

## DECIDED HERE (for review)

1. **Generalise, don't copy.** One builder and one script for every system, each system's differences in its own known list. A third system then needs only a list.
2. **The target is named, never defaulted.** A suite built for the wrong system fails loudly, not quietly.
3. **`timeout` where it exists, perl's `alarm` otherwise.** Git Bash ships `timeout`, and perl's `alarm` on Windows can't end a native program.
4. **`MSYS_NO_PATHCONV=1` and `core.autocrlf false` in CI.** Without them Git Bash and Git change the bytes the cases see.
5. **The `.pdb` question goes to TODO,** not into this PRP.
