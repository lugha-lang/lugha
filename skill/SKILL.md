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

Other commands: `lughac skill --list` lists every page with a one-line
summary, `lughac skill --search <text>` finds lines across them, and
`lughac skill <page>` prints one.
