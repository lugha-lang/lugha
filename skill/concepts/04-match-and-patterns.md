---
title: Match and patterns
prerequisites: [values-and-types, functions, structs-and-enums]
summary: Taking values apart with match, if let, while let and let, with guards and wildcards.
---

# Match and patterns

## The idea

A pattern describes the shape of a value and names its parts. `match` tries
patterns in order and runs the first arm that fits; the compiler checks
that every possible value is covered. `if let` and `while let` try one
pattern, and `let` takes apart a value that always fits.

## Example

```lugha
enum Shape { Circle(f64), Rect(f64, f64), Empty }

fun describe(s: Option<Shape>): string = match s {
    .Some(.Circle(r)) if r > 1.0 => "big circle",
    .Some(.Circle(_)) => "small circle",
    .Some(.Rect(w, h)) if w == h => "square",
    .Some(.Rect(_, _)) => "rect",
    .Some(.Empty) => "empty",
    .None => "nothing",
};

fun grade(score: i64): char = match score {
    s if s > 100 || s < 0 => '?',
    s if s >= 90 => 'A',
    s if s >= 75 => 'B',
    _ => 'C',
};

fun main(console: Console) {
    console.println(describe(Some(Shape.Circle(2.0))));
    console.println(describe(Some(Shape.Rect(3.0, 3.0))));
    console.println(describe(None));
    console.println("{grade(95)} {grade(80)} {grade(120)}");

    let (q, r) = (17 / 5, 17 % 5);
    console.println("{q} {r}");

    let found: Option<i64> = Some(4);
    if let .Some(n) = found {
        console.println("found {n}");
    }

    let mut stack = [1, 2, 3];
    while let .Some(top) = stack.pop() {
        console.print("{top} ");
    }
    console.println("");
}
```
```output
big circle
square
nothing
A B ?
3 2
found 4
3 2 1 
```

## Rules

- **`match value { pattern => expr, … }`** is an expression. Its arms are
  tried top to bottom, and it must cover every value (E0417 names what is
  missing).
- **Patterns:**
  - a literal integer, string or `bool`: `0`, `"yes"`, `true` (a `char`
    is compared in a guard instead: `c if c == 'a' =>`);
  - a name, which binds the value: `n` (`mut n` gives the arm its own copy
    to change);
  - `_`, which matches anything and binds nothing;
  - a variant with a leading dot: `.Circle(r)`, `.Some(x)`, `.None`,
    `.Ok(v)`, `.Err(e)`; its parts are patterns too, so they nest:
    `.Some(.Circle(r))`;
  - a tuple: `(a, b)`, `(0, _)`;
  - alternatives: `.Rect(_, _) | .Empty`, `"q" | "quit"`.
- **Structs aren't patterns:** bind the whole value and read its fields,
  with a guard when the arm depends on them.
- **Ranges aren't patterns:** use a guard, `n if n >= 90 =>`.
- **Guards:** `pattern if condition =>` fits only when the condition holds.
  A guarded arm doesn't count towards covering its pattern.
- **Every variant in a pattern takes the dot**, `.Some(x)` and `.None`
  included; building a value doesn't (`Some(x)`, `Shape.Circle(2.0)`).
- **`if let pattern = value { } else { }`** runs the block when the pattern
  fits. **`while let`** repeats while it fits.
- **`let pattern = value;`** works when the pattern always fits, such as a
  tuple: `let (a, b) = pair;`.
- **Arms give values of one type;** an arm may `return`, `break`,
  `continue` or `panic` instead.

```lugha
fun where_is(p: (i64, i64)): string = match p {
    (0, 0) => "origin",
    (0, _) => "on the y axis",
    (_, 0) => "on the x axis",
    (x, y) => "at {x},{y}",
};

fun command(word: string): string = match word {
    "q" | "quit" => "leave",
    "h" | "help" => "show help",
    _ => "unknown",
};

fun kind(c: char): string = match c {
    c if c >= 'a' && c <= 'z' => "letter",
    c if c >= '0' && c <= '9' => "digit",
    _ => "other",
};

fun main(console: Console) {
    console.println(where_is((0, 5)));
    console.println(where_is((2, 3)));
    console.println("{command("quit")} {command("x")}");
    console.println("{kind('e')} {kind('7')} {kind('!')}");
}
```
```output
on the y axis
at 2,3
leave unknown
letter digit other
```

## Mistakes

- **A match that misses a case** is E0417; add the arm, or `_ => …` when
  the rest share one answer.

```lugha error E0417
enum Shape { Circle(f64), Empty }

fun area(s: Shape): f64 = match s {
    .Circle(r) => r * r,
};

fun main() {}
```

- **A variant without its dot** in a pattern is E0212: write `.Some(x)`,
  not `Some(x)`.

```lugha error E0212
fun main() {
    let o: Option<i64> = Some(1);
    let n = match o {
        Some(x) => x,
        .None => 0,
    };
}
```

## Exercises

1. Write `fun fizz(n: i64): string` with one `match` on `(n % 3, n % 5)`.
2. Given `enum Cmd { Move(i64, i64), Say(string), Quit }`, write a function
   that describes a command, with a guard for a `Move` of zero steps.
3. Use `while let` to pop and print every value of an array.
