# Lugha v18 — Draft (delta against v17)

**Status:** design approved in discussion (November 2026). Phase 1 is being built: nominal interfaces landed as milestone 164 (PRP-196); the rest is not built yet. The phases are in Part 7; each becomes one or more PRPs.

**Base:** `Language v17 Specification.md` at milestone 162 (commit `c8c6b30`). Everything not mentioned keeps its v17 meaning.

**Theme:** a language you can read the truth from. A type states what it can do (`impl`, `with`); a function's parameters state what it may touch (capabilities); a scope states how long its tasks live; nothing happens because a method has a certain name, a value looks like a pointer, or a result was quietly dropped. Underneath, v18 moves to reference counting and many cores, and actors become the unit of distribution.

**Principles** [PRI-01, PRI-02]:
- Correct by construction: whole classes of bugs are impossible, not discouraged.
- Explicit over implicit, and force good design, even at some cost in typing.
- One way to do each thing.
- Predictable: determinism is available as a mode.
- Security is non-negotiable; trust boundaries are stated.
- The spec is the source of truth; design comes first, and implementation cost decides only the order of work.

**The language has no UI concepts** [PRI-04]: GUI toolkits are libraries built on the general mechanisms in 4.8.

**Compatibility:** v18 is a breaking redesign [PRI-03]. Almost every v17 program needs changes: printing, I/O, `spawn`, interfaces, patterns, casts and map literals all change shape. A migration tool is needed and is planned separately [PRI-05].

**Reading this draft:** rules are normative and cite the decision ledger (`[INT-01]`). Code blocks illustrate; where a block and a rule disagree, the rule wins.

---

## Tour

```
import std.http;
import std.json;

struct Site { name: string, url: string } with Json

enum Status { Up(u16), Down(string) }

impl Display for Status {
    fun show(self): string = match self {
        .Up(code) => "up ({code})",
        .Down(why) => "down: {why}",
    };
}

fun check(web: http.Client, site: Site): (string, Status) {
    let status = match web.get(site.url) {
        .Ok(resp) => Status.Up(resp.status),
        .Err(e) => Status.Down("{e}"),
    };
    (site.name, status)
}

fun run(root: Scope, conf: Dir, web: http.Client, out: Console): Result<(), Error> {
    let sites = json.decode<Site[]>(json.parse(conf.read_file("sites.json")?)?)?;
    let (results, inbox) = Chan<(string, Status)>.new(16);
    let mut tally = Map<string, i64> { "up": 0, "down": 0 };

    scope s within 10_000 {
        s.spawn(fun(): Result<(), Failure> {
            scope checks limit 8 {
                for site of sites {
                    checks.spawn(fun() { results.send(check(web, site)); }());
                }
            }?;
            results.close();
            Ok(())
        }());

        for (name, status) of inbox {
            out.println("{name}: {status}");
            let key = match status { .Up(_) => "up", .Down(_) => "down" };
            tally[key] = tally[key] + 1;
        }
    }?;

    out.println("{tally["up"]} up, {tally["down"]} down");
    Ok(())
}

fun main(root: Scope, fs: Fs, net: Net, console: Console): i32 {
    let conf = fs.dir("/etc/sitewatch").read_only();
    let web = http.Client.new(net.connect_only());
    match run(root, conf, web, console.stdout_only()) {
        .Ok(()) => 0,
        .Err(e) => {
            console.stderr_only().println("sitewatch: {e}");
            1
        }
    }
}
```

`main` asks for exactly what the program may touch. `Site` states that it can become JSON; `Status` states how it is shown. Every task lives inside a scope with a deadline; the checks run at most eight at a time and feed a bounded channel, closed once their scope has finished. Every variant in a pattern takes a dot, and every failure is a `Result` that something handles.

---

# Part 1 — Language surface

## 1.1 Interfaces

**Rules**

1. A type implements an interface only through `impl I for T { … }`, which defines every item without a default and may replace defaults. Inherent `impl T { … }` blocks remain for a type's own methods. [INT-01]
2. `impl I for T` may appear only in the module that declares `I` or the module that declares `T`. So no two packages can give one type two implementations of one interface. [INT-02]
3. Built-in behaviour goes through prelude interfaces, found by implementation, never by method name: `Debug` and `Display` (printing, 1.7), `Eq` (`==`), `Key` (map keys), `Ord` (`<`, sorting), `Iter<T>` (`for … of`), the operator interfaces (1.3), and `std.json`'s `Json`. [INT-03]
4. Operators are generic interfaces: `Add<R>`, `Sub<R>`, `Mul<R>`, `Div<R>`, `Rem<R>`, each `fun op(self, other: R): Out`, and `Neg`. `a * b` uses the implementation for `a`'s type whose `R` is `b`'s exact type; a number on the left is an ordinary implementation for the number type, written in the module of the right-hand type (rule 2). `a op= b` means `a = a op b` and needs the result to have `a`'s type. [INT-04]
5. A generic interface may be implemented once per type argument: `impl From<string> for Path` and `impl From<Bytes> for Path` coexist. [INT-05]
6. When more than one implemented interface supplies a method of a given name, the call must be qualified: `Shape.area(c)`. [INT-06]
7. When a type's own method and an implemented interface's method share a name, `c.area()` calls the type's own, and `Shape.area(c)` reaches the interface's. [INT-15]
8. `impl I for T` may also be written in the module that declares the type `T` is built from: `V`'s module may write `impl I for V[]`. [INT-16]
9. `interface I: Sendable { … }` requires every implementation to be sendable (4.1); values of such an interface are sendable. [INT-07]
10. A value converts to an interface value wherever that interface type is expected, as in v17. This stays the language's one implicit conversion. [INT-08]

**Deriving**

11. `with I, J` after a struct or enum declaration implements the listed interfaces from their derive blocks. The long form, for use elsewhere in the same module, is `impl I for T = derive;`. [INT-10]
12. Only derivable interfaces may be listed: those with a derive block for that kind of type. Every field or payload must implement the interface; otherwise it is an error at the declaration naming the field. [INT-11]
13. Deriving and writing an implementation of the same interface for one type is an error. To customise, write the `impl` instead of listing it in `with`. [INT-12]
14. A generic type derives conditionally on its actual fields: `Pair<T> { a: T, b: T } with Eq` is `Eq` where `T` is; a parameter that appears in no field needs nothing. [INT-13]
15. Nothing is derived without asking: `Eq`, `Key`, `Ord` and `Json` come only from `with` or `impl`. `Display` is never derivable. `Debug` is implemented for every type (1.7). [INT-14]

**What it looks like**

```
interface Shape {
    fun area(self): f64;
    fun describe(self): string = "a shape of area {self.area()}";
}

struct Circle { r: f64 } with Eq, Json

impl Shape for Circle {
    fun area(self): f64 = 3.14159 * self.r * self.r;
}

interface Job: Sendable {
    fun run(self): Result<(), Error>;
}

let c = Circle { r: 2.0 };
let shapes: Shape[] = [c, Circle { r: 1.0 }];       // implicit conversion, rule 8
let a = Shape.area(c);                              // qualified, when ambiguous
```

**Before / after**

```
// v17: Circle is a Shape because it has `area`; `show` changes printing
impl Circle {
    fun area(self): f64 = 3.14159 * self.r * self.r;
    fun show(self): string = "circle {self.r}";
}

// v18: both are stated
impl Shape for Circle {
    fun area(self): f64 = 3.14159 * self.r * self.r;
}
impl Display for Circle {
    fun show(self): string = "circle {self.r}";
}
```

**Replaces:** structural satisfaction (v17 §4 "Interfaces"); every behaviour selected by method name (`show`, `eq`/`hash`, `cmp`, `next`, `add`…, `to_json`/`from_json`); a type's method overriding a derive block item; calling a default method through "the one visible interface" (E0415, E0430); milestone 158's several operator methods per name and milestone 159's `impl f64 { … }` (E0458). The keyword stays `interface` [INT-09].

**Why:** structural matching let types satisfy interfaces by accident, broke silently on renames, and needed a growing list of name-based special cases. One explicit mechanism covers all of them and makes intent visible. Alternative rejected: keeping structural matching for less typing.

## 1.2 Equality, keys and order

**Rules**

1. `interface Eq { fun eq(self, other: Self): bool; }` defines `==`, `!=` and `assert_eq`, and the bound `T: Eq`. [EQ-01]
2. `interface Key: Eq { fun hash(self, mut h: Hasher); }` is required of map keys and set elements. Its law: if `a == b` then their hashes are equal, and every value equals itself. [EQ-02, EQ-11]
3. `with Key` implies `with Eq`. [EQ-03]
4. Floats implement `Eq`, with IEEE rules (`NaN != NaN`), but not `Key`. [EQ-04]
5. Built-in implementations: integers, `bool`, `char` and `string` are `Eq` and `Key`; floats are `Eq` only; arrays, tuples, `Option`, `Result`, `Map` and `Set` are `Eq` or `Key` when their parts are; C handles (`extern type`, `CPtr`) are `Eq` and `Key` by address; `Ref`, tasks, channel ends, actor handles, functions and interface values are neither, and identity is `a.is(b)` where it exists. [EQ-06]
6. `assert_key_laws(a, b)` checks rule 2's law for two values in a test. [EQ-07]
7. Order splits the same way: floats have `<`, but not `Ord`; sorting floats takes an explicit order, `xs.sort_by(f64.total_order)`. [EQ-08]
8. `with Ord` orders by fields in declaration order, the first unequal field deciding. [EQ-09]
9. The prelude's `eq.la` keeps its derive blocks, split into `Eq` and `Key` blocks, with hashing through `Hasher` (3.2). [EQ-10]

