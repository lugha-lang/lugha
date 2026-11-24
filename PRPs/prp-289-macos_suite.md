## FEATURE: the whole program suite runs on CI's Mac — milestone 256

## OBJECTIVE

Every run case in `tests/programs/` (468 today, a `.stdout` and `.exit` each) is built for `aarch64-apple-darwin` on Linux. CI's Mac runs each one as `tests/programs.rs` runs it on Linux, and compares its stdout, exit code and stderr byte for byte. Today only the 13 cases in `tests/macos/` run on a Mac.

A failure on the Mac fails CI's Mac job, unless the case is on a short, reviewed list of known differences, each with its reason. A case on that list that starts passing fails the job too, so the list can't go stale.

## CONTEXT

- Read first:
  - `tests/programs.rs` and `tests/support/runner/` (`discover`, `Case`, `Extras`: `.args`, `.stdin`, `.envvars`, `.scratch`; `LUGHA_WORKERS=1`);
  - `tests/macos.rs`;
  - `.github/workflows/ci.yml`: the `build` job's "Build the macOS cases" step, and the `macos` job.
- Creates:
  - `tests/macos_suite.rs`: the Linux side, which builds the suite;
  - `tests/macos/suite.sh`: the Mac side, which runs it;
  - `tests/macos/known.txt`: the known differences.
- Changes: `ci.yml`.
- Spec: §11 row 256. No language or runtime change is planned. Bugs the suite finds are dealt with as in requirement 7.
- Implements: Draft 9.3 step 4, "a Mac CI runner", for the whole suite. Decisions: none open.
- Follows: PRP-288 (milestones 255 and 0.18.12).

## REQUIREMENTS

### Must Do

1. **Building, on Linux.** `tests/macos_suite.rs` has one test, ignored by default. It runs when `LUGHA_MACOS_SUITE=<dir>` is set: CI's build job sets it and runs that test with `--ignored`.
   - It finds the cases with the same `discover` as `tests/programs.rs`, so a case means the same thing on both systems.
   - It builds every run case with `lughac build --target aarch64-apple-darwin`, several at a time.
   - Each case gets a folder `<dir>/<n>/` holding:
     - `exe`;
     - `name`: the case's name, such as `concurrency/actors/ping.la`;
     - `stdout` and `exit`, and `stderr` when the case has one;
     - `args`, `stdin` and `envvars` when the case has them;
     - `scratch` when the case asks for a fresh folder.
   - Rejected programs (`lughac check` cases) aren't built. They test the compiler, which is the same on every target.
2. **What can't be built for a Mac** is skipped and listed with its reason, never silently. Those are builds refused with E0334 (`std.tls`, `std.http`, `std.crypto`) or E0335 (a `[link]` `pkg-config` line). Any other build failure fails the test.
3. **Running, on the Mac.** `tests/macos/suite.sh` is POSIX `sh`, with perl for timeouts, both already on the runner. It runs each case folder the way `run_case` runs a case:
   - `LUGHA_WORKERS=1`, plus the case's `envvars`;
   - its `args`, one per line;
   - its `stdin`, or an empty one;
   - from a fresh temporary folder when `scratch`, otherwise from the checked-out repository's root, as `lughac run` is started on Linux.
   - A case may run for 30 seconds: Mac runners are slower than Linux's 10-second limit allows for.
   - stdout, stderr when expected, and the exit code are compared byte for byte.
   - It prints `ok <name>` or `FAILED <name>`, with the differences, for every case.
   - At the end it prints counts: passed, failed, known, skipped.
4. **Known differences.** `tests/macos/known.txt` has one line per case: `<name> — <reason>`. Only two kinds of reason belong there:
   - a real platform difference, such as an error text that comes from the C library;
   - a bug, written up in BUGS.md.

   The job fails on an unknown failure, and on a known one that now passes. A unit test in `tests/macos_suite.rs` checks that every name on the list is a real case and has a reason.
