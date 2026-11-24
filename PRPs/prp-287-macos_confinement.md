## FEATURE: folders on macOS, confined by walking each path one name at a time — milestone 254

## OBJECTIVE

Every `Dir` works on macOS and is confined as on Linux: `..` above the folder, an absolute path and a symbolic link leading out are `Err("path leads outside the folder")`; links that stay inside work (SEC-01, PRT-01). Today every folder operation on macOS fails closed ("folders aren't supported on macOS yet").

macOS has no `openat2`. Its `O_RESOLVE_BENEATH` exists only from 15.4, is weaker, and on 14.x is silently ignored (research §1). So the runtime walks the path itself, as Go's `os.Root` and cap-std do: one `openat` per name, never following a link in the kernel, links read and followed by the runtime, `..` never asked of the kernel. This is the user's choice (design 2).

On Linux, behaviour is unchanged. The code that isn't resolution moves into `os/posix/`, so both systems share it.

## CONTEXT

- Research: `docs/research/macos.md` §1 (XNU's flags by version; Go `os/root_openat.go`, cap-std `fs/manually/open.rs`).
- Read first: `runtime/os/linux/dir.c`, `dir_names.c` (what's promised), `runtime/os/darwin/dir.c`, `dir_names.c` (the stand-ins replaced), `runtime/io/helpers.c` (`Job`), `src/link/runtime_parts.rs`, `src/link/mod.rs` (the boundary tests), `tests/programs/capabilities/dir_confinement/confined.la`, `tests/dir_race.rs`, `tests/macos.rs`.
- Spec: §6 Confinement (line ~1403) and §8's `dir_*` rows, which today name only `openat2`.
- Implements: v18 Draft 9.3 step 4's next part [PRT-01, SEC-01]. Decisions: none open.
- Follows: PRP-288 (lockdown through Seatbelt's coarse parts).
- 🔒 Security-sensitive (folder confinement): needs the user's review before code.

## REQUIREMENTS

### Must Do

1. **The split.** These parts of `os/linux/dir.c` and `dir_names.c` move unchanged to `os/posix/dir.c` and `os/posix/dir_names.c`:
   - `LughaDir`
   - the jobs
   - read, write, list, file, sub, cwd, open
   - make, remove, rename, size, kind, sync

   The Linux-only parts become layer pieces in `os/linux/dir_resolve.c`:
   - `openat2` and `O_PATH`
   - `/proc/self/fd`
   - `fchmodat2`
   - the `ENOSYS` message

   `memrchr` (GNU) becomes a small loop.
2. **The layer for folders.** Each system gives these four functions and one constant:
   - `os_dir_resolve(dirfd, path, flags, mode)`: the descriptor, or -1 with `errno`. `EXDEV` means the path leads outside.
   - `os_dir_stat(dirfd, path, &st)`: the stat of what the path names, links inside followed.
   - `os_dir_chmod(dirfd, path, mode)`: the same lookup, changing the mode.
   - `os_dir_where(fd, buf, len)`: where the folder is now.
   - `OS_DIR_FLAGS`: how a `Dir`'s own descriptor is opened.

   Linux keeps exactly today's calls: `openat2` with `RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS`, `O_PATH | O_DIRECTORY`, `fstat` on an `O_PATH` descriptor, `fchmodat2`/`/proc`, and `readlink` of `/proc/self/fd`.
3. **macOS: the walk** (`os/darwin/dir_resolve.c`):
   - **Starting point.** An absolute path is `EXDEV`. The walk starts at `dirfd` with an empty stack of names.
   - **Each name, taken in turn:**
     - An empty name or `.` is skipped.
     - `..` pops the stack; on an empty stack it is `EXDEV`. After a pop, the walk re-opens from `dirfd` along the remaining names, each step as below.
     - Any other name is opened alone: `openat(cur, name, O_SEARCH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)` for a folder on the way, and the caller's `flags | O_NOFOLLOW | O_CLOEXEC` (plus `mode`) for the last name.
   - **When an open fails** with `ELOOP` or `ENOTDIR`, `readlinkat(cur, name)` checks the name:
     - If it is a link, its target replaces that name in the remaining path; an absolute target is `EXDEV`.
     - If it isn't (`EINVAL`), the open's own error stands.
   - **Limits.**
     - At most 40 links per lookup, as Linux allows; beyond that, `ELOOP`.
     - The path with its links expanded is at most 4096 bytes (`ENAMETOOLONG`).
     - At most two descriptors are open at a time.
   - **A trailing `/`** makes the last name a folder (`O_DIRECTORY`), as on Linux.
   - **The path names the folder itself** (`.`, `a/..`): the result is `openat(cur, ".", flags)`.
   - **`stat` and `chmod`** walk to the folder holding the last name and resolve that name until it isn't a link. They then call `fstatat` or `fchmodat` on it with `AT_SYMLINK_NOFOLLOW`. If a link is swapped in after the check, the call acts on the link itself, never on its target.
4. **macOS's other pieces:**
   - `OS_DIR_FLAGS` = `O_SEARCH`: a folder needs search permission only, as Linux's `O_PATH`.
   - `os_dir_where` uses `fcntl(F_GETPATH)`.
   - `fs.cwd()` and `fs.dir(path)` open with ordinary resolution and `O_SEARCH`, as Linux's do with `O_PATH`.
5. **Making, removing and renaming** keep their shape on both systems. The folder holding the last name is opened through `os_dir_resolve` with `O_DIRECTORY`, then `mkdirat`, `unlinkat` and `renameat` act on that one name. None of them follows a link at the last name.
6. **Runtime parts:**
   - `os/darwin/dir.c` and `os/darwin/dir_names.c` go.
   - `runtime_parts` joins `os/{linux,darwin}/dir_resolve.c`, then `os/posix/dir.c` and `os/posix/dir_names.c`.
   - The boundary test still holds: nothing Linux-only in `os/posix/`.
7. **CI's Mac job:**
   - `tests/macos/folders.la` becomes a working-folder case: make, write, read, list, rename, remove, size, set_mode, path.
   - New `tests/macos/confined.la`: the same program and expected output as `tests/programs/capabilities/dir_confinement/confined.la`. A symlink copy can't be used, because the `.la` reads `confined.scratch`.
   - New `tests/macos/dir_race.la`: one task keeps swapping `box/sub` between a real folder and a link to a folder outside, with `here.rename` (atomic). Another task reads `box/sub/secret.txt` 20,000 times. It prints how many reads leaked, which must be `0`.
   - New `tests/macos/search_only.la`: a path through a folder with mode `0o111` works.
8. **Docs:**
   - CHANGELOG (Added: folders on macOS)
   - spec §6 Confinement and the §8 rows (how macOS confines; one sentence each)
   - §11 row 254
   - TODO (folders removed from the macOS list)
   - research §1 noted as acted on

### Must NOT Do

- No change to what a Linux program sees: the same calls, results and messages. `tests/dir_race.rs` and the `dir_*` program cases pass unchanged.
- No use of `O_RESOLVE_BENEATH`, `AT_RESOLVE_BENEATH` or `O_NOFOLLOW_ANY`. They are absent on 14.x, or silently ignored there, and the walk needs none of them. With one name per call, `O_NOFOLLOW` is the whole guarantee.
- No unconfined fallback on any system.
- No lockdown (PRP-288).

## DESIGN

**Why the walk is sound.**
- Each `openat` names exactly one component that isn't `.` or `..`, from a descriptor the walk already holds, with `O_NOFOLLOW`. So the kernel never follows a link and never climbs.
- Every link is read by the runtime and its target pushed through the same walk. `..` only ever pops what the walk itself went down through.
- A folder renamed or swapped for a link mid-walk is met by the next `O_NOFOLLOW` open. It is then either refused or read as a link and confined like any other.
- So no step can leave the folder, whatever runs concurrently. `dir_race.la` checks this on the Mac, as `tests/dir_race.rs` does on Linux.

**Why a stack of names, not descriptors** (cap-std keeps descriptors):
- A path like `a/a/a/…` would hold one descriptor per level, and macOS's default limit is 256.
- With names, a `..` re-opens from the `Dir` along the remaining names. Each step is the same single-name `O_NOFOLLOW` open, so re-walking can't escape: it is never a string handed to the kernel to resolve. If a name on the way became a link meanwhile, the re-walk follows it through the walk, as on first sight.
- The cost is quadratic in `..`s, and paths are short. Go does the same.

**Why move code into `os/posix/`:**
- About 600 of the 740 lines are plain POSIX.
- Copying them into `os/darwin/` would keep two copies of security code in step by hand.
- The split also matches the runtime's layout: what systems share lives in `os/posix/`, what differs in `os/<system>/`.

**Differences that remain, documented:**
- **Mount points:** on macOS a firmlink or mount point is crossed as on Linux.
- **Case:** APFS is case-insensitive, so `Secret.txt` and `secret.txt` are one file. Confinement doesn't depend on names.
- **Hard links:** inside a folder they can point anywhere, on both systems.
- **Changing a link's mode:** on macOS, a `set_mode` whose file was swapped for a link at the last moment changes the link's own mode. On Linux the race can't happen.

**Judgement calls (for review):**
- The split into `os/posix/` with layer functions.
- A stack of names over descriptors.
- `O_SEARCH` for a `Dir` (search permission only, as Linux's `O_PATH`).
- 40 links per lookup (Linux's `MAXSYMLINKS`) over macOS's own 32.
- `fstatat`/`fchmodat` with `AT_SYMLINK_NOFOLLOW` on the last name, instead of opening the file: no read permission is needed, and a FIFO doesn't block.
- The new Mac cases as copies of the Linux ones, instead of the whole program suite on the Mac (that comes later).

## TESTS

- [ ] `src/link/mod.rs`: the boundary test holds with `os/posix/dir*.c`; `openat2`, `O_PATH`, `/proc` and `fchmodat2` are named only in `os/linux/`.
- [ ] `tests/macos.rs`: the macOS runtime compiles with zig and no warning; the new cases build.
- [ ] CI's Mac job:
  - `folders`, `confined` (output identical to Linux's), `dir_race` (`0` leaked), `search_only`
  - the existing hello, udp_long and start
- [ ] Linux unchanged:
  - `--test programs` (the `dir_*` cases), `dir_race`, `test_capabilities` (a plain test's `Dir`), `helpers`, `lockdown`
  - `arm`, `musl`, `--lib`

## ACCEPTANCE

- [ ] Folders on macOS confined as on Linux; CI's Mac job green with the new cases.
- [ ] Linux's results and messages unchanged.
- [ ] Tests pass; fmt and clippy clean; files ≤ 500 lines.
- [ ] CHANGELOG, spec §6/§8, row 254, TODO updated.
