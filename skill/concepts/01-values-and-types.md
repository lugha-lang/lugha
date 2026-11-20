---
title: Values and types
prerequisites: []
summary: Numbers, bool, char and string; let and let mut; operators and conversions.
---

# Values and types

## The idea

Every value has one type, known when the program compiles. A name is bound
with `let` and can't be changed afterwards unless it is declared `let mut`.
Numbers never convert by themselves: you say how.

## Example

```lugha
fun main(console: Console) {
    let width = 7;              // i64: a whole number without a type is an i64
    let height: i32 = 3;        // the type written after the name
    let ratio = 2.5;            // f64
    let ok = width > 5;         // bool
    let initial = 'L';          // char: one Unicode character
    let name = "Lugha";         // string: UTF-8 text

    let mut total = 0;
    total += width;
    total = total * 2;

    let area = width * (height as i64);
    console.println("{name} {initial} {ok} {total} {area} {ratio * 2.0}");
    console.println(name.len);
}
```
```output
Lugha L true 14 21 5.0
5
```

## Rules

- **Integers:** `i8`, `i16`, `i32`, `i64` (signed) and `u8`, `u16`, `u32`,
  `u64` (unsigned). A literal takes the type its place expects, else `i64`.
  Underscores may separate digits (`1_000_000`); `0x`, `0o` and `0b` start
  hexadecimal, octal and binary literals.
- **Floats:** `f64` and `f32`. A float literal has a point: `2.0`, not `2`.
  A float always prints with its point: `5.0`.
- **`bool`** is `true` or `false`, with `&&`, `||` and `!`.
- **`char`** is one Unicode character, written in single quotes: `'a'`,
  `'\n'`, `'\u{1F600}'`.
- **`string`** is immutable UTF-8 text in double quotes. `s.len` counts
  bytes, `s.char_count()` characters. `+` joins two strings.
- **Interpolation:** `"{expr}"` inside a string puts in the value of any
  expression that prints; `{{` and `}}` write braces. A format may follow a
  colon: `"{x:.2}"` (two decimals), `"{n:5}"` (width 5), `"{n:x}"` (hex).
- **Operators:** `+ - * / %` on numbers of the same type; `== != < <= > >=`
  compare; `&`, `|`, `^`, `<<`, `>>` on integers. `x += 1` and its kin
  assign. There is no `++`.
- **Both sides of an operator have the same type.** An `i32` plus an `i64`
  is an error until one is converted.
- **Conversions are written:**
  - `x as T` only where nothing is lost: `i32` to `i64`, `u8` to `u32`,
    `f32` to `f64`, `i32` to `f64`.
  - `n.truncate<u8>()` keeps the low bits.
  - `x.round<f64>()` rounds to the nearest representable value.
  - `x.saturate<i64>()` clamps a float into an integer's range.
  - `n.try_into<u8>()` gives `Some(value)` when it fits, else `None`.
- **Overflow and division by zero panic**, in every build, so a wrong number
  never goes unnoticed. `wrapping_add` and its kin wrap on purpose.
- **Constants:** `const LIMIT: i64 = 100;` at the top level, always with its
  type, computed once before `main`.
- **Text from numbers:** `to_string(x)` or `"{x}"`; numbers from text:
  `parse_i64(s)` and `parse_f64(s)` give an `Option`.

```lugha
const LIMIT: i64 = 1_000;

fun main(console: Console) {
    let small: u8 = 200;
    let wide = small as u32 + 100;
    let big = 300;
    console.println("{wide} {big.truncate<u8>()} {big.try_into<u8>().is_some()}");
    console.println("{3.14159:.2} {255:x} {LIMIT / 3}");
    console.println(parse_i64("42").unwrap_or(0) + 1);
}
```
```output
300 44 false
3.14 ff 333
43
```

## Mistakes

- **Mixing number types:** adding an `i32` and an `i64`, or `2` where an
  `f64` is wanted in a calculation. Convert one side.

```lugha error E0401
fun main() {
    let x: i32 = 5;
    let y = x + 2.5;
}
```

- **Assigning a name declared without `mut`** is E0501.

```lugha error E0501
fun main() {
    let n = 1;
    n = 2;
}
```

- **Narrowing with `as`**, such as `n as u8` for an `i64`, is E0469: say how
  with `truncate`, `round`, `saturate` or `try_into`.

```lugha error E0469
fun main() {
    let n = 300;
    let b = n as u8;
}
```

## Exercises

1. Store a temperature in Celsius as an `f64` and print it in Fahrenheit
   with one decimal (`"{f:.1}"`).
2. Count down from 10 with a `let mut` counter and print the total of the
   numbers you passed.
3. Take the `i64` 1000 and print whether it fits in a `u8` and in a `u16`,
   using `try_into`.
