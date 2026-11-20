---
name: lugha
description: Write, read, fix, explain and teach programs in Lugha, a small statically typed language compiled to native code by `lughac`. Use when working with `.la` files or `lugha.toml`, running `lughac` commands, fixing errors like `error[E0401]`, or answering questions about the Lugha language and its standard library.
---

# Lugha

Lugha (Swahili for "language") is a small, statically typed language that
compiles to native executables through LLVM. Memory is managed by reference counting:
there is no garbage collector, no null and no manual freeing. Source files end
in `.la`; the compiler is `lughac`.

```lugha
fun main(console: Console) {
    let name = "Lugha";
    console.println("Hello from {name}!");
}
```
```output
Hello from Lugha!
```

## How to work

1. Write the program in a `.la` file. A program starts at `fun main`.
2. Check it: `lughac check main.la` (add `--diagnostics=json` for one JSON
   object per error, easier to read by a tool).
3. For an error code you don't know, run `lughac explain E0401`: it prints the
   code's page, with an example and the fix.
4. Run it: `lughac run main.la`, or build it: `lughac build main.la -o main`.
5. Format it: `lughac fmt main.la`. Test it: `lughac test` runs the
   `@test` functions.

```sh
lughac check main.la
lughac explain E0417
lughac run main.la
```

## Rules that surprise newcomers

Read these before writing Lugha: each is a place where Lugha differs from the
languages it looks like.

- **`main` is handed what it may touch.** There is no global `println`. A
  program takes `console: Console` to print, `fs: Fs` for files, `net: Net`
  for the network, and passes them to the functions that need them.
- **Functions are `fun`; types follow names.** `fun add(a: i64, b: i64): i64`.
  A one-expression body is written with `=`: `fun double(x: i64): i64 = x * 2;`
- **Bindings are immutable unless `let mut`.** The same goes for a method
  changing its value: `fun grow(mut self)`.
- **Values are copied.** Assigning a struct or an array gives an independent
  copy. There are no references and no pointers in ordinary code.
- **A variant in a pattern has a leading dot.** Build with the enum's name,
  `Shape.Circle(2.0)`; match with `.Circle(r) =>`. `Some`, `None`, `Ok` and
  `Err` are built without a prefix but matched with the dot too: `.Some(x)`.
- **Loops:** `for x of items` goes over values; `for i in 0..n` over a range.
- **No implicit number conversions.** `1 + 2.5` with an `i64` is an error.
  `as` only widens without loss (`n as i64` for an `i32` `n`); narrowing says
  how: `n.truncate<u8>()`, `x.round<f64>()`, `n.try_into<u8>()`.
- **Errors are values.** A function that can fail returns `Result<T, E>`;
  `?` passes an error up. A `Result` used as a statement must be handled, or
  dropped on purpose with `let _ = f();`.
- **Printing needs `Display`.** Numbers, `bool`, `char` and `string` print.
  For your own type write `impl Display for T`, or print `x.debug()`.
- **Behaviour comes from `impl`.** `==` needs `with Eq`, ordering `with Ord`,
  a map key `with Key`; an interface is met only by `impl Interface for Type`.
- **Integer overflow panics**, in every build.

## Pages

Read the page for what you are doing. The concepts build on each other in
this order, so a course can follow it.

| Page | Read it when |
| --- | --- |
| [Values and types](concepts/01-values-and-types.md) | you need the number types, strings, `let`, conversions or operators |
| [Functions](concepts/02-functions.md) | you write or call functions, closures or function values |
| [Structs and enums](concepts/03-structs-and-enums.md) | you define your own types, methods or `with` derives |
| [Match and patterns](concepts/04-match-and-patterns.md) | you take values apart with `match`, `if let` or `let` |
| [Errors](concepts/05-errors.md) | you use `Option`, `Result`, `?`, `panic` or design error types |
| [Collections](concepts/06-collections.md) | you use arrays, `Map` or `Set` |
| [Interfaces](concepts/07-interfaces.md) | you share behaviour between types, implement `Display` or operators, or use interface values |
| [Generics](concepts/08-generics.md) | you write `<T>` functions or types, or need a bound like `T: Ord` |
| [Iterators](concepts/09-iterators.md) | you loop with `for … of`, make a type loopable, or chain `map`, `filter`, `take` |
| [Capabilities](concepts/10-capabilities.md) | you print, read input, touch files, the network, the clock, the environment or other programs, or fake them in tests |
| [Tasks and scopes](concepts/11-tasks-and-scopes.md) | you run work at the same time, wait for it, cancel it or give it a deadline |
| [Channels and actors](concepts/12-channels-and-actors.md) | tasks pass values to each other, wait on several things with `select`, or share state through an actor |
| [Modules and packages](concepts/13-modules-and-packages.md) | you split code into files, set visibility, write `lugha.toml`, use `std` or git packages, or write `@test`s |
| [C interop](concepts/14-c-interop.md) | you call C, link a C library, pass callbacks or C buffers, or grant a package `ffi` |

## Library

Look up exact methods here.

| Page | Read it when |
| --- | --- |
| [Text](library/text.md) | you need a `string` or `char` method, a format specifier, parsing, or UTF-8 |
| [Collections](library/collections.md) | you need an array, `Map`, `Set`, iterator, tuple, `Option` or `Result` method |
| [Console and files](library/console-and-files.md) | you need a `Console`, `Fs`, `Dir` or `File` method and its errors |
| [Numbers, time and randomness](library/numbers-time-random.md) | you convert numbers, use `std.math`, dates and times, random numbers or `std.task` |
| [Network](library/net.md) | you connect, listen, resolve names, use UDP or narrow a `Net` |
| [HTTP](library/http.md) | you make HTTP requests or serve HTTP with `std.http` |
| [TLS and cryptography](library/tls-and-crypto.md) | you encrypt connections, hash, sign, agree on keys or seal data |
| [JSON and TOML](library/json-and-toml.md) | you parse or write JSON or TOML, or encode your own types as JSON |
| [Environment and processes](library/env-and-processes.md) | you read environment variables or run other programs |

## Tasks

How the concepts combine to do one job, end to end.

| Page | Read it when |
| --- | --- |
| [A command-line tool](tasks/command-line-tool.md) | you write a tool taking arguments, files or standard input, with exit codes |
| [A TCP server](tasks/tcp-server.md) | you serve connections: a task each, shared state, timeouts, stopping |
| [An HTTP service](tasks/http-service.md) | you build a JSON API: routes, decoding bodies, errors as statuses |
| [Wrapping a C library](tasks/wrap-a-c-library.md) | you link C code and give it a safe Lugha API |
| [Testing](tasks/testing.md) | you write `@test`s, fake a capability, or run `lughac test` |

## Examples

Complete programs to read or adapt; each ends with the output it prints.

| Program | What it shows |
| --- | --- |
| [word-count](examples/word-count.la) | arguments, files, text, an operator on your own type |
| [parallel-sum](examples/parallel-sum.la) | a scope's tasks sending results on a channel; a failing task |
| [kv-server](examples/kv-server.la) | a TCP protocol, a task per connection, an actor holding the data |
| [notes-api](examples/notes-api.la) | an HTTP JSON API with an actor, routes and decoding |
| [config](examples/config.la) | TOML settings with defaults and an environment override |

Other commands: `lughac skill --list` lists every page with a one-line
summary, `lughac skill --search <text>` finds lines across them, and
`lughac skill <page>` prints one.
