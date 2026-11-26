## FEATURE: Folders on Windows, confined by walking each path one name at a time — milestone 260

## OBJECTIVE

Every `Dir` works on Windows and is confined as on Linux and macOS: `..` above the folder, an absolute path and a link leading out are `Err("path leads outside the folder")`; links that stay inside work (SEC-01, PRT-11). Today every folder operation on Windows fails closed ("folders aren't supported on Windows yet").

```lugha
fun main(console: Console, fs: Fs) {
    let here = fs.cwd();
    here.make_dirs("box/inner").unwrap();
    here.write_file("box/inner/f.txt", "inside").unwrap();
    let box = here.dir("box").unwrap();
    console.println(box.read_file("inner/f.txt").debug());      // Ok("inside")
    console.println(box.read_file("..\\secret.txt").debug());   // Err("path leads outside the folder")
    console.println(box.read_file("C:\\Windows\\win.ini").debug()); // Err("path leads outside the folder")
}
```

Windows has no `openat2`. The runtime walks each path itself, as it does on macOS and as Go's `os.Root` does on Windows:
- one `NtCreateFile` per name, relative to the folder handle already reached, never following a link in the kernel;
- links read and followed by the runtime;
- `..` never asked of the system.

Every change (make, remove, rename) acts on a handle reached by that walk, never on a full path. 🔒 Security-sensitive (folder confinement): it needs your review before code.

## CONTEXT

- **Read first:**
  - `runtime/os/darwin/dir_resolve.c`, the walk this mirrors;
  - `runtime/os/posix/dir.c` and `dir_names.c`, what's promised;
  - `runtime/os/windows/missing.c`, whose folder refusals are replaced;
  - `runtime/io/file.c`, the `File` streams over a descriptor;
  - `tests/macos/confined.la`, `tests/macos/folders.la`, `tests/programs/capabilities/dir_confinement/`.
- **Spec:**
  - §6 Confinement (line ~1403), which names `openat2` and macOS's walk;
  - the `Dir` table (line ~1205): `set_mode`, `sync`, `list`, `rename`.
- **Implements:** v18 Draft 9.3 step 5's next part; PRT-11's answer for Windows folders (lockdown stays deferred).
- **Follows:** PRP-292 (milestone 259). **Next:** sockets through the completion port.

## RESEARCH (summary)

**Go's `os.Root` on Windows** (`os/root_windows.go`, `root_openat.go`, `internal/syscall/windows/at_windows.go`):
- **How it opens.** It walks name by name. Each name is opened with `NtCreateFile` and `OBJECT_ATTRIBUTES.RootDirectory` set to the parent handle, with `OBJ_CASE_INSENSITIVE` and `OBJ_DONT_REPARSE`. Create options include `FILE_OPEN_FOR_BACKUP_INTENT`, so folders open as handles.
- **Links.** On `STATUS_REPARSE_POINT_ENCOUNTERED` it reopens with `FILE_OPEN_REPARSE_POINT` and reads the data (`FSCTL_GET_REPARSE_POINT`).
  - Only symlinks and junctions count as links, at most 8 per lookup; any other reparse point fails.
  - Absolute targets are refused. A junction's target is always absolute (`\??\C:\…`), so every junction is refused.