**What it looks like**

```
struct Point { x: i64, y: i64 } with Key          // == and usable as a map key
struct Reading { at: i64, value: f64 } with Eq    // == only: f64 isn't Key

struct Email { address: string }

impl Eq for Email {
    fun eq(self, other: Email): bool =
        self.address.to_lower() == other.address.to_lower();
}
impl Key for Email {
    fun hash(self, mut h: Hasher) { h.add(self.address.to_lower()); }
}

@test
fun email_is_a_good_key() {
    assert_key_laws(Email { address: "Ann@x.com" }, Email { address: "ann@X.COM" });
}
```

**Before / after**

```
// v17: two methods by name, both or neither (E0454)
impl Email {
    fun eq(self, other: Email): bool = …
    fun hash(self, h: u64): u64 = …
}
let seen: Set<f64> = Set {};      // E0441: no floats in keys

// v18
impl Eq for Email { … }
impl Key for Email { … }
let seen: Set<f64> = Set {};      // error: `f64` doesn't implement `Key`
```

**Replaces:** the single `Eq` with `eq` and `hash` (v17 §4 "`Eq`", milestone 144); E0441 and E0454, both retired [EQ-04, EQ-05]; FNV-1a folding in `eq.la`; every struct being `Eq` automatically.

**Why:** equality and "safe as a key" are different promises; floats show the difference. Two interfaces replace two special-case errors. Alternative rejected: Rust's three (`PartialEq`, `Eq`, `Hash`), one more than the rules need.

## 1.3 Operators

No rules of its own: operators follow 1.1 rule 4 [INT-04].

**What it looks like**

```
struct Vec2 { x: f64, y: f64 } with Eq

impl Add<Vec2> for Vec2 { fun add(self, o: Vec2): Vec2 = Vec2 { x: self.x + o.x, y: self.y + o.y }; }
impl Mul<f64> for Vec2  { fun mul(self, k: f64): Vec2 = Vec2 { x: self.x * k, y: self.y * k }; }
impl Mul<Vec2> for f64  { fun mul(self, v: Vec2): Vec2 = v * self; }   // in Vec2's module

let v = Vec2 { x: 1.0, y: 2.0 };
let w = 2.0 * v + v * 0.5;
```

**Replaces:** milestones 158–159's rules, keeping their meaning: one implementation per right-hand type, chosen by its exact type, and a number on the left defined in the right-hand type's module. Milestone 158's literal rule (E0457: a literal must fit exactly one implementation) stays.

## 1.4 Implicit behaviour removed

**Rules**

1. Discarding a value of type `Result` is a compile error. `let _ = e;` discards one deliberately. [IMP-01]
2. Every enum variant in a pattern is written with a leading dot, the prelude's included: `.Some(x)`, `.None`, `.Ok(v)`, `.Err(e)`, `.Circle(r)`. A bare name in a pattern is always a new binding. [IMP-02, OPN-09]
3. `as` converts only where no information can be lost: widening integers, integers to floats that hold them exactly (`i32` to `f64`), `f32` to `f64`. `i64` to `f64` and `f64` to `f32` are not plain `as`. [IMP-03, OPN-03]
4. Conversions that may not fit return an `Option`: `x.try_into<u8>()`. [IMP-03, OPN-03]
5. Deliberately lossy conversions name the loss: `x.truncate<u8>()` (keep the low bits), `x.saturate<u8>()` (clamp to the range), and for floats to integers or to narrower floats, `y.round<i64>()` and its siblings. [IMP-03, OPN-03]
6. Handles share when copied: `Ref`, channel ends, tasks and actor handles. The spec says so where it defines value semantics; nothing in the syntax changes. [IMP-04]
7. Copies stay implicit, as value semantics require. A lint for large copies in loops belongs to the linter. [IMP-05]
8. These stay implicit, because they never hide behaviour: `let` type inference, literals typed from context, generic argument inference, closure parameter inference, a block's last expression as its value, and conversion to an interface value. [IMP-06]

**What it looks like**

```
let _ = fs_log.write("started");          // a deliberate discard

match lookup(id) {
    .Some(.Admin) => grant(),
    .Some(user) => greet(user),           // `user` is a binding
    .None => refuse(),
}

let small = big.saturate<u8>();           // 300 → 255
let byte = big.truncate<u8>();            // 300 → 44
let maybe = big.try_into<u8>();           // 300 → .None
let n = 2.7.round<i64>();                 // 3
```

**Before / after**

```
// v17
fs.write_file(path, data);                // error silently lost
match shape { Circle(r) => …, Empty => … }
let b = 300 as u8;                        // 44, silently

// v18
dir.write_file(name, data)?;
match shape { .Circle(r) => …, .Empty => … }
let b = 300.truncate<u8>();
```

**Replaces:** v17 §5 "Discarding the result of any call is allowed"; v17 §3's rule that a bare name in a pattern is a variant when one exists; lossy `as` (v17 §4 "Casts", and the CLAUDE.md known issue that `as` truncates or saturates silently). W0103 (unused binding) stays, but a misspelled variant is now an error rather than a binding.

**Why:** each of these hid behaviour: a lost error, a typo turned into a variable, a silent truncation. Alternative rejected for conversions: `as?` and `as!` operators, which keep a per-type-pair rule table out of sight.

## 1.5 Explicit type arguments

**Rules**

1. Type arguments may be written at a use. They are optional; inference stays the default. [TYA-01]
2. `_` leaves one argument to inference: `convert<_, string>(x)`. [TYA-02]
3. They may be written on generic calls (`max<i64>(a, b)`), function values (`max<i64>`), associated functions of generic types (`Chan<Job>.new(16)`), a method's own type parameters (`xs.map<string>(f)`), struct and collection literals (`Pair<i64> { … }`, `Map<string, i64> {}`), and variants (`Option<i64>.None`). [TYA-03]
4. After a name, `<` begins type arguments when a list of types closed by `>` is followed immediately by `(`, `.`, `{`, or the end of an expression (`)`, `,`, `;`, `]`). Otherwise `<` is a comparison. [TYA-04]
5. So `f(a < b, c > (d))` passes `a` type arguments; the error, if comparisons were meant, suggests `f((a < b), (c > d))`. [TYA-05]
6. `Stack::<i64>` stays E0207, with the help `Stack<i64>`. [TYA-06]
7. Errors: a wrong number of arguments; an argument that misses its bound (E0419); an argument that contradicts the code is a type mismatch at the argument. [TYA-07]

**What it looks like**

```
let (jobs, queue) = Chan<Job>.new(64);
let cfg = json.decode<Config>(text)?;
let names = users.map<string>(fun(u) = u.name);
let biggest = max<i64>;                   // a function value
```

**Replaces:** v17 §3 "Type arguments are never written at a use, so `<` after a name in an expression is always a comparison". E0412's help now suggests writing the arguments. Resolves the TODO.md item "Explicit type arguments".

**Why:** several v18 rules need them (channels, empty collections, lossy conversions), and inference can't always decide. Alternative rejected: `f.<T>()`, unambiguous but unlike the declaration `fun f<T>`.

## 1.6 Map and set literals

**Rules**

1. A map is written `Map { "ann": 31, "bo": 40 }`, a set `Set { 1, 2, 3 }`, and the empty ones `Map {}` and `Set {}`, typed from context or with type arguments. `#{…}` is removed. [LIT-01]
2. Map keys are expressions: `Map { name: 1 }` uses the variable `name`. If no such variable exists, the error suggests `"name"`. [LIT-02]
3. The prelude interfaces `FromEntries<K, V>` and `FromItems<T>` give any implementing type the forms `Name { k: v, … }` and `Name { a, … }`. `Map` and `Set` implement them in the prelude. [LIT-03]
4. Braces after a type name: for a struct, field names; for a `FromEntries` type, entries; for a `FromItems` type, items. A struct implementing either is built with braces only as a collection. [LIT-04]
5. In a condition these literals need parentheses, as struct literals do: `if m == (Map { "a": 1 }) { … }`. [LIT-05]
6. The debug form prints them as written: `Map { "ann": 31 }`, `Set { 1, 2 }`. [LIT-06]

**What it looks like**

```
let ages = Map { "ann": 31, "bo": 40 };
let primes = Set { 2, 3, 5, 7 };
let index = Map<string, i64[]> {};
let fruit = Bag { "apple", "apple", "pear" };     // a type implementing FromItems
```

**Before / after**

```
let ages = #{"ann": 31, "bo": 40};        // v17
let ages = Map { "ann": 31, "bo": 40 };   // v18
```

