---
title: Capabilities
prerequisites: [values-and-types, functions, structs-and-enums, match-and-patterns, errors, collections, interfaces]
summary: Console, Fs, Dir, Net, Clock, Env, Proc, Entropy: what main is given, passing it on, narrowing it, and faking it.
---

# Capabilities

## The idea

A Lugha program can reach outside itself (the terminal, files, the network,
the clock, other programs) only through values `main` is given, called
capabilities. A function that doesn't receive one can't use it, so reading a
function's parameters tells you everything it may touch. Each capability is
an interface, so it can be narrowed to less, or replaced by a fake in tests.

## Example

```lugha
fun report(console: Console, dir: Dir): Result<(), string> {
    dir.write_file("notes.txt", "first line\n")?;
    dir.append_file("notes.txt", "second line\n")?;
    let text = dir.read_file("notes.txt")?;
    console.println("{text.lines().len} lines");
    console.println(dir.exists("notes.txt"));
    dir.remove_file("notes.txt")?;
    Ok(())
}

fun main(console: Console, fs: Fs, clock: Clock): i32 {
    let started = clock.monotonic_ms();
    match report(console, fs.cwd()) {
        .Ok(()) => {}
        .Err(e) => {
            console.eprintln("error: {e}");
            return 1;
        }
    }
    console.println(clock.monotonic_ms() >= started);
    0
}
```
```output
2 lines
true
true
```

## Rules

- **`main` takes what it needs,** by type, in any order: `args: string[]`,
  `console: Console`, `fs: Fs`, `net: Net`, `clock: Clock`, `env: Env`,
  `proc: Proc`, `entropy: Entropy`, and `root: Scope` for tasks. Each at
  most once.
- **Pass them on** as ordinary parameters. There are no globals: no
  `println`, `read_file` or `exit` outside a capability. `main` ends the
  program by returning, with an `i32` for the exit code.
- **`Console`:** `print`, `println`, `eprint`, `eprintln` (stderr);
  `read_line()`, a `Result<Option<string>, string>` (`Ok(None)` at the end
  of input); `read_bytes()`; `flush()`.
- **`Fs` and `Dir`:**
  - `fs.cwd()` is the working folder as a `Dir`, and `fs.dir(path)` opens
    another.
  - A `Dir` reads and writes inside itself only. A path leading outside
    (`"../x"`, `"/etc/hosts"`) is an error, and `dir.dir("sub")` narrows
    further.
  - `read_file`, `write_file`, `append_file`, `read_bytes`, `write_bytes`,
    `list`, `make_dir`, `make_dirs`, `remove_file`, `remove_dir`, `exists`,
    `is_file`, `is_dir`, `size`, `rename`; `open`, `create` and `append`
    give a `File` stream.
  - Every one that can fail returns a `Result`.
- **`Net`:** `connect("host:port")`, `listen(addr)`, `resolve(host)` and UDP.
  `net.only("api.example.com:443")` narrows it to one host.
- **`Clock`:** `now_ms()` (the wall clock), `monotonic_ms()` (for measuring),
  `local_offset(ms)` (`Ok` with the time zone's offset, in seconds east of UTC).
- **`Env`:** `get(name)` gives an `Option<string>`; `env.only(["HOME"])`
  narrows.
- **`Proc`:** `run(cmd, args)`, `run_with(cmd, args, input)`, `call(cmd, args)`,
  and `run_bytes`/`run_bytes_with` for output that isn't text, start another program. There is never a shell, so
  arguments are passed exactly as given.
- **`Entropy`:** `bytes(n)` and `next_u64()`, random from the system.
- **Narrowing:** `console.stdout_only()`, `dir.dir("data")`, `net.only(…)`
  and `env.only(…)` give a weaker value to pass to code you trust less.
  `dir.read_only()` gives a `ReadDir`, a folder with only the reading
  methods: a function taking one can't write, which its signature shows.
- **Faking:** any type can implement a capability's interface, so a test
  can hand a function a console, folder or clock of its own. Capabilities
  may be passed to other tasks, so a fake is `Sendable`: one that records
  what it was given keeps the record in an actor (see channels and actors),
  not a `Ref`.
- **Tests** take capabilities too: `@test fun t(console: Console)`. A plain
  test's `Dir` is a fresh empty folder, and its `Clock`, `Env`, `Entropy` and
  `Net` are fixed, empty, seeded and loopback-only.

```lugha
// A console of our own: it puts a tag before every line, then hands the
// line to the console it wraps.
struct Tagged { under: Console, tag: string }

impl Console for Tagged {
    fun print(self, x: Display) { self.under.print("[{self.tag}] {x.show()}"); }
    fun println(self, x: Display) { self.under.println("[{self.tag}] {x.show()}"); }
    fun eprint(self, x: Display) { self.under.eprint(x); }
    fun eprintln(self, x: Display) { self.under.eprintln(x); }
    fun read_line(self): Result<Option<string>, string> = Ok(None);
    fun read_bytes(self): Result<u8[], string> = Ok([]);
    fun flush(self) { self.under.flush(); }
}

fun greet(console: Console, name: string) {
    console.println("hello, {name}");
}

fun main(console: Console) {
    greet(console, "world");
    greet(Tagged { under: console, tag: "test" }, "world");
    let quiet = console.stdout_only();
    quiet.eprintln("never shown");
    quiet.println("shown");
}
```
```output
hello, world
[test] hello, world
shown
```

## Mistakes

- **Printing without a `Console`,** with a bare `println`, is E0471: take
  `console: Console` in `main` and pass it on.

```lugha error E0471
fun main() {
    println("hi");
}
```

- **Taking something `main` can't be given,** or the same capability twice,
  is E0304.

```lugha error E0304
fun main(console: Console, other: Console) {}
```

- **A path outside the folder** is an `Err` at run time, never a read of
  that file.

```lugha
fun main(console: Console, fs: Fs) {
    match fs.cwd().read_file("../secret.txt") {
        .Ok(_) => console.println("read it"),
        .Err(e) => console.println(e),
    }
}
```
```output
path leads outside the folder
```

## Exercises

1. Write `fun count_lines(dir: ReadDir, path: string): Result<i64, string>`
   and use it from `main` with `fs.cwd().read_only()`.
2. Write a function that times another with a `Clock` and prints how long
   it took.
3. Write an `Env` fake over a `Map<string, string>`, and test a function
   that reads `HOME` with it.
