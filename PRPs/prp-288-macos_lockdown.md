## FEATURE: process lockdown on macOS through Seatbelt's coarse parts: no sockets without `Net`, no new processes without `Proc` — milestone 255

## OBJECTIVE

On macOS the kernel limits a program to the capability kinds `main` took, for the two kinds Seatbelt can refuse reliably. Today nothing is applied on macOS, and `LUGHA_LOCKDOWN=report` says so.

```lugha
extern fun socket(domain: i32, kind: i32, protocol: i32): i32;
extern fun fork(): i32;

fun main(console: Console) {
    console.println(socket(2, 1, 0));   // -1: no Net
    console.println(fork());            // -1: no Proc
}
```

On a Mac this prints `-1` twice, as it does on Linux. With `LUGHA_LOCKDOWN=report` it also prints, on stderr:

```
lockdown: seatbelt applied, files not limited; net refused, proc refused, fs refused
```

Files stay unlimited by the kernel on macOS (the user's choice: coarse parts only). Lugha's own capability checks still hold for Lugha code, as they always do.

## CONTEXT

- Research: `docs/research/macos.md` §2. Seatbelt is `sandbox_init` with a profile in SBPL. It is deprecated and private, but it is in libSystem, and zig's `libSystem.tbd` exports it. Chromium (`sandbox/mac/seatbelt.cc`), Firefox and Bazel declare it themselves. Once applied it can't be lifted, and children inherit it.
- Read first: `runtime/os/darwin/lockdown.c` (the stand-in replaced), `runtime/os/linux/lockdown.c` (what's promised, the report line), `tests/lockdown.rs`, `tests/macos.rs`, `.github/workflows/ci.yml` (the Mac job's `lockdown report` check).
- Spec: §6 Process lockdown (line ~1416, "Linux only"); §8's `lugha_rt_lockdown` row; the `--target` row ("process lockdown isn't applied there"); §11 row 255.
- Draft 9.2 rule 5 ("Process lockdown stays Linux-only") and PRT-11 (deferred: what a program promises on an OS whose kernel can't lock it down, decided with that OS's port, with security review). This PRP is that decision for macOS's lockdown, as the user chose it: Seatbelt, coarse parts only. The Draft's rule 5 and the ledger's PRT-11 line need updating to say so; I'll propose the wording with the code, not edit them before approval.
- Follows: PRP-287 (milestone 254). Decisions: none open once this is approved.
- 🔒 Security-sensitive (process lockdown): needs the user's review before code.

## REQUIREMENTS

### Must Do

1. **Apply a profile.** `lugha_rt_lockdown(kinds)` on macOS calls `sandbox_init(profile, 0, &error)` with an SBPL string, built from `kinds`:
   ```
   (version 1)
   (allow default)
   ;; without Net:
   (deny network*)
   (deny system-socket (require-not (socket-domain AF_UNIX)))
   ;; without Proc:
   (deny process-fork)
   (deny process-exec*)
   ```
   With both `Net` and `Proc` held there is nothing to deny, and nothing is applied.
2. **Without `Net`:**
   - no socket of any domain but `AF_UNIX` can be made;
   - no socket can connect, bind, listen or send to an address. That includes a named UNIX socket, which matches Linux, where `socket` itself is refused.
   - an unnamed `AF_UNIX` pair (`socketpair`) still works, as on Linux (milestone 238). Started tools need one.
3. **Without `Proc`:** `fork`, `vfork`, `posix_spawn` and every `exec` are refused.
4. **Inherited.** Programs started with `Proc` but without `Net` can't make sockets either, as on Linux.
5. **Best effort, as on Linux.** If `sandbox_init` fails (a profile the system won't take, a future macOS without it), the program goes on unlimited by the kernel, and the report names the error. It never ends the program.
6. **Declared by the runtime.** `sandbox_init` and `sandbox_free_error` are declared in `os/darwin/lockdown.c` itself, as Chromium does. The SDK headers mark them unavailable, and zig ships no `sandbox.h`.
7. **The report.** `LUGHA_LOCKDOWN=report` prints one line, ending as Linux's does (`net …, proc …, fs …`):
   - `lockdown: seatbelt applied, files not limited; …`
   - `lockdown: seatbelt not needed, files not limited; …` (both kinds held)
   - `lockdown: seatbelt not applied (<error>), files not limited; …`
8. **Helpers and workers** start after lockdown, as now (`helpers_allowed`). Seatbelt limits the whole process anyway.

### Must NOT Do

- **No file limits.** The user chose coarse parts only. A file profile would have to allow DNS, trust stores, time zones and lazily loaded libraries (research §2), which breaks quietly between macOS versions.
- **No profile deny-by-default** (`(deny default)`): the same breakage, everywhere.
- **No new crate, no SDK, no linking flag.** libSystem is linked already.
- **No change on Linux.**

## DESIGN

**Why these operations.**
- `system-socket` is checked when a socket is made, by `socket` and `socketpair` alike. The domain filter lets the `AF_UNIX` pair through and refuses `AF_INET`, `AF_INET6`, `AF_ROUTE`, `AF_SYSTEM` and the rest.
- `network*` (outbound, inbound, bind) refuses every use of an address. That covers a UNIX socket made with `socket(AF_UNIX)` and then connected to a path.
- `process-fork` and `process-exec*` cover fork, vfork, posix_spawn and the execs. Chromium's renderer profile relies on the same two.

**What it can't promise** (spec §6 will say so):
- Files aren't limited by the kernel. With `Fs` or without, a C library or a started program can read and write anything the user can. Lugha code still can't, without `Fs`.
- Descriptors open before lockdown keep working. There are none from the program at that point; the runtime's own are its kqueue and pipes.
- Seatbelt's profile language is private. Apple could change it, so a macOS update could make `sandbox_init` refuse the profile; requirement 5 covers that, and the CI test catches it.
- Denials return `EPERM`, as on Linux. I'll check that on CI and say so in the spec if not.

**Untested until CI's Mac.**
- Whether `socketpair` passes `network*`. A connected pair has no address, so I expect it to.
- If it doesn't, the fix is to narrow the profile to `(deny network* (remote ip))`, `(local ip)` and `(remote unix-socket (path-regex …))`. I'd stop and show you before doing that.

**Error codes, ABI.** No new diagnostics. `lugha_rt_lockdown`'s signature and the `kinds` bits are unchanged.

**Boundary test.** `sandbox_init` joins a macOS-only words list in `src/link/mod.rs`, so it stays in `os/darwin/`.

## TESTS

- [ ] `tests/macos/lockdown_bare.la` (no capabilities): `socket(2, 1, 0)`, `socket(1, 1, 0)` and `fork()` print `-1`. Runs on CI's Mac. Built and run on Linux, it prints the same, under Linux's lockdown.
- [ ] `tests/macos/lockdown_held.la` (`Net`, `Proc`): `socket(2, 1, 0) >= 0` is `true`, and `proc.run("/bin/echo", ["started"])` prints `started`.
- [ ] `tests/macos/lockdown_child.la` (`Proc`, no `Net`): `perl -e` started to make an inet socket exits refused, and one making a `socketpair` and passing a byte through it works.
- [ ] CI's Mac job: `LUGHA_LOCKDOWN=report ./hello` says `seatbelt applied`, and with `lockdown_held`, `seatbelt not needed`.
- [ ] `tests/macos.rs`: every case builds, and the macOS runtime compiles with zig under `-Wall` with no warning (as now).
- [ ] `src/link/mod.rs`: `sandbox_init` appears only in `os/darwin/`.
- [ ] `tests/lockdown.rs` unchanged and passing on Linux.

## ACCEPTANCE

- [ ] On CI's Mac: the three lockdown cases print their `.stdout`, and both report lines match.
- [ ] Spec §6, §8 and the `--target` row say what macOS's lockdown refuses and what it doesn't; §11 row 255; CHANGELOG; TODO.
- [ ] The Draft's 9.2 rule 5 and the ledger's PRT-11 updated with your approval of the wording.
- [ ] `cargo test --lib` and `--test macos`, `lockdown`, `helpers` pass; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.

## CALLS I MADE (for review)

1. **Best effort when `sandbox_init` fails.** The program goes on and the report says why, as on Linux when seccomp or Landlock is missing. The alternative, ending the program, would make a future macOS break every Lugha program at start.
2. **Nothing applied when both kinds are held.** On Linux, `io_uring` is refused always. macOS has no equivalent hole, so an empty profile would only add a private API call for nothing.
3. **Named UNIX sockets refused without `Net`** (by `network*`), matching Linux, where `socket(AF_UNIX)` itself is refused.
4. **The report's wording** says `files not limited` every time, so nobody reads `fs refused` as a kernel promise on macOS.
5. **Allow-by-default profile** with denies, not Chromium's deny-by-default. Only allow-by-default is safe without file rules.