**Replaces:** the `#{` token and its grammar (v17 §3, §4 "Maps and sets"); the rule that the first entry decides map or set; the loop-label special case inside `#{…}`; the compiler's `$map_of` and `$set_of` handling (`codegen/maps.rs`). W0105 (a key written twice) applies to the new form.

**Why:** the kind is named rather than inferred from the first entry, empty maps and sets are distinct, and any collection can use the same syntax. Alternative rejected: `=>` between key and value, which is clearer to newcomers but less neat.

## 1.7 What the prelude takes over

**Rules**

1. The compiler defines how values are represented, how memory works, how control flows, and how Lugha meets C. Everything else that Lugha with a few intrinsics can express lives in the prelude or `std`. [PRE-01]
2. Every type implements `Debug`, through prelude derive blocks, giving the debug form (`Point { x: 1, y: 2 }`, strings quoted inside). `Display` is user-facing text, implemented by hand. [PRE-02]
3. String interpolation `"{x}"` and `to_string(x)` require `Display`; `debug(x)` shows the debug form of anything (2.1). [OPN-02]
4. The `Error` interface requires `Display`. [OPN-10]
5. The prelude writes ordinary `impl` blocks for built-in types (`impl string`, `impl<T> T[]`, `impl char`, `impl<T> Option<T>`, `impl<T, E> Result<T, E>`), so their methods are ordinary methods. [PRE-03]
6. Tuples are ordered by a prelude `derive Ord for tuple` block. [PRE-04]
7. `@caller_location` marks a function whose panics report its caller's location. `unwrap`, `expect`, `assert`, `assert_eq` and `assert_ne` are prelude functions marked with it, and programs may mark their own. [PRE-05]
8. The ambient I/O intrinsics are removed: `print`, `println`, `eprint`, `eprintln`, `read_line`, `read_file`, `write_file`, `append_file`, `exit` and `args`. Their replacements are capability methods (2.1). [PRE-06]
9. Sizes are written with prelude methods on integers that return bytes: `8.kb()`, `64.mb()`, `1.gb()`. [PRE-08]
10. The compiler keeps: arithmetic, casts and overflow checks; array layout, growth, copy-on-write and copying; enums, tuples, closures, interface values and their dispatch; `?`, `match`, loops, `defer` and unwinding; `spawn`, scopes, channels and actors' message types; interpolation and `format` checking; the C boundary. [PRE-07]

**What it looks like**

```
struct Money { cents: i64 } with Key, Ord

impl Display for Money {
    fun show(self): string = "{self.cents} cents";
}

debug(price);                             // Money { cents: 1250 }, with file and line
console.println("Total: {price}");        // Total: 1250 cents

@caller_location
fun assert_close(a: f64, b: f64) {
    if a - b > 1.0e-9 || b - a > 1.0e-9 {
        panic("{a} is not close to {b}");     // reported at the caller's line
    }
}
```

**Before / after**

```
println(point);                           // v17: any value, built-in form
console.println("{point}");               // v18: needs Display
debug(point);                             // v18: any value, debug form
```

**Replaces:** compiler-generated printing (`codegen/show.rs`, `show_iface.rs`; v17 §5 "How values print"); the method-name table in `check/helpers.rs`; tuple order in `codegen/ordering.rs`; `codegen/unwrap.rs` and `assertions.rs`; the I/O intrinsics in `codegen/io.rs` and `input.rs` (v17 §5 "Intrinsics"). Resolves the TODO.md item "A separate debug form".

**Why:** v17 already moved equality, hashing, JSON, maps and iterator helpers into Lugha (milestones 101–144) and showed the speed holds, since prelude code is inlined. Finishing the move leaves one mechanism, interfaces, for all built-in behaviour, and a smaller compiler.

---

# Part 2 — Capabilities

Standard-library names in the examples (`dir.read_file`, `clock.now_ms`, `http.Client`) are illustrative; `std`'s capability API is designed in the capabilities PRP. The rules below are what it must follow.

## 2.1 Capabilities

**Rules**

1. There is no ambient authority. Touching the world outside the program needs a capability value: files, the network, wall-clock time, environment variables, other processes, operating-system randomness, the console and signals. [CAP-01]
2. The capabilities are: [CAP-02]

   | Capability | Gives |
   | --- | --- |
   | `Console` | standard input, output and error |
   | `Fs`, narrowed to `Dir` | files and folders |
   | `Net` | connecting, listening, name lookup |
   | `Clock` | the real date and time |
   | `Env` | environment variables |
   | `Proc` | starting and waiting for programs |
   | `Entropy` | operating-system randomness (keys, nonces) |
   | `Signals` | signals other than Ctrl-C |

