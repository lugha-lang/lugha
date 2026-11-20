---
title: A command-line tool
summary: A tool that reads arguments, files and standard input, reports errors, and exits with a code.
uses: [functions, errors, collections, capabilities, text, console-and-files]
---

# A command-line tool

## Goal

Build `find-text`, a small `grep`: `find-text PATTERN [FILE…]` prints every
line containing `PATTERN`, with its file and line number. With no files it
reads standard input. It exits 0 when something matched, 1 when nothing
did, and 2 for a usage mistake or an unreadable file, as `grep` does.

## Steps

1. **Take what the tool needs.** `main` takes the arguments, the console
   and the file system, and returns its exit code. Nothing else can reach
   the outside world.

```lugha part
fun main(args: string[], console: Console, fs: Fs): i32 {
```

2. **Check the arguments first.** `args[0]` is the program's name. A usage
   message goes to standard error, and the exit code says it was a misuse.

```lugha part
    if args.len < 2 {
        console.eprintln("usage: find-text PATTERN [FILE…]");
        return 2;
    }
```

3. **Keep the work in functions that return `Result`,** taking only what
   they need. `search` takes a `ReadDir`, so it can't write files, which its
   signature shows. It doesn't print errors or exit: it returns them, and
   `main` decides. `?` passes a
   read error up with the file's name added by `map_err`.

```lugha part
fun search(console: Console, dir: ReadDir, pattern: string, files: string[]): Result<i64, string> {
    let mut found = 0;
    for name of files {
        let text = dir.read_file(name).map_err(fun(e) = "{name}: {e}")?;
        found += report(console, name, text, pattern);
    }
    Ok(found)
}
```

4. **Read standard input when no files are named,** one line at a time,
   stopping at the end or on an error.

```lugha part
fun search_stdin(console: Console, pattern: string): Result<i64, string> {
    let mut found = 0;
    let mut n = 0;
    while let .Some(line) = console.read_line()? {
        n += 1;
        if line.contains(pattern) {
            console.println("{n}: {line}");
            found += 1;
        }
    }
    Ok(found)
}
```

5. **Turn the result into an exit code** in one place.

```lugha part
    match result {
        .Ok(0) => 1,
        .Ok(_) => 0,
        .Err(e) => {
            console.eprintln("find-text: {e}");
            2
        }
    }
```

## The whole program

Run without arguments it prints its usage and exits 2; try
`lughac run find-text.la fun find-text.la`.

```lugha exit 2
fun report(console: Console, name: string, text: string, pattern: string): i64 {
    let mut found = 0;
    let mut n = 0;
    for line of text.lines() {
        n += 1;
        if line.contains(pattern) {
            console.println("{name}:{n}: {line}");
            found += 1;
        }
    }
    found
}

fun search(console: Console, dir: ReadDir, pattern: string, files: string[]): Result<i64, string> {
    let mut found = 0;
    for name of files {
        let text = dir.read_file(name).map_err(fun(e) = "{name}: {e}")?;
        found += report(console, name, text, pattern);
    }
    Ok(found)
}

fun search_stdin(console: Console, pattern: string): Result<i64, string> {
    let mut found = 0;
    let mut n = 0;
    while let .Some(line) = console.read_line()? {
        n += 1;
        if line.contains(pattern) {
            console.println("{n}: {line}");
            found += 1;
        }
    }
    Ok(found)
}

fun main(args: string[], console: Console, fs: Fs): i32 {
    if args.len < 2 {
        console.eprintln("usage: find-text PATTERN [FILE…]");
        return 2;
    }
    let pattern = args[1];
    let files = args.iter().skip(2).collect();
    let result = if files.len == 0 {
        search_stdin(console, pattern)
    } else {
        search(console, fs.cwd().read_only(), pattern, files)
    };
    match result {
        .Ok(0) => 1,
        .Ok(_) => 0,
        .Err(e) => {
            console.eprintln("find-text: {e}");
            2
        }
    }
}
```
```output
```

## Going further

- Add a `-i` flag for case-insensitive matching: check `args` for it before
  the pattern, and compare with `line.to_lower()` (see `library/text`).
- Test `report` and `search` with `@test` functions and a fresh folder (see
  `tasks/testing`).
- The `examples/word-count.la` program is another complete tool.
