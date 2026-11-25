## FEATURE: Hello world on Windows: `--target=x86_64-pc-windows-gnu` builds a program Windows runs — milestone 259

## OBJECTIVE

The first, smallest step of portability step 5 (Draft 9.3): prove the whole path to Windows before building the rest of the Windows runtime, as milestone 251 did for the Mac.

```bash
# on Linux, with zig 0.17
$ lughac build hello.la --target=x86_64-pc-windows-gnu      # writes hello.exe
$ file hello.exe
hello.exe: PE32+ executable for MS Windows 6.00 (console), x86-64
# on Windows 10 or newer (CI's Windows runner), or here under Wine
> hello.exe
hi
```

What works in this step:
- the console (out, err and in, UTF-8 both ways), arguments, the environment, the exit code;
- the clock, randomness, tasks on every CPU, channels, timers, panics that unwind, and stack overflow reported as on Linux.

What this step does **not** promise, each stated where a program meets it:
- **Folders** (`Dir`, `Fs`): every operation is `Err("folders aren't supported on Windows yet")`, failing closed, as on the Mac until its confinement came.
- **Sockets** (`Net`, TCP, UDP) and **processes** (`Proc`): `Err("sockets aren't supported on Windows yet")` and `Err("processes aren't supported on Windows yet")`.
- **Process lockdown**: not applied; `LUGHA_LOCKDOWN=report` says so. Lugha's own capability checks hold.
- **`extern struct`s passed to C by value**: refused at build time (E0337) until Windows x64's rules for them land; scalars and pointers work.
- `std.tls`/`std.crypto` (E0334, as for any foreign target without an OpenSSL), ARM64 Windows, and `lughac run`/`test` for a Windows target.

## CONTEXT

- **Read first:**
  - PRP-285 (the Mac's first step), the layout this one follows;
  - `runtime/os/layer.c`, `runtime/os/linux/os.c`, `runtime/os/linux/x86_64.c`, `runtime/os/darwin/os.c`;
  - `runtime/tasks/stacks.c`, `runtime/tasks/stack.c`, `runtime/tasks/unwind.c`;
  - `src/link/zig.rs`, `src/link/runtime_parts.rs`, `src/codegen/targets.rs`, `src/codegen/unwind.rs`, `src/codegen/c_calls.rs`, `src/driver/emulate.rs`, `src/driver/pipeline.rs` (`default_output`);
  - `.github/workflows/ci.yml` (the `macos` job, the pattern for a `windows` job).
- **Implements:** v18 Draft, Part 9, step 5's first part; 9.2 rule 3 (`WaitOnAddress`) and rule 4 (the task switch also switches the thread's recorded stack bounds; stack overflow through a vectored exception handler) [PRT-10, PRT-14]. PRT-11's interim answer for folders and lockdown follows your Mac decision (fail closed, not applied), not its final one.
- **Decisions:** PRT-02, PRT-06, PRT-10, PRT-14 (approved); PRT-11 and PRT-13 (the Windows socket mechanism) stay deferred to later Windows PRPs.
- **Follows:** PRP-291 (milestone 258). **Next:** Windows folders, then sockets, processes, lockdown, the whole suite.

## RESEARCH (probed here with zig 0.17.1 and LLVM 21)

- **zig builds Windows executables:**
  - `zig cc -target x86_64-windows-gnu` links a C program using pthreads, `WaitOnAddress`, `VirtualAlloc`, a thread-local and `_Unwind_Backtrace` into a PE32+ console executable. `aarch64-windows-gnu` works too.
  - It imports only `KERNEL32`, `ntdll`, the `api-ms-win-core-synch` set and the Universal C Runtime (`api-ms-win-crt-*`, part of Windows 10 and later). winpthreads and libunwind are linked in, so nothing needs installing.
  - Two link flags are needed: `WaitOnAddress` needs `-lapi-ms-win-core-synch-l1-2-0`, and panics need zig's `-lunwind`, as on musl.
- **Unwinding:**
  - On Windows x64, LLVM writes the personality function into each function's SEH table (`.seh_handler`), so it must be the SEH form.
  - zig provides `__gcc_personality_seh0` and `_Unwind_ForcedUnwind` for Windows, but not `__gcc_personality_v0`, which Lugha uses elsewhere.
  - So on Windows codegen names `__gcc_personality_seh0`, and the runtime's panic unwinding (`tasks/unwind.c`) links as it is.
- **Task stacks:**
  - A task's 8 MB stack is reserved without committing memory on POSIX (`MAP_NORESERVE`). Windows charges committed memory up front, so 10,000 tasks would need 80 GB.
  - Windows' own fibers and Boost.Context reserve each stack, commit only the top pages, and put a guard page below; the system commits more as the stack grows.
  - That works only when the thread's recorded stack bounds are the running task's: `StackBase`, `StackLimit` and `DeallocationStack` in the TEB.
  - Boost.Context's Windows x64 switch (`jump_x86_64_ms_pe`) saves and restores those three fields with the registers. The Draft's rule 9.2.4 asks for exactly this.
  - The same bounds let SEH unwinding walk a task's stack: unwinding checks each frame lies within them.
