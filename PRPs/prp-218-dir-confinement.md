## FEATURE: `Dir` confinement: every `Dir` reaches only its folder and below, enforced by the kernel (`openat2` with `RESOLVE_BENEATH`); `fs.dir(path)` and `dir.dir(sub)` narrow, `dir.read_only()` drops writing; milestone 186, in v18's Phase 2. **Changes how files are opened in the C runtime: needs the user's security review before any code.**

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
fun main(console: Console, fs: Fs): i32 {
    let here = fs.cwd();                                        // the working folder and below
    console.println(here.read_file("../outside.txt").debug());  // Err("path leads outside the folder")
    console.println(here.read_file("/etc/hostname").debug());   // Err("path leads outside the folder")
    let etc = fs.dir("/etc").unwrap();                          // the Fs reaches anywhere
    console.println(etc.read_file("hostname").is_ok());         // true
    let data = here.dir("data").unwrap();                       // narrower: data/ and below
    let w = data.write_file("note.txt", "hi");
    let ro = data.read_only();
    console.println(ro.read_file("note.txt").debug());          // Ok("hi")
    console.println(ro.write_file("note.txt", "x").debug());    // Err("the folder is read-only")
    0
}
```
A symbolic link inside `data/` pointing at `/etc/passwd` reads as `Err("path leads outside the folder")`, and so does a folder swapped for a link while a path is being opened: the kernel checks every step.

## CONTEXT
- Draft v18 §2.1 rules 6 and 11 [CAP-06, CAP-11], §2.2 rule 1 [SEC-01]: `fs.dir(path)` gives a `Dir`, "that folder and below, never above it"; `dir.read_only()` the same folder, reading only; narrowing is one-way. "A `Dir` can't reach outside its folder through `..`, symbolic links or a folder swapped mid-operation: on Linux every path is opened with `openat2` and `RESOLVE_BENEATH` (Linux 5.6 and later)." SEC-01 is approved in the ledger.
- Decided with the user: **every `Dir` is confined, `fs.cwd()`'s too**; reaching elsewhere goes through the `Fs`: `fs.dir("/etc")`, `fs.dir("..")`.
- Today: `$Dir` (prelude) calls path-based runtime functions (`runtime/input.c`'s `read_file`/`write_file`, `bytes.c`'s `read_bytes`/`write_bytes`, `fs.c`'s `list_dir`, `make_dir`, `remove_*`, `stat_kind`, `file_size`, `rename`, `chmod`, `sync`, `file.c`'s `file_open`), each resolving the path from the working directory with no limit. The test runner's `Dir` (PRP-217) checks the path's text only.
- Measured: two programs in the repository read an absolute path through `fs.cwd()` (`/proc/self/status`, `/proc/self/cmdline`), and one lists `/proc/self/fd`; they move to `fs.dir("/proc/self")`.
- Verified here: `/usr/include/linux/openat2.h` (struct `open_how`, `RESOLVE_BENEATH`, `RESOLVE_NO_MAGICLINKS`) and `SYS_openat2` on x86-64 and ARM64; this machine runs Linux 7.0.

## DESIGN — for the security review

### A `Dir` is a directory file descriptor
A `Dir` holds a handle to a small collected C object `{ int fd; }`: an `O_PATH | O_DIRECTORY | O_CLOEXEC` descriptor of its folder, closed by a collector finalizer when the last `Dir` value for it is gone (as `std.crypto`'s hashers are freed). Copies share it. Opening one is the only time the folder's own path is looked up; afterwards renaming or moving the folder doesn't change what the `Dir` reaches.

### Every path goes through one resolver
`dir_resolve(dirfd, path, flags, mode)` = `syscall(SYS_openat2, dirfd, path, &how, sizeof how)` with `how.resolve = RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS`. The kernel refuses an absolute path, a `..` above the folder, and a symbolic link (anywhere in the path, the last part included) whose target leaves it, checked at each step, so a swap mid-open is caught. `EAGAIN` (a rename racing the lookup) is retried up to 8 times. `EXDEV` becomes `Err("path leads outside the folder")`. An empty path is refused; a path with a NUL byte is refused as today.

Each operation, by what it needs:

| Operation | How |
| --- | --- |
| `read_file`, `read_bytes`, `open` | `dir_resolve(path, O_RDONLY)`, then read as today |
| `write_*`, `append_*`, `create`, `append` | `dir_resolve(path, O_WRONLY \| O_CREAT \| O_TRUNC` or `O_APPEND, 0666)` |
| `exists`, `is_dir`, `is_file`, `size` | `dir_resolve(path, O_PATH)`, then `fstat` |
| `list` | `dir_resolve(path, O_RDONLY \| O_DIRECTORY)`, then `fdopendir`/`readdir` |
| `sync` | `dir_resolve(path, O_RDONLY)`, then `fsync` |
| `set_mode` | `dir_resolve(path, O_PATH)`, then `fchmodat(fd, "", mode, AT_EMPTY_PATH)` on Linux 6.6+ (`fchmodat2`), else `chmod("/proc/self/fd/N")` on the resolved descriptor: either changes the file the kernel resolved inside the folder, never a link's target outside it |
| `make_dir`, `remove_file`, `remove_dir` | the path split at its last `/`: the parent resolved with `dir_resolve(parent, O_PATH \| O_DIRECTORY)` (`.` when there is none), then `mkdirat`, `unlinkat` or `unlinkat(…, AT_REMOVEDIR)` on the last part. The last part may not be empty, `.` or `..`. A last part that is a symbolic link is removed itself, never followed |
| `rename(from, to)` | both parents resolved as above, then `renameat` of the two last parts |
| `dir(sub)` | `dir_resolve(sub, O_PATH \| O_DIRECTORY)`: a new, narrower `Dir` |
| `path()` | `readlink("/proc/self/fd/N")` of the `Dir`'s descriptor: where the folder is now |

`fs.cwd()` opens `"."` (`O_PATH | O_DIRECTORY`) at the call; `fs.dir(path)` opens `path` relative to the working directory with ordinary resolution: the `Fs` is the whole file system, and only what comes out of it is confined.

**No `openat2`** (a kernel older than 5.6, or a sandbox that blocks the call with `ENOSYS`/`EPERM`): every `Dir` operation fails closed with `Err("this system can't confine a folder: openat2 needs Linux 5.6")`; nothing falls back to unconfined paths.

### `read_only()` and narrowing
`read_only()` gives the same folder with a flag; every method that writes, makes, removes, renames, sets a mode or opens for writing returns `Err("the folder is read-only")` before any system call, checked in Lugha. `dir(sub)` of a read-only `Dir` is read-only. There is no way back to writing (one-way, CAP-06). This binds Lugha code; C code with `ffi` can still do anything (SEC-05's lockdown is a later PRP).

### What changes in the runtime
- New `runtime/dir.c` (≤ 500 lines): the handle and finalizer, `dir_resolve`, and one function per operation above, all taking the `Dir` handle and a path. The existing path-based functions stay for `Fs`-level use only where still needed (`fs.dir`), and the ones only `$Dir` called are removed.
- `src/check/float_ops.rs`, `src/codegen/os.rs`: the new `$os.dir_*` built-ins; `src/link/mod.rs`'s runtime symbol list updated.
- The prelude: `$Dir { handle, read_only }` over the new built-ins; `Fs` gains `dir(path)`; `Dir` gains `dir(sub)` and `read_only()`.
- The test runner's `Dir` (PRP-217) becomes `fs.dir(LUGHA_TEST_DIR)`: kernel-confined, replacing the text check.

### For the security review, in one list
1. Every `Dir` path through `openat2` + `RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS`; `EAGAIN` retried 8 times; no fallback when `openat2` is missing (fail closed).
2. Operations on a last path part (`mkdirat`, `unlinkat`, `renameat`) only after the parent is resolved beneath; `.`, `..` and empty last parts refused; links removed, not followed.
3. `set_mode` on the resolved descriptor (`fchmodat2` with `AT_EMPTY_PATH`, else `/proc/self/fd/N`).
4. Directory descriptors in collected objects, closed by a finalizer; `O_CLOEXEC` everywhere, so a started program inherits none.
5. `fs.cwd()` confined; `fs.dir(path)` the only unconfined lookup.
6. `read_only()` enforced in Lugha, not by the kernel.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Error messages:** `"path leads outside the folder"` (`EXDEV`, an absolute path), `"the folder is read-only"`, the openat2 message above; every other failure is the system's message, as today.
2. **`RESOLVE_NO_MAGICLINKS` too:** `/proc`-style links (`/proc/self/fd/3`) inside a folder would otherwise lead anywhere.
3. **`dir.dir(sub)` and `fs.dir(path)` return `Result<Dir, string>`**: the folder may not exist.
4. **Mount points are crossed** (no `RESOLVE_NO_XDEV`): a folder with a mount inside it still reaches what is mounted there, as `ls` would.
5. **`path()` follows the folder** if it is moved: it names where the descriptor's folder is now.

## REQUIREMENTS

### Must Do
- C: `runtime/dir.c` as above; the old path functions only `$Dir` used, removed; link list, checker and codegen tables updated.
- Prelude: `Fs.dir`, `Dir.dir`, `Dir.read_only`, `$Dir` over descriptors; the test `Dir` kernel-confined.
- Migrate the repository's absolute and `..` paths through `fs.cwd()`.
- Spec (`Fs`/`Dir` paragraph, "Folders and files", the testing section, §11 row 186, Changes, Breaking), CHANGELOG.

### Must NOT Do
- Process lockdown (seccomp, Landlock), the `ffi` permission, `net.only`: their own PRPs.
- Kernel enforcement of `read_only()` (a read-only descriptor can't stop writes through it from C's view anyway; Lugha code is bound).
- Any platform but Linux.

## TESTS
- [ ] `tests/programs/capabilities/dir_confinement/` (scratch folder): the objective; `..`, absolute paths, a symbolic link to outside (file and folder), a link inside to inside (allowed), `remove_file` on a link removing the link only, `rename` across subfolders, `make_dir`/`remove_dir` with `.`/`..` last parts refused, `set_mode` through a link inside, `dir(sub)` narrowing, `read_only()` refusing each writing method, `path()`.
- [ ] A swap race: a test program that renames a folder to a link pointing outside while another green thread opens paths through it in a loop, never reading outside (`tests/dir_race.rs`, bounded run).
- [ ] The test runner's `Dir` refuses a link made inside it that points out.
- [ ] Full `cargo test`.

## ACCEPTANCE
- [ ] The objective prints as commented and exits 0.
- [ ] No `Dir` method reads, writes, lists, stats, renames, removes or changes anything outside its folder, by any path or link.
- [ ] `cargo test` passes; `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated where they change