3. Not capabilities: seeded random generators (the same seed, the same numbers), scheduling time (`sleep`, `after`, a scope's `within`, owned by the runtime and scopes), and panics. [CAP-03]
4. `main` declares what it needs as parameters, matched by type, in any order, each type at most once: `args: string[]`, `root: Scope`, and any of the capabilities. Its result is `()` or an `i32` exit code. Anything else is E0304. [CAP-04]
5. Capabilities are only ever passed as ordinary parameters and fields: never carried by a scope, never implicit. [CAP-05]
6. Narrowing makes a weaker capability from a stronger one, and is one-way: [CAP-06]

   | Narrowing | Gives |
   | --- | --- |
   | `fs.dir(path)` | a `Dir`: that folder and below, never above it (2.2) |
   | `dir.read_only()` | the same folder, reading only |
   | `net.only("host:port")` | connections to one host (2.2) |
   | `net.connect_only()`, `net.listen_only(":8080")` | connecting only; listening on one address only |
   | `env.only(["HOME", "LANG"])` | those variables only |
   | `console.stdout_only()`, `console.stderr_only()` | one output stream |

7. Each capability is an interface, so a fake is an ordinary type that implements it. [CAP-07]
8. Printing needs a `Console`: hello world is `fun main(console: Console) { console.println("Hello, world!"); }`. [CAP-08]
9. `debug(x)` needs no capability: it writes `x`'s debug form, with its file and line, to standard error, and is a compile error in release builds. [CAP-09]
10. `exit` is removed. Code that must stop the program cancels the root scope (4.3) or returns an error to `main`. [CAP-10]
11. The working directory can't be changed: `fs.cwd()` gives the starting directory as a `Dir`, and code that needs another directory receives another `Dir`. [CAP-11]
12. `Signals` gives a `Receiver` of signal events (4.4). Ctrl-C cancels the root scope whether or not anyone holds `Signals` (4.3). [CAP-12]
13. A test asks for capabilities by type, like `main`, and the test runner supplies safe ones: a fresh temporary `Dir`, deleted afterwards; a fake `Clock` at a fixed date; a captured `Console`, shown when the test fails; a seeded `Entropy`; a fake, local `Net`. Real `Fs`, `Net` and `Proc` are given only to `@test(integration)` tests, which run only under `lughac test --integration`. [CAP-13, OPN-06]
14. Capabilities are sendable, so they can be passed to tasks, but they are not `Wire`: they never cross to another machine. [CAP-14]
15. The only per-package permission is `ffi` (2.2): everything else a package can do, it can do only with capabilities its caller hands it. [CAP-16]
16. The prelude declares `Scope` and the capability interfaces, with their core methods, so they need no import; the runtime supplies the real implementations. Higher-level libraries (`std.http`, `std.tls`, `std.json`, `std.toml`, folder helpers) stay `std` imports and take capabilities as parameters. A module that declares one of these names means its own, as with `Iter`. A name in the prelude is not a capability: code still needs the value. [CAP-17]

**Conventions** (documentation, not rules) [CAP-15]: take the narrowest capability that does the job; accept capabilities as interface types so callers can pass fakes; capture them in constructors rather than passing them on every call; keep pure functions free of them.

**What it looks like**

```
struct Store { dir: Dir }

impl Store {
    fun open(dir: Dir): Store = Store { dir: dir };      // captured once
    fun load(self, id: string): Result<Order, Error> =
        Ok(json.decode<Order>(json.parse(self.dir.read_file("{id}.json")?)?)?);
}

fun main(args: string[], root: Scope, fs: Fs, net: Net, console: Console): i32 {
    let store = Store.open(fs.dir("/var/orders"));
    let api = net.only("payments.example.com:443");
    serve(root, net.listen_only(":8080"), store, api, console.stderr_only())
}

@test
fun loads_an_order(dir: Dir) {                           // a fresh temporary folder
    dir.write_file("7.json", "{{\"id\": 7}}").unwrap();
    assert_eq(Store.open(dir).load("7").unwrap().id, 7);
}

struct FixedClock { ms: i64 }
impl Clock for FixedClock {
    fun now_ms(self): i64 = self.ms;
}
```

**Before / after**

```
// v17
fun main() {
    let text = read_file(args()[1]);
    match text {
        Ok(t) => println(t),
        Err(e) => { eprintln(e); exit(1); }
    }
}

// v18
fun main(args: string[], fs: Fs, console: Console): i32 {
    match fs.cwd().read_file(args[1]) {
        .Ok(t) => { console.println(t); 0 }
        .Err(e) => { console.stderr_only().println("{e}"); 1 }
    }
}
```

**Replaces:** the ambient parts of `std.fs`, `std.net`, `std.tls`, `std.http`, `std.time` (`now`, `local_offset`), `std.env`, `std.process` and the system-seeded parts of `std.random` and `std.crypto`; the intrinsics `args`, `exit`, `print`, `println`, `eprint`, `eprintln`, `read_line`, `read_file`, `write_file` and `append_file` (v17 §5); `env.set_current_dir`; v17 §6's two forms of `main`.

**Why:** a function's parameters now say what it may do, so `fun price(order: Order): Money` provably touches nothing. The same boundary gives fakes for deterministic tests (4.6), recordings for replay (5.2), and confinement for untrusted code (2.2). Alternative rejected: capabilities carried implicitly by scopes, which saves parameters but hides what code can do.

## 2.2 Security and containing C

You approved these explicitly, as CLAUDE.md requires for security decisions. Kernel versions and system-call lists are to be verified against kernel documentation before the PRP.

**Rules**

1. A `Dir` can't reach outside its folder through `..`, symbolic links or a folder swapped mid-operation: on Linux every path is opened with `openat2` and `RESOLVE_BENEATH` (Linux 5.6 and later). Other platforms need an equivalent guarantee when they're supported. [SEC-01]
2. `net.only` restricts by host name, and refuses private and local addresses unless `.allow_private()` is added. [SEC-02]
3. Listening and connecting are narrowed separately. [SEC-03]
4. A package may declare `extern` functions only if the root `lugha.toml` grants it `ffi`, indirect dependencies included. A package with `ffi` is documented as fully trusted, since C can do anything. [SEC-04]
5. Process lockdown: once the runtime has opened what `main` asked for, it restricts the whole process to the kinds of capability `main` declared (seccomp and Landlock on Linux), so C code can't exceed them either: without `Net`, no sockets; without `Proc`, no starting programs; without `Fs`, no files. It is as coarse as `main`'s parameters: a `main` that holds `Fs` leaves the whole filesystem open to C, and narrowing to a `Dir` later binds Lugha code only. It works per program, not per library. [SEC-05]
6. Sandboxed C libraries, a later milestone: `sandbox = true` on a dependency compiles its C through WebAssembly into bounds-checked native code that can't read Lugha's memory or make system calls, and receives only the capabilities it is passed. [SEC-06]
7. Running a library in a separate process is a library pattern, not a language feature. [SEC-07]

**What it looks like**

```toml
[dependencies]
json = { git = "…", version = "1.2" }                            # pure Lugha: no permission
sqlite = { git = "…", version = "3.0", allow = ["ffi"] }         # declares extern: trusted
imagelib = { git = "…", version = "2.1", allow = ["ffi"], sandbox = true }
```

```
let api = net.only("api.example.com:443");                      // public addresses only
let db = net.only("db.internal:5432").allow_private();          // an internal host, on purpose
```

**Replaces:** v17's rule that any package may declare `extern` (v17 §8).

**Why:** capabilities only mean something if nothing can bypass them. The kernel enforces folder confinement and process lockdown where Lugha's own checks can't reach, and the `ffi` permission states honestly where trust is given. Alternative rejected: trusting C silently, as v17 does.

---

# Part 3 — Memory

## 3.1 Reference counting

**Rules**

1. Every heap object (strings, arrays, closure environments, interface boxes, `Ref` boxes, channels, tasks, actor handles) carries a count of its holders and is freed when the count reaches zero. There is no tracing garbage collector: the Boehm collector is removed. [MEM-01]
2. A count is non-atomic while its object is reachable from one task only. When a value crosses to another task (sent on a channel, passed to `spawn`, captured by a spawned call), everything reachable from it that is still task-local is marked shared, with atomic counts, in one walk; parts already shared are skipped. Marking the outer object alone isn't enough: a task can share a child through a copy while the other task frees the parent. [MEM-02]
3. The compiler removes most count operations: a value's last use moves it instead of copying it, using the liveness analysis of milestone 149. Programs can't observe counts. [MEM-03]
4. Copy-on-write writes in place when an array's count is 1, and otherwise clones it one level first. [MEM-04]
5. `Weak<T>` refers to a `Ref<T>` without keeping it alive; reading it gives an `Option<Ref<T>>`, `.None` once the box is gone. It is the tool for caches, observer lists, registries and back-links. Like `Ref`, it isn't sendable. [MEM-05]
6. Cycles can only form through `Ref` boxes, channels and task or actor handles, since everything else is immutable when shared or has one holder. A small cycle collector, per task, examines only those objects, in budgeted slices at fixed scheduling points, and frees cycles nothing outside holds. [MEM-06]
7. When a scope ends, it breaks the links inside the task and actor handles it created, freeing their cycles without the collector. [MEM-07]
8. Under `lughac test`, every cycle the collector had to free is reported with where its objects were created. A lint flags `Ref` types that can form cycles without a `Weak`. [MEM-08]
9. Freeing a large structure is deferred and done in slices charged to the scheduling budget, so dropping a big tree never stalls a task. [MEM-09]
10. `Pinned<T>` holds a count on a value for as long as C keeps a pointer to it, until it is released. C keeping a pointer without one stays undefined behaviour. Callback user data (4.8) uses the same mechanism. [MEM-10]
11. `scope s memory n { … }` limits what the scope's tasks may allocate; going over fails the scope with `Failure.Memory`. [MEM-11]
12. Nothing is closed automatically when its last holder goes. Dropping a file or socket that is still open writes a warning to standard error, naming where it was opened. [MEM-12]
13. Objects never move, so pointers handed to C stay valid. [MEM-13]
14. Generated code allocates only through `lugha_rt_alloc`, now backed by an allocator built for many threads. [MEM-14]
15. All of this is conditional on the performance targets (7.2). If they are missed badly, plan B is a heap per task with tracing collection, which keeps every other v18 rule. [MEM-15]

**What it looks like**

```
struct TreeNode {
    name: string,
    kids: Ref<TreeNode>[],
    parent: Option<Weak<TreeNode>>,          // doesn't keep the parent alive
}

let buf = Pinned.new(frame_bytes);           // C may keep a pointer while pinned
audio.queue(buf.pointer());
// …once C says it is finished:
buf.release();

scope s memory 64.mb() {                     // an untrusted decoder gets 64 MB
    s.spawn(decode(upload));
}
```

**Before / after**

```
// v17: undefined behaviour if C keeps `data` after the call (§7)
c_lib_store(data);

// v18: kept alive until released
let held = Pinned.new(data);
c_lib_store(held.pointer());
```

**Replaces:** v17 §7's collector: the Boehm collector (`third_party/bdwgc`), conservative scanning, the sticky shared flag of copy-on-write, uncollectable memory for callback closures, and the GC hooks in `runtime/tasks.c`; the CLAUDE.md known issue that an integer looking like a pointer keeps garbage alive; the CLAUDE.md stack entry for Boehm.

**Why:** Lugha's value semantics mean most objects have exactly one holder and cycles can form only through a few types, which is the case where reference counting is cheap and precise: memory is freed as soon as it's garbage, nothing pauses every core, copy-on-write can write in place, and sending a large value to another task costs one walk over its not-yet-shared part, paid once. Alternative kept as plan B: a heap per task.

## 3.2 Hash seeding

**Rules**

1. Maps and sets hash with a keyed hash function that resists collisions chosen by an attacker who doesn't know the key. It replaces FNV-1a; the exact algorithm is chosen by benchmark. [HSH-01]
2. `Hasher` has one method, `h.add<T: Key>(x: T)`. Derived `Key` implementations add each field in declaration order; built-in types add their bytes. [HSH-07]
3. The key is one secret seed per process, read from operating-system randomness by the runtime at startup. It is not a capability: maps iterate in insertion order and compare regardless of order, so the seed can't change any result. [HSH-02]
4. The spec states that the runtime may use randomness that cannot affect a program's results. [HSH-03]
5. In deterministic mode the seed is fixed, derived from the test seed; in shuffle mode it varies with the shuffle seed (4.6). [HSH-04]
6. Hash values are visible to programs, but differ between runs: the spec says never to store or send them. [HSH-05]
7. A map sent through `Wire` (5.1) travels as its entries and is rebuilt with the receiver's seed. [HSH-06]

**Replaces:** FNV-1a and its `$mix` in `prelude/eq.la`; v17 §4's note that hashing isn't hardened against keys chosen to collide.

**Why:** a fixed, public hash lets anyone send keys that all collide, turning a server's map inserts quadratic. A secret seed fixes that, and Lugha's insertion-ordered maps keep it invisible, so determinism survives. Alternative rejected: making the seed an `Entropy` capability, which would force every function that builds a map to take one.

---

# Part 4 — Concurrency

## 4.1 Coroutines and the sendable rule

**Rules**

1. Tasks are stackful coroutines: any function can pause, at any depth, and nothing is marked `async` or awaited. [COR-01]
2. A task starts only inside a scope, with `s.spawn(f(x))`; the bare `spawn` expression is removed. [COR-02]
3. A value that crosses a task boundary must be **sendable**: arguments and captures of a spawned call, channel elements, and actor message arguments and replies. The compiler works sendability out from a type's parts; it is never implemented or derived. Not sendable: `Ref`, `Weak`, thread-affine values (4.8), closures that capture any of these, and interface values unless their interface requires `Sendable` (1.1). [COR-03]
4. A spawned call receives copies of its arguments, by value semantics; copy-on-write and moves on last use keep that cheap. [COR-04]
5. Programs mean the same on one core and many; only timing differs. [COR-05]
6. The new words are contextual, meaningful only where they begin a construct: `scope`, `shield`, `within`, `until`, `limit`, `background`, `memory`, `stack`, `independent`, `pinned`, `inline`, `queued`, `blocking`. [OPN-08]

**What it looks like**

```
let counter = Ref.new(0);
scope s {
    s.spawn(bump(counter));
//           ^^^^^^^ error: `Ref<i64>` isn't sendable, so it can't cross to another task
}
```

**Before / after**

```
let t = spawn work(x);            // v17: anywhere, and may outlive its function
let t = s.spawn(work(x));         // v18: inside `scope s { … }`
```

**Replaces:** v17's `spawn` expression (§3, §5) and E0211's `spawn` part; E0438's rule against spawning a call that changes a copy (covered by copies and the sendable rule); E0439's sharing rules, which applied only to actors and now apply everywhere.

**Why:** the sendable rule makes data races impossible by construction, on any number of cores, without locks. Alternative rejected: allowing sharing on one core and isolation only for actors, which forced a choice between concurrency and parallelism up front.

## 4.2 Scopes

**Rules**

1. `scope s { … }` runs a block that may start tasks through its handle `s`. [SCO-01]
2. Scope handles, and task handles from them, are second-class: they may be passed as arguments, and nothing else. They can't be returned, stored in fields or arrays, sent, or captured by a closure that can outlive them. [SCO-02]
3. The body runs in the current task. The scope's value is the body's value, once every task started in it has finished. [SCO-03]
4. A task fails by panicking or by returning `Err`; a scope also fails when its deadline passes or its memory limit is exceeded. A scope's type is `Result<T, Failure>`, where `Failure` is `.Error(Error)`, `.Panic(string)`, `.Deadline` or `.Memory`, and records which task failed. [SCO-04, MEM-11]
5. By default, `Failure.Error` carries the task's error as the `Error` interface. `scope<E> s { … }` makes the type `Result<T, Failure<E>>`, and every task's error must convert to `E`, checked at each `spawn`. [OPN-05]
6. `Failure` implements `Error`, so `?` applies to a scope in a function that returns `Result<_, Error>`. [SCO-13]
7. Default policy, all-or-nothing: the first failure cancels the other tasks and the body (4.3), the scope waits for them, and its value is that failure. Cancellation stops at the scope's edge; the function containing the scope carries on with the `Result`. [SCO-05]
8. The `independent` policy keeps the other tasks running and passes each failure to the scope's handler as it happens. `independent` alone prints the failure to standard error with the task's name; `independent(f)` calls `f`. [SCO-06]
9. Other patterns, such as taking the first success, are library functions. [SCO-07]
10. A scope's settings are header words before its body, in any order, each at most once: `independent`, `within ms`, `until t`, `limit n`, `background` (4.6), `memory n` (3.1), `stack n` (4.7). [OPN-07]
11. `within ms` and `until t` give the body and every task a deadline; `s.remaining()` reads what is left; a nested scope's deadline can only be earlier. [SCO-08]
12. `limit n` makes `s.spawn` wait while `n` of the scope's tasks are running. [SCO-09]
13. When the body finishes, the scope first waits for every task, then runs the body's defers, so a resource the body opened outlives every task that uses it. [SCO-10]
14. A task handle has `join()`, `cancel(reason)` and `name(text)`. `join()` returns `Result<V, Stopped>`: `V` is the task's value, or its `Ok` value when the task returns a `Result`; `Stopped` is `.Cancelled(reason)`, `.Failed(Error)` or `.Panicked(string)`, and implements `Error`, so `t.join()?` works. Joining is optional: an unjoined result is dropped, but an unjoined failure still fails the scope. [SCO-11]
15. Leaving the body early, by `return`, `?` or `break`, cancels the scope's remaining tasks with `.Parent`, waits for them, runs the body's defers, and only then completes the exit. Nothing outlives its scope. [SCO-14]
16. `main` runs inside the root scope, which it receives by asking for a `Scope` (2.1). When `main` returns, the program waits for the root scope's tasks. [SCO-12]

**What it looks like**

```
fun fetch_both(web: http.Client, a: string, b: string): Result<(Page, Page), Error> {
    scope s within 5_000 {
        let x = s.spawn(web.get(a));
        let y = s.spawn(web.get(b));
        Ok((x.join()?, y.join()?))
    }?
}

fun serve(listener: net.Listener, app: App): Result<(), Failure> {
    scope conns independent(log_failure) limit 10_000 {
        loop {
            match listener.accept() {
                .Ok(conn) => { conns.spawn(handle(conn, app)); }
                .Err(e) => log_error(e),
            }
        }
    }
}

let synced = scope<SyncError> s within 30_000 {
    s.spawn(download(url));          // NetError converts to SyncError
    s.spawn(save(path));             // DiskError converts to SyncError
};
```

**Before / after**

```
// v17: if fetch(a) fails and we return, fetch(b) keeps running
let x = spawn fetch(a);
let y = spawn fetch(b);

// v18: the scope cancels fetch(b) and reports the failure
scope s { let x = s.spawn(fetch(a)); let y = s.spawn(fetch(b)); … }
```

**Replaces:** tasks outliving the function that started them; v17 §6 "When `main` returns, the program exits; green threads still running are dropped".

**Why:** a scope makes a task's lifetime visible in the code, gives every failure somewhere to go, and gives cancellation a path. Alternative rejected: free-standing tasks, whose orphans leak work and lose errors.

## 4.3 Cancellation and unwinding

**Rules**

1. Cancelling a task is invisible inside it: at its next safe point the task stops, its stack unwinds, and its defers run. Nothing in its code mentions cancellation. [CAN-01]
2. Safe points are every pause point (waiting on a channel, a task, a timer, I/O or a helper thread) and every preemption point (function entries and loop iterations, 4.6), so computations stop too. [CAN-02]
3. A cancellation carries a reason: `.Deadline`, `.SiblingFailed`, `.Parent`, `.Requested(string)` or `.Shutdown`. Observers see it; the cancelled code doesn't. [CAN-03]
4. A scope reports the first failure that wasn't a cancellation it caused itself. [CAN-04]
5. `shield within ms { … }` holds cancellation off until the block ends or its deadline passes, for cleanup that must wait. A shield always has a deadline. [CAN-05]
6. Completed work is never discarded: a value already received stays received; a value not yet taken stays in its channel; an operation already given to a helper thread finishes even though its task stopped waiting. [CAN-06]
7. Cancellation never unwinds through C frames: a task in a C call, or in a Lugha callback from C, is stopped after control returns to Lugha. [CAN-07]
8. A panic unwinds the same way: defers run, and the task fails its scope. Only a panic that reaches the root scope ends the program. [CAN-08]
9. Ctrl-C cancels the root scope with `.Shutdown`; a second Ctrl-C ends the program at once. [CAN-09]
10. Cancellation is control flow for the cancelled task and information for everyone else: `join` and scope results report it as a value. [CAN-10]
11. A `select` whose task is cancelled simply stops. [CAN-11]
12. Defers should release resources, not inspect data, because a task can stop in the middle of a computation. [CAN-12]

**What it looks like**

```
fun session(conn: net.Conn) {
    defer {
        shield within 1_000 {
            let _ = conn.write("bye\n".to_bytes());
        }
        conn.close();
    };
    loop {
        let line = conn.read_line();       // cancelled here: the defer still runs
        …
    }
}

let t = s.spawn(download(url));
t.cancel("not needed");
match t.join() {
    .Ok(file) => use(file),                        // finished before the cancel
    .Err(.Cancelled(_)) => {},
    .Err(e) => report(e),
}
```

**Before / after**

```
// v17: a panic skips pending defers (§6); the file stays open
// v18: a panic unwinds, the defer closes the file, the scope reports the failure
let f = dir.open("data.bin")?;
defer f.close();
process(f);                                // panics
```

**Replaces:** v17 §6's rule that a panic, `exit` and the end of the program don't run pending defers; the no-unwinding design of `defer` (`docs/decisions/defer.md`); the panic rule for actors (4.5).

**Why:** cancellation as an ordinary value would have forced every function that waits to return a `Result`, which is function colouring in another form. Invisible cancellation keeps signatures honest, can't be ignored, and reaches computations as well as waits. Alternative rejected: cancellation as values at pause points.

## 4.4 Channels

**Rules**

1. `Chan<T>.new(n)` returns a `(Sender<T>, Receiver<T>)` pair holding up to `n` values. The capacity is required; `0` hands each value over directly; `Chan<T>.unbounded()` has no limit and must be asked for by name. [CHN-01]
2. `send` waits while the channel is full and returns nothing. Sending on a closed channel panics. [CHN-02]
3. `recv` returns an `Option<T>`, `.None` only when the channel is closed and empty. [CHN-03]
4. Only a `Sender` can close. Nothing closes automatically; a scope that holds the senders' tasks is how a program knows when to close. [CHN-04]
5. A `Receiver<T>` is an iterator: `for x of rx` runs until the channel is closed and empty. [CHN-05]
6. `try_recv` returns `.Ready(x)`, `.Empty` or `.Closed`; `try_send` returns `.Sent`, `.Full(v)` or `.Closed(v)`, giving the value back. [CHN-06]
7. `select` keeps v17's `recv`, `send`, `after(ms)` and `default` arms; when several are ready, the pick is fair and seeded. [CHN-07]
8. `s.merge(receivers)` returns one `Receiver` fed by all of them. [CHN-08]
9. The element type must be sendable; senders and receivers are sendable; channels are not `Wire`. [CHN-09]
10. Broadcast channels and `Watch<T>` (the latest value) are library types. [CHN-10]
11. A channel records where it was created, for deadlock reports and diagnostics. [CHN-11]
12. `Ring<T>` is a channel for callbacks on foreign threads (4.8): the foreign thread may only `try_send` and `try_recv`, while a task at the other end may also wait, with `recv` or in a `select`. [CHN-12]

**What it looks like**

```
fun resize_all(paths: string[], out: Dir): Result<i64, Failure> {
    let (jobs, queue) = Chan<string>.new(16);
    let (done, thumbs) = Chan<Thumb>.new(16);

    scope work {
        work.spawn(fun() {
            for p of paths { jobs.send(p); }
            jobs.close();                  // every job is queued
        }());
        for _ in 0..4 {
            work.spawn(resize_worker(queue, done));
        }
        let mut n = 0;
        for thumb of thumbs {
            save(out, thumb);
            n += 1;
            if n == paths.len { break; }
        }
        n
    }
}

match jobs.try_send(next) {
    .Sent => {},
    .Full(job) => shed(job),               // the value comes back
    .Closed(job) => drop_job(job),
}
```

**Before / after**

```
let c: Chan<Job> = Chan.buffered(16);               // v17: one handle
let (jobs, queue) = Chan<Job>.new(16);              // v18: two ends
```

**Replaces:** v17 §4's channel table: `Chan.new()` (unbuffered), `Chan.buffered(n)`, `try_recv` as an `Option`, any holder closing.

**Why:** split ends put direction in the signature and settle who may close; a required capacity makes backpressure the default. Alternative rejected: closing automatically when the last sender goes, which needs destructors Lugha doesn't have.

## 4.5 Actors

**Rules**

1. Actors are the unit of distribution (Part 5). An actor is started in a scope, `s.spawn(Stats { hits: 0 })`, and stops when its scope ends. [ACT-01]
2. An actor's mailbox is bounded: 1,024 messages by default, set at the declaration (`actor Logger mailbox 10_000 { … }`). Locally, a caller waits while it is full, as with a channel; a remote mailbox that is full answers `Rejected` at once, so one busy actor never stalls a connection that others share (5.1). [ACT-02, DST-11]
3. An actor's methods are its protocol: the compiler generates a message type from them, one message per method. [ACT-03]
4. Actors may be generic, `actor Cache<K: Key, V>`; each instance gets its own message type. [OPN-04]
5. `impl I for SomeActor` makes the actor's handle implement `I`: the impl's methods are message handlers, and calls through `I` are messages. A `Remote<T>` (5.1) implements only interfaces whose methods return `Result`. [OPN-04]
6. Calling a local actor that has stopped or failed panics in the caller ("actor `Counter` has stopped"), as in v17. Actor handles keep `stop()` and `is()`; v17's `join()` on actor handles is removed, since the actor's scope reports its failures. [ACT-04]

**What it looks like**

```
actor Cache<K: Key, V> {
    entries: Map<K, V>,
    fun get(self, k: K): Option<V> = self.entries.get(k);
    fun put(mut self, k: K, v: V) { self.entries[k] = v; }
}

interface Store {
    fun find(self, id: string): Option<User>;
}

actor UserStore { users: Map<string, User> }

impl Store for UserStore {
    fun find(self, id: string): Option<User> = self.users.get(id);
}

scope s {
    let users = s.spawn(UserStore { users: Map {} });
    greet(users, "ann");               // greet(store: Store, …) works with any Store
}
```

**Replaces:** actors started anywhere with `spawn`; unbounded mailboxes; E0439's bans on generic actors and on actors as interface values (TODO.md).

**Why:** distribution needs messages that are data, which actors' generated messages are; with every task now isolated, actors no longer need rules of their own for sharing. Alternative rejected: an `Owned<T>` library type sending closures, which can't cross machines.

## 4.6 Scheduling

**Rules**

1. A program runs one worker thread per core. Each has its own ready queue; a shared queue takes new and overflowing work; an idle worker steals half a busy worker's queue; a task otherwise stays on its worker. [SCH-01]
2. Preemption counts steps: a step is a function entry or a loop iteration, and a task's slice is a fixed number of steps (4,096 today, to be tuned by measurement). At the end of a slice the task yields if other tasks are ready. [SCH-02]
3. Runtime operations charge one step per chunk of work (about 4 KB), and very long ones pause partway, so copying a large array can't hold a worker. [SCH-03]
4. The compiler may leave the countdown out of loops it can prove short. [SCH-04]
5. The ready queue is first in, first out. A task woken by a message may run next, at most three times in a row, finishing the waker's slice rather than starting a new one. A worker checks the shared queue at least every 61 picks. [SCH-05]
6. A busy worker polls for I/O without waiting every few switches; an idle worker waits for I/O and timers; results from helper threads arrive through the same poll. [SCH-06]
7. Blocking work runs on a bounded pool of helper threads (about 16, configurable) with a bounded queue, while its task waits: file operations, name lookups, starting and waiting for programs, and `extern blocking` calls. Callers wait when the queue is full. Helpers touch only copied data. [SCH-07]
8. Other `extern` calls run inline; the runtime warns when one held its worker too long. [SCH-08]
9. `scope s background { … }` runs its tasks at background priority, inherited by nested scopes; background work runs at least once every eight normal slices. There is no priority inheritance: normal work waiting on background work waits at background speed. [SCH-09]
10. Each worker keeps its timers in a heap, checked at every scheduling decision; timers use scheduling time, not the `Clock` capability. [SCH-10]
11. Three modes: **normal** (every core, real time); **deterministic** (one worker, virtual time that jumps to the next timer when every task waits, fake or recorded capabilities); **shuffle** (deterministic, with the ready queue reordered from a seed). `lughac test` runs deterministic by default; `--shuffle=SEED` reorders. [SCH-11]
12. In deterministic mode, results from helper threads are taken only at fixed scheduling points, so a run repeats exactly. [SCH-12]
13. Determinism is a mode, not a promise about every run. [SCH-14]
14. The runtime can report per-task accounting, a dump of every scope and task and what each waits on, slow calls and scheduler statistics; tasks may be named; a deadlock report names what each task waits on, with channels' creation sites. [SCH-13]

**What it looks like**

```
scope s background {
    s.spawn(rebuild_index(store)).name("reindex");
}
```

```
$ lughac test --shuffle=42
test orders::concurrent_checkout ... FAILED (seed 42)
```

```
panic: deadlock
  task "main"      waits on join of "worker 2"
  task "worker 2"  waits on recv from Chan<Job> (created at jobs.la:12)
```

**Replaces:** v17 §6 "Green threads": one OS thread for every task, a sorted list of sleepers, and blocking I/O stopping every task.

**Why:** the step countdown is Lugha's source of determinism and is stronger than most runtimes' preemption, so it stays; what changes is everything around it: many cores, helper threads for blocking work, and fairness. Alternative rejected: signal-based preemption, which breaks determinism.

## 4.7 Stacks and C calls

**Rules**

1. C code never runs on a task's stack: every `extern` call switches to its worker's C stack (8 MiB), and helper threads have their own. [STK-01]
2. A task's stack is a large reservation (8 MiB by default) carved from pooled regions; only pages it touches use memory, they are returned before the stack is reused, and a stack never moves. [STK-02]
3. Task stacks have no guard pages: a check at each function entry raises a `stack overflow` panic that unwinds, with room kept for the unwinding. [STK-03]
4. On a small address space (such as 39-bit ARM64) the default reservation shrinks automatically. [STK-04]
5. `scope s stack n { … }` gives the scope's tasks `n` bytes each; `lugha.toml` sets a project default. [STK-05]
6. `main` is a task like any other, on a pooled stack. [STK-06]

**What it looks like**

```
scope deep stack 64.mb() {
    deep.spawn(parse_nested(document));
}
```

**Replaces:** v17 §7 "Green threads' stacks": 1 MiB reservations mapped one by one with 64 KiB guards, C code on task stacks, and `main` on the process stack. Removes the ceiling of about 30,000 tasks set by the kernel's mapping limit.

**Why:** separating C's stack from tasks' stacks lets tasks be cheap while C keeps the large stack it expects; large lazy reservations avoid moving stacks, which pointers into them (C's `out` parameters) rule out. Alternative rejected: growable stacks that are copied when full.

## 4.8 Callbacks and thread-bound values

**Rules**

1. The language provides general mechanisms only, with no UI concepts: thread-affine values (`extern type GlContext pinned;`), which never leave the worker that created them; pinned scopes (`scope ui pinned { … }`), whose tasks all run on the current worker; and the callback rules below. [CBK-01]
2. A C callback that returns a value runs inline and can't pause; trying to pause is a runtime error saying to start a task instead. [CBK-02]
3. A C callback that returns nothing is queued as a task in the scope it was registered with, and may pause. Marking it `inline` makes it run immediately, without pausing. The mark follows the callback parameter's type: `cb: fun(Window, data) inline`. [CBK-03]
4. A callback arriving on a thread C created runs inline on that thread, can't pause, may touch only sendable data, and talks to tasks through `Ring<T>`. Marking it `queued` delivers it as a task instead. A lint flags allocation inside it. [CBK-04]
5. A panic in an inline callback is caught where it returns to C: C receives a zero value, and the scope that registered the callback receives the failure. [CBK-05]
6. Cancellation never interrupts an inline callback. [CBK-06]
7. A callback is registered against a scope: after the scope ends, its events are dropped, and the values it captured are released when C releases the callback or the scope ends. [CBK-07]

**What it looks like**

```
extern type Window pinned;
extern "toolkit_on_click" fun on_click(w: Window, cb: fun(Window, data), user: data, free: release);
extern "sensor_start" fun sensor_start(cb: fun(f64, data), user: data, free: release);   // calls cb on its own thread

scope ui pinned {
    let win = make_window();
    on_click(win, fun(w: Window) { refresh(w); });   // queued as a task in `ui`
}

let (readings, incoming) = Ring<f64>.new(4_096);
sensor_start(fun(x: f64) {                           // inline on the foreign thread
    readings.try_send(x);                            // never waits; drops when full
});
```

**Replaces:** v17 §8's rule that a callback is called on the OS thread that registered it; uncollectable memory for callback closures (3.1).

**Why:** toolkits and C libraries impose thread rules of their own; the language states them as types and callback rules instead of building any one toolkit's model in. Alternative rejected: a UI capability in the language.

---

# Part 5 — Distribution and replay

## 5.1 Distribution

**Rules**

1. Two levels of "safe to pass on": **sendable** (to another task, 4.1) and **`Wire`** (to another machine). `Wire` is a derivable interface, built from derive blocks like `std.json`'s `Json`, and listed with `with Wire` like any other (1.1). Channels, closures, `Ref`, `Weak`, C handles, thread-affine values and capabilities are never `Wire`. [DST-01]
2. An actor can be used remotely only if every message argument and reply is `Wire`. The check is made where the actor is registered or looked up, so a local-only actor may still take channels. [DST-02]
3. `Remote<T>` is a separate handle type for an actor that may live on another machine. Every call through it returns `Result<R, CallError>`, where `R` is the method's result (`()` for a method without one) and `CallError` is `.Timeout`, `.Unreachable`, `.Stopped` or `.Rejected`. A call's deadline is its scope's. [DST-03]
4. Messages from one sender to one actor arrive in the order sent. Delivery is at most once: a message is never duplicated, but may be lost. A `.Timeout` doesn't say whether the call ran, so calls that may be retried should be safe to repeat. [DST-04]
5. Scopes stop at the machine boundary. A remote actor runs in a scope on its own machine; a caller watches it with `r.monitor()`, a `Receiver` that reports `.Stopped`, `.Crashed(string)` or `.Unreachable`. Supervision, such as restarting a crashed worker, is a library pattern built on monitors. [DST-05]
6. A node's `register(name, handle)` makes a local actor reachable by name, and its `lookup<T>(name)` returns a `Remote<T>`. A `Remote<T>` is itself `Wire`, so it can be sent inside a message. [DST-06]
7. Messages are encoded by field and variant names, not positions. Each actor's set of messages, with its type arguments, has a fingerprint that the two sides compare when they connect; a mismatch is refused with a clear message. [DST-07]
8. Nodes authenticate each other (mutual TLS) before anything else; only messages cross, never code; decoding has size, depth and count limits; only registered actors are reachable. The authentication and cryptography details need your review at PRP time, as CLAUDE.md requires. [DST-08]
9. A `Remote<T>` whose actor is on the same machine skips encoding and the network, with the same `Result`-returning calls, so code written for remote actors works unchanged on one machine. [DST-12]
10. Nodes come from the `std.cluster` library: `cluster.start(net, identity)` starts one from a `Net` capability and the node's identity, and returns it. The language itself provides only `Wire`, `Remote<T>` and actors' generated messages. [DST-13]
11. `cluster.simulate(nodes, seed)` runs several nodes in one process under deterministic mode (4.6): a virtual network and clock, and seeded delays, losses, reordering, crashes and partitions. A failing run reproduces from its seed. [DST-09]

**What it looks like**

```
actor Counter {
    count: i64,
    fun add(mut self, n: i64) { self.count += n; }
    fun get(self): i64 = self.count;
}

// on node B, in a function returning Result<_, Error>
let c = root.spawn(Counter { count: 0 });
node.register("counter", c)?;

// on node A, likewise
let counter = node.lookup<Counter>("counter")?;
scope s within 2_000 {
    counter.add(5)?;
    report(counter.get()?);                  // get() returns Result<i64, CallError>
}?;

let down = counter.monitor();
select {
    why = down.recv() => restart_or_report(why),
    after(60_000) => {},
}

@test
fun survives_a_partition() {
    let sim = cluster.simulate(3, 7);
    sim.partition([0], [1, 2], 8_000, 9_000);
    sim.crash(1, 5_000);
    sim.run_until(60_000);                   // a minute of virtual time
    …
}
```

**Replaces:** nothing in v17; distribution is new. Removes nothing beyond what 4.5 already replaced.

**Why:** a remote call can fail in ways a local one can't, so `Remote<T>` makes those failures part of the type instead of hiding them behind local-looking calls. Messages generated from methods, encoded by `Wire`, are data that can be checked, versioned, logged and simulated. Alternative rejected: making remote handles look exactly like local ones.

## 5.2 Replay

**Rules**

1. A recorded run logs every result that crosses a capability or a helper thread, with the step at which the program took it. Replaying feeds those results back at the same steps, so the program retraces the run exactly, every task switch included. Replay refuses a different binary. Recording is opt-in, and recordings are treated as secret, since they hold everything the program read. A replay never starts real programs. Command names are placeholders: `lughac run --record=FILE`, `lughac replay FILE`. [DST-10]
2. A recorded run uses deterministic scheduling, on one worker, with real capabilities: task switches across several workers can't be reproduced. Recording therefore costs parallelism while it runs. [DST-14]

**What it looks like**

```
$ lughac run --record=crash.lrec server.la     # on the machine where the bug happens
$ lughac replay crash.lrec                     # anywhere: the same run, every time
```

**Why:** Lugha's scheduler is deterministic given its inputs, and capabilities mark exactly where inputs enter, so recording those is enough to reproduce a run. Most other systems need hardware counters or a special operating-system layer for the same result.

---

# Part 6 — Changes by section

Derived from Parts 1–5; adds no rules. Section numbers are v17's. New error codes take the next unused number in their stage's range when built, each with a page in `docs/errors/`; retired codes are never reused.

**§1 Overview.** The goals gain capabilities, structured concurrency and many cores; "Syntax at a glance" becomes the tour (top of this draft). Hello world takes a `Console` (2.1).

**§2 Lexical structure.**
- Removed: the `#{` token (1.6).
- The bare `spawn` keyword goes (4.1); `scope` and `shield`, and the header words of 4.1 rule 6, are contextual.
- Attributes: `@caller_location` (1.7) and `@test(integration)` (2.1) join `@test`; E0210 covers misuse.
- A letter after a number stays E0108; sizes are `64.mb()` (1.7).

**§3 Grammar.**
- New: `impl I for T { … }` and `impl I for T = derive;` (1.1); `with I, J` after struct and enum declarations (1.1); `interface I: Sendable` (1.1).
- New: type arguments at uses, with the context rule (1.5); `Name { k: v }` and `Name { a, b }` literals (1.6).
- New: `.Variant` in every pattern (1.4).
- New: `scope<E> name header-words { … }` (4.2); `shield within ms { … }` (4.3); `extern type X pinned;`, `extern blocking fun`, and `inline`/`queued` after a callback parameter's type (4.6, 4.8); `actor Name mailbox n { … }` (4.5).
- Removed: the `spawn` expression and E0211's `spawn` part (4.1).

**§4 Type system.**
- Interfaces are nominal; built-in behaviour goes through `Debug`, `Display`, `Eq`, `Key`, `Ord`, `Iter`, the operator interfaces and `Json` (1.1–1.3).
- `as` converts losslessly only; `truncate`, `saturate`, `round` and `try_into` do the rest (1.4).
- Map keys and set elements need `Key`; hashing is keyed and seeded (1.2, 3.2).
- New types: `Sender`, `Receiver`, `Ring`, `Stopped`, `Failure`, `Weak`, `Pinned`, `Remote`, `Hasher`, the capability interfaces and `Scope` (Parts 2–5).
- The sendable rule (4.1) and `Wire` (5.1).
- Retired: E0441, E0454 (1.2); E0430 (1.1); E0458 (1.3); E0439's sharing and generics rules (4.1, 4.5).
- New errors: an `impl` outside its interface's or type's module; an ambiguous method name needing qualification; a derived interface a field lacks, or derived and implemented both; a value that isn't sendable or isn't `Wire`; a scope or task handle escaping; a lossy `as`.

**§5 Expressions and statements.**
- A discarded `Result` is an error (1.4).
- Interpolation and `to_string` need `Display`; `debug(x)` shows anything (1.7).
- The I/O intrinsics `print`, `println`, `eprint`, `eprintln`, `read_line`, `read_file`, `write_file`, `append_file`, `exit` and `args` are removed (2.1).
- Scopes (4.2); cancellation and unwinding (4.3); channel operations (4.4). `select` keeps its arms.
- `defer` runs on cancellation and panics too (4.3).

**§6 Functions and program structure.**
- `main` asks for what it needs by type; E0304 covers wrong parameters (2.1).
- `main` waits for the root scope; Ctrl-C cancels it (4.2, 4.3).
- Panics unwind and fail their scope (4.3).
- Green threads become workers across cores, with helper threads and three modes (4.6).
- Tests receive capabilities; `lughac test` runs deterministic by default (2.1, 4.6).

**§7 Memory model.** Reference counting replaces Boehm; copy-on-write writes in place at count 1; `Weak`, `Pinned`, the cycle collector, memory limits (3.1). Task stacks are pooled lazy reservations without guard pages (4.7).

**§8 C interop.**
- `extern` needs the `ffi` permission (2.2).
- C runs on each worker's C stack (4.7).
- `extern blocking` runs on a helper thread (4.6).
- Callback rules and thread-affine types (4.8).
- `Pinned` is the supported way for C to keep a value (3.1).
- Process lockdown (2.2).

**§9 Compilation model.**
- Moves to the prelude: printing (`codegen/show.rs`, `show_iface.rs`), the method-name table (`check/helpers.rs`), tuple order (`codegen/ordering.rs`), `unwrap` and assertions (`codegen/unwrap.rs`, `assertions.rs`), map literals (`codegen/maps.rs`), I/O (`codegen/io.rs`, `input.rs`) (1.6, 1.7).
- The runtime: a multi-threaded allocator behind `lugha_rt_alloc` (3.1); workers, helper threads, timer heaps (4.6); process lockdown and the hash seed at startup (2.2, 3.2); the step countdown charges runtime work by chunk (4.6).
- `lughac`: `test --shuffle=SEED` and `--integration`; record and replay (2.1, 4.6, 5.2).

**§10 Example programs.** Every acceptance program needs rewriting in v18 form; they stay the acceptance tests.

**§11 Milestones and scope.** The phases of Part 7 become milestone rows. In scope now: green threads on several OS threads, generic actors, actors as interface values, bounded mailboxes (previously "out of scope").

**§12 Open questions.** Settled: explicit type arguments; `From<T>` once per type argument; a separate debug form; the warning for a `show` of the wrong shape (obsolete).

**CLAUDE.md** (once v18 lands):
- Known issues removed: `as` truncating silently (1.4); an integer looking like a pointer keeping garbage alive (3.1).
- Known issue changed: C keeping a Lugha pointer is undefined behaviour unless `Pinned` (3.1).
- Stack: the Boehm entry goes (3.1).
- Security list: capabilities' narrowing, process lockdown and the `ffi` permission join the items needing explicit review.

---

# Part 7 — Milestones

## 7.1 Phases

Each phase is one or more PRPs, approved one at a time [ORD-01]. The order [ORD-02]:

| Phase | Work | From |
| --- | --- | --- |
| 0 | This draft | — |
| 1 | Language surface: nominal interfaces, `with`, `Eq`/`Key`/`Ord`, operators, `.Variant`, must-use `Result`, conversions, type arguments, literals, prelude moves, size methods | Part 1 |
| 2 | Capabilities, the `ffi` permission, `Dir` confinement, process lockdown | 2.1, 2.2 (rules 1–5) |
| 3 | Memory prototype: reference counting with moves on last use and marking on send, measured against 7.2; it decides between reference counting and plan B | 3.1 (rules 1–4, 15) |
| 4 | Unwinding: panics run defers and fail their scope | 4.3 (rules 7–8) |
| 5 | Structured concurrency: scopes, cancellation, the sendable rule, channels, actors | 4.1–4.5 |
| 6 | Runtime: workers, helper threads, scheduling, stacks and C calls, callbacks, reference counting, hash seeding, virtual time | 3.1, 3.2, 4.6–4.8 |
| 7 | `Wire` | 5.1 (rule 1) |
| 8 | Replay and cluster simulation | 5.1 (rule 11), 5.2 |
| 9 | Distribution: nodes, `Remote<T>`, monitors | 5.1 |
| later | Sandboxed C libraries | 2.2 (rule 6) |

Phase 1 comes first because capabilities rely on nominal interfaces (for fakes) and type arguments. The memory prototype comes before unwinding because every unwinding cleanup path differs between reference counting and plan B, so the memory model is settled by measurement before unwinding, scopes and actors are built on it [ORD-03]. Each phase breaks programs; the migration tool is planned separately [PRI-05].

## 7.2 Performance targets

Baseline: `bench/RESULTS.md`, 2026-10-31, commit `5d5ab18`, Intel i5-4308U. The baseline is recorded again with `python3 bench/run.py --save`, on one fixed machine, before Phase 1 starts.

| Measure | Today | Target |
| --- | --- | --- |
| fannkuch, fib, mandelbrot, nbody (time) | 0.251 / 0.562 / 0.364 / 0.650 s | within 3% [PRF-01] |
| binarytrees, wordfreq (time) | 1.114 / 0.557 s | at most 10% slower [PRF-02] |
| binarytrees (peak memory) | 70.3 MB | at most 35 MB [PRF-03] |
| wordfreq (peak memory) | 83.4 MB | no worse; aim for 47 MB [PRF-03] |
| memory-management pauses | (new benchmark) | none over 1 ms [PRF-04] |
| parallel binarytrees | (new) | at least 70% efficiency across cores [PRF-05] |
| channel ping-pong | (new) | within 1.5× of Go [PRF-05] |
| idle task | (new) | under 10 KB [PRF-05] |
| step countdown on fib, fannkuch | (to measure) | at most 5% [PRF-06] |

**Method** [PRF-07]: the same machine, the median of 5 runs, `-O2`; new benchmarks get Go and Rust versions, as the existing ones have; `bench/` stays out of CI.

**The prototype's measurement** [PRF-09]: fib carries every new cost of a call at once (the stack check at entry, the countdown, cleanup paths for unwinding, count operations), so Phase 3 measures them together against the fib target, not one at a time.

**If a target is missed** [PRF-08]: a small miss on allocation-heavy code calls for more count elimination; a large miss calls for plan B, a heap per task with tracing collection (3.1 rule 15).

## 7.3 TODO.md items this resolves

Explicit type arguments (1.5); `From<T>` once per type argument (1.1); a separate debug form (1.7); the warning for a `show` of the wrong shape, now obsolete (1.1); generic actors and actors as interface values (4.5); bounded mailboxes (4.5); green threads on several OS threads (4.6); non-blocking input and output (4.6).

---

# Part 8 — Deferred

Decided during building, one line each:

- Generators, `generate { … yield … }`, built on stackful coroutines. [DEF-01]
- Data parallelism: `s.par_map(xs, f)`. [DEF-02]
- Debugger and profiler support for tasks; needs debug information. [DEF-03]
- Tracing across tasks: a request ID carried by a logging capability. [DEF-04]
- Detecting actor call cycles: A waiting on B waiting on A. [DEF-05]
- Fairness between scopes. [DEF-06]
- Revocable capabilities. [DEF-07]
- Where scheduler settings live: `lugha.toml`, the environment or code. [DEF-08]
- Teaching material: why hello world takes a `Console`. [DEF-09]
- An optional `io_uring` backend behind the helper-thread interface. [DEF-10]
- Name lookup without threads; waiting for processes through `pidfd`. [DEF-11]
- Rebuilding a map with a fresh seed after a long probe sequence. [DEF-12]
- Per-field derive options such as `@json(skip)` (already on TODO.md). [DEF-13]
- The spec's wording of the memory model for helper threads and C. [DEF-14]
- `mut` closure parameters (today E0209). [DEF-15]
- Recording and replaying runs that use several workers. [DEF-16]
