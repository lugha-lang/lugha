---
title: Functions
prerequisites: [values-and-types]
summary: fun, parameters and results, blocks as values, control flow, closures and function values.
---

# Functions

## The idea

A function is declared with `fun`, its parameters' types after their names
and its result's type after the parentheses. A block's last expression,
without a semicolon, is its value, so most functions need no `return`.
Functions are values too: they can be stored, passed and returned, and a
closure is a function written in place that can use the names around it.

## Example

```lugha
fun square(x: i64): i64 = x * x;

fun describe(n: i64): string {
    if n < 0 {
        return "negative";
    }
    let kind = if n % 2 == 0 { "even" } else { "odd" };
    "{kind} {n}"
}

fun apply_twice(f: fun(i64): i64, x: i64): i64 = f(f(x));

fun main(console: Console) {
    console.println(square(4));
    console.println(describe(7));
    console.println(describe(-1));
    let step = 3;
    let add_step = fun(x: i64): i64 = x + step;   // a closure using `step`
    console.println(apply_twice(add_step, 10));
    console.println(apply_twice(square, 3));
}
```
```output
16
odd 7
negative
16
81
```

## Rules

- **Declaring:** `fun name(a: T, b: U): R { body }`, or `fun name(…): R = expr;`
  for one expression. Without `: R` the function returns `()`, nothing.
- **Order doesn't matter:** a function may call one declared later, and
  itself.
- **The value of a block** is its last expression without a `;`. `if`,
  `match` and blocks are expressions: `let x = if c { 1 } else { 2 };`
- **`return value;`** leaves early. In a function returning `()`, `return;`.
- **Parameters are copies.** Changing one inside the function needs it
  written `mut` (`fun f(mut xs: i64[])`), and still changes only the copy.
- **Loops:**
  - `for i in 0..n` counts from 0 to n−1 (the end is left out).
  - `for x of items` goes over an array's, a string's or an iterator's values.
  - `while condition { }` repeats while the condition holds.
  - `loop { }` repeats until a `break` or `return`. A function whose only way
    out is a `return` inside a loop must use `loop`, not `while true`.
  - `break` and `continue` work in all of them.
- **Function values:** a function's name is a value of type
  `fun(i64): i64`, which can be stored in a variable, an array or a field and
  called like a function.
- **Closures:** `fun(x: i64): i64 = x + k` or `fun(x) { … }` where the
  types are known from context. A closure copies the values it uses when it
  is made; it can't assign the names it captured.
- **Named arguments don't exist;** arguments are given in order.
- **`main`** takes, in any order, `args: string[]` and the capabilities it
  needs (`console: Console`, `fs: Fs`, `net: Net` …), and returns `()` or an
  `i32` exit code.

```lugha
fun counter(start: i64): fun(i64): i64 = fun(n) = start + n;

fun first_square_over(limit: i64): i64 {
    let mut n = 1;
    loop {
        if n * n > limit {
            return n * n;
        }
        n += 1;
    }
}

fun main(args: string[], console: Console): i32 {
    let from_ten = counter(10);
    console.println(from_ten(5));
    console.println(first_square_over(50));
    let mut total = 0;
    for i in 1..5 {
        total += i;
    }
    console.println("{total} {args.len}");
    0
}
```
```output
15
64
10 1
```

## Mistakes

- **A semicolon after the last expression** makes the block `()`, so a
  function promising a value may end without one: E0503.

```lugha error E0503
fun double(x: i64): i64 {
    x * 2;
}

fun main() {}
```
- **Calling with the wrong number of arguments** is E0405.

```lugha error E0405
fun add(a: i64, b: i64): i64 = a + b;

fun main() {
    let n = add(1);
}
```

- **A closure assigning a captured name** is E0511: capture a `Ref` when
  several places must share one changing value.

```lugha error E0511
fun main() {
    let mut count = 0;
    let bump = fun() { count += 1; };
}
```

Boxes that hold each other, such as a closure stored in a `Ref` it
captures, are freed by the task's cycle collector once nothing else holds
them, so a callback slot or a doubly linked list doesn't leak.

## Exercises

1. Write `fun max3(a: i64, b: i64, c: i64): i64` without `return`.
2. Write `fun compose(f: fun(i64): i64, g: fun(i64): i64): fun(i64): i64`
   that gives a function applying `g` and then `f`.
3. Print the first ten Fibonacci numbers with a `for` loop and two `let mut`
   variables.
