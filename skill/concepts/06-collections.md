---
title: Collections
prerequisites: [values-and-types, functions, structs-and-enums, match-and-patterns, errors]
summary: Arrays, maps and sets: building, reading, changing and looping over them.
---

# Collections

## The idea

Three collections are built in. An array, `T[]`, holds values in order and
can grow. A `Map<K, V>` finds a value by its key. A `Set<T>` holds each value
once. All three are values like any other: assigning one gives an
independent copy (made cheaply, only when one side changes), and changing
one needs `let mut`.

## Example

```lugha
fun main(console: Console) {
    let mut scores = [70, 85];
    scores.push(92);
    console.println("{scores.len} scores, first {scores[0]}");
    let passed = scores.filter(fun(s) = s >= 80);
    let total = scores.fold(0, fun(sum, s) = sum + s);
    console.println("{passed.len} passed, total {total}");

    let mut ages = Map { "ann": 31, "bo": 40 };
    ages["cy"] = 25;
    ages["bo"] += 1;
    match ages.get("dee") {
        .Some(a) => console.println(a),
        .None => console.println("no dee"),
    }
    for (name, age) of ages {
        console.println("{name}: {age}");
    }

    let mut seen = Set { "red", "blue" };
    let added = seen.insert("red");
    console.println("{added} {seen.has("blue")} {seen.len}");
}
```
```output
3 scores, first 70
2 passed, total 247
no dee
ann: 31
bo: 41
cy: 25
false true 2
```

## Rules

- **Arrays:**
  - Built with `[1, 2, 3]`, or `[value; count]` for `count` copies. An empty
    one needs its type: `let xs: i64[] = [];`.
  - `xs.len` is the length; `xs[i]` reads and `xs[i] = v` writes. An index
    outside `0..len` panics.
  - `push(v)`, `pop()` (an `Option`) and `clear()` change the array, which
    must be `let mut`.
  - `map(f)`, `filter(f)`, `fold(init, f)`, `any(f)`, `all(f)`,
    `sort()` (for `Ord` items) and `sort_by(less)` give new values and leave
    the array as it was. `names.join(", ")` joins a `string[]`.
  - `a + b` is a new array, `a`'s items then `b`'s.
  - For more, `xs.iter()` gives an iterator with `take`, `skip`, `find`,
    `enumerate` and the rest (see iterators).
- **Maps:**
  - Built with `Map { key: value, … }`; empty, `Map<string, i64> {}` or
    `Map {}` where the type is known.
  - `m[k]` reads (and panics when `k` is missing); `m.get(k)` gives an
    `Option`; `m.has(k)` asks.
  - `m[k] = v` adds or replaces; `m[k] += 1` changes an existing value in
    place; `m.remove(k)` takes one out.
  - `m.len`, `m.keys()`, `m.values()`; `for (k, v) of m` loops over the
    entries.
- **Sets:** `Set { a, b }`; `insert(x)` (true when it was new),
  `remove(x)`, `has(x)`, `len`; `a.union(b)`, `a.intersect(b)`, `a.minus(b)`.
- **Order:** maps and sets keep insertion order, in loops, printing,
  `keys()` and `values()`.
- **Keys:** a map's keys and a set's values must be `Key`: numbers, `bool`,
  `char`, `string`, and your own types declared `with Key`. Floats can't be
  keys.
- **A loop sees a copy:** `for x of xs` loops over the array as it was when
  the loop started, so the body may change `xs`. `for x of mut xs` changes
  each item in place.
- **Printing:** a collection prints through `.debug()`:
  `[1, 2]`, `Map { "a": 1 }`.

```lugha
struct Point { x: i64, y: i64 } with Key

fun main(console: Console) {
    let mut grid: Map<Point, char> = Map {};
    grid[Point { x: 0, y: 0 }] = '#';
    grid[Point { x: 1, y: 0 }] = '.';
    console.println(grid.len);

    let mut xs = [3, 1, 2];
    for x of mut xs {
        x *= 10;
    }
    console.println(xs.sort().debug());
    let words = ["b", "a"] + ["c"];
    console.println(words.sort().join("-"));
    let firsts = Set { 1, 2, 3 }.intersect(Set { 2, 3, 4 });
    console.println(firsts.debug());
}
```
```output
2
[10, 20, 30]
a-b-c
Set { 2, 3 }
```

## Mistakes

- **An index past the end** panics: check `xs.len` first, or use `pop` or a
  loop.

```lugha exit 101
fun main(console: Console) {
    let xs = [1, 2, 3];
    console.println(xs[3]);
}
```
```output
```

- **Changing an array that isn't `let mut`** is E0501.

```lugha error E0501
fun main() {
    let xs = [1, 2];
    xs.push(3);
}
```

- **A key type without `Key`** is E0422: add `with Key` to your type.

```lugha error E0422
struct Point { x: i64, y: i64 }

fun main() {
    let m = Map { Point { x: 0, y: 0 }: "origin" };
}
```

## Exercises

1. Count how often each word appears in `"the cat and the hat"` with a
   `Map<string, i64>`, and print the counts in order of first appearance.
2. Remove the duplicates from `[3, 1, 3, 2, 1]` with a `Set`, keeping the
   first appearance of each.
3. Sort an array of `struct Player { name: string, score: i64 }` by score,
   highest first, with `sort_by`.
