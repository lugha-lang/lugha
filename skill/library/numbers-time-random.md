---
title: Numbers, time and randomness
summary: Number types and methods, std.math, Clock, std.time, std.random, Entropy and std.task.
modules: [math, random, time, task]
---

# Numbers, time and randomness

Contents: [Number types](#number-types) · [std.math](#stdmath) ·
[Clock and std.time](#clock-and-stdtime) ·
[std.random and Entropy](#stdrandom-and-entropy) · [std.task](#stdtask) ·
[Examples](#examples)

## Overview

The number types and their methods are built in. `std.math`, `std.random`,
`std.time` and `std.task` are imported (`import std.math;`). Reading the
clock and the system's randomness needs the `Clock` and `Entropy`
capabilities, which `main` takes.

## Reference

### Number types

| Type | Range |
| --- | --- |
| `i8`, `i16`, `i32`, `i64` | signed: −2⁷ … 2⁷−1 up to −2⁶³ … 2⁶³−1 |
| `u8`, `u16`, `u32`, `u64` | unsigned: 0 … 2⁸−1 up to 0 … 2⁶⁴−1 |
| `f32`, `f64` | IEEE 754 floats |
| `i64.MIN`, `i64.MAX`, … | each integer type's limits |

| Method | What it gives |
| --- | --- |
| `x as T` | a conversion that loses nothing (`i32` to `i64`, `u8` to `f64`) |
| `n.truncate<T>()` | the low bits of an integer, as integer type `T` |
| `n.try_into<T>()` | `Some(value)` when it fits `T` (an integer type or `char`), else `None` |
| `x.round<T>()`, `x.floor<T>()`, `x.ceil<T>()` | a float rounded to an integer type, or a number rounded to a float type; panics when out of range |
| `x.saturate<T>()` | a float clamped into integer type `T`'s range (NaN gives 0) |
| `a.wrapping_add(b)`, `wrapping_sub`, `wrapping_mul` | the result wrapped around, never a panic |
| `a.checked_add(b)`, `checked_sub`, `checked_mul`, `checked_div` | `Some(result)`, or `None` instead of a panic |
| `a.saturating_add(b)`, `saturating_sub`, `saturating_mul` | the result clamped to the type's limits |
| `n.rotate_left(k)`, `n.rotate_right(k)` | the bits rotated |
| `n.count_ones()`, `n.leading_zeros()`, `n.trailing_zeros()` | bit counts, as `i64` |
| `n.kb()`, `n.mb()`, `n.gb()` | `n` times 1 024, 1 048 576, 1 073 741 824 (on `i16` … `u64`) |
| `x.to_bits()`, `f64.from_bits(b)` | a float's IEEE 754 bits, and back |
| `f64.total_order` | an ordering of every float (NaN included), for `sort_by` |

Integer overflow, division by zero and an out-of-range conversion panic.

### std.math

| Item | What it gives |
| --- | --- |
| `math.pi()`, `math.e()` | π and e, as `f64` |
| `math.sqrt(x)`, `math.pow(x, y)`, `math.exp(x)` | for `f32` and `f64`; `sqrt` of a negative is NaN |
| `math.ln(x)`, `math.log10(x)` | logarithms; `ln(0.0)` is −∞ |
| `math.sin(x)`, `math.cos(x)`, `math.tan(x)`, `math.atan2(y, x)` | trigonometry in radians |
| `math.floor(x)`, `math.ceil(x)`, `math.round(x)` | whole numbers as floats; `round` takes halves away from zero |
| `math.abs(x)` | `x` without its sign, for any number type; the minimum integer panics |
| `math.min(a, b)`, `math.max(a, b)` | for any `Ord` type; `a` when equal |
| `math.clamp(x, lo, hi)` | `x` limited to `lo` … `hi`; panics when `hi < lo` |

### Clock and std.time

| Item | What it gives |
| --- | --- |
| `clock.now_ms()` | milliseconds since 1970-01-01 00:00 UTC, from the system clock |
| `clock.monotonic_ms()` | milliseconds from an arbitrary start that never goes back, for measuring |
| `clock.local_offset(ms)` | `Ok(seconds east of UTC)` for the system's time zone at that moment |
| `time.civil(ms, offset_s)` | `Ok(Civil)`: the date and time at that offset (`0` for UTC) |
| `time.from_civil(c, offset_s)` | `Ok(ms)`: the inverse; `Err` for a date that doesn't exist |
| `Civil` | `year`, `month` (1–12), `day`, `weekday` (1 Monday … 7 Sunday), `hour`, `minute`, `second`, `ms` |
| `TimeError` | the error: `e.message()`, and it prints as its message |

### std.random and Entropy

| Item | What it gives |
| --- | --- |
| `entropy.bytes(n)` | `n` random bytes from the system, for keys and tokens |
| `entropy.next_u64()` | a random `u64` from the system |
| `random.seeded(seed)` | an `Rng` whose numbers depend only on `seed`: one seed, one sequence |
| `random.from_entropy(entropy)` | an `Rng` seeded from the system: different each run |
| `rng.next_u64()` | the next 64 random bits |
| `rng.uniform()` | an `f64` in [0, 1) |
| `rng.below(n)` | an `i64` in `0..n`, without bias; panics unless `n > 0` |
| `rng.between(lo, hi)` | an `i64` in `lo..hi`; panics unless `lo < hi` |
| `rng.normal(mean, std)` | a normally distributed float |
| `rng.chance(p)` | `true` with probability `p` |
| `rng.choice(xs)` | a random item: `Some(x)`, `None` for an empty array |
| `rng.shuffle(xs)` | a copy of `xs` in random order |

`Rng` is fast but not cryptographically secure: use `entropy.bytes(n)` for
keys, tokens and passwords. Every method of `Rng` takes `mut self`, so keep
it in a `let mut`.

### std.task

| Item | What it does |
| --- | --- |
| `task.sleep(ms)` | pauses this task for `ms` milliseconds; other tasks run |
| `task.yield_now()` | lets other ready tasks run first |

## Examples

```lugha
import std.math;
import std.random;
import std.time;

fun main(console: Console) {
    let big: i64 = 300;
    console.println("{big.truncate<u8>()} {big.try_into<u8>().is_none()} {(2.7).floor<i64>()} {(1.0e20).saturate<i64>() == i64.MAX}");
    console.println("{u8.MAX.wrapping_add(1)} {i32.MAX.checked_add(1).is_none()} {u8.MAX.saturating_add(9)}");
    console.println("{4.kb()} {0b1011.count_ones()} {math.clamp(15, 0, 10)} {math.abs(-3)}");
    console.println("{math.sqrt(16.0)} {math.round(-2.5)} {math.max("pear", "plum")}");

    let mut rng = random.seeded(42);
    let roll = rng.between(1, 7);
    console.println("{roll >= 1 && roll <= 6} {rng.shuffle([1, 2, 3]).len} {rng.choice(["a"]).unwrap()}");

    let moon = time.civil(-14182940000, 0).unwrap();
    console.println("{moon.year}-{moon.month:02}-{moon.day:02} {moon.hour:02}:{moon.minute:02} weekday {moon.weekday}");
    console.println(time.from_civil(moon, 0).unwrap());
    let bad = time.Civil { year: 2026, month: 2, day: 31, weekday: 0, hour: 0, minute: 0, second: 0, ms: 0 };
    match time.from_civil(bad, 0) {
        .Ok(_) => console.println("?"),
        .Err(e) => console.println(e),
    }
}
```
```output
44 true 2 true
0 true 255
4096 3 10 3
4.0 -3.0 plum
true 3 a
1969-07-20 20:17 weekday 7
-14182940000
no day 31 in month 2 of 2026
```

```lugha
import std.task;

fun main(console: Console, clock: Clock, entropy: Entropy) {
    let start = clock.monotonic_ms();
    task.sleep(20);
    task.yield_now();
    console.println(clock.monotonic_ms() - start >= 20);
    console.println("{entropy.bytes(16).len} {clock.now_ms() > 0}");
}
```
```output
true
16 true
```
