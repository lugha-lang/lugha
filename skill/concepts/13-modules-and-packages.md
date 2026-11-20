---
title: Modules and packages
prerequisites: [values-and-types, functions, structs-and-enums, match-and-patterns, errors, collections, interfaces, generics, capabilities]
summary: Files as modules, import and visibility, lugha.toml, packages, the standard library, and tests.
---

# Modules and packages

## The idea

Each `.la` file is a **module**, named by its path from the main file's
folder: `shapes/circle.la` is `shapes.circle`. A file uses another with
`import`, and reaches its items through the module's last name. A
**package** is a folder of modules with a `lugha.toml`, which lists the
packages it uses. Three visibility levels decide what other modules and
packages can see.

## Example

```lugha file geometry/vec.la
public struct Vec2 { x: f64, y: f64 }

impl Vec2 {
    public fun len2(self): f64 = self.x * self.x + self.y * self.y;
}

public fun origin(): Vec2 = Vec2 { x: 0.0, y: 0.0 };

private fun helper(): i64 = 42;
```

```lugha
import geometry.vec { Vec2 };

fun main(console: Console) {
    let o = vec.origin();
    let p = vec.Vec2 { x: 3.0, y: 4.0 };
    let q = Vec2 { x: 1.0, y: 0.0 };
    console.println("{o.len2()} {p.len2()} {q.len2()}");
}
```
```output
0.0 25.0 1.0
```

## Rules

- **Modules:** a file is a module; `import a.b;` loads `a/b.la` from the
  main file's folder and binds `b`. Items are reached as `b.name`. Each file
  is loaded once, and modules may import each other.
- **Named imports:** `import a.b { X, f };` also binds `X` and `f` directly.
  `import a.b as c;` binds the module as `c`; `{ X as Y }` renames an item.
- **Visibility,** on functions, types, interfaces, methods and fields:
  - `private`: its own module only;
  - nothing written: every module of its own package;
  - `public`: every package that imports it.
  A program without a `lugha.toml` is one package.
- **A struct with a private field** can only be built inside its module,
  through a function such as `Range.new(lo, hi)`. This is how a type keeps
  a rule like `lo <= hi`.
- **Re-exports:** `public import a.b { X };` makes `X` part of this module.
- **The standard library** is imported the same way:
  - `std.json`, `std.http`, `std.tls`, `std.crypto` and `std.toml`;
  - `std.math`, `std.random`, `std.task` and `std.time`.
- **`lugha.toml`** sits beside the main file. `[packages]` lists packages
  by path, or from git with a version:
  ```toml
  [package]
  name = "shop"
  version = "0.1.0"

  [packages]
  geo = "libs/geo"
  json5 = { git = "https://example.com/json5.git", version = "1.2" }
  sqlite = { git = "https://example.com/sqlite.git", version = "3", allow = ["ffi"] }
  ```
- **Git packages** are fetched when first built and pinned in `lugha.lock`,
  which you commit. `lughac update main.la` fetches newer compatible
  versions; `--offline` builds from what is cached. A version requirement
  `"1.2"` accepts 1.2.0 up to, not including, 2.0.0.
- **`allow = ["ffi"]`** lets a package run C code. Without it, a package
  declaring `extern` functions doesn't build.
- **Starting a project:** `lughac init` makes a program, and
  `lughac init --lib` a library with `src/` and `examples/`.
- **Tests:** a function marked `@test` is a test; `lughac test` runs each
  in its own process. `assert(cond)`, `assert_eq(a, b)` and `assert_ne(a, b)`
  check, and a panic fails the test.

```lugha
fun add(a: i64, b: i64): i64 = a + b;

@test
fun adds_small_numbers() {
    assert_eq(add(2, 3), 5);
    assert(add(-1, 1) == 0);
}

fun main(console: Console) {
    console.println(add(2, 3));
}
```
```output
5
```

```sh
lughac test                      # every @test in the package
lughac test --filter adds        # the tests whose names contain `adds`
```

## Mistakes

- **Importing a file that doesn't exist** is E0312.

```lugha error E0312
import shapes.missing;

fun main() {}
```

- **Using an item that isn't visible** is E0313, saying where it is
  visible.

```lugha file tools.la
private fun secret(): i64 = 42;
```

```lugha error E0313
import tools;

fun main() {
    let n = tools.secret();
}
```

- **Two imports with the same last name** clash: E0314. Rename one with
  `as`.

```lugha file a/util.la
fun one(): i64 = 1;
```

```lugha file b/util.la
fun two(): i64 = 2;
```

```lugha error E0314
import a.util;
import b.util;

fun main() {}
```

## Exercises

1. Split a program into `main.la` and `money.la`, with a `public struct
   Money` whose `cents` field is private, built only through `Money.new`.
2. Write `@test` functions for a `fun is_leap(year: i64): bool` and run
   them with `lughac test`.
3. Make a library with `lughac init --lib`, and use it from a program
   through a `path` entry in `[packages]`.
