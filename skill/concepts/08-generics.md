---
title: Generics
prerequisites: [values-and-types, functions, structs-and-enums, match-and-patterns, errors, collections, interfaces]
summary: Type parameters on functions and types, with bounds that say what a T can do.
---

# Generics

## The idea

A generic function or type works for many types at once: `fun first<T>(xs:
T[]): Option<T>` works for any array. A bare `T` can only be stored, passed
and returned; a **bound** says what else it can do, such as `T: Ord` for
`<` or `T: Shape` for a `Shape`'s methods. Each use is compiled for its
concrete type, so generics cost nothing at run time.

## Example

```lugha
fun largest<T: Ord>(xs: T[]): Option<T> {
    if xs.len == 0 {
        return None;
    }
    let mut best = xs[0];
    for x of xs {
        if x > best {
            best = x;
        }
    }
    Some(best)
}

struct Stack<T> { items: T[] }

impl<T> Stack<T> {
    fun new(): Stack<T> = Stack { items: [] };
    fun push(mut self, x: T) {
        self.items.push(x);
    }
    fun pop(mut self): Option<T> = self.items.pop();
    fun size(self): i64 = self.items.len;
}

fun main(console: Console) {
    let top = largest([3, 9, 4]).unwrap();
    let word = largest(["pear", "apple", "plum"]).unwrap();
    console.println("{top} {word}");
    let mut s: Stack<string> = Stack.new();
    s.push("a");
    s.push("b");
    console.println("{s.pop().unwrap()} {s.size()}");
}
```
```output
9 plum
b 1
```

## Rules

- **Type parameters** come after the name: `fun f<T>(…)`,
  `struct Pair<A, B> { … }`, `enum Tree<T> { … }`, and for an `impl` block
  right after `impl`: `impl<T> Stack<T> { … }`.
- **Bounds:** `T: Eq` (`==`), `T: Ord` (`<`, `sort`), `T: Key` (map keys),
  `T: Display` (printing), `T: Num` (arithmetic), or any interface. Several
  join with `+`: `T: Ord + Display`. An `impl` block may bound its
  parameters too: `impl<T: Num> Grid<T> { … }`.
- **Inside the body,** a `T` can do only what its bounds allow. `==` without
  `Eq`, `+` without `Num`, or a method no bound declares is an error.
- **Type arguments** are usually inferred from the arguments or the
  expected type. Write them when nothing tells: `Stack<i64>.new()`,
  `let s: Stack<i64> = Stack.new();`, `largest<string>([])`.
- **Using a type** that misses a bound is an error at the call, naming the
  missing ability.
- **The expected type steers inference:** a generic call written directly
  as `console.println(…)`'s argument is expected to give a `Display`, so its
  `T` is taken as `Display`. Bind the result first (`let top = largest(xs).unwrap();`)
  or put it in a string (`"{…}"`).
- **Generic types are written with their arguments** in type positions:
  `Stack<i64>`, `Option<Point>`, never a bare `Stack`.
- **Bound or interface value?**
  - A bound (`fun total<T: Shape>(xs: T[])`) keeps one concrete type and
    calls it directly.
  - An interface value (`fun total(xs: Shape[])`) can mix types in one
    array.

```lugha
enum Tree<T> {
    Leaf,
    Node(Tree<T>, T, Tree<T>),
}

fun insert<T: Ord>(t: Tree<T>, x: T): Tree<T> = match t {
    .Leaf => Tree.Node(Tree.Leaf, x, Tree.Leaf),
    .Node(l, v, r) => if x < v {
        Tree.Node(insert(l, x), v, r)
    } else {
        Tree.Node(l, v, insert(r, x))
    },
};

fun in_order<T: Display>(t: Tree<T>): string = match t {
    .Leaf => "",
    .Node(l, v, r) => "{in_order(l)}{v} {in_order(r)}",
};

fun sum_all<T: Num>(xs: T[], zero: T): T {
    let mut total = zero;
    for x of xs {
        total = total + x;
    }
    total
}

fun main(console: Console) {
    let mut t: Tree<i64> = Tree.Leaf;
    for x of [5, 2, 8, 1] {
        t = insert(t, x);
    }
    console.println(in_order(t));
    console.println("{sum_all([1, 2, 3], 0)} {sum_all([0.5, 0.25], 0.0)}");
}
```
```output
1 2 5 8 
6 0.75
```

## Mistakes

- **`==` on a `T` without `Eq`** is E0404: add the bound, `<T: Eq>`.

```lugha error E0404
fun contains<T>(xs: T[], x: T): bool {
    for y of xs {
        if y == x {
            return true;
        }
    }
    false
}

fun main() {}
```

- **A type that can't be inferred** is E0412: write the type.

```lugha error E0412
struct Stack<T> { items: T[] }

impl<T> Stack<T> {
    fun new(): Stack<T> = Stack { items: [] };
}

fun main() {
    let s = Stack.new();
}
```

- **Calling with a type that misses a bound** is E0422 for your own types:
  derive or implement it.

```lugha error E0422
struct Point { x: i64 }

fun biggest<T: Ord>(xs: T[]): T = xs[0];

fun main() {
    let p = biggest([Point { x: 1 }]);
}
```

## Exercises

1. Write `fun count_of<T: Eq>(xs: T[], x: T): i64`.
2. Write `struct Pair<A, B> { first: A, second: B }` with a method `swap`
   giving a `Pair<B, A>`.
3. Write a generic `Queue<T>` with `push`, `pop` (from the front) and
   `is_empty`.
