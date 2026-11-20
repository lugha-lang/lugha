---
title: Console and files
summary: Console, Fs, Dir, ReadDir and File: every method, what it gives, and how folders are confined.
---

# Console and files

## Overview

The terminal and the file system are capabilities: `main` takes
`console: Console` and `fs: Fs` and passes them, or a `Dir` from `fs`, to
the code that needs them. Every operation that can fail returns a `Result`
with the system's message as a `string`. A `Dir` is confined: no path
reaches outside it.

## Reference

### Console

| Item | What it does |
| --- | --- |
| `console.print(x)`, `console.println(x)` | writes `x` (any `Display`) to standard output, `println` with a newline |
| `console.eprint(x)`, `console.eprintln(x)` | writes to standard error, after flushing standard output |
| `console.read_line()` | `Ok(Some(line))` without its `\n` (and a `\r` before it), `Ok(None)` at the end of input, `Err("line N is not valid UTF-8")` (that line skipped) or `Err("stdin: …")` |
| `console.read_bytes()` | `Ok(bytes)`: all of standard input that is left; `Err("stdin: …")` |
| `console.flush()` | sends out what standard output holds now |
| `console.stdout_only()` | a `Console` whose `eprint`s do nothing and with no input |
| `console.stderr_only()` | a `Console` whose `print`s do nothing and with no input |

### Fs

| Item | What it gives |
| --- | --- |
| `fs.cwd()` | the working folder, as a `Dir` |
| `fs.dir(path)` | another folder, by any path: `Ok(Dir)` or `Err` |

### Dir

Paths are relative to the folder, with `/` between parts. A path leading
outside it (`..` past its top, an absolute path, a link pointing out) is
`Err("path leads outside the folder")`.

| Item | What it gives |
| --- | --- |
| `dir.read_file(path)` | `Ok(text)`; `Err` when missing or not UTF-8 |
| `dir.read_bytes(path)` | `Ok(u8[])` |
| `dir.write_file(path, text)`, `dir.write_bytes(path, bytes)` | replaces the file's contents: `Ok(bytes written)` |
| `dir.append_file(path, text)`, `dir.append_bytes(path, bytes)` | adds to the end: `Ok(bytes written)` |
| `dir.list(path)` | `Ok(names)`, sorted, without `.` and `..` |
| `dir.exists(path)`, `dir.is_file(path)`, `dir.is_dir(path)` | a `bool` |
| `dir.size(path)` | `Ok(bytes)` |
| `dir.make_dir(path)`, `dir.make_dirs(path)` | `Ok(())`; `make_dirs` makes the parents too and accepts an existing folder |
| `dir.remove_file(path)`, `dir.remove_dir(path)` | `Ok(())`; a folder must be empty |
| `dir.rename(from, to)` | `Ok(())` |
| `dir.set_mode(path, mode)` | `Ok(())`: permission bits, such as `0o644` |
| `dir.sync(path)` | `Ok(())` once the file's data is on the disk |
| `dir.open(path)`, `dir.create(path)`, `dir.append(path)` | `Ok(File)` to read, to write from empty, or to add to the end |
| `dir.dir(path)` | `Ok(Dir)`: a folder inside, confined to itself |
| `dir.read_only()` | the same folder as a `ReadDir`, which has no method that changes anything |
| `dir.path()` | `Ok(text)`: the folder's full path |

### ReadDir

A folder that can only be read, from `dir.read_only()`: confined as its
`Dir` is, with only the reading methods, so code given a `ReadDir` can't
write, which its signature shows. Calling a writing method on one is a
compile error (E0415). Take a `ReadDir` parameter where a function only
reads.

| Item | What it gives |
| --- | --- |
| `r.read_file(path)`, `r.read_bytes(path)` | as on `Dir` |
| `r.list(path)`, `r.exists(path)`, `r.is_file(path)`, `r.is_dir(path)`, `r.size(path)` | as on `Dir` |
| `r.open(path)` | `Ok(File)` to read |
| `r.dir(path)` | `Ok(ReadDir)`: a folder inside, confined and read-only too |
| `r.path()` | `Ok(text)`: the folder's full path |

### File

| Item | What it gives |
| --- | --- |
| `f.read(max)` | `Ok(bytes)`: up to `max` (1 to 16 777 216); `[]` at the end |
| `f.read_line()` | as `Console.read_line`, on the file |
| `f.write(bytes)`, `f.write_text(s)` | `Ok(())` once all is written |
| `f.close()` | closes it; use `defer f.close();` after opening |

### Files in the program

| Item | What it gives |
| --- | --- |
| `include_bytes("logo.png")` | the file's bytes as a `u8[]`, read when the program is compiled; it must lie inside the package and be at most 16 MiB |

## Examples

```lugha
fun save_and_count(dir: Dir): Result<i64, string> {
    dir.make_dirs("out/logs")?;
    let logs = dir.dir("out/logs")?;
    logs.write_file("a.txt", "one\ntwo\n")?;
    logs.append_file("a.txt", "three\n")?;
    let f = logs.open("a.txt")?;
    defer f.close();
    let mut n = 0;
    while let .Some(_) = f.read_line()? {
        n += 1;
    }
    Ok(n)
}

fun main(console: Console, fs: Fs) {
    let here = fs.cwd();
    match save_and_count(here) {
        .Ok(n) => console.println("{n} lines"),
        .Err(e) => console.eprintln(e),
    }
    console.println(here.list("out/logs").unwrap().debug());
    console.println(error_of(here.dir("out").unwrap().read_file("../secret")));
    let locked = here.read_only();
    console.println(locked.list("out").unwrap().debug());
    console.println("{here.size("out/logs/a.txt").unwrap()} {here.is_dir("out")}");
}

fun error_of<T>(r: Result<T, string>): string = match r {
    .Ok(_) => "ok",
    .Err(e) => e,
};
```
```output
3 lines
["a.txt"]
path leads outside the folder
["logs"]
14 true
```
