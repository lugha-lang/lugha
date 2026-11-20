---
title: Environment and processes
summary: Env and Proc: reading environment variables and running other programs, with no shell.
---

# Environment and processes

## Overview

Environment variables come through the `Env` capability, and starting
other programs through `Proc`; `main` takes `env: Env` and `proc: Proc`.
**There is never a shell:** the command and each argument reach the program
exactly as given, so `;`, `|`, `$( )`, quotes and `*` in them mean nothing,
and text from a user can't run commands.

## Reference

### Env

| Item | What it gives |
| --- | --- |
| `env.get(name)` | `Some(value)`; `None` when unset, not UTF-8, or `name` holds a NUL byte |
| `env.only(names)` | an `Env` that reads only those variables; the rest look unset |

Variables are read-only: a program can't change its own environment.

### Proc

| Item | What it does |
| --- | --- |
| `proc.run(cmd, args)` | runs `cmd` with an empty input and captures its output: `Ok(ProcessOutput)` |
| `proc.run_with(cmd, args, input)` | the same, writing `input` to the program's standard input |
| `proc.call(cmd, args)` | runs `cmd` on this program's own input and output: `Ok(exit code)` |
| `ProcessOutput` | `code` (the exit code, or 128 + n for signal n), `stdout`, `stderr` |

- A command without `/` is looked up on `PATH`; one with `/` is a path.
  Whoever controls `PATH` chooses what a bare name runs, so use a full path
  when that matters.
- Any exit code is `Ok`. `Err` means the program couldn't start, an argument
  held a NUL byte, or its output wasn't UTF-8.
- The program gets this one's environment and working folder, and only
  standard input, output and error.

## Examples

```lugha
fun main(console: Console, env: Env, proc: Proc) {
    let out = proc.run("echo", ["hello", "$HOME; rm -rf /"]).unwrap();
    console.println("{out.code} {out.stdout.trim()}");
    let sorted = proc.run_with("sort", [], "pear\napple\n").unwrap();
    console.println(sorted.stdout.lines().join(","));
    let failed = proc.run("sh", ["-c", "exit 3"]).unwrap();
    console.println(failed.code);
    console.println(proc.run("no-such-program-x", []).is_err());

    console.println(env.get("PATH").is_some());
    let narrow = env.only(["HOME"]);
    console.println(narrow.get("PATH").is_none());
}
```
```output
0 hello $HOME; rm -rf /
apple,pear
3
true
true
true
```
