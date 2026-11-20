---
title: Iterators
prerequisites: [values-and-types, functions, structs-and-enums, match-and-patterns, errors, collections, interfaces, generics]
summary: for … of over anything with impl Iter, and the helpers map, filter, take, enumerate, zip and collect.
---

# Iterators

## The idea

An iterator gives values one at a time. Any type becomes one by
implementing `Iter<T>`, whose one method is `fun next(mut self): Option<T>`:
`Some(value)` for the next value, `None` when it is done. `for … of` loops
over it, and helpers such as `map`, `filter` and `take` build new iterators
from it lazily, computing each value only when it is asked for.

## Example

```lugha
struct Countdown { n: i64 }

impl Iter<i64> for Countdown {
    fun next(mut self): Option<i64> {
        if self.n == 0 {
            return None;
        }
        self.n -= 1;
        Some(self.n + 1)
    }
}

fun main(console: Console) {
    for n of (Countdown { n: 3 }) {
        console.print("{n} ");
    }
    console.println("go");

    let evens = (Countdown { n: 10 })
        .filter(fun(n) = n % 2 == 0)
        .map(fun(n) = n * n)
        .take(3)
        .collect();
    console.println(evens.debug());

    let names = ["ann", "bo", "cy"];
    for p of names.iter().enumerate() {
        console.print("{p.i}={p.value} ");
    }
    console.println("");
    for (name, age) of names.iter().zip([31, 40, 25].iter()) {
        console.print("{name}:{age} ");
    }
    console.println("");
}
```
```output
3 2 1 go
[100, 64, 36]
0=ann 1=bo 2=cy 
ann:31 bo:40 cy:25 
```

## Rules

- **Making an iterator:** `impl Iter<T> for X { fun next(mut self):
  Option<T> { … } }`. A method merely named `next`, without the block,
  doesn't make a type loopable.
- **What `for x of e` loops over:**
  - an array (its items) or a map or set (its entries or values);
  - a string, which gives its bytes as `u8`; `s.chars()` gives characters;
  - any iterator, an `Iter<T>` value, or a `T: Iter<…>` parameter.
  A struct literal after `of` needs parentheses: `for n of (Countdown { n: 3 })`.
- **Sources:** `xs.iter()` (an array's items), `s.chars()`, `s.bytes()`,
  `range(a, b)` (from `a` up to `b − 1`), and your own types.
- **Helpers that make a new iterator,** lazily:
  - `map(f)`, `filter(f)`;
  - `take(n)`, `skip(n)`, `take_while(f)`, `skip_while(f)`;
  - `enumerate()`, giving items with `.i` (from 0) and `.value`;
  - `zip(other)`, giving tuples; `chain(other)`, one after the other;
  - `flat_map(f)`.
- **Helpers that use it up:** `collect()` (into an array), `count()`,
  `fold(init, f)`, `any(f)`, `all(f)`, `find(f)`, `position(f)`, `sum()`,
  `min()`, `max()`; `collect_map()` and `collect_set()`.
- **An array's own `map` and `filter`** give arrays at once; call `.iter()`
  first for the lazy helpers and the ones arrays lack (`take`, `find`,
  `enumerate`, `zip`).
- **The loop works on a copy** of the iterator, so the variable it came
  from is unchanged after the loop.
- **Generic code** takes any iterator through a bound:
  `fun total<I: Iter<i64>>(it: I): i64`.

```lugha
struct Fib { a: i64, b: i64 }

impl Iter<i64> for Fib {
    fun next(mut self): Option<i64> {
        let out = self.a;
        self.a = self.b;
        self.b = out + self.b;
        Some(out)
    }
}

fun total<I: Iter<i64>>(it: I): i64 {
    let mut sum = 0;
    for x of it {
        sum += x;
    }
    sum
}

fun main(console: Console) {
    let fib = Fib { a: 0, b: 1 };
    console.println(fib.take(10).collect().debug());
    console.println(fib.skip_while(fun(x) = x < 100).find(fun(x) = x % 2 == 0).unwrap());
    console.println(total(range(1, 5)));
    console.println("héllo".chars().count());
    let lengths = ["a", "bb", "ccc"].iter().map(fun(s) = s.len).collect_set();
    console.println(lengths.has(2));
}
```
```output
[0, 1, 1, 2, 3, 5, 8, 13, 21, 34]
144
10
5
true
```

## Mistakes

- **A type with only a `next` method** can't be looped over: E0411, whose
  help names the missing `impl Iter<T>` block.

```lugha error E0411
struct Ticks { n: i64 }

impl Ticks {
    fun next(mut self): Option<i64> = None;
}

fun main() {
    for t of (Ticks { n: 0 }) {
    }
}
```

- **Calling an iterator helper on an array without `.iter()`**, such as
  `xs.take(2)`, is E0415: write `xs.iter().take(2)`.

```lugha error E0415
fun main() {
    let xs = [1, 2, 3];
    let first = xs.take(2);
}
```

## Exercises

1. Write an iterator `Squares { n: i64 }` giving 1, 4, 9, … forever, and
   print the first five squares above 50.
2. Use `enumerate` to print each line of a string with its line number.
3. Write `fun pairs(xs: i64[]): (i64, i64)[]` giving each item with the
   next one, using `zip` and `skip`.