5. **CI.**
   - The build job runs the suite build after the existing macOS cases, and uploads the folders in the same `macos-programs` artifact.
   - The macOS job checks out the repository, for the cases that read files beside them, then runs the 13 cases as now, then `suite.sh`.
   - The 13 `tests/macos/` cases stay: they test what's specific to a Mac (lockdown, confinement, a closed stdin).
6. **No change to `tests/programs.rs`** or to its runner's behaviour on Linux. The new test only reuses `discover`.
7. **What the first run finds** is reported to you before anything is fixed. Each failure gets one of three outcomes:
   - a small fix, done in this PRP if it's a few lines, test-first;
   - a known difference with a BUGS.md entry and a follow-up PRP;
   - a platform difference, added to `known.txt` with its reason.

   I'll show you the list and my proposal for each before merging.

### Must NOT Do

- **No Rust or LLVM on the Mac.** lughac running on a Mac is a separate step.
- **No new crate** and no change to the runner's case format.
- **No skipping by pattern** (whole folders, `std/*`). Every case not run is named, with its reason.

## DESIGN

**Why shell on the Mac.** The Mac job has no lughac and no Rust build. A POSIX script with perl's `alarm`, the same way the current step limits a program's time, needs nothing installed. The Linux side writes everything the script needs as plain files, so the script never parses a case format.

**Numbered folders.** Case names hold `/`; a number per case keeps paths simple, and `name` carries the real one into the report.

**Build time.** 468 builds through zig run several at a time in the build job (`std::thread::scope`, one per CPU). Each case's own object is new, but the runtime object is cached once. I'll measure it and report the added minutes. If it's more than a few minutes, the fallback is a separate Linux job, which I'd show you first.

**Order.** Cases run one at a time on the Mac, in name order, so a failure is easy to read and two cases never share a folder or a port.

**Security.** Nothing new runs on a developer's machine: the Linux side only calls `lughac build`, and the Mac side runs only on CI. No change touches lockdown, confinement, how programs start, or `src/link/`.

## TESTS

- [ ] `tests/macos_suite.rs`:
  - every name in `known.txt` is a run case, and every line has a reason (runs on every `cargo test`);
  - a case folder's files mirror the case: args, stdin, envvars and scratch round-trip. This is a unit test that writes one folder for a small fake case.
- [ ] `tests/macos/suite.sh`, dry-run on Linux against folders built for Linux (`LUGHA_MACOS_SUITE_TARGET` unset means the Mac target; a test-only override builds for this machine). All cases pass there, which proves the script runs cases the way `tests/programs.rs` does.
- [ ] CI's Mac job: the suite runs; the first run's failures are triaged per requirement 7.

## ACCEPTANCE

- [ ] CI's Mac runs every buildable run case, and each passes or is on `known.txt` with a reason; the skipped (E0334/E0335) are listed in the job's output.
- [ ] The suite passes on Linux through `suite.sh` (the dry run).
- [ ] Spec §11 row 256; CHANGELOG; TODO ("the full program suite on CI's Mac" done).
- [ ] `cargo test --lib`, `--test macos`, `--test macos_suite` and `--test programs` pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.

## CALLS I MADE (for review)

1. **Built on Linux, run by a shell script on the Mac,** not lughac on the Mac (the user's choice: the suite first, lughac itself later).
2. **Rejected-program cases aren't run on the Mac:** they only test the compiler.
3. **A 30-second limit per case,** against Linux's 10.
4. **A known-differences list that fails when an entry passes,** rather than a skip list that can rot.
5. **Bugs the first run finds are triaged with you before merging** (requirement 7). Small ones are fixed here; bigger ones get BUGS.md entries and their own PRP.
6. **The `LUGHA_MACOS_SUITE_TARGET` override** exists only so the shell runner can be proved on Linux. It's read by the ignored test, never by lughac.
