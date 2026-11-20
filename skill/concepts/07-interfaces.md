---
title: Interfaces
prerequisites: [values-and-types, functions, structs-and-enums, match-and-patterns, errors, collections]
summary: Shared behaviour: interface declarations, impl blocks, interface values and parents.
---

# Interfaces

## The idea

An interface names a set of methods. A type takes part only by saying so,
with `impl Interface for Type { … }`, never just by having methods with the
right names. Code can then work with "any `Shape`", either through an
interface value, which holds any type that implements it, or through a
generic bound (next concept).

## Example

```lugha
interface Shape {
    fun area(self): f64;
    fun name(self): string;
    fun describe(self): string = "{self.name()} of area {self.area()}";
}

struct Circle { r: f64 }
struct Square { side: f64 }

impl Shape for Circle {
    fun area(self): f64 = 3.0 * self.r * self.r;
    fun name(self): string = "circle";
}

impl Shape for Square {
    fun area(self): f64 = self.side * self.side;
    fun name(self): string = "square";
    fun describe(self): string = "a {self.side}x{self.side} square";
}

fun main(console: Console) {
    let shapes: Shape[] = [Circle { r: 1.0 }, Square { side: 2.0 }];
    let mut total = 0.0;
    for s of shapes {
        console.println(s.describe());
        total += s.area();
    }
    console.println(total);
}
```
```output
circle of area 3.0
a 2.0x2.0 square
7.0
```

## Rules

- **Declaring:** `interface Name { fun item(self, …): R; … }`. An item may
  have a default body (`= expr;` or a block), which an `impl` may replace.
  In an interface, `Self` names the implementing type:
  `fun bigger(self, other: Self): bool;`.
- **Implementing:** `impl Shape for Circle { … }` defines every item
  without a default, each with exactly the interface's signature. A type may
  implement many interfaces, each in its own block.
- **Where an `impl` lives:** in the module that declares the interface or
  the one that declares the type.
- **Interface values:** where an interface type is expected, a value of
  any implementing type converts to it: `let s: Shape = Circle { r: 1.0 };`,
  `Shape[]`, a `Shape` parameter. This is the one conversion Lugha makes on
  its own. A `Circle[]` is not a `Shape[]`; build the array as `Shape[]`.
- **Calls** through an interface value run the concrete type's method. The
  value inside has no fields you can reach.
- **Parents:** `interface Pet: Named + Sendable { … }`. Implementing `Pet`
  needs `Named` too, and a `Pet` value converts to a `Named`.
- **Built-in interfaces you implement:**
  - `Display` (`fun show(self): string`) makes a type printable;
  - `Error` (empty, needs `Display`) makes it an error;
  - `Add`, `Sub`, `Mul`, `Div`, `Rem` and `Neg` give it `+`, `-`, `*`, `/`,
    `%` and unary `-`: `impl Add for V { fun add(self, o: V): V = … }`;
  - `Iter<T>` makes it loopable (see iterators);
  - `Eq`, `Ord` and `Key` are usually derived with `with`, and can be
    written by hand.
- **Interface values have no `==`** and don't print unless the interface
  extends `Display`.

```lugha
struct Money { cents: i64 }

impl Add for Money {
    fun add(self, o: Money): Money = Money { cents: self.cents + o.cents };
}

impl Display for Money {
    fun show(self): string = "${self.cents / 100}.{self.cents % 100:02}";
}

interface Named {
    fun name(self): string;
}

interface Pet: Named {
    fun sound(self): string;
}

struct Dog {}

impl Named for Dog {
    fun name(self): string = "dog";
}

impl Pet for Dog {
    fun sound(self): string = "woof";
}

fun introduce(n: Named): string = "this is a {n.name()}";

fun main(console: Console) {
    let price = Money { cents: 1250 } + Money { cents: 305 };
    console.println(price);
    let p: Pet = Dog {};
    console.println("{p.sound()}; {introduce(p)}");
}
```
```output
$15.55
woof; this is a dog
```

## Mistakes

- **Methods with the right names don't make an implementation:** passing
  such a type where the interface is expected is E0422. Write the `impl`
  block.

```lugha error E0422
interface Shape {
    fun area(self): f64;
}

struct Circle { r: f64 }

impl Circle {
    fun area(self): f64 = 3.0 * self.r * self.r;
}

fun main() {
    let s: Shape = Circle { r: 1.0 };
}
```

- **An `impl` missing an item, or with another signature,** is E0422 at
  the block, listing what is wrong.

```lugha error E0422
interface Shape {
    fun area(self): f64;
    fun name(self): string;
}

struct Circle { r: f64 }

impl Shape for Circle {
    fun area(self): f64 = 3.0 * self.r * self.r;
}

fun main() {}
```

- **Comparing interface values** with `==` is E0404.

```lugha error E0404
interface Named {
    fun name(self): string;
}

fun same(a: Named, b: Named): bool = a == b;

fun main() {}
```

## Exercises

1. Write `interface Animal` with `name` and `legs`, implement it for two
   types, and print the total legs of an `Animal[]`.
2. Give `struct Vec2 { x: f64, y: f64 }` `+` and `*` by an `f64`
   (`impl Mul<f64> for Vec2`), and `Display`.
3. Add a default method `fun loud(self): string` to `Animal` that shouts its
   name, and override it for one type.
