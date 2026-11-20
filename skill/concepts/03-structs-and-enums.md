---
title: Structs and enums
prerequisites: [values-and-types, functions]
summary: Your own types: structs with fields, enums with variants, methods in impl blocks, and with derives.
---

# Structs and enums

## The idea

A `struct` groups named fields; an `enum` is one of several variants, each
carrying its own values. Methods live in an `impl` block for the type.
Abilities such as `==`, ordering and printing are given to a type on
purpose, with `with` for the standard ones or an `impl` block written by
hand.

## Example

```lugha
struct Point { x: f64, y: f64 } with Eq

impl Point {
    fun origin(): Point = Point { x: 0.0, y: 0.0 };
    fun dist2(self, o: Point): f64 {
        let dx = self.x - o.x;
        let dy = self.y - o.y;
        dx * dx + dy * dy
    }
    fun shift(mut self, dx: f64) {
        self.x += dx;
    }
}

impl Display for Point {
    fun show(self): string = "({self.x}, {self.y})";
}

enum Shape {
    Circle(f64),
    Rect(f64, f64),
    Empty,
}

fun area(s: Shape): f64 = match s {
    .Circle(r) => 3.0 * r * r,
    .Rect(w, h) => w * h,
    .Empty => 0.0,
};

fun main(console: Console) {
    let mut p = Point { x: 3.0, y: 4.0 };
    console.println(p.dist2(Point.origin()));
    p.shift(1.0);
    console.println(p);
    let q = Point { y: 0.0, ..p };
    console.println("{q} {q == Point { x: 4.0, y: 0.0 }}");
    for s of [Shape.Circle(1.0), Shape.Rect(2.0, 3.0), Shape.Empty] {
        console.println(area(s));
    }
}
```
```output
25.0
(4.0, 4.0)
(4.0, 0.0) true
3.0
6.0
0.0
```

## Rules

- **Structs:** `struct Name { field: Type, … }` at the top level. A literal
  sets every field, in any order: `Point { x: 1.0, y: 2.0 }`, or ends with
  `..other` to take the rest from another value.
- **Fields** are read with `p.x` and changed with `p.x = 1.0` when `p` is
  `let mut`.
- **Methods:** `impl Point { … }` holds functions for the type.
  - `self` takes the value (a copy).
  - `mut self` lets the method change the caller's value, which must be
    `let mut`.
  - Without `self` it is called on the type: `Point.origin()`.
  - Inside the block, `Self` names the type.
- **Enums:** `enum Name { A, B(T), C(T, U) }`. Values are built through the
  enum's name: `Shape.Circle(2.0)`, `Shape.Empty`. An enum may hold itself:
  `enum List { Nil, Cons(i64, List) }` needs no box.
- **Taking an enum apart** is done with `match` (next concept); a `match`
  must cover every variant.
- **Abilities with `with`:** after the type, `with Eq` gives `==` and `!=`,
  `with Ord` gives `<` and sorting (and `Eq`), `with Key` lets it be a map key
  or set item (and `Eq`). Every field must have the ability too.
- **Printing:** `impl Display for T { fun show(self): string = … }` makes
  `T` printable in `println` and `"{…}"`. Without it, `x.debug()` gives a
  readable form of any value: `Point { x: 1.0, y: 2.0 }`.
- **Tuples** group values without a declaration: `let pair = (1, "one");`
  read with `pair.0` and `pair.1`.
- **Generic types** take parameters: `struct Pair<T> { a: T, b: T }`; see
  the generics concept.

```lugha
enum List { Nil, Cons(i64, List) }

fun sum(l: List): i64 = match l {
    .Nil => 0,
    .Cons(x, rest) => x + sum(rest),
};

struct Card { rank: i64, suit: char } with Ord

fun main(console: Console) {
    let l = List.Cons(1, List.Cons(2, List.Cons(3, List.Nil)));
    console.println(sum(l));
    let a = Card { rank: 10, suit: 'h' };
    let b = Card { rank: 3, suit: 's' };
    console.println("{a > b} {b.debug()}");
    let pair = (7, "seven");
    console.println("{pair.0} {pair.1}");
}
```
```output
6
true Card { rank: 3, suit: 's' }
7 seven
```

## Mistakes

- **Leaving a field out of a literal** is E0413.

```lugha error E0413
struct Point { x: f64, y: f64 }

fun main() {
    let p = Point { x: 1.0 };
}
```

- **Comparing a type without `Eq`** is E0422: add `with Eq` to it.

```lugha error E0422
struct Point { x: f64, y: f64 }

fun main() {
    let same = Point { x: 1.0, y: 2.0 } == Point { x: 1.0, y: 2.0 };
}
```

- **Printing a type without `Display`** is E0422: write `impl Display`, or
  print `x.debug()`.

```lugha error E0422
struct Point { x: f64, y: f64 }

fun main(console: Console) {
    console.println(Point { x: 1.0, y: 2.0 });
}
```

## Exercises

1. Write `struct Rect { w: f64, h: f64 }` with methods `area(self)` and
   `scale(mut self, k: f64)`, and print a rectangle before and after scaling.
2. Write `enum Coin { Penny, Nickel, Dime, Quarter }` and a function giving
   each coin's value in cents.
3. Give `Rect` `impl Display` so it prints as `3.0x4.0`.
