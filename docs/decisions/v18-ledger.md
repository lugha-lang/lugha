# Lugha v18 — decision ledger

One line per decision. **Status:** `approved` (you confirmed it), `deferred` (decide during building), `open` (needs a decision before or in the draft), `superseded` (replaced; kept here so it isn't revived). Syntax marked *placeholder* is not settled.

The draft must cite every `approved` line exactly once as a rule, and contain nothing that isn't here.

## PRI — principles

- PRI-01 approved — Design first: implementation cost shapes the order of work, never the design.
- PRI-02 approved — Values: correct by construction; explicit over implicit; force good design; one way to do each thing; predictable (determinism available as a mode); security non-negotiable; the spec is the source of truth.
- PRI-03 approved — Breaking changes are acceptable to get the design right.
- PRI-04 approved — The language has no UI concepts; GUI toolkits are libraries.
- PRI-05 approved — A migration tool is needed; you plan it separately. The draft only notes it.

## INT — interfaces

- INT-01 approved — A type satisfies an interface only through `impl I for T { … }` (nominal). Inherent `impl T { … }` blocks remain for a type's own methods.
- INT-02 approved — `impl I for T` may be written only in the module that declares `I` or the module that declares `T`.
- INT-03 approved — Built-in behaviours are ordinary interfaces, found through implementations, never method names: printing (`Debug`, `Display`), `==` (`Eq`), map keys (`Key`), `<` and sorting (`Ord`), `for … of` (`Iter`), operators, JSON. Complete with milestone 225 (`Iter`, PRP-257).
- INT-04 approved — Operators are generic interfaces (`Add<R>`, `Sub<R>`, `Mul<R>`, `Div<R>`, `Rem<R>`, `Neg`); several right-hand types are several impls (`impl Mul<f64> for Vec2`), a number on the left is `impl Mul<Vec2> for f64`. Replaces milestones 158–159's rules.
- INT-05 approved — A generic interface may be implemented once per type argument (`impl From<string> for Path` beside `impl From<Bytes> for Path`). Resolves TODO.md's `From<T>` item.
- INT-06 approved — When two interfaces in scope supply a method of the same name, the call is qualified: `Shape.area(c)`.
- INT-07 approved — `interface I: Sendable` requires every implementation to be sendable; values of such an interface are sendable.
- INT-08 approved — Conversion to an interface value stays implicit where an interface type is expected.
- INT-09 approved — The keyword stays `interface`.
- INT-10 approved — `with I, J` on a struct or enum declaration derives the listed interfaces from their derive blocks. Long form: `impl I for T = derive;`.
- INT-11 approved — Only derivable interfaces (a derive block exists for that kind of type) may be listed; every field or payload must implement the interface, else an error at the declaration naming the field.
- INT-12 approved — Deriving and hand-writing the same interface for one type is an error; customising means writing the `impl` instead of `with`.
- INT-13 approved — Generic types derive conditionally on their actual fields: only parameters that appear in fields need the interface.
- INT-15 approved — When a type has its own method and an implemented interface supplies one of the same name, `x.m()` calls the type's own; `I.m(x)` reaches the interface's.
- INT-16 approved — `impl I for T` may also be written in the module that declares the type `T` is built from (`V`'s module may write `impl I for V[]`), as milestone 159 allowed for operators.
- INT-17 approved — A type built from two or more program types (`(Point, Line)`, `Map<Point, Line>`) gets `impl I for` it only in the module that declares `I`, so two libraries can never both write it; one built from a single program type keeps INT-16. Wrap the pair in a struct to implement it elsewhere. (From the v18 spec review, 2026-11-20.)
- INT-18 approved — An interface value of type `J` converts to interface `I` only when `J` extends `I` (`interface J: I`, at any depth); having items of `I`'s names no longer counts, for conversions and type arguments alike. (From the v18 spec review, 2026-11-20.)
- INT-19 approved — A generic `impl` block has one spelling: its type parameters right after `impl`, `impl<T: Num> Grid<T> { … }`, `impl<T: Show> Show for T[]`. The short form `impl Grid<T: Num>` is removed (an error naming the long form). (From the v18 spec review, 2026-11-20.)
- INT-20 approved — `+` joins interfaces that must all hold: bounds (`K: Key + Sendable`) and an interface's parents (`interface Store: Eq + Sendable`, was `,`). `with` keeps `,`: it lists separate derivations (`with Eq, Ord`). (From the v18 spec review, 2026-11-20.)
- INT-21 approved — A built-in name is reserved (E0302 to declare) exactly when syntax depends on it: `Option`/`Some`/`None`, `Result`/`Ok`/`Err`, `Eq`, `Ord`, `Display`, `Iter`, `Error`, `Add`…`Neg`, `FromEntries`/`FromItems`, `Ref`. Every other prelude name (`Map`, `Set`, `Console`, `Dir`, helpers) may be declared by a module, its own hiding the built-in there. (From the v18 spec review, 2026-11-20.)
- IMP-07 approved — One loop rule: `for … of c` over an array, map or set goes through `c` as it was when the loop started, and the body may change `c` (no longer an error for arrays); copy-on-write makes it free unless the body writes. (From the v18 spec review, 2026-11-20.)
- LEX-01 approved — Strings take `\u{…}` escapes as chars do. (Claude's call from the v18 spec review, listed for review, 2026-11-20.)
- LEX-02 approved — `return`, `break` and `continue` stay statements, so a match arm that leaves is a block: `.Ok(r) => { return r; }`. `.Ok(r) => return r,` stays invalid: one spelling, and the block is what "an arm that never finishes fits any type" means. (The user, 2026-11-20, from writing the skill.)
- CAP-18 approved — Every `Dir` method with nothing to return gives `Result<(), string>` (`make_dir`, `remove_file` and kin gave `Result<i64, string>` with `Ok(0)`). (Claude's call from the v18 spec review, listed for review, 2026-11-20.)
- INT-14 approved — Nothing is derived automatically: `Eq`, `Key`, `Ord`, `Json` only through `with` or `impl`. `Display` is never derivable. `Debug` is universal (PRE-02).

## EQ — equality, keys, order

- EQ-01 approved — `interface Eq { fun eq(self, other: Self): bool; }`, used by `==`, `!=`, `assert_eq`, `T: Eq`.
- EQ-02 approved — `interface Key: Eq { fun hash(self, mut h: Hasher); }`, required for map keys and set elements. Law: `a == b` implies equal hashes, and every value equals itself.
- EQ-03 approved — `with Key` implies `with Eq`.
- EQ-04 approved — Floats implement `Eq` (IEEE: `NaN != NaN`) but not `Key`. Retires E0441.
- EQ-05 approved — E0454 retired: `Key` requires `Eq`.
- EQ-06 approved — Built-ins: integers, `bool`, `char`, `string`: `Eq` + `Key`. Floats: `Eq` only. Arrays, tuples, `Option`, `Result`, `Map`, `Set`: conditional on their parts. C handles (`extern type`, `CPtr`): `Eq` + `Key` by address. `Ref`, tasks, channel ends, actor handles, functions, interface values: neither (`a.is(b)` for identity where it exists).
- EQ-07 approved — Test helper `assert_key_laws(a, b)`.
- EQ-08 approved — Same split for order: floats have `<` but not `Ord`; sorting floats takes an explicit order (`xs.sort_by(f64.total_order)`).
- EQ-09 approved — `with Ord` compares fields lexicographically in declaration order.
- EQ-10 approved — The prelude's `eq.la` blocks stay, split into `Eq` and `Key` blocks, hashing through `Hasher` instead of folding FNV-1a.
- EQ-11 approved — The interface for map keys is named `Key` (not `Hash`).

## IMP — implicit behaviour

- IMP-01 approved — Discarding a `Result` is a compile error; `let _ = e;` discards visibly.
- IMP-02 approved — Enum variants in patterns are written `.Variant` (top-level arms and inside payloads); a bare name in a pattern is always a binding.
- IMP-03 approved — `as` only for conversions that can't lose information; `x.try_into()` gives an `Option`; deliberately lossy conversions are spelled out (spelling: OPN-03).
- IMP-04 approved — Handles (`Ref`, channel ends, tasks, actor handles) share on copy; documented, no syntax change.
- IMP-05 approved — Implicit copies (value semantics, copy-on-write) stay; a lint for large copies in loops belongs to the linter.
- IMP-06 approved — Stays implicit: `let` inference, literals typed from context, generic argument inference, closure parameter inference, tail expressions as results, conversion to interface values.

## TYA — explicit type arguments

- TYA-01 approved — Type arguments may be written at a use; optional, inference stays the default.
- TYA-02 approved — `_` infers an individual argument: `convert<_, string>(x)`.
- TYA-03 approved — Allowed on generic calls `max<i64>(a, b)`, function values `max<i64>`, associated functions `Chan<Job>.new(16)`, a method's own parameters `xs.map<string>(f)`, struct and collection literals `Pair<i64> { … }`, `Map<string, i64> {}`, variants `Option<i64>.None`.
- TYA-04 approved — Parsing (context rule): after a name, `<` begins type arguments when a list of types closed by `>` is followed immediately by `(`, `.`, `{` or the end of an expression (`)`, `,`, `;`, `]`); otherwise it is a comparison.
- TYA-05 approved — `f(a < b, c > (d))` reads as type arguments; the error for intended comparisons suggests parentheses.
- TYA-06 approved — `Stack::<i64>` stays E0207, with help `Stack<i64>`.
- TYA-07 approved — Errors: wrong count; an unmet bound (E0419); an argument contradicting the code is a type mismatch at the argument.

## LIT — map and set literals

- LIT-01 approved — `Map { "a": 1, "b": 2 }`, `Set { 1, 2, 3 }`, `Map {}`, `Set {}`; `#{…}` is removed.
- LIT-02 approved — Map keys are expressions; an unknown bare name used as a key gets help suggesting the string form.
- LIT-03 approved — Prelude interfaces `FromEntries<K, V>` and `FromItems<T>` give any implementing type the literal forms `Name { k: v }` and `Name { a, b }`; `Map` and `Set` implement them in the prelude (the special handling in `codegen/maps.rs` moves there).
- LIT-04 approved — Braces after a type name: a struct takes field names; a `FromEntries` type takes entries; a `FromItems` type takes items. A struct implementing either is built with braces only as a collection.
- LIT-05 approved — In conditions, these literals need parentheses, as struct literals do.
- LIT-06 approved — The debug form prints `Map { "a": 1 }` and `Set { 1, 2 }`.
- LIT-07 approved — In the module that declares a type implementing `FromEntries` or `FromItems`, braces after its name are its struct literal (how it builds itself); everywhere else they are entries or items.

## PRE — what moves out of the compiler

- PRE-01 approved — Principle: the compiler defines value representation, memory, control flow and the C boundary; everything expressible in Lugha with a few intrinsics lives in the prelude or `std`.
- PRE-02 approved — Printing: a universal `Debug` interface with prelude derive blocks for every type; `Display` hand-written. Replaces `codegen/show.rs` and `show_iface.rs`.
- PRE-03 approved — The prelude writes `impl` blocks for built-in types (`impl string`, `impl<T> Option<T>`, arrays, `char`, `Result`), replacing the name table in `check/helpers.rs`.
- PRE-04 approved — Tuple ordering is a `derive Ord for tuple` block (the tuple code in `codegen/ordering.rs` goes).
- PRE-05 approved — `@caller_location`: panics in a marked function report the caller's location. `unwrap`, `expect`, `assert`, `assert_eq`, `assert_ne` become prelude code, and users can write their own helpers.
- PRE-06 approved — The ambient I/O intrinsics (`print`, `println`, `eprint`, `eprintln`, `read_line`, `read_file`, `write_file`, `append_file`, `exit`, `args`) are removed, replaced by capability methods in `std` over the `$os` built-ins.
- PRE-08 approved — Sizes are written with prelude methods on integers returning bytes: `8.kb()`, `64.mb()`, `1.gb()`.
- PRE-07 approved — Stays in the compiler: arithmetic, casts and overflow checks; array layout, growth, copy-on-write and copying; enums, tuples, closures, interface values and dispatch; `?`, `match`, loops, `defer`, unwinding; `spawn`, scopes, channels, actor message plumbing; interpolation and `format` checking; the C ABI.

## CAP — capabilities

- CAP-01 approved — No ambient authority: files, network, wall-clock time, environment, processes, OS randomness, the console and signals are capability values.
- CAP-02 approved — The capabilities: `Console`, `Fs` (narrowed to `Dir`), `Net`, `Clock`, `Env`, `Proc`, `Entropy`, `Signals`.
- CAP-03 approved — Not capabilities: seeded random generators, scheduling time (`sleep`, `after`, `within`: owned by the runtime and scopes), panics.
- CAP-04 approved — `main` asks for what it needs by type, in any order, each at most once: `args: string[]`, `root: Scope`, capabilities. `args()` is removed; the exit code is `main`'s `i32` result.
- CAP-05 approved — Capabilities are explicit parameters: never carried by scopes, never implicit parameters.
- CAP-06 approved — Narrowing is one-way: `fs.dir(path)`, `.read_only()`, `net.only("host:port")`, `net.listen_only(":8080")`, `net.connect_only()`, `env.only([…])`, `console.stdout_only()`, `console.stderr_only()`. A `Dir` can't reach above itself.
- CAP-07 approved — Capabilities are interfaces, so fakes are ordinary types implementing them.
- CAP-08 approved — Printing needs a `Console`: `fun main(console: Console) { console.println("Hello, world!"); }`.
- CAP-09 approved — `debug(x)` is ambient, writes the debug form with file and line to stderr, and is a compile error in release builds.
- CAP-09a approved — The release-build error covers the program's own package only; in a dependency, `debug(x)` compiles at -O2 to just `x`, printing nothing. A library's author still meets E0465 building that library at -O2. (From the v18 spec review, 2026-11-20.)
- CAP-10 approved — `exit` is removed; deep code cancels the root scope or returns an error.
- CAP-11 approved — `env.set_current_dir` is removed; `fs.cwd()` gives the starting directory as a `Dir`.
- CAP-12 approved — `Signals` gives a `Receiver` of signal events; Ctrl-C cancels the root scope regardless (CAN-09).
- CAP-13 approved — Tests ask for capabilities by type: a fresh temporary `Dir` (deleted after), a fake `Clock` at a fixed date, a captured `Console`, a seeded `Entropy`, a fake local `Net`; real `Fs`, `Net` and `Proc` only in integration tests (OPN-06).
- CAP-14 approved — Capabilities are sendable (they can be passed to tasks) but not `Wire`.
- CAP-15 approved — Library conventions (documentation, not rules): take the narrowest capability, accept interface types, capture capabilities in constructors, keep pure functions free of them.
- CAP-17 approved — The prelude declares `Scope` and the capability interfaces (`Console`, `Fs`, `Dir`, `Net`, `Clock`, `Env`, `Proc`, `Entropy`, `Signals`) with their core methods; the runtime supplies the real implementations; higher-level libraries (`std.http`, `std.tls`, `std.json`, `std.toml`, folder helpers) stay `std` imports taking capabilities as parameters. A module's own declaration of one of these names means its own.
- CAP-16 approved — With capabilities, the only per-package permission needed is `ffi` (SEC-04).

## SEC — security (explicitly approved, per CLAUDE.md)

- SEC-01 approved — `Dir` confinement is enforced by the kernel (`openat2` with `RESOLVE_BENEATH`, Linux 5.6+); other platforms need an equivalent when ported.
- SEC-02 approved — `net.only` restricts by host name; private and local addresses are refused unless `.allow_private()`.
- SEC-03 approved — Listening and connecting are narrowed separately.
- SEC-04 approved — `extern` requires `allow = ["ffi"]` in the root `lugha.toml` for every package that declares one, indirect dependencies included; `ffi` packages are documented as fully trusted.
- SEC-05 approved — Process lockdown: after opening what `main` asked for, the runtime restricts the process (seccomp, Landlock) to the kinds of capability `main` declared, containing C code too: without `Net`, no sockets; without `Proc`, no starting programs; without `Fs`, no files. It is as coarse as `main`'s parameters: a `main` holding `Fs` keeps the whole filesystem for C, and narrowing to a `Dir` later binds Lugha code only. Per program; Linux first; kernel versions and system-call lists verified before the PRP.
- SEC-06 approved — Sandboxed C libraries (`sandbox = true`: C compiled through WebAssembly to bounds-checked native code), a later milestone.
- SEC-07 approved — Running a library in a separate process is a library pattern, not a language feature.

## MEM — memory

- MEM-01 approved — Precise reference counting; Boehm is removed (`third_party/bdwgc`, conservative scanning, and the "integer that looks like a pointer" known issue).
- MEM-02 approved — Counts are non-atomic within a task. When a value crosses to another task, everything reachable from it that is still task-local is marked shared (atomic counts) in one walk; parts already shared are skipped, so a send costs the size of its not-yet-shared part, once. Marking only the outer object would let two tasks change a shared child's count at once.
- MEM-03 approved — The compiler removes most count operations using liveness (last uses become moves).
- MEM-04 approved — Copy-on-write writes in place when the count is 1, replacing the sticky shared flag.
- MEM-05 approved — `Weak<T>` for non-owning references: caches, observers, registries, back-links.
- MEM-06 approved — Cycles can only form through `Ref`, channels and task or actor handles; a small per-task cycle collector looks only at those, in budgeted slices at deterministic points.
- MEM-07 approved — A scope's end breaks the handle cycles it created. Met differently (2026-11-21): an actor lets go of its state when it ends, and scopes stop their actors when they end, so the scope needn't track what it made; channel cycles break when a channel's last receiver goes, which then lets go of what it buffers. Left: a channel holding its own receiver.
- MEM-08 approved — In tests, a report of every cycle the collector freed, with creation sites. The lint for cycle-prone `Ref` types without `Weak` is dropped (2026-11-21): with the collector freeing `Ref` cycles it would fire on every singly linked list and graph, and the test note already points at cycles worth a `Weak`.
- MEM-09 approved — Large frees are deferred and done in budgeted slices.
- MEM-10 approved — `Pinned<T>` holds a count for a value C keeps; keeping one without `Pinned` stays undefined behaviour.
- MEM-11 approved — Memory limits per scope: `scope s memory 64.mb() { … }`; exceeding it fails the scope with `Failure.Memory`.
- MEM-12 approved — Handles aren't closed automatically; dropping an unclosed file or socket warns.
- MEM-13 approved — Objects never move.
- MEM-14 approved — Allocation goes through a fast multi-threaded allocator behind `lugha_rt_alloc`.
- MEM-15 approved — Reference counting is conditional on the performance targets (PRF); plan B is a heap per task with tracing collection.

## HSH — hash seeding

- HSH-01 approved — A keyed, flood-resistant hash replaces FNV-1a; the exact algorithm is chosen by benchmark.
- HSH-02 approved — One secret seed per process, read from OS randomness by the runtime at startup; not a capability, because insertion-ordered iteration and order-independent equality make it unobservable.
- HSH-03 approved — The spec states that the runtime may use randomness that cannot affect a program's results.
- HSH-04 approved — Deterministic mode uses a fixed seed derived from the test seed; shuffle mode varies it.
- HSH-05 approved — Raw hash values are visible; the spec says they differ between runs and must never be stored or sent.
- HSH-07 approved — `Hasher` has one method to feed a value in: `h.add<T: Key>(x: T)`.
- HSH-06 approved — Maps cross `Wire` as entries and are rebuilt with the receiver's seed.

## COR — coroutines

- COR-01 approved — Stackful coroutines: any function can pause; no `async`/`await`, no colouring.
- COR-02 approved — Bare `spawn` is removed; tasks start only in a scope: `s.spawn(f(x))`.
- COR-03 approved — Sendable rule: spawn arguments, captures of spawned calls and channel elements must be sendable, worked out from a type's parts. `Ref`, and anything that may hide one (closures capturing one, interface values unless `: Sendable`), isn't.
- COR-04 approved — Children receive copies (value semantics; copy-on-write keeps it cheap).
- COR-05 approved — The model is the same on one core and many.

## SCO — scopes

- SCO-01 approved — `scope s { … }` with a named handle.
- SCO-02 approved — Scope handles, and task handles from them, are second-class: passed as arguments only; never returned, stored, put in arrays, sent, or captured by a closure that can outlive them.
- SCO-03 approved — The body runs in the current task; the scope's value is the body's, once every child has finished.
- SCO-04 approved — A child fails by panicking or returning `Err`; a deadline also fails the scope. A scope's type is `Result<T, Failure>`; `Failure` is `Error(Error)`, `Panic(string)`, `Deadline` or `Memory` (a memory limit exceeded), and records which task.
- SCO-14 approved — Leaving a scope's body early (`return`, `?`, `break`) cancels its remaining tasks with `.Parent`, waits for them, runs the body's defers, then completes the exit.
- SCO-13 approved — `Failure` implements `Error`, so `?` on a scope works in a function returning `Result<_, Error>`.
- SCO-05 approved — Default policy, all-or-nothing: the first failure cancels the other children and the body; the scope evaluates to that failure; cancellation stops at the scope's edge.
- SCO-06 approved — `independent` policy: failures go to the scope's failure handler as they happen; by default printed to stderr with the task's name; `independent(f)` supplies another (OPN-07).
- SCO-07 approved — Other patterns (racing, first success) are library functions.
- SCO-08 approved — Deadlines: `within ms`, `until t`, `s.remaining()`; nesting only tightens; they cover the body and every child.
- SCO-09 approved — `limit n`: `spawn` pauses while `n` children run.
- SCO-10 approved — When the body finishes: wait for every child, then run the body's defers.
- SCO-11 approved — Task handles: `join()`, `cancel(reason)`, `name(…)`. `join()` returns `Result<V, Stopped>`: `V` is the task's value (its `Ok` value when it returns a `Result`); `Stopped` is `.Cancelled(reason)`, `.Failed(Error)` or `.Panicked(string)`, and implements `Error`. Joining is optional; an unjoined failure still fails the scope.
- SCO-12 approved — `main` runs in a root scope it may receive; returning from `main` waits for its children.

## CAN — cancellation

- CAN-01 approved — Cancellation is invisible in the cancelled task: it stops at its next safe point, its stack unwinds, and its defers run.
- CAN-02 approved — Safe points are every pause point and every preemption point (function entries, loop iterations).
- CAN-03 approved — Reasons: `Deadline`, `SiblingFailed`, `Parent`, `Requested(string)`, `Shutdown`; visible to observers.
- CAN-04 approved — The first real error wins; cancellations it caused aren't reported as failures.
- CAN-05 approved — `shield within ms { … }` holds off cancellation, and always has a deadline.
- CAN-06 approved — Completed work is never discarded: a value already received stays received, a value not yet taken stays in its channel, and helper-thread operations finish even if their task stopped waiting.
- CAN-07 approved — Cancellation never unwinds through C; it is held until C returns.
- CAN-08 approved — Panics unwind, run defers and fail their scope; only a panic reaching the outermost scope ends the program (replaces "panics skip defers").
- CAN-09 approved — Ctrl-C cancels the root scope with `Shutdown`; a second Ctrl-C exits at once.
- CAN-10 approved — Principle: cancellation is control flow for the cancelled task and information for anyone observing it.
- CAN-11 approved — `select` simply stops when its task is cancelled.
- CAN-12 approved — The spec notes that defers should release resources, not inspect half-built data.
- CAN-13 approved — In a program that never started a task, the first Ctrl-C also cancels `main` with `Shutdown`, noticed at its next pause (no preemption checks are added to task-free programs); a busy loop that never pauses ends at the second Ctrl-C. (From the v18 spec review, 2026-11-20.)

## CHN — channels

- CHN-01 approved — `Chan<T>.new(n)` returns `(Sender<T>, Receiver<T>)`; the capacity is required; `0` is a direct handoff; `Chan<T>.unbounded()` must be asked for.
- CHN-02 approved — `send` waits while the channel is full and returns nothing; sending after `close` panics.
- CHN-03 approved — `recv` returns `Option<T>`; `None` only when closed and empty.
- CHN-04 approved — Only senders close; nothing closes automatically; scopes time the close.
- CHN-05 approved — A `Receiver` is an iterator: `for x of rx`.
- CHN-06 approved — `try_recv` gives `Ready(x)`, `Empty` or `Closed`; `try_send` gives `Sent`, `Full(v)` or `Closed(v)`.
- CHN-07 approved — `select` keeps its `recv`, `send`, `after` and `default` arms, with a fair, seeded pick.
- CHN-08 approved — `s.merge(receivers)` in the library.
- CHN-09 approved — The element type must be sendable; senders and receivers are sendable; channels are not `Wire`.
- CHN-10 approved — Broadcast and `Watch<T>` are library types.
- CHN-11 approved — Channels record where they were created, for diagnostics.
- CHN-12 approved — `Ring<T>`: a channel for callbacks on foreign threads. The foreign thread uses only `try_send`/`try_recv`; a task on the other end may also wait (`recv`, and in `select`).

## ACT — actors

- ACT-01 approved — Actors stay, as the unit of distribution; started in scopes (`s.spawn(Stats { … })`) and stopped when their scope ends.
- ACT-02 approved — Mailboxes are bounded: 1,024 by default, set at the declaration (`actor Logger mailbox 10_000 { … }`).
- ACT-04 approved — Calling a local actor that has stopped or failed panics in the caller, as in v17. `stop()` and `is()` stay; v17's `join()` on actor handles is removed, since scopes report actor failures.
- ACT-03 approved — An actor's methods are its protocol; the compiler generates its message type.

## SCH — scheduling

- SCH-01 approved — One worker per core, each with its own queue, plus a shared queue; idle workers steal half a busy worker's queue; tasks stay put unless stolen.
- SCH-02 approved — Preemption by budget: a step is a function entry or a loop iteration; a slice is a fixed number of steps (4096 today, tuned by measurement).
- SCH-03 approved — Runtime operations charge one step per chunk of work (around 4 KB); very long ones pause partway.
- SCH-04 approved — The compiler may omit the countdown in loops it can prove short.
- SCH-05 approved — FIFO ready queue; a "run next" lane for a woken receiver, at most 3 times in a row, inheriting the waker's slice; the shared queue is checked every 61 local picks.
- SCH-06 approved — Workers poll I/O without waiting every N switches; idle workers wait on I/O and timers; helper-thread completions arrive through the same poller.
- SCH-07 approved — Blocking work inside capabilities (files, DNS, starting and waiting for processes) and `extern blocking` calls run on a bounded helper pool (configurable, about 16 threads) with a bounded queue (callers wait when it's full); helpers touch only copied data.
- SCH-08 approved — Ordinary `extern` calls run inline; the runtime warns when one held a worker too long.
- SCH-09 approved — Two priorities, `normal` and `background`, set per scope (`scope s background { … }`) and inherited; background runs at least every 8 normal slices; no priority inheritance (documented).
- SCH-10 approved — Per-worker timer heaps, checked at every scheduling decision; timers use scheduling time.
- SCH-11 approved — Modes: normal (every core, real time), deterministic (one worker, virtual time, fake or recorded capabilities), shuffle (seeded ready-queue order). `lughac test` defaults to deterministic; `--shuffle=SEED`.
- SCH-12 approved — In deterministic mode, helper-thread results are taken only at fixed scheduling points.
- SCH-13 approved — Visibility: per-task accounting, a task dump, slow-call warnings, scheduler statistics, named tasks, deadlock reports naming what each task waits on.
- SCH-14 approved — Determinism is a mode, not a property of every run.

## STK — stacks and C calls

- STK-01 approved — C never runs on task stacks: every `extern` call switches to its worker's C stack (8 MiB); helper threads have their own.
- STK-02 approved — Task stacks are large lazy reservations (8 MiB by default) carved from pooled regions; only touched pages use memory; pages are released before reuse; stacks never move.
- STK-03 approved — No guard pages on task stacks: a check at function entry raises a clean `stack overflow` panic that unwinds, with headroom reserved for the unwinding.
- STK-04 approved — The default shrinks automatically on small address spaces (such as 39-bit ARM64).
- STK-05 approved — Stack size is a scope header word, `scope s stack 64.mb() { … }`; a project default in `lugha.toml`. Tasks have no per-task stack setting.
- STK-06 approved — `main` is a task like any other.

## CBK — callbacks and thread-bound values

- CBK-01 approved — General mechanisms only: thread-affine values (`extern type X pinned;`), pinned scopes whose tasks run on one specific worker, and the callback rules below.
- CBK-02 approved — A callback that returns a value is `direct`: it runs during C's call and can't pause; pausing is a runtime error.
- CBK-03 approved — Every extern callback says how C's call runs it, before `fun`: `cb: direct fun(Window, data)` runs during C's call, on whatever thread C uses, and can't pause; `cb: spawned fun(Window, data)` starts a task in the scope it was registered with on each call, which may pause (needs `data` and no result; its captures `Sendable`). Neither written is an error. Behaviour must be visible in the syntax, never implied by a callback's shape (the user, 2026-11-20).
- CBK-04 approved — A `direct` callback on a foreign thread runs there, can't pause, touches only sendable data, and talks to tasks through `Ring<T>`; a `spawned` one is delivered to the workers as a task. A third word, `cb: realtime fun(…)`, is a `direct` callback checked at compile time (Clang's `nonblocking` model): nothing in it or the Lugha functions it calls allocates or waits; an error, not a lint (the user, 2026-11-20, replacing "a lint flags allocation in a `direct` callback").
- CBK-05 approved — A panic in an inline callback is caught at the C boundary: C gets a zero value, and the registering scope gets the failure.
- CBK-06 approved — Cancellation never interrupts an inline callback.
- CBK-07 approved — Callbacks are registered against a scope; events after it ends are dropped, and captured values are released at C's `release` or the scope's end.
- CBK-08 approved — The scope-end rule (calls after the registering scope are dropped) covers callbacks with `data`, which hold captures; a callback without `data` is a capture-free function and runs whenever C calls it. (From the v18 spec review, 2026-11-20.)
- CBK-09 approved — Registering a `realtime` callback reserves one spare thread record (fiber and C stack); a C thread's first call claims it with one compare-and-swap, so no `realtime` call allocates; the next registration refills it. Several new C threads before another registration still allocate, and the spec says so. (From the v18 spec review, 2026-11-20.)

## DST — distribution and replay

- DST-01 approved — Two levels: sendable (between tasks) and `Wire` (encodable; a derivable interface, built like `json.Json`). Channels, closures, C handles and capabilities aren't `Wire`.
- DST-02 approved — An actor can be used remotely only if its message arguments and replies are `Wire`; checked only where it is used remotely.
- DST-03 approved — `Remote<T>` is a separate handle type whose calls return `Result<_, CallError>` (`Timeout`, `Unreachable`, `Stopped`, `Rejected`); deadlines come from the scope.
- DST-04 approved — Order is kept per sender and actor; delivery is at most once; a `Timeout` doesn't say whether the call ran.
- DST-05 approved — Scopes stop at the machine boundary: a remote actor runs in its own machine's scope, and callers `monitor()` it (`Stopped`, `Crashed`, `Unreachable`); supervision is a library pattern.
- DST-06 approved — A node's `register(name, handle)` and `lookup<T>(name)`; `Remote<T>` handles are `Wire`.
- DST-13 approved — Nodes come from a `std.cluster` library: `cluster.start(net, identity)` returns a node; `cluster.simulate` lives there too. The language provides only `Wire`, `Remote<T>` and actors' generated messages.
- DST-14 approved — A recorded run uses deterministic, single-worker scheduling with real capabilities, documented as costing parallelism while recording.
- DST-07 approved — Messages are encoded by name or tag; each actor's message set has a fingerprint checked when nodes connect.
- DST-08 approved — Nodes authenticate each other (mutual TLS); only messages cross, never code; decoding has size, depth and count limits; only registered actors are reachable. (Authentication and cryptography still need your review at PRP time.)
- DST-09 approved — Cluster simulation in one process: virtual network and clock, seeded faults, crashes and partitions.
- DST-11 approved — A full mailbox on a remote actor answers `Rejected` at once, so one busy actor never stalls a connection shared by others.
- DST-12 approved — A `Remote<T>` that `lookup` finds on the same node skips encoding and the network, keeping the same `Result`-returning calls.
- DST-10 approved — Replay: capability results are recorded with the step they arrived at; replay injects them and refuses a different binary; recordings are opt-in and treated as secret; replay never starts real processes.

## PRT — portability

- PRT-01 approved — lughac builds programs for platforms other than the one it runs on. This changes where a program runs, not what it means, so it stays in v18 and ships as `0.18.x` releases; only a language change (such as `@cfg`) would need v19.
- PRT-02 approved — Building for the machine lughac runs on uses `cc`, as today; any other target uses `zig cc -target <zig triple>`, for both the runtime and the link; `$LUGHA_CC` overrides both. zig is a program found on `PATH`, never a crate.
- PRT-03 approved — The runtime stays C source, compiled for each target by the build's compiler and cached; no precompiled runtimes are shipped.
- PRT-04 approved — One platform at a time, in order: zig and Linux musl; running and testing ARM builds under `qemu`; the runtime split by OS; macOS; Windows; WebAssembly (WASI).
- PRT-05 approved — `std.tls` and `std.crypto` on a foreign target are an error; the host's OpenSSL is never linked for another platform.
- PRT-06 approved — zig 0.17.0 or newer, its version checked once per build with a clear error; zig's cache under lughac's cache root; a target's zig spelling comes from a fixed table, while codegen keeps the LLVM triple.
- PRT-07 approved — The first new targets are `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`, linked statically with zig's libunwind; `aarch64-unknown-linux-gnu` moves from `aarch64-linux-gnu-gcc` to zig (breaking).
- PRT-08 approved — The runtime splits into shared code, `os/posix.c` and one file per OS, joined per target. The OS layer's line is the operation (a socket's receive, send, accept, connect, cancel, close), not registering a descriptor; the poller hands back finished waiters, and helper jobs finish the same way.
- PRT-09 approved — A cancelled socket call resumes its task only once the OS has let go of its buffer: at once on POSIX, after the completion on Windows.
- PRT-10 approved — Idle workers park on their own parker: a futex word where the OS has one (Linux, Windows, macOS 14.4+), a semaphore otherwise.
- PRT-14 approved — The task switch is written per CPU and OS (on Windows it also switches the thread's recorded stack bounds); stack overflow is caught per OS (`SIGSEGV` and `SIGBUS` on POSIX, a vectored exception handler on Windows).
- PRT-11 deferred — What a program promises on an OS whose kernel can't lock it down (seccomp, Landlock), and the `Dir` confinement each OS uses (SEC-01): decided with that OS's port, with security review. macOS approved: a `Dir` confined by walking each path one name at a time, as Go's `os.Root` does (milestone 254); lockdown through Seatbelt's coarse parts, no sockets without `Net` and no new processes without `Proc`, files not limited by the kernel (milestone 255). Windows and WASI still to decide.
- PRT-12 deferred — Platform-specific Lugha code (`@cfg`), only if a port needs it; a language change, so v19 or an approved Draft rule.
- PRT-13 deferred — Windows sockets through overlapped I/O or the AFD poll; precompiled runtimes with `lld` so foreign builds need no zig; OpenSSL for foreign targets.

## PRF — performance targets (baseline: `bench/RESULTS.md`, 2026-10-31, commit 5d5ab18)

- PRF-01 approved — fannkuch, fib, mandelbrot, nbody: within 3% of today.
- PRF-02 approved — binarytrees, wordfreq: at most 10% slower than today.
- PRF-03 approved — Peak memory: binarytrees at most 35 MB (today 70.3); wordfreq no worse than 83.4 MB, aiming for 47.
- PRF-04 approved — No memory-management pause over 1 ms (new benchmark).
- PRF-05 approved — Parallel binarytrees at least 70% efficient across cores; channel ping-pong within 1.5× of Go; under 10 KB per idle task (new benchmarks).
- PRF-06 approved — The preemption countdown costs at most 5% on fib and fannkuch.
- PRF-07 approved — Method: record a baseline with `bench/run.py --save` before any change; same machine; median of 5; `-O2`; new benchmarks get Go and Rust versions; never in CI.
- PRF-08 approved — A small miss: more count elimination. A large miss: plan B (MEM-15).
- PRF-09 approved — The memory prototype (ORD-03) measures the whole new cost of a call on fib together: the stack check at entry (STK-03), the countdown, cleanup paths for unwinding, and count operations, against PRF-01.

## ORD — build order

- ORD-01 approved — Phase contents: capabilities include process lockdown; structured concurrency covers scopes, cancellation, the sendable rule and channels; the runtime phase covers workers, helper threads, reference counting, scheduling, stacks and virtual time. Each phase is one or more PRPs. Numbering: ORD-02.
- ORD-02 approved — A phase before capabilities for the language-surface changes (INT, EQ, IMP, TYA, LIT, PRE), since capabilities need nominal interfaces (for fakes) and type arguments. The phases become: 0 design; 1 language surface; 2 capabilities; 3 unwinding; 4 structured concurrency; 5 runtime; 6 `Wire`; 7 replay and simulation; 8 distribution.
- ORD-03 approved — A memory prototype comes before unwinding: reference counting with moves on last use and marking on send (MEM-02, MEM-03), measured against PRF-01–03 and PRF-09. It decides between reference counting and plan B (MEM-15) before unwinding, scopes and actors are built, since every unwinding cleanup path differs between the two. The phases become: 0 design; 1 language surface; 2 capabilities; 3 memory prototype; 4 unwinding; 5 structured concurrency; 6 runtime; 7 `Wire`; 8 replay and simulation; 9 distribution.

## OPN — open questions

- OPN-02 approved — `"{x}"` interpolation and `to_string(x)` require `Display`; `debug(x)` shows any value's debug form.
- OPN-03 approved — Lossy conversions are methods naming the loss, with type arguments: `x.truncate<u8>()`, `x.saturate<u8>()`, `y.round<i64>()` (and the other float-to-integer modes); `x.try_into<u8>()` gives an `Option`. `as` only where no information can be lost, so `i64` to `f64` and `f64` to `f32` aren't plain `as`.
- OPN-04 approved — Generic actors are allowed (`actor Cache<K: Key, V>`): each instance has its own message type; remote use needs concrete `Wire` type arguments, included in the fingerprint. `impl I for SomeActor` is allowed: its methods are message handlers, and the actor's handle satisfies `I`. A `Remote<T>` satisfies only interfaces whose methods return `Result`.
- OPN-05 approved — By default a scope's `Failure.Error` carries the child's error as the `Error` interface (exact type through type patterns). `scope<E> s { … }` makes it `Failure<E>`: every child's error must convert to `E` (checked at each `spawn`).
- OPN-06 approved — `@test(integration)` tests may ask for real `Fs`, `Net` and `Proc`, and run only with `lughac test --integration`.
- OPN-07 approved — Every scope setting is a header word before the body, in any order, each at most once: `independent` or `independent(handler)`, `within ms`, `until t`, `limit n`, `background`, `memory size`. Example: `scope conns independent(log_failure) limit 1_000 { … }`.
- OPN-09 approved — Every enum variant in a pattern takes the dot, including the prelude's: `.Some(x)`, `.None`, `.Ok(v)`, `.Err(e)`. A bare name in a pattern is always a binding.
- OPN-10 approved — The `Error` interface requires `Display`, so any error can be shown with `"{e}"`.
- OPN-08 approved — Final words, contextual (not reserved) where possible: `pinned`, `inline`, `queued`, `blocking`, `shield`, `within`, `until`, `limit`, `background`, `memory`.

## DEF — deferred (decide during building)

- DEF-01 deferred — Generators (`generate { … yield … }`).
- DEF-02 deferred — Data parallelism (`s.par_map(xs, f)`).
- DEF-03 deferred — Debugger and profiler support for tasks (needs debug information).
- DEF-04 deferred — Tracing across tasks (a request ID in a log capability).
- DEF-05 deferred — Detecting actor call cycles (A waits on B waits on A).
- DEF-06 deferred — Fairness between scopes.
- DEF-07 deferred — Revocable capabilities.
- DEF-08 deferred — Where scheduler settings live (`lugha.toml`, environment, code).
- DEF-09 deferred — Teaching: why hello world takes a `Console`.
- DEF-10 deferred — An optional `io_uring` backend behind the helper-pool interface.
- DEF-11 deferred — DNS without threads; process waiting through `pidfd`.
- DEF-12 deferred — Rebuilding a map with a fresh seed after a long probe sequence.
- DEF-13 deferred — Per-field derive options (`@json(skip)`; already on TODO.md).
- DEF-14 deferred — The spec's wording of the memory model for helper threads and C.
- DEF-15 deferred — `mut` closure parameters (today E0209).
- DEF-16 deferred — Recording and replaying runs that use several workers.

## SUP — superseded (never in the draft)

- SUP-01 — Cancellation as values at pause points, `task.check()`, "sticky" cancellation → CAN-01, CAN-02.
- SUP-02 — A `cancelled` arm in `select` → CAN-11.
- SUP-03 — Only actors may run on several cores; plain tasks share `Ref` on one core → COR-03, COR-05.
- SUP-04 — `Owned<T>` instead of actors → ACT-01.
- SUP-05 — A `Ui` capability, `ui_only` types, the toolkit owning the main loop → PRI-04, CBK-01.
- SUP-06 — `Wire` moved early for an app host → ORD-01.
- SUP-07 — 64 KiB default task stacks; 1 MiB reservations with guard pages; C on task stacks → STK-01, STK-02.
- SUP-08 — `=>` in map literals; `#{…}` → LIT-01.
- SUP-09 — Boehm, a thread-aware Boehm, a heap per task as the main plan → MEM-01 (heap per task survives only as plan B).
- SUP-10 — Capabilities carried by scopes; per-package `fs`/`net` permissions → CAP-05, CAP-16.
- SUP-11 — Structural interfaces; behaviour by method name; a method overriding a derive block → INT-01, INT-03, INT-12.
- SUP-12 — Every struct automatically `Eq`, printable and JSON-encodable → INT-14.
- SUP-13 — E0441 and E0454 → EQ-04, EQ-05.
- SUP-14 — Sharing checked only for actors (E0439's rules) → COR-03.
- SUP-15 — Bare `spawn`; tasks dropped when `main` returns → COR-02, SCO-12.
- SUP-16 — `Chan.new()` unbuffered, `Chan.buffered(n)`, `try_recv` as `Option`, unbounded mailboxes → CHN-01, CHN-06, ACT-02.
- SUP-17 — Printing generated by the compiler (`lugha_show_<type>`) → PRE-02.
- SUP-18 — `args()`, ambient `println`, `exit`, `set_current_dir` → CAP-04, CAP-08, CAP-10, CAP-11.
- SUP-19 — FNV-1a hashing → HSH-01.
- SUP-20 — Panics skip defers → CAN-08.
- SUP-21 — `as` truncating silently (a CLAUDE.md known issue) → IMP-03.
- SUP-22 — Bare variant names in patterns → IMP-02.
- SUP-23 — Sending a large value to another task costing one count; marking only the object that crosses → MEM-02.
- SUP-24 — Process lockdown containing C to the narrowed capabilities → SEC-05 (to the kinds `main` declared).
- SUP-25 — Unwinding as phase 3, before the memory model is settled → ORD-03.
- SUP-26 — `h: mut Hasher`, the `mut` after the colon → `mut h: Hasher`, as externs and `mut self` write it (PRP-198).
- SUP-27 — A callback returning nothing queued by default, `inline` (after the type) to opt out, `queued` to opt in on foreign threads (milestone 215 built the default) → CBK-03, CBK-04: `direct`/`spawned` before `fun`, always written.

## TODO.md items this resolves

Explicit type arguments (TYA); `From<T>` once per type argument (INT-05); a separate debug form (PRE-02); the warning for a `show` of the wrong shape (obsolete under INT-03); generic actors and actors as interface values (OPN-04); non-blocking I/O (SCH-07); portability stages B3–E (PRT-04).