- **`..`** restarts the walk from the root: "dir may have moved since we opened it".
- **Names refused outright:**
  - a leading separator;
  - any `:` (drive letters, drive-relative paths, alternate data streams: "conservative but safe");
  - any `?` (which covers `\\?\`);
  - reserved device names: CON, PRN, AUX, NUL, COM1–9, LPT1–9, their superscript forms, CONIN$ and CONOUT$, also with an extension or trailing spaces.
- **Changes** are made from handles:
  - make: `NtCreateFile(FILE_CREATE, FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT)`;
  - remove: `FileDispositionInformationEx` with POSIX semantics;
  - rename: `FILE_RENAME_INFORMATION_EX` with `RootDirectory` set to the target's folder.
- **Old Windows.** `OBJ_DONT_REPARSE` is rejected on Windows 10 1507 (Go issue 78131), so Go falls back there to opening with `FILE_OPEN_REPARSE_POINT` and checking the handle's attributes.

**cap-std** (`cap-primitives/src/windows/fs`):
- It walks the same way, but makes changes through full paths (`GetFinalPathNameByHandle` plus the name), protected only by share modes. Its own note calls that a weakness.
- CVE-2024-51756: it missed the superscript device names (`COM¹`), fixed in 3.4.1.

**Other ways out on Windows:**
- **Junctions** need no privilege to create. They're refused here, as by Go.
- **Symlinks** need administrator rights or Developer Mode to create.
- **Hard links:** no path walk can see one, which is also true of `openat2`.
- **8.3 short names** are aliases of names inside the folder, not ways out.
- **Alternate data streams and drive-relative paths:** both need `:`, which is refused.

## REQUIREMENTS

### Must Do

1. **Paths.**
   - `/` and `\` both separate names.
   - These are `Err("path leads outside the folder")`, as an absolute path is on Linux: a path beginning with either separator, any path holding `:`, and any `?`.
   - These are `Err("path holds a name Windows reserves: `<name>`")`:
     - a name holding `*`, `"`, `<`, `>`, `|` or a control character;
     - a name ending in `.` or a space (other than `.` and `..`);
     - a reserved device name, also with an extension, or with trailing spaces or dots.
   - Empty names (`a//b`) and `.` are skipped, as elsewhere.
2. **The walk** (`os/windows/dir_resolve.c`), the same algorithm and limits as macOS's:
   - **Opening a name.** Each name is opened alone with `NtCreateFile`, relative to the folder handle reached so far. It uses `OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE` and `FILE_OPEN_FOR_BACKUP_INTENT`, sharing read, write and delete.
   - **A reparse point.** When the open is refused with `STATUS_REPARSE_POINT_ENCOUNTERED`, the name is opened again with `FILE_OPEN_REPARSE_POINT` and its reparse data read:
     - a symlink with a relative target is spliced into what's left and walked the same way;
     - a symlink with an absolute target, and any junction, leads outside;
     - any other reparse point (a OneDrive placeholder, a deduplicated file) is `Err("can't follow a reparse point that isn't a link")`, as in Go.
   - **`..`** only undoes a name the walk went down through, by starting again from the `Dir`'s handle with the names before it. It never asks the system for `..`.
   - **Limits:** at most 40 links and 4096 bytes per lookup, then `Err("too many links")` (Linux's `ELOOP` words).
   - **Fallback.** Where `OBJ_DONT_REPARSE` is refused (`STATUS_INVALID_PARAMETER`: Windows 10 1507), each name is opened with `FILE_OPEN_REPARSE_POINT` instead, and its attributes checked for `FILE_ATTRIBUTE_REPARSE_POINT`. The results are the same.
3. **Every `Dir` operation** (`os/windows/dir.c`, `dir_names.c`), each on a helper thread as on POSIX:

   | Operation | On Windows |
   | --- | --- |
   | `read_file`, `read_bytes` | open the resolved file and read it |
   | `write_file`, `write_bytes` | create or truncate (`FILE_OVERWRITE_IF`) |
   | `append_file`, `append_bytes` | `FILE_APPEND_DATA` only, so each write lands at the end |
   | `list` | `NtQueryDirectoryFile` on the folder's handle, names converted to UTF-8 and sorted by their bytes; a name that isn't valid UTF-16 is `Err("a name in <path> is not valid Unicode")` |
   | `make_dir`, `make_dirs` | `NtCreateFile(FILE_CREATE, FILE_DIRECTORY_FILE \| FILE_OPEN_REPARSE_POINT)` in the parent reached by the walk |
   | `remove_file`, `remove_dir` | the last name opened from its parent with `DELETE` and `FILE_OPEN_REPARSE_POINT`, so a link is removed itself; `FileDispositionInformationEx` with POSIX semantics (an open file goes at once; ignoring read-only), else the plain disposition on a file system without them. A folder given to `remove_file`, or a file to `remove_dir`, is `Err` as on Linux |
   | `rename` | `FileRenameInformationEx` on the source, opened from its parent with `DELETE` and `FILE_OPEN_REPARSE_POINT`, with `RootDirectory` set to the target's parent handle, replacing in one step (POSIX semantics, else `ReplaceIfExists`); across volumes it is `Err` |
   | `exists`, `is_dir`, `is_file`, `size` | the handle's own information, links followed by the walk |
   | `set_mode` | Windows has no mode bits: the file is made read-only when `mode` lacks the owner's write bit (`0o200`), else writable, as Go's `Chmod` does; links followed |
   | `sync` | `FlushFileBuffers`; for a folder, which Windows won't flush, `Ok` (NTFS journals its entries) |
   | `open`, `create`, `append` | the handle turned into a C descriptor (`_open_osfhandle`), so `File` streams work unchanged |
   | `path()` | `GetFinalPathNameByHandleW`, without its `\\?\` (`\\?\UNC\` becomes `\\`) |
   | `fs.cwd()`, `fs.dir(path)` | open the working folder or `path` through Win32, folders only: the program's own starting points, as on Linux |
   | `dir(sub)`, `read_only()` | as everywhere |

   Handles are never inheritable, so a started program receives none.
4. **The same messages as Linux** where Windows has the same failure: not found, `Is a directory`, `Not a directory`, `Directory not empty`, `File exists` and `Permission denied`. Each `NTSTATUS` is mapped to the `errno` the POSIX code would show, so the shared message code is used.
5. **Tests.**
   - `tests/windows/folders.la` is the Mac's `folders.la`.
   - `tests/windows/confined/` is the Mac's `confined.la` with its links made by a small C helper (`CreateSymbolicLinkW`, and a junction through `FSCTL_SET_REPARSE_POINT`), since processes are refused on Windows. It adds Windows' own cases: `C:\…`, `\\server\share`, `a:stream`, `NUL`, `com¹.txt`, `name.`, a junction pointing inside (refused) and one pointing out (refused).
   - Both run on CI's Windows runner, which allows symlinks. Under Wine they run as far as Wine supports reparse points; a case Wine can't run is listed with why.
6. **Docs:** spec §6 Confinement (Windows), the `Dir` table's Windows notes (`set_mode`, `sync`), §11 row 260; CHANGELOG, TODO, CLAUDE.md.

### Must NOT Do

- No operation by full path, and no unconfined fallback.
- No following of junctions or absolute links, even ones pointing inside.
- No change to Linux or macOS.
- No sockets, processes or lockdown on Windows (later PRPs).

## DESIGN

**Why walk, not trust one call.**
- `OBJ_DONT_REPARSE` on a whole path would refuse every link, including those that stay inside.
- It also wouldn't stop `..` at the folder, since the NT path parser would resolve `..` against the folder handle.
- So, as Go does, the flag only guards a single name at a time, and the walk decides everything else.

**Junctions** always store absolute targets, so they lead outside, as an absolute symlink does on Linux. A folder that holds a junction pointing inside can't use it through a `Dir`. That's the conservative call Go made; it costs little in project folders.

**`set_mode` and `sync`** are the two methods whose Windows meaning differs. The spec says so in their rows. Making either fail would break portable programs that set a file read-only or sync a folder, so they work as described above.

**Security.**
- Every lookup starts from a handle the `Dir` holds.
- Each step opens exactly one name.
- No reparse point is followed by the kernel; a link's target is read and checked by the runtime.
- `..` is never resolved by the system.
- Every change acts on a handle the walk reached, so a folder swapped for a junction mid-walk is caught at the next name, as on macOS.
- Handles are opened with sharing for delete, so files behave as on POSIX: a file being read can still be renamed or removed.

## TESTS

- [ ] **Unit (Rust):** none; lughac's side doesn't change.
- [ ] **`tests/windows/folders.la` and `tests/windows/confined/`:** on CI's Windows runner, and under Wine as far as it can.
- [ ] **Linux, unchanged:** `cargo test --lib`, `--test programs`, `--test dir_race` and `--test macos` pass.
- [ ] The Windows runtime compiles without warnings (`tests/windows.rs`).

## ACCEPTANCE

- [ ] CI's Windows job passes `folders`, `confined` and every earlier case. The Linux and Mac jobs pass unchanged.
- [ ] Spec: §6 Confinement, the `Dir` table, and §11 row 260.
- [ ] CHANGELOG, TODO and CLAUDE.md are updated.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.

## CALLS I MADE (for review)

1. **Walk one name at a time**, the design you approved for macOS, with Go's Windows calls (`NtCreateFile` relative to a handle, `OBJ_DONT_REPARSE`, with a fallback for 1507).
2. **Every junction leads outside**, and absolute symlinks too, as in Go.
3. **Other reparse points fail**, `Err("can't follow a reparse point that isn't a link")`, as in Go, rather than opening OneDrive placeholders or deduplicated files through a second, riskier path.
4. **`:` and `?` anywhere lead outside**; Windows-invalid names, trailing dots and spaces, and device names (with superscripts, after cap-std's CVE) are refused by name.
5. **`/` and `\`** both separate names, so the same Lugha path works on every system.
6. **`set_mode` maps the owner's write bit to read-only**, and **`sync` on a folder is `Ok`**, as described, so portable programs keep working.
7. **POSIX delete and rename semantics**, falling back to the classic ones where a file system lacks them.
8. **Windows' errors are shown in Linux's words** where they mean the same.
9. **The links in the test** come from a C helper, since `Proc` is refused on Windows for now.
