## FEATURE: Processes on Windows, with never a shell — milestone 267

## OBJECTIVE

`Proc` works on Windows as on Linux and macOS: `run`, `run_with`, `call`, `run_bytes` and `run_bytes_with` start a program and wait for it. The same promises hold:
- **never a shell:** each argument reaches the program exactly as given;
- a bare name is looked up on `PATH`;
- the child gets stdin, stdout and stderr and no other handle;
- the three pipes can't deadlock;
- failures read in Linux's words.

Today every call fails closed with `Err("processes aren't supported on Windows yet")`.

```lugha
fun main(console: Console, proc: Proc) {
    let r = proc.run("whoami", []).unwrap();          // found on PATH as whoami.exe
    console.println(r.code);                          // 0
    let echoed = proc.run("my-tool", ["a b", "c\\\"d", ""]);   // each argument arrives exactly as given
    console.println(proc.run("tool \"x\"", []).is_err());    // true: no program file's name holds `"`
    console.println(proc.run("build.cmd", []).debug());
    // Err("build.cmd: a batch file runs through cmd.exe, a shell, which Proc never starts")
}
```

🔒 **Security-sensitive** (how programs are started): this needs your review before any code.

## CONTEXT

- **Read first:**
  - `runtime/os/posix/process.c`, what's promised and how;
  - the spec's `Proc` paragraph (§6, "`Proc` (milestone 181)");
  - `runtime/os/windows/missing.c`, whose refusal is replaced.
- **Implements:** Draft 9.3 step 5's "processes".
- **Lockdown is not part of this:** PRT-11's Windows answer is its own PRP. `LUGHA_LOCKDOWN=report` keeps saying nothing is applied.
- **Decided with you (discovery):** a `.bat` or `.cmd` file is refused, so `Proc` never starts a shell, with no exception for batch files.
- **Follows:** PRP-295 (milestone 265). **Next:** lockdown on Windows.

## REQUIREMENTS

### Must Do

**The command line.** Windows passes a program one string, which the program splits itself. Nearly every program does that with the Microsoft C runtime's rules, which `CommandLineToArgvW` follows.
- Each argument is quoted by those rules, as Rust's `std::process` and Go's `syscall.EscapeArg` do:
  - quoted when it's empty or holds a space, tab, newline or `"`;
  - a `"` inside becomes `\"`;
  - backslashes are doubled only where a `"` follows them, the closing quote included.
- The program's own name, first, is quoted without escapes, as Windows reads it. A command holding `"` is refused: no program file's name holds one.
- A command line longer than Windows' 32,767 UTF-16 units is refused: `Err("command line is longer than Windows allows")`.
- A NUL byte in the command or an argument is refused, as now.

**Finding the program.**
- A command holding `/`, `\` or `:` is a path, as one holding `/` is on POSIX.
- Any other command is looked up on `PATH`, each folder in order:
  - with its own extension, as given;
  - without one, as `<name>.exe`, then `<name>.com`.
- Never the current folder, and never this program's folder. Windows' own search (`CreateProcessW` with no application name) looks in both before `PATH`, which lets a file dropped there win. The found path is given as `lpApplicationName`, so Windows searches nothing.
- Not found: `Err("tool: No such file or directory")`.

**Batch files are refused.** A found program ending `.bat` or `.cmd`, in any case, gives:
- `Err("build.cmd: a batch file runs through cmd.exe, a shell, which Proc never starts")`;
- the check is on the resolved path, before anything starts. `CreateProcessW` given a batch file would quietly start `cmd.exe` with it;
- naming `cmd.exe` yourself still works, and is then visibly a shell.

**Handles.** The child inherits exactly three handles, named in `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`, as Rust and Go do:
- **`run` and `run_with`:**
  - stdin is a pipe (`run_with`) or `NUL` (`run`);
  - stdout and stderr are pipes;
  - the child starts with `CREATE_NO_WINDOW`, since its output is captured;
- **`call`:** inheritable copies of this program's stdin, stdout and stderr.

Every other handle the runtime holds stays out, even one made inheritable by C code.

**Pumping.**
- stdin is written on the helper thread that started the child;
- stdout and stderr are each read on a short-lived thread of their own;
- a child that fills one pipe while another is being written can't deadlock;
- a child that closes stdin early just ends the input.

**Waiting and exit code.**
- The helper waits for the child even after its task stops waiting, as on POSIX. The handles are closed after.
- The code is `GetExitCodeProcess`'s, as an `i32`: a crash's exception code (`0xC0000005`) comes out negative. Windows has no signals, so there's no `128 + n`.

**Text.** The command and the arguments go as UTF-16 (`CreateProcessW`); the output comes back as bytes, as on POSIX (`run` checks UTF-8).

**Environment and folder:** inherited unchanged, as on POSIX.

**Errors** read in Linux's words:
- `ERROR_FILE_NOT_FOUND` and `ERROR_PATH_NOT_FOUND` → "No such file or directory";
- `ERROR_ACCESS_DENIED` → "Permission denied";
- `ERROR_BAD_EXE_FORMAT` → "Exec format error";
- anything else → "Input/output error".

**Docs and lists:**
- Remove the refusal from `missing.c` and `tests/windows/refusals.la`. `refusals.la` goes entirely: nothing is refused any more but lockdown, which another case covers.
- `tests/windows/known.txt`: the process cases come off it where they now pass on CI. Those that still fail get their real reason, e.g. a Unix tool such as `sh` or `printf` that a Windows `PATH` lacks.

### Must NOT Do

- **No shell, ever:** no `cmd.exe /c`, no batch files, no `ShellExecute`.
- **No search of the current folder, this program's folder or `App Paths`.**
- **No job objects:** a child doesn't die with its parent, as on POSIX. That belongs with lockdown.
- **No lockdown, no timeout, no output limit:** none exists on POSIX either.
- **No change to `Proc`'s interface or to POSIX's code.**

## DESIGN

**Files**

| File | Contents | Size |
| --- | --- | --- |
| `runtime/os/windows/process.c` | `lugha_rt_spawn`: the checks, the job, `CreateProcessW` with the handle list, the pumps and the wait | ≈330 lines |
| `runtime/os/windows/command_line.c` | the quoting, the `PATH` lookup and the batch-file check | ≈150 lines; small functions with no I/O, so its rules are easy to review |

Both join after `missing.c`'s place in the Windows parts. `missing.c` keeps only lockdown.

**The quoting rule, exactly** (one argument, `out` a UTF-8 buffer, made UTF-16 after):

```
needs quotes = empty, or holds ' ', '\t', '\n', '\v' or '"'
if not: copy as is
else: '"', then for each char:
        count a run of backslashes;
        before '"': write 2×run + 1 backslashes, then '"'
        at the end: write 2×run backslashes (the closing quote follows)
        else: write the run as is, then the char
      then '"'