- **Windows x64's calling rules:**
  - LLVM applies them by itself for scalars and pointers. Calls into the runtime pass only those.
  - Only `extern struct`s passed to C by value use the System V rules lughac lowers by hand (`c_abi.rs`). So they wait for their own PRP, refused until then.
- **`long` is 32 bits on Windows:** no runtime function codegen calls takes or returns one.
- **The console:**
  - Rust and Go write to a console as UTF-16 (`WriteConsoleW`), so any text shows correctly whatever the console's code page. To a file or a pipe they write the bytes as they are.
  - Both write `\n` as `\n`, never `\r\n`.
- **Randomness:**
  - Rust (1.78) and Go (1.22) use `ProcessPrng` from `bcryptprimitives.dll`, which can't fail.
  - `BCryptGenRandom` can fail in some sandboxes.
- **Shared runtime code:** compiled for Windows with Windows' headers in place of POSIX's, it has 77 errors:
  - task stacks: `mmap`, `mprotect`, `madvise`;
  - faults: `sigaction`, `sigaltstack`;
  - `flockfile`, `getline`, `localtime_r`;
  - `fcntl` and process-spawn types.

  Each moves behind the OS layer.
- **Wine** runs these executables on Linux. Tests use it as `tests/arm.rs` uses qemu; CI also runs them on a real Windows runner.

## REQUIREMENTS

### Must Do

1. **The target.** `--target=x86_64-pc-windows-gnu` on `build`.
   - Codegen uses LLVM's own triple, so objects are COFF.
   - zig's name for it is `x86_64-windows-gnu`.
   - The link adds `-lunwind` and `-lapi-ms-win-core-synch-l1-2-0`, plus the import library for `ProcessPrng`.
   - Without `-o`, the executable is `<stem>.exe`; an explicit `-o` is kept as given.
   - `run` and `test` refuse a Windows target (exit 2: "a program built for x86_64-pc-windows-gnu runs only on Windows: build it with `lughac build`").
2. **Codegen.**
   - On Windows the personality is `__gcc_personality_seh0`.
   - An `extern` function taking or returning an `extern struct` by value is **E0337** on a Windows target, with a page in `docs/errors/E0337.md`.
3. **Parts per system.**
   - `System::Windows` joins `os/windows/*` and the shared parts, and leaves out `os/posix/*`.
   - The core's POSIX `#include`s move into `os/posix/`'s parts and each system's `os.c`, so a Windows join includes none.
4. **The layer, widened** (the 77 errors). Each item moves out of shared code into the OS layer:
   - stack reserve, commit and release (`stacks.c`);
   - the fault and stack-overflow handlers (`stack.c`);
   - locking a C stream (`flockfile`);
   - reading a line (`getline`);
   - local time (`localtime_r`);
   - the close-on-exec and process-spawn details.

   Linux and macOS keep exactly what they do today, moved into `os/posix/` or their `os.c`.
5. **`os/windows/os.c`**, the layer on Windows:
   - **Console:** `WriteConsoleW` when the handle is a console, `WriteFile` otherwise; reading through `ReadConsoleW` or `ReadFile`; `\n` written as is.
   - **Arguments and environment:** `GetCommandLineW` and `CommandLineToArgvW`, and `GetEnvironmentStringsW`, all converted to UTF-8.
   - **Clock:** `QueryPerformanceCounter` (monotonic) and `GetSystemTimePreciseAsFileTime`.
   - **Randomness:** `ProcessPrng`.
   - **CPUs:** `GetActiveProcessorCount(ALL_PROCESSOR_GROUPS)`.
   - **Parking:** `WaitOnAddress`, `WakeByAddressSingle` and `WakeByAddressAll`.
   - **The running thread's worker:** a pthread key, as on the Mac.
   - **The poller:** an I/O completion port, woken by `PostQueuedCompletionStatus` for helper jobs and the signals. It's the port sockets will complete to later.
   - **Ctrl-C:** `SetConsoleCtrlHandler` gives the interrupt signal.
   - **Stack overflow and faults:** a vectored exception handler turns `EXCEPTION_STACK_OVERFLOW` and an access violation into the same report Linux gives.
6. **`os/windows/x86_64.c`**, the task switch for Windows x64:
   - It saves and restores the callee-saved registers (`rbx`, `rbp`, `rdi`, `rsi`, `r12`–`r15`, `xmm6`–`xmm15`), MXCSR and the x87 control word.
   - It also switches the TEB's `StackBase`, `StackLimit` and `DeallocationStack`.
   - A new task's first frame has SEH unwind data that ends unwinding there.
   - A task's stack is reserved at its full size, with the top pages committed and a guard page below them.
