## FEATURE: Process lockdown on Windows — milestone 269

## OBJECTIVE

On Windows, once the runtime has made what `main` asked for, Windows itself limits the whole process to the capability kinds it took. That covers C code and every program it starts, as on Linux and macOS. Two kinds can be limited this way:
- **Without `Proc`:** no new processes. `CreateProcessW` from C code fails with access denied.
- **Without `Fs`:** files are read-only. The process's integrity level drops to Low, so Windows refuses writes to almost every ordinary file and folder (the working folder, the user's profile, `%TEMP%`), while reads still work.

`Net` can't be limited from inside a process on Windows. The report says so, as macOS's report says files aren't limited there.

```
> set LUGHA_LOCKDOWN=report
> hello.exe
lockdown: processes refused, files read-only, network not limited; net refused, proc refused, fs refused
```

🔒 **Security-sensitive** (what the kernel lets a program do): this needs your review before any code.

## CONTEXT

- **Read first:**
  - `runtime/os/darwin/lockdown.c`, the closest model: coarse parts, an honest report;
  - `runtime/os/linux/lockdown.c`;
  - the spec's process-lockdown paragraph (§9);
  - `runtime/os/windows/missing.c`, which this replaces.
- **Resolves PRT-11 for Windows (the Draft's 9.2 rule 5).** What a program promises where the kernel can't lock it down was left to each OS's port. For Windows: processes refused, files read-only, network not limited.
- **Decided with you (discovery):** without `Fs`, the integrity level is lowered to Low.
- **Follows:** PRP-296 (milestone 267).

## RESEARCH (summary)

- **No new processes:**
  - `SetProcessMitigationPolicy(ProcessChildProcessPolicy, NoChildProcessCreation)` applies to the calling process (Windows 10, version 1709 or later). Microsoft's reference doesn't say plainly that it can be set after start, so CI must confirm it.
  - A job object with `JOB_OBJECT_LIMIT_ACTIVE_PROCESS` = 1, the process assigned to it and the job's handle then closed. Browser sandboxes use it. Nested jobs work since Windows 8, so a program already in a job (a terminal's, CI's) can still join one. With no handle left, nothing in the process can raise the limit again.
  - Both are applied. Either alone refuses a child: the job holds where the policy is refused, and the policy holds where the job is.
- **Files read-only:** `SetTokenInformation(TokenIntegrityLevel)` with the Low label lowers the process's own token. Windows allows lowering and never raising. A Low process can't write objects labelled Medium (by default, nearly every file and folder a user has) and can still read them. The few folders labelled Low (`%LOCALAPPDATA%Low`, a browser's cache) stay writable, which the spec says.
- **The network:** a process can't refuse itself sockets.
  - The Windows Filtering Platform needs administrator rights.
  - An AppContainer must be chosen when a process starts, so the program would have to restart itself inside one. Its files would then also be cut off, which a program holding `Fs` can't have.
  - So `Net` stays bound by Lugha's types alone on Windows, and the report says so.

## REQUIREMENTS

### Must Do

- **`runtime/os/windows/lockdown.c`:** `lugha_rt_lockdown(kinds)`, joined where the other systems' lockdown is. `missing.c` goes, now empty.
- **Without `Proc`:** the mitigation policy, then the job. "Processes refused" when at least one applied.
- **Without `Fs`:** the token's integrity level set to Low. "Files read-only" when it applied.
- **Kept as now:**
  - helper threads start only after lockdown (`helpers_allowed`), so they run limited too;
  - nothing is applied for a kind that's held.
- **When Windows refuses a part,** the program goes on unlimited in that part, as on a kernel without seccomp, and the report says why: "processes not refused (Windows refused both: error 87)".
- **`LUGHA_LOCKDOWN=report`** prints one line to stderr:
  `lockdown: <parts>, network not limited; net <held|refused>, proc <held|refused>, fs <held|refused>`.
  - `<parts>` is "processes refused" and/or "files read-only" for the kinds not held, each replaced by its "not …" sentence when Windows refused it;
  - "nothing needed" when both `Proc` and `Fs` are held.
- **Tests and CI:**
  - `tests/windows.rs`: `a_windows_build_reports_lockdown_not_applied` becomes a check of the new line for a program holding everything ("nothing needed, network not limited; net held, proc held, fs held"), which holds under Wine too;
  - CI's Windows job's report check is updated to the new words.
- **Docs:**
  - the spec's lockdown paragraph: Windows' parts, `%LOCALAPPDATA%Low` staying writable, the network not limited;
  - §11 row 269, CHANGELOG, TODO;
  - the ledger's PRT-11 (Windows answered);
  - CLAUDE.md.

### Must NOT Do

- **No AppContainer, no restricted tokens, no restarting itself:** each changes how a program starts and what it can reach well beyond this PRP.
- **No limit on the network,** and no claim of one.
- **Nothing undone:** no part can be lifted later. A process can't raise its own integrity level, and it holds no handle to its job.

## DESIGN

- **Files:** `runtime/os/windows/lockdown.c` (≈160 lines) replaces `missing.c` in `WINDOWS_ONLY`.
- **No new crates, no `unsafe` Rust.** The C side uses:
  - `SetProcessMitigationPolicy`;
  - `CreateJobObjectW`, `SetInformationJobObject`, `AssignProcessToJobObject`;
  - `OpenProcessToken`, `CreateWellKnownSid(WinLowLabelSid)`, `SetTokenInformation`.

  All are in kernel32 and advapi32, which zig's MinGW links. The import check in `tests/windows.rs` gains `advapi32.dll` if it's needed.
- **What a reviewer should check:**
  - the job's handle is closed after assigning, so the limit can't be raised;
  - the integrity level is lowered on the process's own primary token, not a thread's;
  - both happen before any helper thread starts and before the program's code runs.

## TESTS

- [ ] **`tests/windows/lockdown_bare/`** (C through `[link]`, run by CI on Windows): no `Proc` and no `Fs`, and C code tries to:
  - start `whoami.exe` (refused);
  - create a file in the working folder (refused);
  - read a file that exists (works).
- [ ] **`tests/windows/lockdown_held/`:** holds `Proc` and `Fs`, so the same C code starts it, writes and reads.
- [ ] **`tests/windows/lockdown_proc_only/`:** holds `Proc`, not `Fs`. A program it starts can't write either: the Low level is inherited.
- [ ] **Report lines:**
  - CI checks a bare program's ("processes refused, files read-only, network not limited; net refused, proc refused, fs refused");
  - under Wine, which may implement none of this, only the held program's line is checked. The three cases go on `WINE_CANT` with why.
- [ ] **Unchanged on Linux and macOS:** their lockdown tests.

## ACCEPTANCE

- [ ] CI's Windows job: the three cases pass, the report lines match, and the program suite still passes. A case reading a file but not writing one must not be broken by the Low level.
- [ ] `cargo fmt --check`, clippy and the touched tests are clean.

## DECIDED HERE (for review)

1. **Both process mechanisms, not one.** The mitigation policy's runtime use is less documented, and the job is proven, so each covers the other's gap. The report counts it applied if either is.
2. **Low integrity for no `Fs`** (your call). Reads stay, as Linux's Landlock allows reading for a program holding `Proc`. The spec states the Low-labelled folders that stay writable.
3. **The network is named as not limited,** in the report and the spec. No pretending.
4. **One report line in the same shape as Linux's and macOS's,** so a script reading it reads all three.
5. **`missing.c` goes.** Nothing on Windows fails closed any more, and lockdown gets a file named for what it does.

## MADE WHILE BUILDING (for review)

6. **Only two cases carry C.** `lockdown_bare/` and `lockdown_held/` each hold a copy of the same `probe.c`, since a `[link]` path can't use `..`. `lockdown_proc_only.la` needs no C: it starts `cmd.exe` through `Proc`, and `type nul> made.txt` fails (exit 1) at Low.
7. **CI's report check uses `lockdown_held`** for the "nothing needed" line. `hello` holds no capability at all, so its line is the bare one.
8. **The report gives both errors** when Windows refuses both process parts: "processes not refused (Windows refused both: error P, J)".
9. **`advapi32.dll` joins the imports allowed** (the token calls). Every Windows ships it.
10. **Wine accepts every part and enforces none.** Its report reads "processes refused, files read-only", but a child still starts and files are still written. So the Wine test checks only the parts of the line that hold either way, and the bare and proc-only cases are on `WINE_CANT`.