```

**Security notes for review:**
- **Untrusted input:** the command and arguments come from the program. They're checked for NUL bytes, a `"` in the command, length, and batch files, then quoted.
- **No `unsafe` Rust, no new crate.** The C side uses `CreateProcessW`, `InitializeProcThreadAttributeList`, `UpdateProcThreadAttribute`, `CreatePipe`, `DuplicateHandle`, `SetHandleInformation`, `WaitForSingleObject` and `GetExitCodeProcess`.
- **What a reviewer should check hardest:**
  - the quoting function against the rule above;
  - that the batch check runs on the resolved path;
  - that `lpApplicationName` is always the resolved path, never NULL.

## TESTS

- [ ] **`tests/windows/process.la`** (run under Wine by `tests/windows.rs`, and on Windows by CI). The program runs itself (its own path, `args[0]`) as the child; a child echoes what it got, so quoting is checked as a round trip. The cases:
  - arguments with spaces, empty ones, `"`, backslashes before a `"` and at the end, tabs, non-ASCII text, `&|<>^%` (which mean nothing without a shell), and a 1,000-argument list;
  - `run_with`: input piped through, 1 MiB each way at once, so no deadlock;
  - the exit code comes back;
  - stderr is captured apart from stdout;
  - `call` shares the console;
  - a missing program gives "No such file or directory";
  - `x.cmd` and `x.BAT` are refused, the file existing or not;
  - a command holding `"` is refused;
  - a NUL is refused;
  - a too-long command line is refused;
  - a bare name found on `PATH`, `whoami`.
- [ ] **`tests/windows/process_lookup.la`:** a program file put in the current folder isn't found by a bare name. A path with `\` reaches it.
- [ ] **CI's suite:** the process cases on `known.txt` are re-checked; any that now pass come off.
- [ ] **No change on Linux or macOS:** `--test programs`, `--test macos` and the suite on the Mac.

## ACCEPTANCE

- [ ] The new cases pass under Wine and on CI's Windows runner.
- [ ] `cargo fmt --check` and clippy are clean; each C file is at or under 500 lines.
- [ ] Updated:
  - the spec's `Proc` paragraph: Windows' rules (quoting, lookup without the current folder, batch files refused, exit codes as Windows gives them);
  - §11 row 267, CHANGELOG, TODO;
  - CLAUDE.md's stack line ("processes fail closed for now" goes).

## DECIDED HERE (for review)

1. **Microsoft's C runtime rules for quoting,** as Rust and Go use. A program that splits its command line another way (`cmd.exe` itself, some old tools) may see something else. That's the nature of Windows, and the spec says so.
2. **The lookup is our own:** `PATH` only, `.exe` and `.com`, never the current folder or this program's. It's safer than Windows' own search, and the same promise POSIX makes.
3. **Exactly three handles are inherited,** through the handle list, so nothing the runtime or C code holds leaks into a child.
4. **Exit codes as Windows gives them,** crashes negative. There are no signals to add 128 to.
5. **Threads pump stdout and stderr,** not overlapped named pipes. It's simpler, and a run already holds a helper thread for its whole length.
6. **`CREATE_NO_WINDOW` for captured runs,** so a program with no console doesn't flash one open. `call` shares the console.
7. **`refusals.la` goes:** nothing else is refused on Windows but lockdown, which `a_windows_build_reports_lockdown_not_applied` already covers.

## MADE WHILE BUILDING (for review)

8. **The lookup check lives in `process.la`,** not in a `process_lookup.la` of its own. It writes a file `lookup_probe.exe` that isn't a program into the current folder:
   - the bare name `lookup_probe` is "No such file or directory": the folder wasn't searched;
   - the path `.\lookup_probe.exe` is "Exec format error": the file was reached, and isn't a program.
9. **The batch check runs twice:**
   - on the command as written, so `x.cmd` is refused whether or not it exists, with the PRP's message;
   - on the path found, in case a lookup ever finds one.
10. **The command line's length is counted in UTF-16 units,** as Windows counts it. A line of more than 3 × 32,766 UTF-8 bytes is refused without converting it.