7. **Not yet:** `os/windows/` gives every folder, socket, UDP and process function lughac's codegen calls, each returning the error above. `lugha_rt_lockdown` applies nothing, and reports `lockdown: not applied on Windows; …` under `LUGHA_LOCKDOWN=report`.
8. **Tests.**
   - `tests/windows/*.la` with `.stdout` files: hello, Unicode output, arguments and the environment, an exit code, tasks across workers and channels, a timer, a panic unwinding through `defer`, stack overflow, and the refusals.
   - `tests/windows.rs` builds them with zig and runs them under `wine` when it's on `PATH`, skipping without it, as qemu's tests do.
   - It also checks the refusals at build time (`run`/`test`, E0337) and that the executable imports only system DLLs.
9. **CI.**
   - The build job builds `tests/windows/*.la` for Windows and uploads them.
   - A `windows` job on GitHub's `windows-latest` runner runs each and compares its output, as the `macos` job does.
10. **Docs:**
    - the spec: the `--target` row, E0337's row, and §11 row 259;
    - CHANGELOG, TODO (step 5's remaining parts), CLAUDE.md (targets and commands).

### Must NOT Do

- No change to what Linux or macOS programs do: moved code only.
- No Windows sockets, folders, processes or lockdown beyond the refusals: each gets its own PRP, and folders and lockdown get your security review (PRT-11).
- No `lughac run`/`test` under Wine: Wine is for the test files only, so `src/driver/emulate.rs` gains only the refusal.
- No ARM64 Windows yet.
- No new crate.

## DESIGN

**Threads stay pthreads.** The runtime's threads, mutexes, condition variables and keys stay pthread calls, served by the winpthreads zig links in. Moving them behind the layer would touch every part of the runtime for no gain in this step. If winpthreads ever falls short, that move becomes its own PRP.

**The console:**
- A program's text is UTF-8 everywhere. To a Windows console it's converted to UTF-16 for `WriteConsoleW`, so it shows correctly whatever the console's code page.
- To a file or pipe the bytes go out unchanged, and `\n` stays `\n`, so a Windows program's output matches Linux's byte for byte, as Rust's and Go's do.
- Console input is read as UTF-16 and converted to UTF-8.

**The output name.** `lughac build app.la` writes `app.exe` for Windows, since Windows runs only names ending in `.exe`. An explicit `-o` is kept as given, as Go and Rust keep it.

**E0337**, "an `extern struct` passed by value to C on Windows", is a diagnostic at the `extern` declaration. Without it the program would build and pass the struct by the wrong rules: a silent memory error.

**Security:**
- Nothing here widens what a program may do: everything not built yet fails closed.
- `emulate.rs` only gains a refusal.
- The Wine tests run programs lughac built from test sources, as the qemu tests do.
- Lockdown is reported as not applied, never claimed.

## TESTS

- [ ] **Unit:**
  - the target's triple, zig's name, `System::Windows`, the link flags, and the default `.exe` name;
  - the personality chosen per target;
  - E0337 on Windows and not elsewhere;
  - the Windows join holds no `os/posix/` or `os/linux/` part, and every Linux and Mac join is unchanged (the existing join tests).
- [ ] **`tests/windows.rs`:** every `tests/windows/` case under Wine; `run`/`test` refused; E0337; only system DLLs imported.
- [ ] **CI:** the `windows` job runs every case on a real Windows runner.
- [ ] **Unchanged on Linux:** `cargo test --lib`, `--test programs`, `--test macos`, `--test macos_suite`, `--test arm`, `--test musl`, `--test pinned`, `--test workers` and `--test spawn_stress` pass. The macOS runtime still compiles with zig without warnings.

## ACCEPTANCE

- [ ] CI's `windows` job passes every `tests/windows/` case; the Mac and Linux jobs pass unchanged.
- [ ] Spec: the `--target` row, E0337, and §11 row 259.
- [ ] CHANGELOG, TODO and CLAUDE.md are updated.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.

## CALLS I MADE (for review)

1. **x86-64 first**, the triple `x86_64-pc-windows-gnu` (MinGW, as the Draft names). ARM64 Windows comes later; zig builds it already.
2. **Fail closed** for folders, sockets and processes; **lockdown not applied** and reported. This is your Mac decision, applied again.
3. **E0337** for `extern struct`s by value on Windows, until Windows x64's struct rules get their own PRP.
4. **Task stacks grow through a guard page**, with the TEB's bounds switched, as Windows' fibers and Boost.Context do. The alternative, committing all 8 MB per task, makes many tasks impossible.
5. **winpthreads** for threads, not a move of threads behind the layer.
6. **UTF-16 to a console, bytes to anything else, `\n` kept as `\n`**, as Rust and Go do.
7. **`ProcessPrng`** for randomness, as Rust and Go use.
8. **An I/O completion port as the poller** from the start, so sockets later complete to the same place.
9. **`.exe` added only to the default name**; `-o` kept as given.
10. **Wine for the tests only**, never behind `lughac run`.
