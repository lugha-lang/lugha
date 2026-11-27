## FEATURE: No `.pdb` beside a Windows build — milestone 276

## OBJECTIVE

A Windows build writes one file, the `.exe`, as every other target writes one executable.

```sh
lughac build app.la --target x86_64-pc-windows-gnu
# Today:  app.exe (531 KB) and app.pdb (2.4 MB)
# After:  app.exe (359 KB)
```

## CONTEXT

- `src/link/zig.rs` (`link_args`): what zig's link takes for each target.
- zig links a Windows executable with `-DEBUG -PDB:<stem>.pdb` unless told to strip (`-s`), even with `-g0`. That `.pdb`, and the debug info in the `.exe` that points at it, describe only zig's MinGW C runtime, compiler-rt and libunwind: **lughac emits no debug information for Lugha code on any target**, and has no flag asking for it.
- TODO: "Windows `.pdb` files", left as the user's call.

## REQUIREMENTS

### Must Do

- Every Windows link (x86-64 and ARM64) passes `-s` to zig: no `.pdb` is written and the `.exe` carries no debug sections.
- Unwinding data stays: `.pdata` and `.xdata` aren't debug information, so panics, task unwinding and SEH work as before.
- Tests:
  - unit: `link_args` for both Windows triples has `-s`, and no other target has it;
  - `tests/windows.rs` and `tests/windows_arm64.rs`: a build leaves no `<stem>.pdb` beside its `.exe`;
  - `--test windows` under Wine unchanged (the panic, unwinding and C-overflow cases included).
- Docs: the spec's `--target` row (one file), §11 row 276, CHANGELOG, TODO (the item done).

### Must NOT Do

- No flag to keep debug information: there is none to keep for Lugha code. A `-g` that emits it for every target is a feature of its own.
- No change on Linux or macOS.

## DECIDED HERE (for review)

1. **Strip, rather than keep the `.pdb`:** it describes only zig's C libraries, never the program, so it helps nobody debug a Lugha program, and it is 2.4 MB per build. `-s` also drops 170 KB of debug sections from the `.exe`.
2. **Always, with no flag,** until lughac emits debug information for Lugha code; then it is decided with that feature, for every target at once.

## MADE WHILE RESEARCHING

3. With `-s` linked in, `cargo test --test windows` passes under Wine (7 of 7), and the TLS trust test with Windows' OpenSSL passes. Measured on `tests/windows/unwinding.la`: 531 KB + a 2.4 MB `.pdb` before, 359 KB and no `.pdb` after.
