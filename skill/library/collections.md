---
title: Collections
summary: Every method of arrays, Map, Set, iterators, tuples, Option and Result.
---

# Collections

Contents: [Arrays](#arrays) · [Map](#map) · [Set](#set) ·
[Iterators](#iterators) · [Tuples](#tuples) ·
[Option and Result](#option-and-result) · [Examples](#examples)

## Overview

Arrays (`T[]`), `Map<K, V>`, `Set<T>`, tuples, `Option` and `Result` are
built in, with no import. They are values: assigning one gives an
independent copy (shared until one side changes), and changing one needs
`let mut` or `mut self`. Methods that give a new collection leave the
receiver as it was.

## Reference

### Arrays

| Item | What it gives |
| --- | --- |
| `[a, b, c]`, `[v; n]` | a literal; `n` copies of `v` (`n < 0` panics) |
| `xs.len` | the length |
| `xs[i]`, `xs[i] = v` | read or write an item; `i` of any integer type; outside `0..len` panics |
| `xs.push(v)` | appends `v` (needs `let mut`) |
| `xs.pop()` | removes the last item: `Some(x)`, or `None` when empty |
| `xs.clear()` | removes every item |
| `xs.map(f)` | a new array of `f(x)` for each item |
| `xs.filter(f)` | the items `f` is true for |
| `xs.fold(init, f)` | `f(…f(f(init, xs[0]), xs[1])…)` |
| `xs.any(f)`, `xs.all(f)` | whether some, or every, item passes `f` |
| `xs.sort()` | sorted ascending, for `Ord` items; stable |
| `xs.sort_by(less)` | sorted so `less(a, b)` holds between neighbours; stable. Floats: `xs.sort_by(f64.total_order)` |
| `xs.iter()` | an iterator over a copy, for the iterator helpers below |
| `a + b` | a new array: `a`'s items, then `b`'s |
| `names.join(sep)` | a `string[]`'s items with `sep` between |
| `for x of xs`, `for x of mut xs` | loop over the items, or change each in place |

### Map

| Item | What it gives |
| --- | --- |
| `Map { k: v, … }`, `Map<K, V> {}` | a literal; keys must be `Key` |
| `m.len` | the number of entries |
| `m[k]` | the value; panics `no key … in the map` when missing |
| `m[k] = v` | adds or replaces; `m[k] += 1`, `m[k].push(x)` change an existing value in place |
| `m.get(k)` | `Some(value)` or `None` |
| `m.has(k)` | whether `k` has a value |
| `m.insert(k, v)` | the same as `m[k] = v` |
| `m.remove(k)` | removes `k`: `Some(value)` or `None` |
| `m.clear()` | removes every entry |
| `m.keys()`, `m.values()` | arrays, in insertion order |
| `m.iter()`, `for (k, v) of m` | the entries as `(K, V)` tuples, in insertion order |
| `a.same(b)`, `a == b` | whether both have the same entries, in any order |
| `Map.from_entries(keys, values)` | a map from two arrays (what a literal calls) |

### Set

| Item | What it gives |
| --- | --- |
| `Set { a, b }`, `Set<T> {}` | a literal; values must be `Key` |
| `s.len` | the number of values |
| `s.has(x)` | whether `x` is in it |
| `s.insert(x)` | adds `x`: `true` when it was new |
| `s.remove(x)` | removes `x`: `true` when it was there |
| `s.clear()` | removes every value |
| `s.iter()`, `for x of s` | the values, in insertion order |
| `a.union(b)` | `a`'s values, then `b`'s new ones |
| `a.intersect(b)`, `a.minus(b)` | the values in both; the values of `a` not in `b` |
| `a.same(b)`, `a == b` | whether both hold the same values |
| `Set.from_items(xs)` | a set from an array (what a literal calls) |

### Iterators

Anything with `impl Iter<T>`: `xs.iter()`, `m.iter()`, `s.iter()`,
`s.chars()`, `s.bytes()`, `range(a, b)` (`a` up to `b − 1`), a channel's
`Receiver`, and your own types. The helpers are lazy until one uses the
iterator up.

| Helper | What it gives |
| --- | --- |
| `it.next()` | the next value, or `None` |
| `it.map(f)`, `it.filter(f)` | each value changed; the values `f` is true for |
| `it.take(n)`, `it.skip(n)` | the first `n` values; every value after them |
| `it.take_while(f)`, `it.skip_while(f)` | values while `f` holds; values from the first that fails `f` |
| `it.enumerate()` | each value as `p.value`, with its position `p.i` from 0 |
| `it.zip(other)` | `(a, b)` tuples, ending with the shorter |
| `it.chain(other)` | `it`'s values, then `other`'s |
| `it.flat_map(f)` | the values of each iterator `f` gives |
| `it.collect()` | every value in an array |
| `it.collect_map()`, `it.collect_set()` | a `Map` from `(K, V)` values; a `Set` |
| `it.count()` | how many values |
| `it.fold(init, f)` | the values folded from `init` |
| `it.any(f)`, `it.all(f)` | whether some, or every, value passes `f` |
| `it.find(f)`, `it.position(f)` | the first value passing `f`; its position (both `Option`) |
| `it.sum()` | the total of numbers |
| `it.min()`, `it.max()` | the smallest or largest `Ord` value, `None` when empty |

### Tuples

| Item | What it gives |
| --- | --- |
| `(a, b, …)` | a tuple of two or more values; `(A, B)` is its type |
| `t.0`, `t.1` | an element; `t.0 = v` changes one in a `let mut` tuple |
| `let (a, b) = t;` | the elements, by pattern |
| `==`, `<` | element by element |

### Option and Result

| Item | What it gives |
| --- | --- |
| `o.unwrap()`, `r.unwrap()` | the value; panics on `None` or `Err` |
| `x.expect(msg)` | the same, panicking with `msg` |
| `x.unwrap_or(d)` | the value, or `d` |
| `x.map(f)` | `f` applied to the value inside, if any |
| `r.map_err(f)` | `f` applied to the error |
| `x.and_then(f)` | `f` of the value, where `f` itself gives an `Option` or `Result` |
| `o.is_some()`, `o.is_none()`, `r.is_ok()`, `r.is_err()` | which it is |
| `o.ok_or(e)` | `Some(v)` as `Ok(v)`, `None` as `Err(e)` |
| `r.ok()` | `Ok(v)` as `Some(v)`, `Err` as `None` |
| `x?` | the value, or return the `None` or `Err` from the function |

## Examples

```lugha
fun main(console: Console) {
    let words = "the cat and the hat and the bat".split(" ");
    let mut counts: Map<string, i64> = Map {};
    for w of words {
        if counts.has(w) {
            counts[w] += 1;
        } else {
            counts[w] = 1;
        }
    }
    console.println(counts.debug());
    let common = counts.iter().filter(fun(e) = e.1 > 1).map(fun(e) = e.0).collect();
    console.println(common.join(" "));
    let removed = counts.remove("cat");
    console.println("{removed.debug()} {counts.keys().len} {counts.values().fold(0, fun(a, b) = a + b)}");

    let letters = words.iter().flat_map(fun(w) = w.chars()).collect_set();
    console.println("{letters.len} {letters.has('z')}");
    let a = Set { 1, 2, 3 };
    let b = Set { 3, 4 };
    console.println("{a.union(b).debug()} {a.minus(b).debug()} {Set { 2, 1, 3 } == a}");
}
```
```output
Map { "the": 3, "cat": 1, "and": 2, "hat": 1, "bat": 1 }
the and
Some(1) 4 7
8 false
Set { 1, 2, 3, 4 } Set { 1, 2 } true
```

```lugha
struct Player { name: string, score: i64 }

fun main(console: Console) {
    let players = [
        Player { name: "ann", score: 30 },
        Player { name: "bo", score: 50 },
        Player { name: "cy", score: 40 },
    ];
    let ranked = players.sort_by(fun(a, b) = a.score > b.score);
    for p of ranked.iter().enumerate() {
        console.println("{p.i + 1}. {p.value.name}");
    }
    let scores = players.map(fun(p) = p.score);
    let total: i64 = scores.iter().sum();
    let best = scores.iter().max().unwrap();
    let at = scores.iter().position(fun(s) = s == best).unwrap();
    console.println("{total} {best} {players[at].name}");
    console.println(range(0, 10).skip(2).take_while(fun(n) = n < 6).collect().debug());
    console.println([1.5, -2.0, 0.5].sort_by(f64.total_order).debug());
    let (lo, hi) = (scores.iter().min().unwrap(), best);
    console.println("{lo}..{hi} {parse_i64("x").ok_or("bad").is_err()}");
}
```
```output
1. bo
2. cy
3. ann
120 50 bo
[2, 3, 4, 5]
[-2.0, 0.5, 1.5]
30..50 true
```
