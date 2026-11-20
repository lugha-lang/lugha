---
title: Errors
prerequisites: [values-and-types, functions, structs-and-enums, match-and-patterns]
summary: Option for a value that may be missing, Result and ? for work that may fail, and panic for bugs.
---

# Errors

## The idea

Lugha has no null and no exceptions. A value that may be missing is an
`Option<T>`: `Some(value)` or `None`. Work that may fail returns a
`Result<T, E>`: `Ok(value)` or `Err(error)`. The `?` operator passes an
`Err` (or a `None`) up to the caller, so the happy path reads straight
down. A `panic` is for bugs: it stops the program, running the `defer`s on
the way out.

## Example

```lugha
fun half(n: i64): Result<i64, string> =
    if n % 2 == 0 { Ok(n / 2) } else { Err("{n} is odd") };

fun quarter(n: i64): Result<i64, string> {
    let h = half(n)?;       // an Err here returns it from `quarter`
    half(h)
}

fun first_even(xs: i64[]): Option<i64> {
    for x of xs {
        if x % 2 == 0 {
            return Some(x);
        }
    }
    None
}

fun main(console: Console) {
    for n of [12, 6] {
        match quarter(n) {
            .Ok(q) => console.println("quarter of {n} is {q}"),
            .Err(e) => console.println("error: {e}"),
        }
    }
    console.println(first_even([3, 5, 8, 9]).unwrap_or(-1));
    console.println(first_even([1, 3]).is_none());
    console.println(parse_i64("x").map(fun(n) = n * 2).unwrap_or(0));
}
```
```output
quarter of 12 is 3
error: 3 is odd
8
true
0
```

## Rules

- **`Option<T>`** is `Some(T)` or `None`. `None` needs its type known from
  context: `let x: Option<i64> = None;`.
- **`Result<T, E>`** is `Ok(T)` or `Err(E)`. The error is often a `string`,
  or a type of your own.
- **Useful methods** on both:
  - `unwrap()` gives the value or panics;
  - `expect("why")` does the same with your message;
  - `unwrap_or(default)` gives the value or the default;
  - `map(f)` changes the value inside, if there is one;
  - `is_some()`/`is_none()` and `is_ok()`/`is_err()` ask which it is;
  - `o.ok_or(error)` turns an `Option` into a `Result`, and `r.ok()` turns a
    `Result` into an `Option`.
- **`?`** after an expression:
  - in a function returning `Result`, unwraps an `Ok` or returns the `Err`;
  - in a function returning `Option`, unwraps a `Some` or returns `None`.
- **A `Result` can't be ignored.** One used as a statement is E0467: handle
  it, or drop it on purpose with `let _ = f();`.
- **Error types:** any type with `impl Display` and `impl Error` can be an
  error. `Result<T, Error>` holds any of them, and `?` converts a specific
  error into `Error`. `string` is already an `Error`.
- **`panic("message")`** stops the program with exit code 101, printing the
  message and where it happened. Out-of-bounds indexes, overflow, division
  by zero and `unwrap` on `None` or `Err` panic the same way.
- **`defer statement;`** runs when its block is left, whichever way: the
  end, `return`, `?`, or a panic. Use it to close what you opened.
- **`main` doesn't return a `Result`.** Handle the error there: print it and
  return an exit code.

```lugha exit 1
enum ParseError { Empty, NotANumber(string) }

impl Display for ParseError {
    fun show(self): string = match self {
        .Empty => "nothing to read",
        .NotANumber(s) => "`{s}` is not a number",
    };
}

impl Error for ParseError {}

fun read_number(s: string): Result<i64, ParseError> {
    if s.trim() == "" {
        return Err(ParseError.Empty);
    }
    parse_i64(s.trim()).ok_or(ParseError.NotANumber(s))
}

fun total(items: string[]): Result<i64, Error> {
    let mut sum = 0;
    for item of items {
        sum += read_number(item)?;    // a ParseError becomes an Error
    }
    Ok(sum)
}

fun main(console: Console): i32 {
    console.println(total(["1", " 2 ", "3"]).unwrap());
    match total(["4", "five"]) {
        .Ok(n) => {
            console.println(n);
            0
        }
        .Err(e) => {
            console.eprintln("error: {e}");
            console.println("failed");
            1
        }
    }
}
```
```output
6
failed
```

```lugha exit 101
fun main(console: Console) {
    defer console.println("cleaned up");
    let xs = [1, 2, 3];
    console.println(xs[5]);
}
```
```output
cleaned up
```

## Mistakes

- **Ignoring a `Result`** is E0467: use `?`, `match`, or `let _ = …;`.

```lugha error E0467
fun save(): Result<(), string> = Ok(());

fun main() {
    save();
}
```

- **`?` in a function that doesn't return `Result` or `Option`** is
  E0424: handle the error with `match` there instead.

```lugha error E0424
fun half(n: i64): Result<i64, string> = Ok(n / 2);

fun main() {
    let h = half(4)?;
}
```

- **`None` without a known type** is E0412: write the type,
  `let x: Option<i64> = None;`.

```lugha error E0412
fun main() {
    let x = None;
}
```

## Exercises

1. Write `fun divide(a: i64, b: i64): Result<i64, string>` that returns an
   `Err` for a zero `b`, and use it with `?` in a function computing
   `a / b / c`.
2. Write `fun find(names: string[], name: string): Option<i64>` giving the
   index of `name`, and print `"not found"` with `unwrap_or` when it's missing.
3. Make an `enum BankError { Insufficient(i64), Frozen }` with `Display` and
   `Error`, and a `withdraw` function that returns it.
