# Lugha v18 Specification

Nov 15, 2026 · @kithinji

**Status:** in progress (milestone 164 onwards). It is built from the approved plan, `Language v18 Draft.md`, one phase at a time, so until v18 is complete some of its rules hold and the rest of the language is still v17's, as described here. This file describes exactly what `lughac` implements. `Language v17 Specification.md` keeps v17 frozen.

**v15.** This is v14 plus package layout, operator overloading, the standard library (`std.math`, `std.random`, `std.env`, `std.fs`, `std.process`), copy-on-write arrays, loop ownership, nested `mut self` calls, numbers as `T`, user-defined `Ord`, and format widths in characters (milestones 51–66).

**v16.** This is v15 plus shared boxes (`Ref<T>`), block and doc comments, the editor tooling (`lughac lsp`, `fmt`, `test`, `explain`, `init`) and C interop (linking C libraries, callbacks and handles, strings from C, C structs by value) (milestones 67–90).

**v17.** This is v16 plus green threads, channels and `select`, actors, constants, struct update syntax, variants as functions, derivable interfaces, the standard library's networking, TLS, cryptography, HTTP and JSON, and ARM64 Linux (milestones 91–163).

**Changes since v17** (milestone 164 onwards):
- Nominal interfaces: a type implements an interface only through `impl I for T { … }`, which defines every item without a default and may replace defaults; having methods of the right names no longer satisfies an interface. The block may be written only in the module that declares the interface or the one that declares the type (E0459), once per interface and argument list (E0461); a generic interface may be implemented once per argument list (`impl From<string> for Path` beside `impl From<i64> for Path`). A type's own method and an interface's item may share a name: `c.area()` calls the type's own, `Shape.area(c)` the interface's. Without an own method, `c.area()` reaches the item of the one implemented interface in sight that declares it, and E0460 when two do (replacing E0430). The prelude's interfaces (`Eq`, `Ord`, `Iter`, `Display`, the operators) and derivable ones (`json.Json`) are still met by their methods' names until their own milestones, and built-in types can't implement a program's interfaces yet (milestone 164; sections 3, 4, 8).

- Deriving on request: a struct or enum meets a derivable interface only when it asks, `struct Site { … } with json.Json` or `impl json.Json for Site = derive;`, or implements it by hand, `impl json.Json for Site { … }`, which replaces the blocks (E0463 for both). A listed interface needs a block for the type's kind (E0462); its fields are checked at the declaration (E0450, E0451). `json.Json` gains a public item, `missing(path, field)`, what an absent field of the type decodes to, with a default; interfaces' associated functions may now have default bodies (milestone 165; sections 3, 4).

- `mut` parameters: `fun bump(mut c: Counter)` changes its caller's place, called as `bump(mut c)`; marked at both ends (E0443), only mutable places (E0501), no aliasing within a call (E0508), not through `spawn`, actor messages or function values (milestone 166; sections 3, 4, 5).

- `Eq` and `Key`: equality and map keys are asked for (`with Eq`, `with Key`) or written (`impl Eq`, `impl Key`); `Key` extends `Eq` and hashes into a `Hasher`; floats are `Eq` but not `Key`; interfaces may extend interfaces (`interface I: J`, E0464 as a type); `assert_key_laws`. E0441 and E0454 are retired (milestone 167; sections 3, 4).
- `Ord` as an interface: `interface Ord: Eq { fun cmp(self, other: Self): Ordering; }`, asked for (`with Ord`: fields in order, variants in declaration order) or written (`impl Ord`); tuples ordered through the prelude's block; floats keep `<` but aren't `Ord`, and sort with `f64.total_order`/`f32.total_order`, IEEE 754's total order (milestone 168; section 4).
- Built-in types implement interfaces: `impl Show for f64`, `impl Show for Point[]` in `Point`'s module, and generic blocks `impl<T: Show> Show for T[]`, for bounds, `x.item()`, `I.item(x)` and interface values; the prelude's blocks count too (`3.cmp(4)`) (milestone 169; section 4).
- Printing through `Debug` and `Display`: `print`, `println`, `to_string` and `"{x}"` need `Display`, written by hand (`impl Display for T`); every type has a debug form, `x.debug()`, from the prelude's `Debug` blocks or a hand-written `impl Debug`; `debug(x)` writes it to stderr with file and line and returns `x` (E0465 in an `-O2` build); panics show values in it (milestone 170; section 5).
- Operators through generic interfaces: `impl Mul<f64> for Vec2`, `impl Mul<Vec2> for f64` in `Vec2`'s module, the result from the block's method; interface parameters may have defaults (`interface Add<R = Self>`); a type may have one block of a generic interface per argument list, built-in types included (milestone 171; sections 4, 5).
- The prelude's methods on built-in types are ordinary blocks of each type's own methods (`impl string`, `impl<T> T[]`, `impl<T: Ord> T[]`, `impl string[]`, `impl char`, `impl<T> Option<T>`, `impl<T, E> Result<T, E>`), written only in the prelude; the method-name table goes, and hover and go to definition reach the prelude's methods (milestone 172; section 9).
- `@caller_location` marks a function or a type's own method whose panics report its caller's place; `unwrap`, `expect`, `assert`, `assert_eq` and `assert_ne` are prelude code marked with it, with the same messages and places as before (milestone 173; sections 2, 5, 9).
- Sizes in bytes: `8.kb()`, `64.mb()`, `1.gb()` on the integer types from `i16` and `u16` up, in powers of 1024, each keeping its receiver's type (milestone 174; section 5).
- A discarded `Result` is an error (E0467): an expression statement of type `Result` must be handled (`?`, `match`) or dropped on purpose with `let _ = …;` (milestone 175; section 6).
- Enum variants in patterns take a leading dot, `.Some(x)`, `.None`, `.Circle(r)`, and a bare name in a pattern is always a binding, at the top level too (E0212, E0468) (milestone 176; sections 3–4).
- Type arguments may be written at a use, `pick<string>([])`, `xs.map<string>(f)`, `Chan<Job>.new(4)`, `Pair<u8> { … }`, `Option<i64>.None`, with `_` for one left to inference; `as` converts only where nothing can be lost, and `try_into`, `truncate`, `saturate`, `round`, `floor` and `ceil` name the rest (E0469) (milestone 177; sections 3–5).
- Maps and sets are written `Map { "ann": 31 }`, `Set { 1, 2 }`, `Map {}` and `Set {}`, through the prelude interfaces `FromEntries` and `FromItems`, which any type may implement; `#{…}` is gone (E0213, E0470) (milestone 178; sections 3–4).
- The prelude's `Error: Display` is the shared error interface (`Result<T, Error>`, `?` from any error, `"{e}"`), implemented by `string` and std's errors; an interface that extends others is a value type, its table holding its ancestors' items (E0464 retired) (milestone 179; sections 4–6).
- Capabilities, first step: `main` takes `args: string[]`, `console: Console` and `fs: Fs` by type; printing, reading input and files go through `Console` and `fs.cwd()`; `print`, `println`, `eprint`, `eprintln`, `read_line`, `read_file`, `write_file`, `append_file`, `args` and `exit` are gone (E0471) (milestone 180; sections 5–6).
- Capabilities, second step: `Clock` (`now_ms`, `monotonic_ms`, `local_offset`), `Env` (`get`), `Proc` (`run`, `run_with`, `call`, giving a `ProcessOutput`) and `Entropy` (`bytes`, `next_u64`), taken by `main` and tests like `Console`; `Dir.path()`; `std.env` and `std.process` are gone, and `std.time`, `std.random` and `std.crypto` take the capabilities (milestone 181; sections 6, 9).
- Capabilities, third step: everything `std.fs` did is a method of `Dir` (`list`, `make_dir`, `make_dirs`, `remove_file`, `remove_dir`, `exists`, `is_dir`, `is_file`, `size`, `rename`, `set_mode`, `sync`, `open`, `create`, `append`, `read_bytes`, `write_bytes`, `append_bytes`), file streams are the prelude's `File` interface, `Console` gains `read_bytes()` and `flush()`, and `read_bytes`, `write_bytes`, `append_bytes`, `read_stdin_bytes` and `flush` are gone from programs (E0471); `std.fs` is removed (milestone 182; sections 5, 6, 9).
- Capabilities, fourth step: `Net` (`connect`, `connect_timeout`, `listen`, `resolve`, `udp_bind`, `udp_bind_shared`, `interfaces`) is taken by `main` and tests; `Listener`, `Conn` and `Udp` are prelude interfaces and `NetInterface` a prelude struct; `std.net` is removed, and `std.tls` and `std.http` take a `Net` first wherever they connect or listen (milestone 183; sections 6, 9).
- TLS over any `Conn`: OpenSSL runs on memory buffers and `std.tls` carries the bytes over the `Conn` a `Net` gives, so a fake `Net` covers TLS and `https://`; `tls.client`, `tls.client_keys` and `tls.server` wrap a connection made any way; `tls.Conn` and `tls.Listener` are `tls.SecureConn` and `tls.SecureListener`, which are a `Conn` and a `Listener`; certificates, keys and trusted CAs are PEM text (milestone 184; section 9).
- Tests get stand-ins: a fresh temporary `Dir`, a fixed `Clock`, an empty `Env`, a seeded `Entropy` and a loopback-only `Net`; `Fs` and `Proc`, and the real world, only in `@test(integration)` tests, run by `lughac test --integration` (milestone 185; section 9).
- Every `Dir` is confined to its folder by the kernel (`openat2`, `RESOLVE_BENEATH`): `..`, absolute paths and links leading out are `Err("path leads outside the folder")`; `fs.dir(path)` reaches any folder, `dir.dir(sub)` narrows, `dir.read_only()` drops writing (milestone 186; section 6).
- Narrowing the network, the environment and the console: `net.only("host:port")` (public addresses only, unless `.allow_private()`), `net.connect_only()`, `net.listen_only(addr)`, `env.only(names)`, `console.stdout_only()`, `console.stderr_only()` (milestone 187; section 6).
- The `ffi` permission: a package declaring C code (`extern`, `[link]`) builds only when the program's `lugha.toml` grants it `allow = ["ffi"]`, a grant covering the package's dependencies too (E0333); `lugha.lock`'s `[ffi]` table and `lughac update` show who may run C (milestone 188; section 6).
- Process lockdown: seccomp and Landlock limit the whole process, C code and started programs included, to the capability kinds `main` took (milestone 189; section 6).
- A panic unwinds its task, running the pending `defer`s and letting go of what each frame holds (milestone 194; sections 5, 6, 9), through unwind tables: only calls with something to clean up get a landing pad. Programs link libgcc's unwinder, as C programs do.
- Stacks and C calls (milestone 205; sections 7, 8, 9): a task's stack, `main`'s included, is an 8 MiB reservation from pooled regions without guard pages, so the number of tasks is bounded by memory, not the kernel's mappings; each function's entry checks the stack, and an overflow is a `stack overflow` panic that unwinds, running `defer`s; C runs on its own stack, the process's, and a C callback can't wait.
- The skill's tasks and examples (milestone 235; section 9): five how-to pages (a command-line tool, a TCP server, an HTTP service, wrapping a C library, testing) and five complete programs, completing the skill's layout; every line of code in the skill is compiled by the tests.
- The skill's library pages, second batch (milestone 234; section 9): the network, HTTP, TLS and cryptography, JSON and TOML, and the environment and other programs, every example run over loopback or checked, none reaching the internet.
- The skill's library pages (milestone 233; section 9): `library/` reference pages for text, collections, console and files, and numbers, time and randomness, each naming every public item of the std modules and every prelude method it covers, as the tests check; `lughac skill NAME` for a short name two pages share is exit 2, listing their full names.
- The skill's third batch (milestone 232; section 9): concept pages on tasks and scopes, channels and actors, modules and packages, and C interop, completing the syllabus; a page's example may span several files.
- The skill's second batch (milestone 231; section 9): concept pages on collections, interfaces, generics, iterators and capabilities, each example checked.
- **Breaking:** `lughac spec` is gone (milestone 230; section 9): `lughac skill` prints the Lugha skill, readable pages on writing Lugha that an AI assistant can load (`lughac skill --export DIR` writes it as a skill folder) and courses can follow; `--list`, `--search` and a page name read it.
- Callbacks with `data` take no lock (milestone 229; section 8): a `direct` or `realtime` call checks that its scope still runs and holds its captures through a counter of its own, never the runtime's callbacks lock, so a real-time call waits on no other thread; when the scope ends during a call, whichever finishes last lets go of the captures.
- **Breaking:** `Console` input reports its errors (milestone 228; section 6): `read_line()` is `Result<Option<string>, string>`, as `File.read_line` is, a line that isn't UTF-8 being `Err("line N is not valid UTF-8")` and skipped; `read_bytes()` is `Result<u8[], string>`; a failed read is `Err("stdin: …")` from both, never the end of input. `match console.read_line() { .Some(l) => …, .None => … }` becomes `.Ok(.Some(l)) => …, .Ok(.None) => …, .Err(e) => …`.
- `net.only`'s gaps closed (milestone 227; section 6): NAT64 and 6to4 addresses are judged by the IPv4 address inside, and Teredo, local-use NAT64, discard, benchmarking, ORCHID, documentation and site-local ranges are never public; `resolve` gives only what `connect` would use; connecting to one of the host's addresses works when a fresh lookup confirms it, so `tls.Options.address` works through `net.only`.
- Review fixes (milestone 226; sections 6, 8): in a program that never started a task, the first Ctrl-C cancels `main` at its next pause, so its `defer`s run, instead of exiting at once; a callback without `data` runs whenever C calls it, after its scope too, as it captures nothing; registering a `realtime` callback sets a thread record aside, so C's thread's first real-time call allocates nothing.
- Iterators through `impl` (milestone 225; sections 4, 5): a type loops, takes the iterator helpers and converts to `Iter<T>` only through `impl Iter<T> for X { fun next(mut self): Option<T> … }`; a method merely named `next` no longer counts (E0411, whose help names the block); an interface value loops when its interface extends `Iter`. Every built-in behaviour is now met through an `impl`. **Breaking:** an iterator without its block.
- Small rules (milestone 224; sections 2, 5, 6): a plain `for x of xs` may change `xs`, going on over the array it started with, as map and set loops do (`for … of mut` still guards, E0507); strings take `\u{…}` as chars do; `make_dir`, `make_dirs`, `remove_file` and `remove_dir` give `Result<(), string>`; E0465 is for the program's own `debug` calls only, a dependency's printing nothing at `-O2`. **Breaking:** the four `Dir` results.
- Nominal rules (milestone 223; sections 2, 4): an interface value converts only to an interface it extends, never by having its items' names (E0422); a block for a type built from two or more program types, `impl Show for (Point, Line)`, is written only in the interface's module (E0459), so two libraries can't both write it; the names syntax uses (`Display`, `Iter`, `Error`, the operator interfaces, `FromEntries`, `FromItems`, `Ref`, beside `Option`, `Result`, `Eq` and `Ord`) are built-in names (E0302), and every other prelude name may be a module's own. **Breaking:** all three.
- One spelling for a generic block (milestone 222; sections 3, 4): a generic `impl`'s parameters follow `impl`, `impl<T: Num> Grid<T>`, `impl<T: Display> Show for Grid<T>`, for a program's type as for a built-in one; the old `impl Grid<T: Num>` is E0214. An interface's parents are joined with `+`, as a bound's are, `interface Store: Eq + Sendable` (E0215 for `,`); `with` keeps `,`. **Breaking:** both old spellings no longer parse.
- Guarded views (milestone 221; sections 6, 8): `p.view(n, f)` reads and writes C's memory at a length the program vouches for, so in a package other than the program's own it needs the `ffi` permission, as an `extern` does (E0333 at its first `p.view`); passing a `CPtr` along, or taking a view, needs none. An `n` too large for a view's `i64` length panics (`view length too large`) instead of wrapping. **Breaking:** an ungranted package's `p.view` no longer builds.
- Views of C's memory (milestone 220; section 8): `View<T>` reads C's memory and `MutView<T>` writes it, in place, bounds-checked, never outliving the call they were given to (E0483). `direct` and `realtime` callbacks take them for C's buffers, and `p.view(n, f)` makes one from a `CPtr`. **Breaking:** `c_read` and `c_write` are gone (`p.view(n, fun(v: View<u8>) = v.to_array())` and `copy_from`); callbacks no longer take `Ref<T[]>` (`MutView<T>`), and `direct` and `realtime` ones no longer take `T[]` (`View<T>`; `spawned` ones still do), E0484; E0481 is no longer reported.
- Real-time callbacks (milestone 219; section 8): `cb: realtime fun(…)` is a `direct` callback the compiler checks never allocates or waits, in its body or any Lugha function it calls (E0482), naming the calls that lead to what it does; the way out to tasks is a `Ring`.
- Rings (milestone 218; sections 4, 8, 9): `Ring<T>.new(n)` gives `(RingSender<T>, Receiver<T>)`, `n` fixed slots whose writing end only has `try_send` (giving `TrySend<T>`, as a channel's), `close` and `is`, and never locks, allocates or waits on any thread, C's own included; one writer at a time. Its reader is an ordinary `Receiver<T>`.
- Callbacks on threads C made (milestone 217; sections 7, 8, 9): such a thread gets a C stack of its own on its first callback; a `direct` callback runs there and can't wait (a panic, reported by its scope), a `spawned` one becomes a task on the workers; no deadlock is declared while C may still call. **Breaking:** a callback with `data`, `direct` too, must capture only `Sendable` values (E0475): it may run on another thread.
- Every C callback says how it runs (milestone 216; sections 8, 9): `cb: direct fun(…)` runs during C's call, `cb: spawned fun(W, data)` starts a task in the registering scope; one with neither is E0435, and `spawned` needs `data` and no result. **Breaking:** milestone 215's rule (a callback with `data` and no result queued unless marked `inline` after its type) is gone; such callbacks now write `spawned`, and every other callback `direct`.
- Callbacks registered against a scope (milestone 215; sections 7, 8, 9): a C callback with user data (`data`) and no result runs as a task in the scope its extern call ran in, so it may wait; `cb: fun(W, data) inline` runs it during C's call, as callbacks with a result or without `data` always run. A panic in an inline callback gives C a zero value and fails that scope, rather than ending the program; cancellation never interrupts one. Once the scope's body ends its callbacks stop: C's later calls are dropped, and the closures' captures are let go of. A queued callback's captures must be `Sendable` (E0475), and it can't take `Ref<T[]>` (E0481). **Breaking:** a callback with `data` and no result that must run during C's call needs `inline`.
- Pinned handles and pinned scopes (milestone 214; sections 4, 6, 7, 8, 9): `extern type Window pinned;` declares a handle whose values never leave the thread that made them: never `Sendable`, and a task that touches one stays on its worker from then on; `scope ui pinned { … }` keeps its body, its tasks and the scopes inside them on one worker, the main thread for `main`. A `blocking` extern can't take or give one (E0480). Thread-bound C libraries (GTK, OpenGL) no longer need `LUGHA_WORKERS=1`.
- Spawning that scales (milestone 213; sections 6, 7): a finished task's stack waits in its worker's cache for the next task there, its pages kept until the worker idles, and a scope counts its tasks in and out under its own lock, so tasks spawning in different scopes don't wait on each other.
- Locks of their own for the hot paths (milestone 212; sections 6, 9): each channel, actor mailbox and task's `join` has its own lock, so tasks passing messages on different channels scale with workers; a waiting task is claimed before a value moves, and a late timeout or socket event never wakes a later wait.
- A scheduler for many cores (milestone 211; sections 6, 9): each worker has its own queue of ready tasks, stolen from by idle workers, and a task woken by a message runs next on its waker's worker; idle workers spin briefly, then park; timers are kept per worker; scheduling itself takes no lock, so tasks on different workers switch independently.
- Tasks on every core (milestone 210; sections 6, 8, 9): tasks run on one worker thread per CPU (`LUGHA_WORKERS=n`; one in deterministic and shuffled runs), started at the first spawn; the order in which different tasks act is no longer fixed, though a `println` prints its line whole; a C library needing one thread needs `LUGHA_WORKERS=1` until `pinned`. The stack check and the step budget come from the stack pointer, every stack aligned to its size.
- An allocator for many threads (milestone 209; section 7): small blocks belong to the worker thread that carved their chunk, and one freed on another thread goes back to its owner with one atomic push; the runtime's per-thread state lives in each thread's own storage.
- Groundwork for many cores (milestone 208; sections 7, 9): every value reaching another task is marked shared by the side handing it over, before the other can see it (a task's result by the task, an actor's answer by the actor, a failure's error and message, task names, cancellation reasons, a scope's handler, what an actor stored in its state, a handle's buffered bytes); constants become immortal once filled; `LUGHA_RC_CHECK=1` builds and runs a program with an owner check that ends it when an unmarked object reaches a second task. A task printing a `Ref` keeps its own record of the boxes being printed, so a print preempted partway no longer confuses another task's.
- Helper threads (milestone 206; sections 6, 7, 9): the runtime's blocking work (a `Dir`'s and a `File`'s operations, reading input, name lookups, starting and waiting for programs) runs on a pool of helper threads while its task waits, so the other tasks run; `LUGHA_HELPERS` sets the pool's size.
- `extern blocking fun` (milestone 207; sections 6, 8, 9): a C function declared `blocking` runs on a helper thread while its task waits, so a slow C call stops only its own task; it can't take a callback (E0479), and every declaration of a C function agrees on `blocking` (E0306).
- Hash seeding (milestone 204; sections 4, 9): maps and sets hash with SipHash-1-3 under a secret key per process instead of FNV-1a, so keys chosen to collide can't make a map slow; results don't change, only hash values, which differ between runs (fixed in deterministic tests).
- Deterministic tests (milestone 203; sections 6, 9): a plain test's time is virtual, jumping to the next timer once every task waits, so sleeps, deadlines and timeouts cost nothing; `lughac test --shuffle=SEED` picks ready tasks at random from a seed, so a test relying on one order fails, the same way for the same seed (`FAILED (seed N)`). Integration tests keep real time.
- Generic actors and actors as interface values (milestone 202; sections 4, 8): `actor Cache<K: Key + Sendable, V: Sendable> { … }`, each instance its own handle type (`Cache<string, i64>`) with its own messages; `impl I for SomeActor` makes its handle implement `I`, the block's methods (and `I`'s defaults) running inside the actor as messages, so a handle passes as an `I` and meets `T: I` bounds.
- Actors in scopes (milestone 201; sections 3, 4, 6, 9): `s.spawn(Counter { … })` starts an actor in a scope, which stops it once its body and other tasks are done and cancels it with them; an actor's panic fails its scope as a task's does; `join` on an actor is gone. A mailbox holds 1,024 messages, or `actor Logger mailbox 10_000 { … }`'s; a call or cast to a full one waits for room.
- Scope policies (milestone 198; sections 3, 4, 6): settings between a scope's name and its body, each at most once: `independent` (a task's failure is printed as it happens, naming the task, and the scope runs on), `independent(f)` (each failure goes to `f`, in the failing task), `limit n` (`s.spawn` waits while `n` tasks run); `scope<E> s` fails with `Failure<E>`, every task's error converting to `E` (E0478). A `Failure` is a struct, `{ task, kind }`: the failing task's name (what `t.name(…)` set, else the called function's) and `.Error(E)` or `.Panic(string)`. A scoped task's failure is reported where it happens, a root task's error in full.
- Channels with two ends (milestone 200; sections 4, 5, 6, 9): `Chan<T>.new(n)` gives `(Sender<T>, Receiver<T>)`, the capacity required (`0` hands each value over directly), and `Chan<T>.unbounded()` has no limit; `Chan` is no longer a type (E0305). Only a `Sender` sends and closes, only a `Receiver` receives (E0415 on the other end). `for x of rx` runs until the channel is closed and empty; `tx.try_send(v)` gives `.Sent`, `.Full(v)` or `.Closed(v)`, and `rx.try_recv()` `.Ready(v)`, `.Empty` or `.Closed` (the prelude's `TrySend` and `TryRecv`); `s.merge(receivers)` makes one receiver of several. A deadlock report names each waiting task and where its channel was made.
- Time and shutdown (milestone 199; sections 3, 4, 5, 6, 9): a scope's deadline, `within ms` or `until t` (monotonic milliseconds, as `task.sleep` counts; one of the two, E0477), never later than its enclosing scope's: once it passes, the body and the scope's tasks are cancelled with `.Deadline`, and the scope fails with `f.kind` `.Deadline` unless something failed first; `s.remaining(): Option<i64>`. `shield within ms { … }` holds cancellation off for its block, at most `ms` (`shield` without `within` is E0211); one entered while its task unwinds (in a `defer`) is cut off at its deadline, and the code after it runs. The first Ctrl-C cancels the root scope's tasks and `main` with `.Shutdown`, their `defer`s run, and the program exits with 130; a second, or one in a program that never started a task, exits at once.
- The sendable rule (milestone 197; sections 4, 6, 9): a value crossing to another task — a spawned call's arguments, captures and result, a channel's elements, an actor's fields, messages and replies — must be `Sendable`, a prelude `auto interface` worked out from a type's parts by its derive blocks: numbers, text, arrays, structs, enums and tuples of sendable parts, channels of them, actors, C handles, interface values whose interface requires `Sendable`, functions whose captures are, and `T: Sendable`. A `Ref` never is, nor a plain `fun(…)` value (E0475). The capabilities, what they open, and `Error` require `Sendable`, so their implementations must be. `impl Sendable`, `with Sendable` and `auto` outside the prelude are E0476. The library's handles keep their shared state in runtime cells instead of `Ref`s.
- Function types are interfaces (milestone 196; sections 3, 4, 8): every function value has a type of its own, a struct of its captures, implementing its signature's interface `fun(A): R`. `fun(A): R` may be a bound, `<F: fun(i64): i64>`, which keeps the exact function (one instance per function passed, called directly); something an interface requires, `interface Pricing: fun(i64): i64 {}`, whose values are callable; and implemented by a struct, `impl fun(i64): i64 for Scale { fun call(self, x: i64): i64 = … }`, making it callable and convertible to the function type.
- Scopes and cancellation (milestone 195; sections 3, 4, 6, 9): `scope s { … }` is an expression of type `Result<T, Failure>`, and every task starts in a scope, `s.spawn(work(x))`, or in the root scope `main` and tests may take (`root: Scope`). A scope waits for its tasks; the first failure (a task's panic or `Err`, or a panic in the body) cancels the rest and is the scope's value. Cancelling is invisible: a cancelled task unwinds at its next pause or preemption point, running its `defer`s. A task handle's `join()` gives `Result<V, Stopped>`; `cancel(why)` and `name(text)`. Scope and task handles are second-class (E0472); `spawn` alone starts only an actor (E0474).
- Reference counting manages every program's memory (milestone 193; section 7), built behind a flag over milestones 190–192 (across tasks, channels and actors with values marked shared as they cross, then measured against the collector). The Boehm collector, `--memory` and `LUGHA_MEMORY` are gone. `Ref` cycles aren't freed.

**Breaking:** an actor starts in a scope (milestone 201): `let c = spawn Counter { count: 0 };` is `let c = root.spawn(Counter { count: 0 });` with `root: Scope`, or `s.spawn(…)` in `scope s { … }`; `c.join()` is gone, and the scope's `Result` says whether the actor panicked.

**Breaking:** a channel has two ends (milestone 200): `let c: Chan<i64> = Chan.buffered(4);` is `let (tx, rx) = Chan<i64>.new(4);`, and `Chan.new()` is `Chan<i64>.new(0)`; a parameter `c: Chan<T>` is `tx: Sender<T>` where it sends or closes, `rx: Receiver<T>` where it receives. `try_recv` gives a `TryRecv<T>`: `.is_none()` is `if let .Empty = rx.try_recv()`.

**Breaking:** a scope's failure is a struct (milestone 198): `.Err(.Panic(m))` is `.Err(f) => match f.kind { .Panic(m) => … }`, and `f.task` names the task that failed; a `Failure` prints as ``task `fetch` failed: …``. A scoped task's panic no longer prints `panic: …` as it happens: its scope reports it, and a root task's prints as ``task `name` panicked: …``. `Scope` and `Failure` written alone mean `Scope<Error>` and `Failure<Error>`.

**Breaking:** what crosses to another task must be sendable (milestone 197): a `Ref` can't reach a spawned call, a channel or an actor (E0475); share state through an actor, or send copies through a channel. A type implementing a capability (`Console`, `Dir`, `Conn`, …) or `Error` must be sendable: a fake that records in a `Ref` keeps that in an actor. `std.http`'s servers take a `Handler` (`interface Handler: fun(Request): Response + Sendable {}`): a named function, or a closure capturing only sendable values; a parameter of type `fun(Request): Response` passed on is refused, since it may have captured anything. Generic code sending a `T` needs `T: Sendable`.

**Breaking:** a task starts in a scope (milestone 195): `let t = spawn work(x); t.join()` is `let t = s.spawn(work(x)); t.join()` inside `scope s { … }`, or `root.spawn(work(x))` with `root: Scope` taken by `main`; `join()` gives a `Result`, so `t.join()` is `t.join().unwrap()` or `t.join()?`; `main` returning waits for the root scope's tasks; a task's panic fails its scope instead of ending the program; a `Task` can't be kept in an array, a field or a closure (E0472).

**Breaking:** printing, input, files and the arguments need capabilities (milestone 180): `fun main() { println(x); }` is `fun main(console: Console) { console.println(x); }`; a function that prints takes the `Console` as a parameter; `read_file(p)` is `fs.cwd().read_file(p)` with `fs: Fs` taken by `main`; `args()` is an `args: string[]` parameter of `main`; `exit(n)` is returning `n` from `main`. The clock, the environment, other programs and the system's randomness need capabilities too (milestone 181): `time.now_ms()`, `time.monotonic_ms()` and `time.local_offset(ms)` are `clock.now_ms()`, `clock.monotonic_ms()` and `clock.local_offset(ms)` (its error a `string`); `env.get(n)` is the same call on an `env: Env`; `env.current_dir()` is `fs.cwd().path()`; `env.set_current_dir` is gone (the working folder never changes); `process.run`, `run_with` and `call` are `proc.run`, `proc.run_with` and `proc.call`, and `process.Output` is `ProcessOutput`; `random.from_os()` is `random.from_entropy(entropy)`; `crypto.random_bytes(n)` is `entropy.bytes(n)`; `crypto.ed25519_keypair()` and `crypto.x25519_keypair()` take the `Entropy`. Files and folders go through a `Dir` (milestone 182): `fs.make_dirs(p)` and the rest of `std.fs` are `dir.make_dirs(p)` on a `dir` from `fs.cwd()`, `fs.list_dir(p)` is `dir.list(p)`, `fs.File` is `File`; `read_bytes(p)`, `write_bytes(p, b)` and `append_bytes(p, b)` are `dir.read_bytes(p)` and so on; `read_stdin_bytes()` is `console.read_bytes()` and `flush()` is `console.flush()`. The network goes through a `Net` (milestone 183): `import std.net;` is gone and `net.listen(a)`, `net.connect(a)`, `net.resolve(n)` and the rest are the same calls on a `net: Net` that `main` takes; `net.Conn`, `net.Listener`, `net.Udp` and `net.Interface` are `Conn`, `Listener`, `Udp` and `NetInterface`; `http.get(url)` is `http.get(net, url)`, and every `std.http` and `std.tls` function that connects or listens takes the `Net` first. TLS certificates are PEM text (milestone 184): `tls.listen(net, addr, "cert.pem", "key.pem")` is `tls.listen(net, addr, dir.read_file("cert.pem")?, dir.read_file("key.pem")?)`, likewise `http.serve_tls`, and `Options.ca_file` is `Options.ca_pem` holding the certificates' text; `tls.Conn` and `tls.Listener` are `tls.SecureConn` and `tls.SecureListener`, and a server wanting `peer_key()` accepts with `accept_secure()`. Tests get stand-ins (milestone 185): a test taking `Fs` takes a `dir: Dir` (a fresh folder) or becomes `@test(integration)`, run by `lughac test --integration`; a plain test's `Clock`, `Env`, `Entropy` and `Net` are fixed, empty, seeded and loopback-only. Every `Dir` is confined (milestone 186): `fs.cwd().read_file("/etc/hosts")` or `"../x"` is `Err("path leads outside the folder")`; write `fs.dir("/etc")?.read_file("hosts")` or `fs.dir("..")?`; a `Dir` fake implements `dir(path)` and `read_only()`. A package declaring C code needs the program's grant (milestone 188): `sqlite = { …, allow = ["ffi"] }` in the program's `lugha.toml`, which covers its dependencies too. C code is locked down with the program (milestone 189): without `Net` it can't open a socket, without `Proc` it can't start a program, without `Fs` it can't open files beyond what the held capabilities read, and programs started through `Proc` share those limits. `#{…}` is gone (milestone 178): `#{"a": 1}` is `Map { "a": 1 }`, `#{1, 2}` is `Set { 1, 2 }`, and `#{}` is `Map {}` or `Set {}`; maps and sets print that way; a struct implementing `FromEntries` or `FromItems` is built with braces only in its own module. A numeric `as` that can lose information is E0469 (milestone 177): `n as u8` for a wider `n` is `n.truncate<u8>()` for the old bits, a float `as` an integer is `x.saturate<i64>()` for the old result, `n as f64` for an `i64` is `n.round<f64>()`, `n as char` is `n.try_into<char>()`; `f(a < b, c > (d))` now passes type arguments. Every variant in a pattern takes a leading dot (milestone 176): `Some(x) =>` is `.Some(x) =>`, `None =>` is `.None =>`, `if let Some(x) = o` is `if let .Some(x) = o`; an undotted one is E0212 or E0468, and `mut None` is E0468, not E0428. A `Result` used as a statement is E0467 (milestone 175): write `let _ = f();` to drop one on purpose. `assert_eq` and `assert_ne` are ordinary generic calls (milestone 173): an argument that can't be typed alone, first (`assert_eq(None, o)`), is E0412; write it second. The prelude's helpers are only methods (milestone 172): `map(xs, f)`, `str_trim(s)` and the like are no longer functions a program can call (E0301); write `xs.map(f)`, `s.trim()`. Operators go through blocks (milestone 171): an own `fun add(self, o: V): V` becomes `impl Add for V { … }`, several `mul`s on one type become one `impl Mul<R> for V` each, and `impl f64 { fun mul(self, v: V): V }` becomes `impl Mul<V> for f64` (that form is now E0466; E0458 is retired); an operator on an interface value is E0404, and a bound gives an operator only when it is an operator interface (`T: Mul<f64>`), not an interface with its own `mul`. Printing needs `Display` (milestone 170): `println(xs)`, `"{opt}"` and `to_string(p)` on an array, `Option`, tuple, map, `()` or a struct or enum without `impl Display` are E0422; write `.debug()` for the old text, or `impl Display`. A type's own `show` method becomes `impl Display for T`, and is no longer used inside an array or another value's text. `unwrap()` on an `Err` shows the error's debug form (`"boom"` quoted). `debug` is a reserved name. A struct or enum ordered with `<`, `sort`, `min` or `max` needs `with Ord` or `impl Ord` (milestone 168); an own `cmp` method no longer orders it, and `Ord` now implies `Eq`. Floats aren't `Ord`: `[1.5].sort()`, `math.min(1.5, 2.0)` and tuples or structs holding a float that are ordered are E0451; sort floats with `xs.sort_by(f64.total_order)`. A struct or enum compared with `==` or used as a map key or set value needs `with Eq` or `with Key` (milestone 167); own `eq` and `hash` methods become `impl Eq` and `impl Key`. A struct or enum encoded, decoded or otherwise used through a derivable interface needs `with I` (milestone 165); a type's own `to_json` or `show` no longer replaces a derived item. A type that met an interface by its methods needs `impl I for T`; methods that implement an interface move from `impl T { … }` into it. A number, string or array converted to a program's interface (`let x: Any = 1;`) is E0422 unless a block implements it for that type (milestone 169).

**Changes in v17 since v16** (milestones 91–163):
- Green threads: `spawn f(x)` runs a call concurrently and gives a `Task<T>` (a plain `Task` for a call that returns nothing) whose `join()` waits for the result. `Chan<T>` carries values between green threads: `Chan.new()` (unbuffered) or `Chan.buffered(n)`, `send`, `recv` (an `Option`, `None` once closed), `try_recv`, `close`, `is`. `select` waits on several channels at once, with an `after(ms)` timeout or a `default` that never waits. `std.task` has `sleep(ms)` and `yield_now()`. Every function stays an ordinary function: there is no `async` and no `await`, so nothing is coloured. All green threads share the program's one OS thread; a panic in any of them ends the program, as does `main` returning, and a deadlock is a panic (sections 2–9).
- Actors: `actor Counter { count: i64, fun add(mut self, n: i64) { … } fun get(self): i64 = … }` declares state and methods together; `spawn Counter { count: 0 }` starts one and gives a handle of type `Counter`. A method call on the handle is a message, handled one at a time: one with a result waits for it, one without is queued. `stop()`, `join(): Option<string>` and `is` are every handle's; a panic ends only its actor (`join` gives the message, callers waiting panic in turn); fields are private, and nothing that may hold a `Ref` crosses an actor's boundary (E0439) (milestone 92; sections 2–9).
- Struct update syntax: `Model { count: m.count + 1, ..m }` takes every field not written from `m` (milestone 93; sections 3, 4, 5).
- Variants as functions: `Msg.Named` is a `fun(string): Msg` when not called, and so are `Some`, `Ok` and `Err` — `input(name, Msg.Named)`, `xs.map(Some)` (milestone 94; sections 4, 9).
- Faster `lughac test`: a package folder's test files are built into one executable instead of one each (milestone 124; section 9).
- Shared names: a method may be named like a field (`p.name` vs `p.name()`) or, with `self`, like a variant, where uses tell them apart; E0310 remains for a function-holding field, an associated function named like a variant, and two methods (milestone 123; section 4).
- Files built into the program (milestone 128; sections 5, 6, 7, 9): `include_bytes("data/logo.png")` is a `u8[]` and `include_str("notes.txt")` a `string` holding the file's contents, read when the program is compiled; constant expressions, so `const LOGO: u8[] = include_bytes("logo.png");` works. The file must be inside the package's folder (E0447, 16 MiB at most); `include_str` needs UTF-8 (E0448).
- Joining arrays: `xs + ys` is a new array of `xs`'s elements then `ys`'s, `xs += ys` and `s += t` join in place of `a = a + b`, and arrays satisfy `Add` (milestone 122; section 4).
- `==` through `Eq` (milestone 144; sections 4, 5, 9): `Eq` is the prelude's derivable interface with `eq` and `hash`; a struct or enum writing both decides its own equality (E0454 for one alone); a type that isn't `Eq` is E0451 with a path at `==` and at `T: Eq`; derive blocks gain `derive.all(a, b, f)`. **Breaking:** a type's existing `eq`/`hash` methods now take part in `==`.
- Hashing is written in Lugha (milestone 143; sections 4, 5): the prelude's derivable `$Hash`, which `Eq` brings with it; derive blocks gain `derive.fold`, `derive.index` and `derive … for handle` with `derive.address`. Fixed: equal sets or maps built in different orders now hash equal, so a `Set<Set<T>>` holds them once.
- Derive blocks (milestone 142; sections 2–6, 9): `derive Show for struct { … }` says, once, how every struct (or enum, tuple, or one type such as `i64` or `T[]`) meets an interface declared in the same module; `derive.fields(…)` and its siblings reach the fields. A type's own method replaces a block's item, so `impl Date { fun to_json(self): json.Value … }` customises its JSON. **Breaking:** a struct or enum with its own `to_json`/`from_json` now has it used by `json.encode`/`decode`, and one of another shape is E0422 where it is encoded. E0451–E0453.
- `json.encode` and `json.decode` are written in Lugha, in derive blocks of `std.$json_value` (milestone 141; section 9): nothing a program can see changes.
- `json.decode(v)` (milestone 140; standard library, section 5): a `json.Value` back into any data type, `DecodeError` naming the path to the first mismatch; `let p: Person = json.decode(v)?;` — `?` now passes the type it is expected to give to its operand, so a generic call's type comes from it.
- `json.encode(x)` (milestone 139; standard library, sections 8, 9): any data type to a `json.Value` with no annotation, the built-in `json.Json` bound, E0450 for what isn't data.
- JSON in `std.http` (milestone 138): `req.json()`, `res.json()`, `post_json(url, v)`, and `http.json(status, v)` taking a `json.Value` instead of text.
- `std.json` (milestone 137; standard library): strict RFC 8259 JSON parsing and writing in Lugha — exact `i64` integers, keys in order, duplicate keys refused, limits for hostile input.
- `net.resolve` and `tls.Options.address` (milestone 121): every address a name resolves to, and a connection to a chosen IP address that still verifies the name (in `std.tls` and `std.http`), against DNS rebinding.
- UDP in `std.net` (milestone 163): `udp_bind`, `udp_bind_shared`, `send_to`, `recv_from`, multicast groups and options, and `interfaces()`.
- HPKE in `std.crypto` (milestone 162): `hpke_seal(public, info, plaintext, aad)` and `hpke_open(secret, info, sealed, aad)`, sealing to an X25519 public key (RFC 9180, base mode, one suite).
- TLS by key (milestone 161; standard library, E0332): `tls.listen_keys`, `tls.connect_keys`, `Conn.peer_key()` and `Listener.set_accept` — mutual TLS 1.3 where each side proves an Ed25519 key from the other's list (raw public keys, RFC 7250), with no certificates. Programs loading `std.tls`, `std.http` or `std.crypto` need OpenSSL 3.2 or newer.
- Sealing and key exchange in `std.crypto` (milestone 160): ChaCha20-Poly1305 (RFC 8439) and X25519 (RFC 7748); `CryptoError` gains `NotAuthentic` and `BadKey`.
- `crypto.hkdf_sha256`: HKDF-SHA256 key derivation, RFC 5869 (milestone 120).
- `http.request_tls_with`: `Limits` and `tls.Options` in one request (milestone 119).
- `and_then` on `Option` and `Result` (milestone 118); a closure whose expected result is only partly known (`Option<U>`) takes its result from its body.
- Octal literals (milestone 117; section 2): `0o600`; a leading zero (`0600`) stays decimal and warns W0107.
- `fs.sync` (milestone 116): `fsync` on a file or folder, so a saved file and its rename survive a power cut.
- Calendar dates and the local offset in `std.time` (milestone 145): `time.civil(ms, offset_s)`, `time.from_civil(c, offset_s)` and `time.local_offset(ms)`, with `Civil` and `TimeError` (standard library).
- File streams in `std.fs` (milestone 146): `fs.open`, `fs.create` and `fs.append` give a `File` to read by line or in chunks and write piece by piece (standard library).
- Constants computed by pure code (milestone 147; sections 6, 8): a `const`'s value may be any expression, calls, `if`, `match`, blocks, closures and interpolation included, as long as nothing it reaches prints, touches files, the operating system or C, or starts tasks, channels or actors (E0445, with the path); constants read inside the functions it calls are computed first (E0446 for a cycle through them). Still computed once before `main`, never while compiling.
- `defer` (milestone 151; sections 2, 3, 5, 6, 9): `defer f.close();`, `defer n += 1;` or `defer { … }` runs when its block is left — its end, `return`, `?`, `break`, `continue` — last first, after a tail or `return` value is computed, reading values as they are then; a panic didn't run it until milestone 194, which unwinds. Leaving a deferred statement is E0455; one with a value is E0456. **Breaking:** `defer` is a keyword.
- `loop` and labels (milestone 152; sections 2, 3, 5, 6, 9): `loop { … }` repeats until a `break`, and is an expression: `let x = loop { … break v; };`. A `loop` no `break` leaves has type `never`, so a function whose only exit is a `return` inside it needs no `panic("unreachable")`. `name:` labels a `loop`, `while` or `for` for `break name;` and `continue name;`. E0512 (unknown label), E0513 (a label reused inside its loop), E0514 (`break` with a value out of `while` or `for`). **Breaking:** `loop` is a keyword.
- `fs.size` (milestone 153; standard library): a regular file's size in bytes from `stat(2)`, links followed, without opening it, so a program can refuse a file too big to read whole.
- Semantic tokens (milestone 154; section 9): `lughac lsp` classifies every token of a file — keywords, strings, numbers, comments, operators, and names by what the check resolved them to — so editors without Lugha's TextMate grammar (Neovim, Helix, Zed) colour it, and VS Code colours names by meaning.
- Indexing your own types (milestone 155; sections 3, 4, 8): `a[i]` on a struct, enum, interface value or bounded `T` calls `fun index(self, i: I): T`, and `a[i] = v` calls `fun set_index(mut self, i: I, v: T)`; `a[x, y]` is `a[(x, y)]` for every type, so maps with tuple keys take `m[x, y]` too.
- Arithmetic between different types (milestones 158–159; sections 4, 8): a type may have one `add`, `sub`, `mul`, `div` and `rem` per right-hand type, chosen by the operand's exact type (`v * w` and `v * 2.0`), and `impl f64 { fun mul(self, v: V): V }` beside `V` makes `2.0 * v`; E0457, E0458.
- `for x of mut g[i]`, `swap(g[i], b)` and `out g[i]` through a type's `index` (milestone 157; sections 4, 8): each works on a copy of the element, handed back to `set_index`; the loop hands it back on every way out but a panic.
- Read-modify-write through `[ ]` (milestone 156; sections 4, 5, 8): a map's entry is a place, changed where it is (`m[k] += 1`, `m[k].x = v`, `m[k].push(x)`, `swap(m[a], m[b])`, `for x of mut m[k]`), and a missing key panics as a read does; `g[i] += v`, `g[i].x = v`, `g[i][j] = v` and `g[i].push(x)` on a type read the element with `index`, change the copy and hand it back with `set_index`.
- An iterator chain's expected type reaches `map`'s closure through `collect` and the helpers between (milestone 150; section 5): `let n: Option<i64>[] = xs.iter().map(fun(x) = None).collect();`.
- Handing over arguments (milestone 149; sections 4, 9): `x = f(x)`, where `f` keeps its parameter, no longer copies `x`'s arrays on every call; a function that stores a parameter on its last use takes it without a copy from a caller that is done with it (quadratic loops become linear). Programs print the same.
- A generic call only its result can type takes its type through a method whose result mentions the receiver's type parameters (milestone 148; sections 4, 5): `let p: Person = json.decode(v).unwrap();`, `.expect(…)`, and a program's own methods (`let n: u8 = zeros().first();`).
- Keeping data on disk (milestone 115): `fs.rename` (atomic within a file system), `fs.set_mode` (`chmod`), the `flush()` built-in, and `std.time` (`now_ms`, `monotonic_ms`).
- `std.toml` (milestone 114; standard library): a read-only TOML 1.0 parser in Lugha with limits for hostile input; the prelude's `xs.join(sep)` is linear (it was quadratic).
- `std.crypto` (milestone 113; standard library): SHA-256 (one-shot and streamed), Ed25519 keys, signing and verifying, secure random bytes and constant-time comparison, over the system's OpenSSL 3.
- TLS (milestone 112; standard library, E0332): `std.tls` over the system's OpenSSL 3 with always-on verification, and `https://` in `std.http` (`serve_tls`, `request_tls`).
- `std.http` (milestone 111; standard library): an HTTP/1.1 server with strict limits and a client, in Lugha over `std.net`; a literal `if` branch takes a typed sibling's type (`if c { b - 1 } else { 255 }` with `b: u8`).
- `std.net` (milestone 110; sections 7, standard library): TCP listeners and connections whose waits pause only their green thread, through `epoll`; timeouts; UDP since milestone 163.
- Global constants (milestone 109; sections 2, 3, 6, 8): `[private | public] const NAME: T = value;` — constant expressions only (E0445; any pure expression since milestone 147), in any order without cycles (E0446), evaluated once before `main`; no mutable statics. `const` is a keyword.
- A float's raw bits (milestone 108; section 4): `x.to_bits()` (`f64` → `u64`, `f32` → `u32`) and `f64.from_bits(b)` / `f32.from_bits(b)`, exactly the IEEE 754 bits.
- The unit type `()` replaces `void` (milestone 107; sections 3, 4, 6): a type and its one value, held anywhere a value is (`Result<(), E>`, `Chan<()>`); E0407 is retired; W0106 warns on `let x = log();`.
- Extern structs hold `Option` of a handle (C's nullable pointer); unions hold plain data only, so no pointer is made up from a number (milestone 106; section 8).
- The C boundary takes pointers (milestones 103–105; section 8): `ptr` and `mut` parameters; arrays as `xs: T[]` and `mut xs: T[]`; `extern union`; `CPtr` with `c_read`/`c_write`; callbacks taking `T[]` and `Ref<T[]>`.
- `out` parameters on externs: `extern fun get_fuel(c: Context, out fuel: u64): bool;` and `get_fuel(ctx, out fuel)` — C writes through the pointer and the place gets the value after the call (milestone 102; sections 3, 8).
- Maps and sets: `Map<K, V>` and `Set<T>`, built in, with `#{"ann": 31}` and `#{1, 2}` literals (`Map { … }` and `Set { … }` since v18); values like arrays, keys any `Eq` type without floats, insertion order, `m[k]`, `for (k, v) of m`, `==` regardless of order. The iterator type `it.map(f)` returns is renamed `Mapped` (milestone 101; sections 2–5, 9, 10).
- Glob re-exports: `public import raw { * };` makes every `public` item of `raw` an item of this module, so a package's entry can flatten a sub-module (milestone 100; sections 3, 6).
- Package entry modules: `import kivuli;` imports the package's `lib.la` (in its `source` folder) as the module `kivuli`; `lughac init --lib` writes one (milestone 99; sections 6, 9).
- Module doc comments: a `/** … */` at the top of a file, followed by a blank line or an `import`, documents its module; hovering the module's name shows it, and the std modules are documented (milestone 98; sections 2, 9).
- `pkg-config --cflags` may give `-mfpmath=sse`, `-msse` and `-msse2`, so a package can list C files alongside GTK (section 8).
- `lughac fmt` hugs a call's trailing array or struct literal (`column([` … `])`) and keeps a function's header whole while its body breaks (milestone 95; section 9).
- A generic call only its result can type takes its type from its siblings: `assert_eq(text("hi"), expected)`, `text("hi") == expected` (milestone 96; section 4).
- Packages and tests: `lughac test` and `lughac check` on a file in a package's `source` folder build it as that package's module (`glow.diff`), so modules importing each other can be tested, and its tests are named `glow.diff.name`; programs in a package's `test/`, `tests/` and `examples/` see its package-level items; `lughac check` takes a file without `main` as a library; `lugha.lock` sits beside the governing `lugha.toml`; and `skip(reason)` reports a test skipped (milestone 97; sections 6, 9).
- `lughac init` writes a `test/` folder with one passing test, and a program's greeting in its own module, `src/greeting.la` (milestone 129; section 9).
- Executables run on any x86-64 CPU: lughac builds for the baseline `x86-64` level, and `--cpu=x86-64-v2|x86-64-v3|x86-64-v4|native` on `build`, `run` and `test` chooses a newer level or this machine's own CPU (milestone 130; section 9).
- The collector is built into lughac: bdwgc 8.2.12 is compiled once from embedded source and linked into every program statically, so neither lughac nor its programs need the system's `libgc` (milestone 131; sections 7, 9). Removed in milestone 193.
- `$LUGHA_CC` chooses the C compiler: `LUGHA_CC=clang` or `LUGHA_CC="zig cc"` (a program and its first arguments, split on spaces, never run by a shell) compiles the runtime and `[link]` C files and links; unset or blank is `cc` (milestone 132; section 9).
- Linux on ARM64: `lughac build --target aarch64-unknown-linux-gnu` writes an ARM64 executable (green threads and actors included), compiled and linked with `aarch64-linux-gnu-gcc` unless `$LUGHA_CC` names another; extern structs cross to C by value there as AAPCS64 passes them (milestones 133–134; sections 8, 9).
- Output lughac couldn't write is exit 2 (section 9): `lughac build --emit=ir > out.ll` on a full disk no longer exits 0. A reader that stops early is not a failure.

Every valid v16 program is a valid v17 one, unless it uses `spawn`, `select`, `actor` or `const` as a name (keywords now), or declares a function named `skip` (an intrinsic since milestone 97, E0302), `flush` (one since milestone 115) or `include_bytes`/`include_str` (since milestone 128), or names the iterator type `Map` (now `Mapped`, milestone 101; a program's own `Map` is fine). A module's own `Task` or `Chan` still means its own, as for `Ref`. Within v17, a `tls.Options { … }` literal naming every field without `..base` needs `address` since milestone 121 (`..tls.options()` keeps it short).

**Changes in v16 since v15** (milestones 67–90):
- Shared boxes: `Ref<T>` is the one value that copies share. `Ref.new(v)`, `r.get()` (a copy), `r.set(v)`, `r.update(f)` and `r.is(s)` are its whole API. A closure may update a box it captured, and linked data can point at itself. Boxes aren't `Eq`, and print as `Ref(…)` when they cycle (sections 4, 7, 9).
- `env.set_current_dir(path)` changes the program's working directory; relative paths and programs started afterwards follow it (milestone 68; "The standard library").
- `lughac init [folder] [--lib] [--name N]` starts a program, or a library with an example, in a folder; it never overwrites a file (milestone 69; section 9).
- `lughac lsp` is a language server: editors get `lughac check`'s diagnostics as you type, for unsaved text too (milestone 70; section 9), and hovering shows the type of an expression or a declared name, also while the program has errors (milestone 71); Go to Definition jumps from a name to its declaration (milestone 72); completion offers members after `.` and the names in scope (milestone 73); signature help shows the call being typed with its current parameter (milestone 74); and all of these keep working in a file with a syntax error, which the server checks partially (milestone 75).
- Block comments: `/* … */`, over any number of lines, nesting, wherever whitespace may be; an unclosed one is E0113 (milestone 76; section 2).
- Doc comments: a `/** … */` right before a declaration documents it, in Markdown with `@param` and `@returns`; the compiler ignores it, the language server shows it, and std and the prelude are documented (milestone 77; sections 2, 9).
- Snippets in the editor: completion inserts calls with a placeholder per parameter, and `value.match` writes out every variant (milestone 78; section 9).
- Quick fixes in the editor: a non-exhaustive `match` (E0417) offers to add every missing arm, or a `_` arm (milestone 79; section 9).
- `lughac fmt [paths…] [--check]` rewrites `.la` files in one canonical style, keeping every comment, and checks its own output before writing (milestone 80; section 9).
- Formatting in the editor: the language server answers Format Document with the same formatter, for unsaved text too (milestone 81; section 9).
- More quick fixes: `let mut` for an assignment to an immutable `let` (E0501), the closest variant for a misspelled one (W0103), a std import for `math.`-style names (E0301) (milestone 82; section 9).
- Find References and Rename in the editor, across the checked files; a rename is checked before it is offered (milestone 83; section 9).
- `lughac test`: `@test` functions (`@` starts an attribute), `assert`, `assert_eq` and `assert_ne`, each test run in its own process, in parallel, with a time limit (milestone 84; sections 2, 3, 5, 9).
- Tests in the editor: `lughac test --list`, `--json` and `--exact` for tools, and VS Code's Testing view with a ▶ beside each test, never in untrusted folders (milestone 85; section 9).
- `lughac explain CODE` explains every error and warning code, diagnostics end with a line pointing to it, and the editor links each code to its explanation (milestone 86; section 9).
- Linking C libraries: a `[link]` table in `lugha.toml` names libraries, library folders, header folders, C files to compile and `pkg-config` packages; the tables of every package a program uses apply (milestone 87; sections 6, 8, 9).
- Callbacks and handles: `extern type Name;` declares an opaque C pointer type; a function type in an extern's parameters is a callback C can call, named functions anywhere and closures through user data (`data`, `release`); `extern "symbol" fun name(…)` gives a C function a Lugha name of its own (milestone 88; sections 3, 4, 7, 8, 9).
- Strings from C: an extern may return `string` or `Option<string>` and take `Option<string>`, and a callback may receive them; C's text is copied, with invalid UTF-8 replaced by U+FFFD (milestone 89; section 8).
- C structs by value: `extern struct Name { … }` has C's layout and goes to and from C (externs and callbacks) by value, exactly as C passes it on x86-64 (milestone 90; sections 3, 4, 8, 9).

Every valid v15 program is a valid v16 one, unless it uses `Ref` as the name of something other than a type (a module's own type `Ref` still means its own), or declares a function named `assert`, `assert_eq` or `assert_ne`, which are intrinsics since milestone 84 (E0302).

**v14.** This is v13 plus binary I/O, `char`, tuples, `format`, import aliases and whole-module re-exports, and git packages (milestones 45–50).

**Changes in v15 since v14** (milestones 51–66):
- A package's `[package]` table may give its `name`, checked against the key a manifest lists it under (E0327), and its `source`, the folder its modules start in: with `source = "src"`, `src/array.la` of package `glowworm` is `glowworm.array` (section 6).
- A program with no `lugha.toml` beside its main file uses the nearest one above it, looking no further than a folder with `.git` or the home folder; when that manifest names a package, the program imports it by that name, so `glowworm/examples/basics.la` writes `import glowworm.array;` with no manifest of its own (section 6).
- A file imported as two modules is E0328 (section 6).
- A manifest path such as `".."` resolves when the main file is given with a folder (`lughac run example/main.la`); v14 reported E0322.
- Operator overloading: `a + b` on a struct, enum or interface value calls `a.add(b)`, and likewise `-` `sub`, `*` `mul`, `/` `div`, `%` `rem` and unary `-` `neg`, for a method of the operator's shape; `a += b` is `a = a.add(b)`. The prelude's `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg` and `Num` are bounds that the numbers (and `string`, for `Add`) satisfy, so `fun add<T: Add>(a: T, b: T): T = a + b;` works for numbers, strings and your own types; `0` and `1` are values of a `T: Num`. An `impl` block may bound its parameters, `impl<T: Num> Grid<T> { … }` (sections 3–5).
- A standard library: `import std.math;` loads a module bundled with `lughac`, only when imported. `std.math` has `sqrt`, `pow`, `exp`, `ln`, `log10`, `sin`, `cos`, `tan`, `atan2`, `floor`, `ceil` and `round` for any `T: Float` (`f32` or `f64`, a new prelude bound), `abs`, `min`, `max` and `clamp`, and `pi()` and `e()`. `std` is a reserved package name (sections 4, 6, "The standard library").
- Arrays are copy-on-write: a copy shares the array until either side writes to it, so copying costs a constant time and reading a copy never copies. Every copy still behaves as an independent value, and no program's output changes (sections 4, 5, 7, 9, 12). A loop makes the arrays it writes, and can't share, unique once before it starts, so its writes cost no more than before copy-on-write (milestone 56; section 9).
- An argument of a `mut self` call may call methods on the receiver, `g.tanh(g.add(a, b))`, or read numbers and strings from it; only an argument that can be the receiver's own storage (`v.add(v)`, `v.add(v.inner)`) is E0508. The arguments run before the receiver is reached, and a change they make to a struct containing the receiver is no longer E0510 unless an array index lies between (milestone 57; sections 4–5).
- Numbers as `T`: `n as T` and `x as f64` cast to and from a type parameter bounded by `Num` or `Float`, and any integer literal is a `T: Num` and any float literal a `T: Float` (`x * 2`, `x * 0.5`); a literal that doesn't fit an instance's type is E0402, naming the instance (milestone 58; section 4).
- User-defined order: a struct or enum with `fun cmp(self, other: Self): Ordering`, the prelude's new `enum Ordering { Less, Equal, Greater }` (written `Ordering.Less`), is `Ord`: `<`, `<=`, `>` and `>=` call `cmp`, tuples compare through it, and `math.min`, `it.min()` and the new `xs.sort()` accept it. `==` stays structural (milestone 59; section 4).
- An imported std module's functions and methods are compiled only when the program reaches them, as the prelude's are; every one is still checked. No program's behaviour changes, only the size of its IR (milestone 60; sections 6, 9).
- `it.sum()` goes through one generic prelude helper for every numeric type; nothing visible changes (milestone 61; section 9).
- `parse_i64`, `parse_u64` and `string_from_utf8` are written in Lugha in the prelude, and `string_from_utf8` and `s.chars()` share one UTF-8 decoder; results and messages are unchanged (milestone 62; section 9).
- A format width counts characters, not bytes (`{"Zoë":<6}` pads with three spaces, not two), and any character may fill (`{n:é^9}`); text without non-ASCII characters prints as before (milestone 63; section 5).
- `std.env`: `env.get(name)` reads an environment variable as an `Option<string>`, and `env.current_dir()` gives the working directory; nothing sets either (milestone 64; "The standard library").
- `std.fs`: `fs.list_dir`, `make_dir`, `make_dirs`, `remove_file`, `remove_dir` (empty folders only), `exists`, `is_dir` and `is_file` (milestone 65; "The standard library").
- `std.process`: `process.run`, `run_with` and `call` run another program with an argument list, never through a shell, and give its exit code and output (milestone 66; "The standard library").
- `std.random`: `random.seeded(42)` or `random.from_os()` makes a generator (xoshiro256\*\*, not cryptographically secure) with `uniform`, `below`, `between`, `normal`, `chance`, `choice` and `shuffle` ("The standard library").

Every valid v14 program and package is a valid v15 one: a module's own type or interface named `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, `Num` or `Ordering` still means its own, as for `Iter`.

**Changes in v14 since v13** (milestones 45–50):

**v13.** This is v12 plus `Self` in declarations, `for … of mut`, more iterator helpers and format specifiers (milestones 41–44).

- Binary I/O: `read_bytes`, `write_bytes`, `append_bytes`, `read_stdin_bytes`; `s.to_bytes()`; `string_from_utf8(bytes)`, which rejects invalid UTF-8. An index may be of any integer type (sections 4–5).
- `char`, a Unicode scalar value: `'a'`, `'é'`, `'\u{1F600}'`; `s.chars()` and `s.char_count()`; `c as u32`, `n as char`, `char_from(n)`; classification and case for ASCII and Latin-1 (sections 2–5).
- Tuples: `(A, B)` types and `(a, b)` values, `t.0`, patterns `(p, q)` in `match`, `if let`, `let (a, b) = t;` and `for (i, w) of …`; element-wise `==`, lexicographic `<`, printed `(1, "a")`. `zip` yields tuples, and `Pair` is gone (sections 3–5).
- `format("{} has {:>4} items", name, n)` returns a `string`: a literal template whose holes `{}` and `{:spec}` take the further arguments in order (E0432; sections 2, 5).
- Imports take `as`: `import a.util as u1;` binds the module as `u1`, and `import a.b { X as Y };` binds the item as `Y`. `public import a.b [as v];` re-exports the whole module, which files importing this one reach as `m.v.X` (sections 3, 6).
- Git packages: a manifest entry `name = { git = "https://…", version = "1.2" }` fetches the newest matching `vX.Y.Z` tag with the system `git` into a per-user cache, and `lugha.lock` pins its commit; `lughac update` re-resolves, and `--offline` never runs `git` (E0325, E0326; sections 6, 9).

A valid v13 program is a valid v14 program unless it uses `Pair` or `zip`'s `Pair` fields (`zip` yields tuples now: `p.first` is `p.0`), uses `char` as a name (a type keyword now), or declares a function named `format`, `read_bytes`, `write_bytes`, `append_bytes`, `read_stdin_bytes` or `string_from_utf8`, which are now intrinsics.

**v12.** This is v11 plus printing any value: the prelude's `Display` and `show`, built-in forms, and `unwrap on Err: <error>` (milestones 39–40).

**Changes in v13 since v12** (milestones 41–44):
- In a struct's fields and an enum's payloads, `Self` is the type being declared: `struct Tree<T> { value: T, kids: Self[] }`, `enum List { Cons(i64, Self), Nil }` (section 4).
- `for x of mut xs` makes `x` each element itself, so `x *= 2` changes `xs`; `for x of mut it` advances the iterator `it` itself, so a later loop resumes where a `break` left it (sections 3, 5).
- Iterators gain `zip` (yielding the prelude's `Pair { first, second }` in v13; a tuple since v14), `chain`, `take_while`, `skip_while`, `flat_map`, `min`, `max` and `position` (section 4).
- An interpolation may end in a format specifier: `"{pi:.2}"`, `"{n:>6}"`, `"{n:05}"`, `"{n:#x}"`, `"{name:*^9}"`; a bad one is E0431 (sections 3, 5).

Every valid v12 program was a valid v13 program.

**v11.** This is v10 plus package versions and named imports and re-exports (milestones 37–38).

**Changes in v12 since v11** (milestones 39–40):
- `print`, `println`, `eprint`, `eprintln`, `to_string` and `"{x}"` accept structs, enums, arrays, `Option`, `Result` and type parameters. A type with its own `fun show(self): string` (the prelude's `Display`) prints through it; any other prints in a built-in form, `Point { x: 1, y: 2 }`, `Circle(1.5)`, `[Some(1), None]`, with the strings inside it quoted (sections 4–6).
- Interface values print as the value inside them, function values as `<fun>`, and `unwrap()` on an `Err` panics with `unwrap on Err: ` followed by the error (sections 4–5).

Every valid v11 program was a valid v12 program; only the `unwrap on Err` panic message is longer.

**v10.** This is v9 plus `Self` in `impl` blocks, default methods on concrete types, `i8`/`i16`/`u16` with checked, saturating and rotating methods, and iterator helpers (milestones 34–36).

**Changes in v11 since v10** (milestones 37–38):
- A package's `lugha.toml` may declare its version, `[package]` then `version = "1.4.0"`, and a manifest may require a compatible one, `geo = { path = "libs/geo", version = "1.2" }`; a version that doesn't meet it is E0324 (section 6).
- `import a.b { X, y };` brings items into a file by name, and `public import a.b { X, y };` re-exports them as items of the module, so a program can use `shapes.Vec2` from a package it doesn't list (sections 3, 6).

Every valid v10 program and manifest was valid in v11.

**v9.** This is v8 plus or-patterns, `mut` bindings, `unwrap`/`expect`, `%=`, and `if let`, `while let` and `let … else` (milestones 31–33).

**Changes in v10 since v9** (milestones 34–36):
- Inside `impl T`, `Self` is `T` (with its parameters, `Stack<T>`), as a type, in struct literals (`Self { x: 0 }`), associated calls (`Self.new()`) and variants (`Self.Red`) (section 4).
- A default method can be called on a struct or enum that lacks its own, `c.describe()`, when exactly one visible interface the type satisfies supplies it; two are E0430 (section 4).
- `i8`, `i16` and `u16` are integer types like the others. Every integer type has `checked_add/sub/mul/div` (an `Option`), `saturating_add/sub/mul` and `rotate_left/right`, and the constants `I.MIN` and `I.MAX` (`i16.MAX`); `parse_u64(s)` parses a `u64` (sections 2–5, 8).
- Iterator helpers: `xs.iter()`, `s.bytes()` and `range(a, b)` make iterators; `it.map(f)`, `filter`, `take`, `skip` and `enumerate` wrap one lazily, and `collect`, `count`, `fold`, `any`, `all`, `find` and `sum` consume it. `for b of s` loops over a string's bytes (sections 4–5).

A valid v9 program was a valid v10 program unless it uses `i8`, `i16` or `u16` as a name, which are type keywords now, or declares a function named `parse_u64`, which is now an intrinsic.

**v8.** This is v7 plus packages: a `lugha.toml` manifest, package imports, and `public` / package / `private` visibility (milestone 30).

**Changes in v9 since v8** (milestones 31–33):
- A pattern may be alternatives, `Circle(r) | Square(r) => r * 2.0` or `"q" | "quit" => …`, nested anywhere; every alternative binds the same names at the same types (E0428). `mut name` makes a binding the arm's own copy, which the arm may change (sections 3–4).
- `o.unwrap()` and `o.expect(msg)` on `Option` and `Result` give the value inside or panic at the call (`unwrap on None`, `unwrap on Err`, or `msg`); `x %= y` is the compound remainder (sections 2–5).
- `if let p = e { … } else { … }`, `while let p = e { … }` and `let p = e else { … };` test one pattern without a full `match`; a `let … else` block must leave (E0429) (sections 3, 5).

Every valid v8 program was a valid v9 program.

**v7.** This is v6 plus `u32`, `u64` and `f32`, bit operators, and wrapping and bit-counting methods on integers (milestones 27–29).

**Changes in v8 since v7** (milestone 30):
- A `lugha.toml` next to the main file lists packages, `name = "path"` under `[packages]`; `import geo.shapes;` then loads `shapes.la` from package `geo`'s folder, and a package may list its own packages in its own `lugha.toml` (section 6).
- `public` is allowed (it was reserved, E0317): an item without a keyword is now visible within its own package, `public` makes it visible to other packages, and `private` still limits it to its module. A `public` declaration may mention only `public` types (sections 3, 6).

Every valid v7 program was a valid v8 program: one without a `lugha.toml` is a single package, where the default visibility still reaches every module.

**v6.** This is v5 plus generic interfaces, iterators, `Self` in interfaces and converting between interfaces (milestones 23–26).

**Changes in v7 since v6** (milestones 27–29):
- `u32`, `u64` and `f32` are numeric types: literals take them from context, arithmetic and overflow panics work at their widths, every numeric type casts to every other, and they print, interpolate and cross the C boundary (sections 2, 4, 5, 7–9).
- Bit operators `&`, `|`, `^`, `~`, `<<` and `>>` on integers, with Rust's precedence (`flags & MASK == 0` is `(flags & MASK) == 0`), the compound forms `&=` `|=` `^=` `<<=` `>>=`, binary literals (`0b1010`), and a panic for a shift that is negative or at least the type's width (sections 2–5, 9).
- Every integer type has `wrapping_add`, `wrapping_sub` and `wrapping_mul`, which never panic, and `count_ones`, `leading_zeros` and `trailing_zeros`, which give an `i64` (sections 4, 9).

A valid v6 program was a valid v7 program unless it used `u32`, `u64` or `f32` as a name: they are type keywords now, like `i32`.

**v5.** This is v4 plus interface values, type patterns, default methods and `?` converting into an interface error (milestones 20–22).

**Changes in v6 since v5** (milestones 23–26):
- An interface may take type parameters, and a bound passes it arguments: `interface Source<T> { fun next(mut self): Option<T>; }`, `fun collect<T, S: Source<T>>(src: S): T[]`. A parameter that appears only in a bound's arguments is read off the concrete type's methods. A generic interface is a type too: `let s: Source<i64> = Count { n: 0, end: 3 };` (sections 3–4, 9).
- The prelude declares `interface Iter<T> { fun next(mut self): Option<T>; }`, and `for x of e` loops over anything with that `next`, working on a copy of `e` (sections 4–5, 9).
- Inside an interface, `Self` is the type that satisfies it: `fun less(self, other: Self): bool`. Such an interface is a bound only; as a type it is E0427 (sections 4, 9).
- A value of one interface type converts to another where that one is expected, when its interface declares every item of the other: a `Pet` becomes a `Named`, and `?` converts interface errors the same way (sections 4, 7, 9).

Every valid v5 program was a valid v6 program.

**v4.** This is v3 plus `Result` and `?`, string and `Option`/`Result` helpers, and input and output (milestones 17–19); v4.1 allows string literals inside interpolations.

**Changes in v5 since v4** (milestones 20–22):
- An interface whose items are all methods is a type: `let shapes: Shape[] = [c, sq];`. A value converts to it where an interface is expected, and a call on it runs the concrete type's method (sections 4, 7, 9).
- A type pattern, `c: Circle => …` in a `match` on an interface value, matches when the value holds that type and binds it (sections 3–4).
- An interface method may have a default body, which a type gets unless it has the method itself (sections 3–4, 9).
- `?` converts an error into an interface error: in a function returning `Result<U, Error>`, `?` on a `Result<T, ParseError>` wraps the `ParseError` as an `Error` (section 4).

Every valid v4 program was a valid v5 program.

**v3.** This is v2 plus command-line arguments and number parsing, string interpolation, functions as values, closures, and array helpers (milestones 13–16).

**Changes in v4 since v3** (milestones 17–19):
- `Result<T, E>` is predeclared, with `Ok(x)` and `Err(e)` written without a prefix (section 4).
- The postfix `?` gives a `Result`'s `Ok` value or an `Option`'s `Some` value, or returns the failure from the function at once (sections 2–4).
- Strings have `lines`, `split`, `trim`, `contains`, `starts_with`, `ends_with` and `slice`, `string[]` has `join`, and `Option` and `Result` have `unwrap_or`, `ok_or`, `ok`, `map`, `map_err` and `is_*` (section 4).
- Programs read stdin with `read_line`, read and write text files with `read_file`, `write_file` and `append_file`, write to stderr with `eprint`/`eprintln`, and stop with `exit(code)` (section 5).

A valid v3 program was a valid v4 program unless it defines something named `Result`, `Ok` or `Err`, or a function named `read_line`, `read_file`, `write_file`, `append_file`, `eprint`, `eprintln` or `exit`.

## 1. Overview

Lugha (Swahili for "language") is a small, statically typed, compiled language with Rust-flavored structure and TypeScript-flavored declarations. It compiles through LLVM to native executables and manages heap memory by reference counting. Source files use the `.la` extension.

v0 is a learning target: every feature in this spec should be implementable by one person in a weekend or two, and every milestone ends in a program that runs.

**Goals**

- A complete pipeline from source text to native binary: lexer, parser, type checker, LLVM codegen, linker.
- Static types with one implicit conversion, to an interface where one is expected (v5), so type errors are caught before codegen.
- Memory safety for ordinary code: no null, no dangling pointers, bounds-checked indexing.
- Clear error messages with source locations.

**Non-goals for v0**

- Generics (other than the built-in `Option<T>`), modules and closures. See section 11.
- Performance tuning beyond what LLVM's optimizer gives for free.
- A self-hosted compiler or a package manager.

**Design principles**

- One way to do each thing. Fewer features, fully specified.
- Immutable by default. A binding can only be reassigned if it is declared `let mut`.
- Every construct maps directly onto LLVM IR without a runtime type system.
- Where Rust and TypeScript disagree, pick whichever is simpler to implement and parse.

**Syntax at a glance**

```
struct Point { x: f64, y: f64 }

fun dist2(a: Point, b: Point): f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fun main(console: Console) {
    let mut total = 0;
    for i in 0..10 {
        total += i;
    }
    console.println(total);
}
```

## 2. Lexical structure

Source files are UTF-8. The lexer produces a flat list of tokens, each carrying a span (start and end byte offsets) for error reporting.

**Whitespace and comments.** Spaces, tabs, carriage returns and newlines separate tokens and are otherwise ignored. A line comment starts with `//` and runs to the end of the line. v0 had no block comments; v16 adds them (milestone 76):
- A block comment starts with `/*` and ends with the matching `*/`, over any number of lines, and may appear wherever whitespace may: `let x = 1 /* inline */ + 2;`.
- Block comments **nest**: `/* a /* b */ c */` is one comment, so code that holds one can be commented out. Nothing else inside is special: `//`, quotes and apostrophes are just text there. `/**` and `/*!` are ordinary block comments.
- Inside a string, `/*` is text. Inside an interpolation, a block comment ends with its line, as the string does: a line break there ends the string (E0102).
- A `/*` never closed is E0113, at the outermost one still open; the rest of the file is the comment. A `*/` outside a comment is the operators `*` and `/`.
- **Doc comments** (milestone 77): a block comment starting `/**` (not `/**/`) followed, apart from whitespace, a `public` or `private` and an `@test` (v16, milestone 84), by a function, method, extern, struct, field, enum, variant, interface or interface method, and (v17) an `extern type` or an `actor`, documents it. Its text is Markdown, each line's leading `*` dropped; a line `@param name text` documents a parameter and `@returns text` the result; other `@tags` stay as written. To the compiler it is a comment like any other; editors show it (section 9).
  - **Module docs** (v17, milestone 98): a file's first doc comment, with only whitespace and plain comments before it, documents the file's module when what follows it, apart from whitespace and plain comments, is a blank line, an `import` (or `public import`) or the end of the file. It then documents no item; directly above an item, with no blank line, it documents that item as before.

**Identifiers.** `[A-Za-z_][A-Za-z0-9_]*`, excluding keywords. Identifiers are case-sensitive. `type` right after `extern`, and `data` and `release` as an extern's parameter types, are ordinary identifiers with a meaning there only (v16, section 8); so are `after` and `default` at the start of a `select` arm (v17, section 3), and `derive` at the start of an item followed by `<` or a name, where it begins a derive block (v17, milestone 142; inside a block `derive.` names the field intrinsics, section 4). In the sources bundled with `lughac` (the prelude and `std`), an identifier may also start with `$`, naming a compiler built-in such as `$float.sqrt` (v15); in a program `$` is E0101, so programs can never name one.

**Keywords.**

```
fun  extern  struct  enum  impl  let  mut  if  else  while  for  in  of  match
import  private  public  interface
return  break  continue  true  false  as  self  spawn  select  actor  const  defer  loop
i32  i64  u8  f64  bool  string  u32  u64  f32  i8  i16  u16  char
```

Built-in type names are keywords, so they can't be shadowed by user structs. The names syntax depends on are not keywords but built-in names, reserved like the intrinsics (E0302 to declare as a struct, enum, actor, extern type or interface; v18, milestone 223): `Option`, `Some`, `None`, `Result`, `Ok`, `Err` (`?` and patterns), `Eq`, `Ord` (`==`, `<`), `Display` (`"{x}"`), `Iter` (`for`), `Error` (`?` into any error), `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg` (the operators), `FromEntries`, `FromItems` (map and set literals) and `Ref`. Every other prelude name (`Map`, `Set`, `Console`, `Dir`, …) may be a module's own, hiding the prelude's there.

**Literals**

| Kind | Examples | Notes |
| --- | --- | --- |
| Integer | `42`, `1_000_000`, `0xFF`, `0b1010`, `0o600` | Decimal, hex, binary (v7) or octal (v17, milestone 117); `_` separators allowed between digits. A leading zero is still decimal (`0600` is six hundred) and warns W0107 |
| Float | `3.14`, `2.0e-3` | Must have digits on both sides of the `.`; optional exponent |
| String | `"hello\n"` | Double-quoted; escapes `\n` `\t` `\r` `\\` `\"` `\0`, and `\u{1F600}` (one to six hex digits naming a Unicode scalar value, as a `char` takes it; v18, milestone 224) |
| Boolean | `true`, `false` | Keywords |

**Number details.**

- `_` may appear only between two digits, and only in integers. `1__0`, `1_`, `0x_FF` and `1_0.5` are errors.
- The hex prefix is lowercase `0x` followed by at least one hex digit, in either case. `0XFF` and `0x` are errors. The binary prefix (v7) is lowercase `0b` followed by at least one `0` or `1`: `0B1`, `0b` and `0b102` are errors.
- Leading zeros are allowed and mean nothing special: `007` is 7.
- A literal without a `.` is an integer. `2e5` is an error; write `2.0e5`.
- An exponent is `e` or `E`, an optional sign, then at least one digit.
- A `.` not followed by a digit ends an integer: `1..10` is `1` `..` `10`.
- A letter directly after a number is an error (`123abc`, `1.5x`).
- The lexer accepts integers up to 2⁶⁴−1. Whether a literal fits its actual type is checked later (section 4).
- A float literal that rounds to infinity is an error. One that underflows rounds to zero.

**String details.** A string may not contain a raw line break: write `\n`. An unclosed string is reported at the end of its line, and lexing continues on the next one. Any other UTF-8 may appear inside strings and comments. Outside them, only ASCII is allowed.

**String interpolation** (v3). Inside a string literal, `{` starts an interpolation that runs to its matching `}`: `"{name} is {age * 2}"`. Braces inside the expression nest, so `"{if c { 1 } else { 2 }}"` works. The expression may contain string literals, which may interpolate in turn (v4.1): `"<{parts.join("-")}>"`, `"{if ok { "yes" } else { "no" }}"`, `"{name}: {"{a}/{b}"}"`. `{{` and `}}` are literal braces. The lexer emits such a string as its literal parts (`STRING_START`, `STRING_MID`, `STRING_END`) with the expression's tokens between them, a nested string's tokens among them; escapes apply in every part. Errors (E0111): a lone `}`, a `{` never closed, or an empty `{}`. Strings stay on one line: a line break ends a string that began inside an interpolation with E0111 at that interpolation's `{`, and otherwise ends the string (E0102).

**`format` templates** (v14). A string literal directly after `format` `(` (the name not after a `.`) is a template: one `STRING` token whose escapes are decoded but whose braces are kept as written, so nothing in it interpolates. Section 5 says what its braces mean.

**Lexical errors.** The lexer reports every error it finds, not just the first.

| Code | Error | Example |
| --- | --- | --- |
| E0101 | Unexpected character | `#`, `` ` ``, `é` or `$` outside a string (`@` starts an attribute) |
| E0102 | Unterminated string | `"abc` at end of line or file |
| E0103 | Unknown escape sequence | `"\q"` |
| E0104 | Integer literal too large | `18446744073709551616` |
| E0105 | Misplaced `_` in a number | `1__0`, `1_`, `0x_FF`, `1_0.5` |
| E0106 | Number is missing digits | `0x`, `2.0e`, `2.0e+` |
| E0107 | Float needs a fractional part | `2e5` (help: write `2.0e5`) |
| E0108 | Invalid character in a number | `123abc`, `0XFF`, `0xFG`, `0b102` |
| E0109 | Float literal out of range | `1.0e999` |
| E0110 | Source is not valid UTF-8 | A Latin-1 `é` byte; reported at the first bad byte |
| E0111 | Bad interpolation in a string | `"a } b"`, `"{x"`, `"{}"` — write `{{`/`}}` for literal braces |
| E0112 | A bad `char` literal | `''`, `'ab'`, `'\q'`, `'\u{D800}'`, `'\u{110000}'`, an unclosed `'a` |
| E0113 | Unterminated block comment | `/* never closed`, `/* a /* b */` |

**Operators and punctuation.** The lexer uses longest match, so `<=` is one token, not `<` then `=`.

```
+  -  *  /  %  ==  !=  <  <=  >  >=  &&  ||  !
&  |  ^  ~  <<                                  (v7)
=  +=  -=  *=  /=  %=  &=  |=  ^=  <<=          (v7: the bit forms; v9: `%=`)
(  )  {  }  [  ]  ,  ;  :  .  ..  =>  ?
@                                               (v16: attributes)
```

`>>` and `>>=` (v7) are not tokens: the lexer gives two `>` (or `>` and `>=`), so `Option<Option<i64>>` closes both type-argument lists, and an expression reads `>` immediately followed by `>`, with nothing between, as a right shift (`>>=` likewise in an assignment). With a space between (`a > > b`), they stay two `>`, a parse error.

`::` is lexed as a token only so the parser can reject it helpfully (E0207): Lugha writes `Shape.Circle`, not `Shape::Circle`.

## 3. Grammar

The grammar is LL(1) apart from expressions, which use Pratt parsing with the precedence table below. Parentheses around the condition of `if` and `while`, and around a `for` header, are optional: `if x > 0 {` and `if (x > 0) {` are both valid.

**Recovery** (v16, milestone 75). After a syntax error the parser skips to the end of the statement (the next `;` or `}` at its depth) or to the next item, and goes on, so it reports every syntax error it can. The compiler stops after parsing when there is one; the language server instead checks the program it recovered — every import, item and statement that parsed, without the broken ones — so its features keep working in a file being edited (section 9).

```ebnf
program      = { import } { item } EOF ;
import       = [ "public" ] "import" IDENT { "." IDENT } [ "as" IDENT ]
               [ "{" ( "*" | import_item { "," import_item } [ "," ] ) "}" ] ";" ;   (* names and `public`: v11; `as`: v14; `*` (only with `public`): v17 *)
import_item  = IDENT [ "as" IDENT ] ;
item         = { attribute } [ visibility ] ( fun_decl | extern_decl | extern_type | [ "extern" ] struct_decl | enum_decl | interface_decl | const_decl ) | impl_decl | derive_decl ;
derive_decl  = "derive" [ type_params ] IDENT "for" ( type | "struct" | "enum" | "tuple" ) "{" { method } "}" ;   (* v17, milestone 142; `derive` contextual *)
const_decl   = "const" IDENT ":" type "=" expr ";" ;   (* v17 *)   (* `extern struct`, v16 *)
attribute    = "@" IDENT [ "(" IDENT ")" ] ;    (* `@test` before a `fun` (v16), `@test(integration)` (v18); `@caller_location` before a `fun` or a method (v18) *)
visibility   = "private" | "public" ;                (* `public`: v8, packages *)

fun_decl     = "fun" IDENT [ type_params ] "(" [ params ] ")" [ ":" type ] fun_body ;
fun_body     = block | "=" expr ";" ;          (* "= e;" means "{ e }" *)
extern_decl  = "extern" [ STRING ] "fun" IDENT "(" [ params ] ")" [ ":" type ] ";" ;   (* the string: the C symbol, v16 *)
extern_union = "extern" "union" IDENT "{" field { "," field } [ "," ] "}" ;   (* v17 *)
extern_type  = "extern" "type" IDENT ";" ;                                         (* v16; `type` isn't a keyword *)
struct_decl  = "struct" IDENT [ type_params ] "{" [ field { "," field } [ "," ] ] "}" [ with ] ;
field        = [ visibility ] IDENT ":" type ;
enum_decl    = "enum" IDENT [ type_params ] "{" [ variant { "," variant } [ "," ] ] "}" [ with ] ;
with         = "with" bound { "," bound } ;   (* v18; `with` is a word only here *)
impl_decl    = "impl" [ type_params ] [ bound "for" ] type ( "{" { [ visibility ] method } "}" | "=" "derive" ";" ) ;   (* one spelling, v18: a generic block's parameters follow "impl" (`impl<T: Num> Grid<T>`), and a struct's, enum's or actor's type names exactly them, in order; `impl Grid<T>` is E0214. "= derive" only with an interface *)
method       = "fun" IDENT [ type_params ] "(" [ receiver [ "," params ] | params ] ")" [ ":" type ] fun_body ;
type_params  = "<" type_param { "," type_param } [ "," ] ">" ;
type_param   = IDENT [ ":" bound { "+" bound } ] ;
bound        = "Eq" | "Ord" | IDENT [ "." IDENT [ "." IDENT ] ] [ "<" type { "," type } [ "," ] ">" ]
             | "fun" "(" [ type { "," type } [ "," ] ] ")" [ ":" type ] ;   (* a function type as an interface (v18, milestone 196) *)
                                          (* an interface, `m.Shape` from module `m`, `Source<T>` (v6) *)
interface_decl = "interface" IDENT [ type_params ] [ ":" bound { "+" bound } ] "{" { iface_item } "}" ;   (* extends: v18; parents joined with "+", a "," is E0215 *)
iface_item   = "fun" IDENT "(" [ receiver [ "," params ] | params ] ")" [ ":" type ]
               ( ";" | "=" expr ";" | block ) ;      (* a default body (v5): methods only *)
receiver     = [ "mut" ] "self" ;
variant      = IDENT [ "(" type { "," type } [ "," ] ")" ] ;
params       = param { "," param } [ "," ] ;
param        = [ "out" | "ptr" | "mut" ] IDENT ":" type ;   (* "out" and "ptr" an extern's only (v17); "mut" an extern's, a function's or a method's (v18) *)

type         = base_type { "[" "]" } ;
base_type    = "i32" | "i64" | "u8" | "f64" | "bool" | "string"
             | "u32" | "u64" | "f32"                (* v7 *)
             | "i8" | "i16" | "u16"                 (* v10 *)
             | "char"                               (* v14 *)
             | "(" type "," type { "," type } [ "," ] ")"   (* a tuple (v14) *)
             | "(" ")"                              (* the unit type (v17) *)
             | IDENT [ "." IDENT [ "." IDENT ] ] [ "<" type { "," type } [ "," ] ">" ]
             | "fun" "(" [ type { "," type } [ "," ] ] ")" [ ":" type ]   (* a function type (v3) *)
             | "(" type ")" ;                  (* `(fun(i64): i64)[]` is an array of functions *)
                                           (* `m.T`, `m.v.T` (v14), `Stack<i64>`; `Option<T>` is one such *)

block        = "{" { stmt } [ expr ] "}" ;   (* trailing expr = tail value *)
stmt         = var_decl | [ label ] while_stmt | [ label ] for_stmt
             | "return" [ expr ] ";"
             | "break" [ IDENT ] [ expr ] ";" | "continue" [ IDENT ] ";"   (* a label and a value: v17, milestone 152 *)
             | "defer" ( block_like [ ";" ] | expr [ assign_op expr ] ";" )   (* v17, milestone 151; an assignment: PRP-183 *)
             | block_like                     (* no ";" needed *)
             | expr [ assign_op expr ] ";" ;
block_like   = if_expr | match_expr | select_expr | scope_expr | shield_expr | [ label ] loop_expr | block ;   (* select: v17; loop: milestone 152; scope: milestone 195; shield: milestone 199 *)
scope_expr   = "scope" [ "<" type ">" ] IDENT { scope_word } block ;   (* `scope` is a keyword only here, before a name and `{` (v18); `<E>` and settings: milestone 198 *)
scope_word   = "independent" [ "(" expr ")" ] | "limit" expr | "within" expr | "until" expr ;   (* each at most once; `within`, `until`: milestone 199 *)
shield_expr  = "shield" "within" expr block ;   (* `shield` is a word only before `within` (milestone 199); before `{` at a statement's start it is E0211 *)
label        = IDENT ":" ;        (* only directly before `loop`, `while` or `for`; never a map literal's key *)
loop_expr    = "loop" block ;     (* v17, milestone 152 *)
var_decl     = "let" [ "mut" ] IDENT [ ":" type ] "=" expr ";"
             | "let" pattern "=" expr [ "else" block ] ";" ; (* let-else (v9); without else, irrefutable (v14) *)
assign_op    = "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" ;   (* bit forms: v7; `%=`: v9 *)

if_expr      = "if" ( cond | "let" pattern "=" cond ) block [ "else" ( if_expr | block ) ] ;   (* if let (v9) *)
while_stmt   = "while" ( cond | "let" pattern "=" cond ) block ;                            (* while let (v9) *)
for_stmt     = "for" ( for_head | "(" for_head ")" ) block ;
for_head     = ( IDENT | "(" pattern "," pattern { "," pattern } ")" ) ( "in" cond ".." cond | "of" [ "mut" ] cond ) ;   (* `of mut`: v13; a tuple pattern: v14 *)
cond         = expr ;            (* parsed in no-struct-literal mode *)

match_expr   = "match" cond "{" [ arm { arm_sep arm } [ "," ] ] "}" ;
select_expr  = "select" "{" [ sarm { arm_sep sarm } [ "," ] ] "}" ;   (* v17 *)
sarm         = ( IDENT "=" postfix | postfix | "after" "(" expr ")" | "default" ) "=>" expr ;
arm          = pattern [ "if" cond ] "=>" expr ;
arm_sep      = "," | (* nothing, after an arm whose expr is a block *) ;
pattern      = alternative { "|" alternative } ;      (* `.A | .B` (v9) *)
alternative  = "_"
             | IDENT                                  (* a binding (v18) *)
             | "." IDENT [ "(" pattern { "," pattern } [ "," ] ")" ]   (* a variant (v18) *)
             | [ "-" ] INT | STRING | "true" | "false"
             | ( IDENT | "_" ) ":" type               (* `c: Circle` (v5) *)
             | "mut" IDENT [ ":" type ]               (* a mutable binding (v9) *)
             | "(" pattern ")"
             | "(" pattern "," pattern { "," pattern } [ "," ] ")"   (* a tuple (v14) *)
             | "(" ")" ;                              (* the unit pattern (v17) *)

expr         = prefix { infix } ;            (* Pratt loop, see table *)
prefix       = ( "-" | "!" ) prefix | "spawn" postfix | postfix ;   (* binds tighter than infix; spawn: v17, an actor only since milestone 195 *)
postfix      = primary [ type_args ] { call | index | member | spawn_in | "?" } ;   (* `read(p)?` (v4) *)
spawn_in     = "." "spawn" "(" expr ")" ;   (* `s.spawn(work(x))`: the expr is a call (v18, milestone 195) *)
type_args    = "<" ( type | "_" ) { "," ( type | "_" ) } ">" ;   (* only by the context rule (v18) *)
call         = "(" [ arg { "," arg } [ "," ] ] ")" ;
arg          = [ "out" | "ptr" | "mut" ] expr ;      (* for an extern's parameter of that mode, v17 *)
index        = "[" expr { "," expr } [ "," ] "]" ;   (* `a[x, y]` is `a[(x, y)]` (v17, milestone 155) *)
member       = "." ( IDENT [ type_args ] | INT ) ;   (* `t.0` (v14); `xs.map<string>` (v18) *)
primary      = INT | FLOAT | CHAR | STRING | interpolated | closure | "true" | "false"
             | "(" expr "," expr { "," expr } [ "," ] ")"   (* a tuple (v14) *)
             | "(" ")"                              (* the unit value (v17) *)
             | IDENT [ "." IDENT ] [ type_args ] [ struct_init ]  (* `m.T { … }`, `Pair<u8> { … }` *)
             | "self"
             | int_type "." IDENT                 (* `i16.MAX` (v10) *)
             | "(" expr ")"
             | "[" [ expr ( ";" expr | { "," expr } [ "," ] ) ] "]"
             | "Set { " [ entry { "," entry } [ "," ] ] " }"      (* v17: a map's entries are `expr ":" expr`, a set's `expr` *)
             | block_like ;
interpolated = STRING_START expr [ FMT_SPEC ] { STRING_MID expr [ FMT_SPEC ] } STRING_END ;   (* "a {x} b {y:>5} c" *)
closure      = "fun" "(" [ cparam { "," cparam } [ "," ] ] ")" [ ":" type ] ( block | "=" expr ) ;   (* v3 *)
cparam       = IDENT [ ":" type ] ;
struct_init  = "{" [ init { "," init } [ "," ] ] "}" ;   (* `..expr` only last, once: v17 *)
init         = expr ":" expr | expr | ".." expr ;   (* a field `IDENT ":" expr`, an entry or an item, by the type (v18) *)
infix        = binop expr | "as" type ;
```

**Operator precedence**, from lowest to highest. All binary operators are left-associative, except equality and comparison (levels 3 and 4), which are non-associative: `a < b < c` and `a == b == c` are parse errors. Write `(a == b) == c` if that is really meant.

| Level | Operators | Kind |
| --- | --- | --- |
| 1 | `\|\|` | Logical or, short-circuit |
| 2 | `&&` | Logical and, short-circuit |
| 3 | `==` `!=` | Equality |
| 4 | `<` `<=` `>` `>=` | Comparison |
| 5 | `\|` | Bitwise or (v7) |
| 6 | `^` | Bitwise exclusive or (v7) |
| 7 | `&` | Bitwise and (v7) |
| 8 | `<<` `>>` | Shifts (v7) |
| 9 | `+` `-` | Additive |
| 10 | `*` `/` `%` | Multiplicative |
| 11 | `as` | Cast |
| 12 | `-` `!` `~` `spawn` | Prefix unary; `spawn` (v17) |
| 13 | call, `[]`, `.` | Postfix |

The bit operators sit between comparisons and `+ -`, as in Rust, so `flags & MASK == 0` means `(flags & MASK) == 0` and `a + b << 2` means `(a + b) << 2`.

**Assignment is a statement**, not an expression, so `a = b = c` and `if (x = 1)` are parse errors. The left side of an assignment must be a place expression: a variable, a field access, an index, or a chain of these (`pts[i].x`). The parser accepts any expression there; the type checker rejects non-places.

**Struct literals in conditions.** Without required parentheses, `if p == Point { x: 0.0, y: 0.0 } { ... }` is ambiguous, because the parser can't tell where the condition ends. So every `cond` is parsed in *no-struct-literal* mode, where `IDENT {` ends the expression instead of starting a literal. The restriction lifts inside every bracket pair — parentheses, brackets, call arguments and blocks — so `if p == (Point { x: 0.0, y: 0.0 }) { ... }` and `if ok { Point { x: 1.0, y: 2.0 } } else { ... }` both work. Rust uses the same rule.

**Enum values and `match`.** `Shape.Circle(2.0)` and `Shape.Empty` need no grammar of their own: they parse as a member access, followed by a call when there are payloads, and the checker decides they name a variant (section 4). A `match` scrutinee is a `cond`, parsed in no-struct-literal mode like an `if` condition. A variant in a pattern is `.IDENT`, with its payload slots after it (`.Some(.Empty)`); a bare `IDENT` is always a new binding, at the top level too (`other =>`), so the parser never needs the types to tell them apart. `IDENT (` without the dot is E0212. An arm whose value is a block ends at its `}`, so `{ … }` then `.Ok(v) =>` is the next arm. A guard after `if` is parsed like a `cond`.

**Generic headers.** Type parameters follow the name, like Rust and TypeScript: `fun max<T: Ord>(a: T, b: T): T`. An `impl` names its type's parameters without bounds, `impl<T> Stack<T> { … }`; the bounds come from the type's declaration. Type arguments may be written at a use (v18, section 4): after a name or a member name, `<` begins them only when a list of types closed by `>` follows and the token after the `>` is `(`, `.`, `{`, `)`, `,`, `;`, `]`, `}` or the end of the input; otherwise it is a comparison. So `f(a < b, c > (d))` passes `a` the type arguments `b, c` (write `f((a < b), (c > d))` for comparisons), while `f(a < b, c > d)` compares. `Stack::<i64>` is E0207, with help `Stack<i64>`.

**Actors** (v17, milestone 92). `actor_decl = [ visibility ] "actor" IDENT [ type_params ] [ "mailbox" INT ] "{" { [ visibility ] IDENT ":" type "," | fun_decl } "}"` declares an actor at the top level (`mailbox n`: milestone 201; `mailbox` is a word only there): its fields (each followed by `,` unless last) and methods, in any order. Inside the body `Self`, and the actor's own name, mean its state. Anything else in the body is E0201.

**Scopes, `spawn` and `select`** (v17; scopes v18, milestone 195). `scope s { … }` names a scope `s` for its block; `scope` is a keyword only where a name and `{` follow it, and a name everywhere else. A task starts through a scope's handle: `s.spawn(…)` takes one expression, which must be a call: `s.spawn(work(x))`, `s.spawn(m.f(x))`, `s.spawn(p.area())`, `s.spawn(fun() { … }())`. `spawn` alone takes the whole postfix expression after it, which must be a struct literal (an actor's state, milestone 92) or end in a call (one giving an actor's state, else E0474 from the checker): `spawn Counter { count: 0 }`, `spawn Counter.new(3)`. Anything else after `spawn` or inside `.spawn(…)` (`s.spawn(f)`, `spawn 1 + f()`, `spawn f()?`) is E0211. A `select` arm is one of `name = c.recv()` (or `_ = c.recv()`), `c.send(v)`, `after(ms)` or `default`, then `=>` and its value, separated as `match` arms are; any other arm is E0211, and the `select` keeps reading at the next arm. `after` and `default` are names everywhere else.

**Block-like statements.** A statement that starts with `if`, `match`, `select` or `{` is parsed as `block_like` and needs no trailing `;`. It ends at its closing brace, so `if c { a } - 1` is a statement followed by the expression `-1`, not a subtraction. If a block's final element is an expression with no `;` after it, including a block-like one, that expression is the block's tail value (section 5). A `;` directly after a block-like statement is allowed and makes it an ordinary statement: `if c { f(); };`. A `;` on its own, where a statement should start, is an error.

**Parse errors.** The parser reports as many syntax errors as it can, skipping to the next statement, or to the next item at top level, after each one.

| Code | Error | Example |
| --- | --- | --- |
| E0201 | Unexpected token | `let x = 1 let y = 2;`, `let x = );`, a stray `;`, `let` at top level; `private import a;` |
| E0202 | Unclosed delimiter | `(`, `[` or `{` still open at end of file |
| E0203 | Chained comparison or equality | `a < b < c`, `a == b != c` |
| E0204 | Assignment used as an expression | `a = b = c;`, `if (x = 1) {}`, `f(x = 1)` |
| E0205 | Struct literal in a condition without parentheses | `if p == Point { x: 0.0 } { }` |
| E0206 | Nesting too deep | More than 256 levels of nested expressions and blocks |
| E0208 | `import` after an item | `fun f() {}` then `import geometry;` |
| E0209 | A closure parameter that isn't a plain name | `fun(self) = 1`, `fun(mut x) = x` |
| E0210 | An unknown or misplaced attribute | `@inline fun f() {}`; `@test struct S {}`; `@caller_location` on an interface's item, an actor's message, `main` or a test |
| E0211 | `spawn` without a call, a malformed `select` arm, or a shield without a deadline | `s.spawn(f)`, `spawn 1 + f()`; `select { c.recv() => 1 }`, `select { x => 1 }`; `shield { … }` (milestone 199) |
| E0212 | A variant in a pattern without its leading dot (v18) | `Circle(r) =>`; `if let Some(x) = o` |
| E0213 | A `#{…}` literal (v18) | `#{"a": 1}` — help: `Map { "a": 1 }` or `Set { 1, 2 }` |
| E0214 | A generic `impl` written with its parameters after the type (v18) | `impl Grid<T> { … }`, `impl Grid<T: Num>`, `impl Show for Grid<T>` — help: `impl<T: Num> Grid<T>` |
| E0215 | An interface's parents separated by `,` (v18) | `interface Store: Eq, Sendable { … }` — help: `interface Store: Eq + Sendable` |
| E0207 | `::` used as a path separator | `Shape::Circle(1.0)` — help: write `Shape.Circle`; parsing continues as if `.` were written; `Stack::<i64>` — help: `Stack<i64>` (v18) |

## 4. Type system

Lugha is statically typed with one implicit conversion: a value becomes an interface value where an interface type is expected (v5, Interface values below). Every expression has exactly one type, computed bottom-up from its parts, with one exception: numeric literals take their type from context (see Inference).

**Types**

| Type | Description | LLVM lowering |
| --- | --- | --- |
| `i32` | 32-bit signed integer | `i32` |
| `i64` | 64-bit signed integer | `i64` |
| `u8` | 8-bit unsigned integer (a byte) | `i8` |
| `f64` | 64-bit IEEE 754 float | `double` |
| `u32` | 32-bit unsigned integer (v7) | `i32` |
| `u64` | 64-bit unsigned integer (v7) | `i64` |
| `f32` | 32-bit IEEE 754 float (v7) | `float` |
| `i8` | 8-bit signed integer, −128 to 127 (v10) | `i8` |
| `i16` | 16-bit signed integer, −32768 to 32767 (v10) | `i16` |
| `u16` | 16-bit unsigned integer, 0 to 65535 (v10) | `i16` |
| `char` | A Unicode scalar value, U+0000–U+D7FF and U+E000–U+10FFFF (v14) | `i32` |
| `bool` | `true` or `false` | `i1` (stored as `i8` in memory) |
| `string` | Immutable UTF-8 bytes | `ptr` to `{ i64 len, bytes, NUL }` on the heap (literals: constant globals) |
| `T[]` | Fixed-length array of `T` | `ptr` to `{ i64 len, T... }` on the heap |
| `S` (struct) | User-declared record | LLVM named struct `%S` |
| `E` (enum) | One of the declared variants, with its payloads | LLVM named struct `%E` = `{ i32 tag, payload area }` (section 7) |
| `Option<T>` | `Some(T)` or `None` | As an enum with those two variants |
| `()` | The unit type (v17): one value, also written `()`; a function without `: type` returns it | `{}` (no bytes) |

**Value semantics.** Every Lugha type behaves as a value. Assigning or storing a value gives the new place its own independent copy, so changing one variable never changes another. Strings are immutable, so the compiler shares them freely and no program can tell. Arrays live on the heap but are copied whenever sharing could be observed (see Array copies below). Enum payloads can't be changed in place, so enum values share them freely too, and no program can tell.

```
let a = Point { x: 1.0, y: 2.0 };
let mut b = a;    // copy
b.x = 9.0;        // a.x is still 1.0

let xs = [1, 2, 3];
let mut ys = xs;  // copy
ys[0] = 9;        // xs[0] is still 1
```

There is no null. Every string and array variable holds a valid object.

**The unit type `()`** (v17, milestone 107; `void` until then). `()` is a type with one value, also written `()`. Statements that produce nothing have it: a function without a return type, a block without a tail, an `if` without `else`, loops and assignments. It is a value like any other: variables, parameters, fields, array elements and type arguments hold it (`Result<(), string>`, `Ok(())`, `Chan<()>`, `Task<()>`, `Map<K, ()>`). It is `Eq` and `Ord` (always equal), hashable, prints as `()` and takes no memory. As a pattern, `()` matches it like `_` (`Some(())`, `let () = log();`); against any other type it is E0403. Binding it with `let x = e` when `e` isn't the literal `()` warns (W0106: usually a call kept for a result it doesn't have); writing the type, `let x: () = e`, or `let _ = e` doesn't. It can't cross the C boundary as a parameter or field (E0409, E0437); an extern without a result type is C's `void`.

**Structs.** Declared at top level with named, typed fields. A struct literal must initialize every field exactly once, in any order — or end with `..base` (v17, milestone 93): `Point { x: 1.0, ..p }` takes every field not written from `base`, an expression of exactly the literal's struct type (type arguments included; another type is E0440). `..base` comes last, once (E0201); the written fields are evaluated first, left to right, then `base` once, and its fields are stored like any value (copied from a place, moved on its last use). Every unwritten field must be visible too (E0313). Writing every field as well as `..base` is warning W0104. Structs may contain other structs and arrays, but a struct may not contain itself directly or through other structs (it would have infinite size). Containing itself through an array is fine, because an array is a pointer: `struct Node { value: i64, kids: Node[] }` builds trees. Other recursive data, such as linked lists, uses an enum or `Option`: a struct may contain itself through either, as in `struct Item { value: i64, next: Option<Item> }`. Only a cycle through structs alone is an error (E0307).

**Enums.** Declared at top level, in any order, as a list of variants that each carry zero or more positional payloads: `enum Shape { Circle(f64), Rect(f64, f64), Empty }`. A variant name is unique within its enum (E0309). A value is built through the enum's name with `.`: `Shape.Circle(2.0)`, or `Shape.Empty` for a variant without payloads. Each payload expects its declared type, and the count must match (E0416); an unknown variant is E0415. `==` compares two enum values by variant and then payload by payload; there is no other operator on enums (E0404). A payload can never be changed in place — there is no syntax for it, and `match` bindings are immutable — so copying an enum shares its payloads, and the copies that matter happen when a payload is stored: building a variant from a place copies it (`Job.Batch(xs)`), and storing a `match` binding copies it (section 4, Array copies). 

**Recursive data.** An enum may contain itself, directly or through structs, other enums or `Option`: `enum List { Nil, Cons(i64, List) }` builds a linked list, and `enum Tree { Leaf, Node(Tree, i64, Tree) }` a binary tree. Nothing has to be annotated and there is no `Box` (writing `Box<List>` is E0305 with a hint). The compiler stores each payload slot that leads back to its enum behind a hidden pointer (section 7), so the type has a finite size, values like `List.Nil` cost nothing, and — since payloads are shared — copying a list of any length takes constant time. A program can't observe the pointer: in `Cons(x, rest)`, `rest` is an ordinary `List` value.

**`Option`.** `Option<T>` is a generic enum predeclared in every module as `enum Option<T> { Some(T), None }`. Its variants are written without a prefix. `Some(x)` has type `Option<T>` where `x : T`, or the expected `Option` type when there is one. `None` needs an expected type, exactly like `[]`: `let x: Option<i64> = None;` — otherwise it is E0412.

**Shared boxes** (v16). `Ref<T>` is predeclared in every module, unless the module declares its own `Ref`. It is one box holding a `T`, and copying a `Ref` (assigning, passing, storing, capturing, returning) copies the handle, never the box: every copy sees the same contents. It is the only value that copies share. Its whole API:

| Call | Result |
| --- | --- |
| `Ref.new(v: T): Ref<T>` | a new box holding `v` (an expected `Ref<U>` types `v` as a `U`) |
| `r.get(): T` | a copy of the contents |
| `r.set(v: T)` | replaces the contents with `v`, stored like any value (copied from a place, moved on last use) |
| `r.update(f: fun(T): T)` | `r.set(f(r.get()))` |
| `r.is(other: Ref<T>): bool` | whether both are the same box |

- **Nothing reaches inside a box in place:** a field (`r.value`) is E0410, an index (`r[0]`) E0411, any other method E0415. You take a copy out and put a value back, so what `get` gave never changes when the box does, and the reverse.
- **No `let mut` needed:** `set` and `update` change the box, not the handle, so they take a plain `self`. `let r = Ref.new(0); r.set(1);` is valid, and a closure may `set` or `update` a `Ref` it captured (assigning the captured name itself is still E0511). `let mut r` is needed only to point `r` at another box.
- **No `==`:** `r == s` is E0404, with help: `r.is(s)` compares boxes, `r.get() == s.get()` contents. A struct, enum, tuple, array or `Option` holding a `Ref` has no `==` either, and isn't `Eq` (E0419).
- **Recursion:** a type may hold itself through a `Ref`, as through an array: `struct Node { value: i64, next: Option<Ref<Node>> }`.

**Tasks and channels** (v17, milestone 91; two ends since milestone 200). `Task`, `Sender`, `Receiver` and the maker `Chan` are predeclared in every module, as `Ref` is, unless the module declares its own. All are shared handles: copying one copies the handle, so every copy names the same task or channel.

- **`scope s { body }`** (v18, milestone 195) has type `Result<T, Failure>`, `T` being the body's type (`()` for a body that never finishes); in the body `s` is a `Scope`. `Scope`, `Failure`, `Stopped` and `Reason` are prelude types: `Failure<E>` (milestone 198) is a struct, `{ task: string, kind: FailureKind<E> }`, the failing task's name and `.Error(E)` (it returned `Err`), `.Panic(string)` or `.Deadline` (milestone 199: the scope's deadline passed, `task` naming the scope), and `Scope` and `Failure` alone mean `Scope<Error>` and `Failure<Error>`; `Stopped` is `.Cancelled(Reason)`, `.Failed(Error)` or `.Panicked(string)`; `Reason` is `.SiblingFailed`, `.Parent`, `.Requested(string)`, `.Deadline` or `.Shutdown` (Ctrl-C; milestone 199). `Failure` and `Stopped` implement `Error`, so `scope s { … }?` and `t.join()?` pass them on. Used as a statement, a scope is a discarded `Result` (E0467).
- **`s.spawn(call)`** has type `Task<T>`, `T` being the call's result type, or the plain type `Task` (no argument) when the call returns nothing; `s` must be a `Scope` (E0473). An expected `Task<U>` types the call as a `U` (`let t: Task<u8> = s.spawn(f(1));`). A call returning `Result<V, E>` needs `E` to implement `Error` (E0422): its `Err` fails the task. `spawn call` without a scope is E0474, unless the call gives an actor's state.
- **`main` and tests may take a `Scope`** (v18), by type as a capability is taken: the root scope.
- **Settings** (v18, milestone 198) come between a scope's name and its body, each at most once (E0477 for another word or a repeat): `independent`; `independent(f)`, `f` a sendable `fun(Failure<E>)` (it runs in the failing task); **`pinned`** (milestone 214), which takes no value: the body, every task the scope starts, and every scope those open run on one worker thread, the main thread when `main` opens it, else the owner's current one (a task already pinned keeps its own); the owner is free again once the scope ends, unless a pinned handle pinned it; `limit n`, `n` an `i64`, evaluated as the scope starts, at least 1 (a panic otherwise). **`scope<E> s { … }`** has type `Result<T, Failure<E>>` and `s` type `Scope<E>`; `E` must implement `Error`, and every task started in it that returns a `Result` must fail with `E` or with an error converting to it as `?` converts (E0478). **`within ms`** and **`until t`** (milestone 199), each an `i64` evaluated as the scope starts (E0403 otherwise), give the scope a deadline: `ms` milliseconds on, or at `t` milliseconds of the monotonic clock `task.sleep` counts (`clock.monotonic_ms()` on the real `Clock`); a scope has one of the two (E0477). **`s.remaining(): Option<i64>`** gives the milliseconds left, at least 0, or `None` without a deadline.
- **`shield within ms { body }`** (v18, milestone 199) has its body's type; `ms` is an `i64` (E0403). `shield` without `within` is E0211.
- **A task's name** is what `t.name(text)` set, else the called function's simple name (`work`, `area` for `p.area()`, `closure` for a closure called on the spot); a failure of the body names the scope's handle.
- **`t.join(): Result<V, Stopped>`** waits until the task has finished and gives a copy of its value, `V` being `T`, or the `Ok` type when `T` is a `Result`; joining again gives another copy at once. A plain `Task`'s `join` gives `Result<(), Stopped>`. **`t.cancel(why: string)`** stops the task with `.Requested(why)`; **`t.name(text: string)`** names it.
- **Second-class handles** (v18): a `Scope` or a `Task` is passed as an argument and nothing else. A function returning one, a field, payload, element, tuple or map holding one, a closure capturing one (a spawned call's arguments among them) is E0472. A local holding one is fine.
- **Sendable** (v18, milestone 197): a value crossing to another task must be `Sendable`. It crosses as a spawned call's argument (the receiver included), as something the call captures (a closure literal's captures included), as its result, as what a channel carries (checked where `Sender<T>` or `Receiver<T>` is written, or `Chan<T>.new` made), and as an actor's field, message parameter or result. `Sendable` is the prelude's `auto interface Sendable {}`: its derive blocks (each number type, `bool`, `char`, `string`, `()`, C handles, `T[]` of a sendable `T`, structs, enums and tuples) apply to every type whose parts meet them, with no `with`. Besides: a channel's ends are sendable when what it carries is; an actor's handle is; an interface value is when its interface requires `Sendable`; a function's own type is when its captures are (a named function captures nothing); `T` is when bounded `T: Sendable`. A `Ref`, a plain `fun(…)` value (which function, and what it captured, is hidden), `Scope` and `Task` never are. A value that isn't is E0475, naming its first part that isn't. `Console`, `Fs`, `Dir`, `File`, `Clock`, `Env`, `Proc`, `Entropy`, `Net`, `Listener`, `Conn`, `Udp` and `Error` require `Sendable`, so a type implementing one must be (E0475 at its `impl`), and a value converted to one must be: a function value only when the call site shows which function it is. `impl Sendable`, `with Sendable` and `auto interface` outside the prelude are E0476.
- **Channels** (milestone 200 for the two ends): `Chan<T>.new(n)` makes a channel and gives its two ends, `(Sender<T>, Receiver<T>)`. `T` is written, or comes from the expected type (E0412 without either). `Chan` is only the maker: as a type it is E0305, and `Chan.f` for any other `f` is E0415.

| Call | Result |
| --- | --- |
| `Chan<T>.new(n: i64): (Sender<T>, Receiver<T>)` | a channel holding up to `n` values; with `0` each `send` waits until a receiver takes the value; `n < 0` panics |
| `Chan<T>.unbounded(): (Sender<T>, Receiver<T>)` | a channel with no limit: `send` never waits for room |
| `Ring<T>.new(n: i64): (RingSender<T>, Receiver<T>)` | a ring of `n` fixed slots (milestone 218): its writing end never locks, allocates or waits, on any thread; `n < 1` panics |
| `ring.try_send(v: T): TrySend<T>` | on a `RingSender`: `.Sent`, or `.Full(v)` or `.Closed(v)`, giving the value back; one writer at a time (a second `try_send` overlapping panics with `a ring has one writer at a time`) |
| `tx.send(v: T)` | waits for a receiver or room, then hands over a copy of `v`; panics when the channel is closed |
| `tx.try_send(v: T): TrySend<T>` | without waiting: `.Sent`, or `.Full(v)` or `.Closed(v)`, giving the value back |
| `tx.close()` | no more sends; waiting and later receivers get `None` after what is buffered, waiting senders panic; closing twice panics |
| `rx.recv(): Option<T>` | waits for a value; `None` once the channel is closed and empty |
| `rx.try_recv(): TryRecv<T>` | without waiting: `.Ready(v)`, `.Empty`, or `.Closed` once closed and empty |
| `for x of rx { … }` | each value received, until the channel is closed and empty |
| `s.merge(rs: Receiver<T>[]): Receiver<T>` | one receiver fed by each of `rs`, through tasks of the scope `s`; it closes once every one of `rs` is closed and drained (`T` sendable) |
| `a.is(b): bool` | on either end: whether both are the same channel |

Only a `Sender` closes, sends and tries to send; only a `Receiver` receives (E0415 on the other end, with help). A `RingSender` (milestone 218) closes and tries to send, never sends (E0415, with help): a ring's reader is an ordinary `Receiver`, in `recv`, `for`, `select` and `merge`, and gets what is left after `close`, then `None`. A ring is for a callback (section 8), C's own threads included, handing values to tasks without ever waiting; between tasks, a channel. Nothing closes a channel by itself. `TrySend` and `TryRecv` are prelude enums.

- **Plain `self`:** like a box's, these methods change the task or channel, not the handle, so none needs `let mut`.
- **No `==`:** tasks and channels' ends are not `Eq` (E0404, E0419); `a.is(b)` compares channels. They print as `<task>` and `<chan>`.
- **`s.spawn` of a call that changes its receiver** — `push`, `pop` or `clear` on an array, a `mut self` method, `swap` — is E0438: the new task gets its own copy (as a closure does), so nobody would see the change.
- **`select`** is an expression: each arm's value agrees with the first arm that has one, as in `match`. A `recv` arm's name is the `Option<T>` received, in scope in that arm only. A `select` needs at least one `recv` or `send` arm, has at most one `after` and at most one `default`, and never both (E0438); a `recv` arm is on a `Receiver` and a `send` arm on a `Sender` (E0438 for something else, E0415 for the other end); `after` takes an `i64` of milliseconds.

**Actors** (v17, milestone 92). An actor is state that one green thread owns, and the methods that run there:

```
actor Counter {
    count: i64,
    fun new(start: i64): Self = Self { count: start };
    fun add(mut self, n: i64) { self.count += n; }
    fun get(self): i64 = self.count;
}
```

- **The state** is a struct of the fields; inside the methods `self` is the state and `self.helper()` an ordinary call. The state exists only there and where it is spawned: a literal of it, or a call returning it, anywhere else is E0439.
- **The handle:** outside the actor's body, the actor's name (`Counter`, or `m.Counter` from another module) is the type of a handle. `s.spawn(Counter { count: 0 })` and `s.spawn(Counter.new(10))` (v18, milestone 201) start an actor in the scope `s` and give one; `spawn` without a scope is E0474. Handles are first-class: unlike a `Scope` or a `Task` they may be stored, sent and returned. Copies of a handle reach the same actor; handles aren't `Eq` (E0404, E0419; `a.is(b)` compares them) and print as `<actor Counter>`.
- **Messages:** a call of a method with a receiver on a handle is a message. A method with a result makes the caller wait for it (a copy); one without is queued, and the caller goes on. A `mut self` method needs no `let mut` handle. Associated functions aren't messages (E0415). The method's visibility applies as for any method.
- **Built-in:** every handle has `stop()` (a message like the others: what is queued before it still runs, then the actor ends) and `is(other)`. An actor may not declare methods with these names (E0439). `join` is gone (milestone 201; E0415): the actor's scope reports how it ended.
- **Its scope** (milestone 201) stops the actor once the scope's body and its other tasks are done (a `stop` after the queued messages), and waits for it to end. Leaving the scope early, its failure, its deadline or Ctrl-C cancel the actor with the tasks: idle, it ends at once; in a handler, it unwinds there as a task does. An actor's panic fails its scope as a task's does (`Failure.task` the actor's type), or under `independent` is reported. Actors don't count toward `limit n`.
- **The mailbox** (milestone 201) holds 1,024 messages, or the declaration's `mailbox n` (a positive integer literal; E0439 otherwise). A call or a cast to a full mailbox waits for room, a pause point; `stop` is queued past the limit.
- **Private fields:** reading or assigning an actor's field through a handle is E0439.
- **Sharing nothing:** an actor's fields, and its methods' parameters and results, may not hold a `Ref`: a `Ref`, a function value (a closure may have captured one), an interface value (the value inside may hold one), or a type containing any of these is E0439. Channels, tasks and other handles may cross. Values crossing are copied as arguments are.
- **Generic actors** (v18, milestone 202): an actor may have type parameters with bounds, as a struct may, each bounded `Sendable` (E0475 otherwise: everything it holds and exchanges crosses to its task): `actor Cache<K: Key + Sendable, V: Sendable> { … }`. Its handle's type carries the arguments of the state it was spawned from, `Cache<string, i64>`, and each instance is its own type with its own messages. Its methods can't have type parameters of their own (E0439).
- **Actors as interface values** (v18, milestone 202): `impl I for Users { … }`, for an actor, makes its handle implement `I`: the block's methods run inside the actor, on its state, as messages, under the rules above (no `mut` parameter, sendable values), and may be called on the handle directly. A handle converts to `I` where an `I` is expected and meets `T: I` bounds; a call through the `I` value is a message, waiting for a result or queued. `I`'s defaults run inside the actor too, as one message. A generic actor's block is written as any generic type's: `impl<T: Sendable> Source<T> for Queue<T> { … }`.

**Extern structs** (v16, milestone 90). `extern struct Name { … }` declares a struct with C's layout: its fields in order, each at C's alignment, the whole padded to its largest. Its fields are numbers, `bool` (a C `bool`), `char` (a `uint32_t`), handles, `Option` of a handle (C's pointer, `NULL` for `None`; v17, milestone 106) and other extern structs; nothing else, no type parameters and at least one field (E0437). In every other way it is an ordinary struct: literals, fields, `==`, methods, printing, copies. Only extern structs may cross to C by value (section 8).

**Handles** (v16, milestone 88). `extern type Name;` declares a handle type: an opaque pointer to a C object (`GtkWidget *`), with the usual visibility (`public extern type GtkWidget;`). Each declaration is its own type, so a `GtkWidget` is never a `GtkApplication`.
- **Only C makes them:** an extern's result, or a callback's argument. There is no literal and no constructor (`W {}` is E0305), so a handle always comes from C.
- **Anywhere a type goes:** locals, parameters, fields, arrays, `Option`, type arguments. Copying one copies the pointer.
- **Equality only:** `==` and `!=` compare addresses, so handles are `Eq`. No order, arithmetic, casts, fields or built-in methods (E0404, E0408, E0410, E0415); an `impl` block may give a handle methods of its own, which usually wrap externs.
- **Printing** shows the type, never the address: `<GtkWidget>`, unless its `impl` has a `show`.
- **`NULL`:** an extern or callback using `Option<H>` receives `None` for C's `NULL` and passes `NULL` for `None`. A plain `H` from C is assumed not to be `NULL`.
- **Pinned** (v18, milestone 214): `extern type Window pinned;` declares a handle its C library allows only on the thread that made it (`pinned` is a keyword only there). A pinned handle is never `Sendable`, so it never reaches another task (E0475). A task that calls an extern taking or giving one is pinned from that call on: it runs only on its current worker thread until it ends, however often it waits. A `blocking` extern can't take or give one (E0480).
- **Printing:** `Ref(<contents>)`. A box already being printed further out prints as `Ref(…)`, so a cycle (a node whose `next` leads back to it) still prints in finite space.

**`match`.** `match e { pattern => value, … }` is an expression. A pattern is `_`, a binding, an integer, `bool` or string literal, or a variant: its name after a dot, without the enum's name, since the matched value's type gives it. A variant's payload slots are patterns too, so they nest: `.Circle(r)`, `.Rect(w, _)`, `.Empty`, `.Some(.Circle(r))`, `.Pair(1, .Some("a"))`. A bare name is always a new binding that matches anything, at any depth (v18): `.Some(x)`, or `other` for a whole arm. A bare name that is a variant of the type it would bind (`None` against an `Option`, `Empty` in a `Shape` slot) is E0468, since it would match anything instead of testing; a binding named like another type's variant is fine. Variant patterns need an enum or `Option` value and literal patterns a value of their type, at any depth (E0403); a variant needs one slot per payload (E0416). An arm may have a guard, `pattern if cond => value`: the guard runs after the pattern matches, with its bindings in scope, and must be `bool` (E0403); if it is false, matching continues with the next arm. The arms' values follow the rules of `if`/`else`: they must agree (E0403), and an arm that never finishes fits any type.

- **Exhaustiveness.** A `match` must cover every value, counting only arms without guards. Otherwise it is E0417, which lists up to three missing cases as patterns (`.Some(.Rect(_, _))`, `.None`) and then "and N more"; a missing variant with any payloads shows as its name alone at the top level (`.Rect`). A `bool` is covered by `true` and `false`. Integers and strings, at the top level or inside a variant, always need a `_` or a binding.
- **Type patterns** (v5). `name: T` matches when an interface value holds a `T`, and binds the value inside, as a `T`, to `name` (immutable, and copied when stored, like any `match` binding); `_: T` matches without binding. Guards and nesting work as for every pattern: `c: Circle if c.r > 1.0 =>`, `.Some(c: Circle) =>`. `T` is a concrete type that satisfies the matched interface, written with its type arguments (`Stack<i64>`) and module (`geo.Crate`). A type pattern on a value that isn't an interface value, with an interface or a type parameter as `T`, with a generic `T` written without its arguments, or with a `T` that doesn't satisfy the interface (so the arm could never match) is E0426. The set of types is open, so a `match` on an interface value needs a `_` arm (E0417, with `_` as the missing case); a later arm that an earlier unguarded one covers is W0102. In a `match` scrutinee, `{ name:` begins the arms, never a struct literal.
- **Unreachable arms.** An arm that no value can reach past the earlier unguarded arms is warning W0102: after `_`, after the same pattern, or after arms that together cover it (`.Some(_)` and `.None` before `.Some(.Empty)`), and after a top-level binding.
- **Bindings.** Payload names are immutable and borrowed, like `for … of` variables: storing or returning one copies it. Each is in scope in its own arm (guard and value) only. A binding that its arm never uses is warning W0103, which also catches a misspelled variant name that became a binding.
- **Or-patterns** (v9). `p | q | …` matches when any alternative does, tried left to right, and may appear wherever a pattern may (`.Some(1 | 2)`); a guard after it covers the whole arm (`.A | .B if g =>`). Every alternative must bind the same names, each at the same type: a name missing from an alternative, or bound at another type, is E0428. For exhaustiveness an or-pattern counts as each of its alternatives (`.Red | .Green` leaves `.Blue` for E0417), and an alternative that earlier arms, or earlier alternatives of the same arm, already cover is W0102.
- **`mut` bindings** (v9). `mut name` (or `mut name: T` in a type pattern) binds a mutable local holding the arm's own copy of what it matched, made at the copy sites, so assigning to it or calling `mut self` methods on it never changes the scrutinee: `.Some(mut n) => { n += 1; … }`. At the top of an arm, `mut x` binds the whole scrutinee. `mut` before a name that is a variant of the matched type (`mut None`) is E0468, as without `mut`. Bindings without `mut` stay read-only (E0501).

**Arrays.** `T[]` has a length, read with `.len` (type `i64`), that changes only through the built-in methods below. Elements are read and written with `xs[i]`, where `i` is of any integer type (v14; before, `i64` only), and an unannotated literal index is an `i64`. Every index is bounds-checked as its value: below 0, or at least `.len`, panics (`the length is 3 but the index is -1`); a `u64` above `i64.MAX` is out of bounds too. Arrays are created two ways:

- List literal `[a, b, c]`. All elements must have the same type. An empty literal `[]` needs an expected type from context, such as `let xs: i64[] = [];`.
- Repeat literal `[value; count]`, where `count` is a runtime `i64`. Every element is a copy of `value`. `value` is evaluated once, before `count`. A negative `count` panics with `negative array length`; a `count` of 0 gives an empty array.

**Joining arrays** (v17, milestone 122). `xs + ys`, on two arrays of exactly the same type `T[]`, is a new `T[]`: `xs`'s elements, then `ys`'s, each copied as any value is; neither operand changes, and `xs` is evaluated before `ys`. Arrays of different types, or an array and anything else, are E0404 (`[1] + ["a"]`, `xs + 1`); a list or repeat literal that can't be typed alone takes the other side's type (`xs + []`, `[] + xs`, `bytes + [0]` with `bytes: u8[]`). `xs += ys` is `xs = xs + ys`, with the place rules of every assignment. `+` is the only arithmetic operator on arrays. It is the prelude's `$concat` (section 9), so `[1] + [2]` costs a copy of both sides.

**Array copies.** The compiler copies an array only when it is read from an existing place and stored into another one. Fresh arrays and function arguments are never copied. A function that *keeps* a parameter — stores it, on its last use, into a local, a field, an element or its result — takes it as its own from a caller that never reads the argument again (its last mention, or `x` inside the value of `x = …`), and a caller that still needs it shares it first, as the callee's store would have (v17, milestone 149): `bag = insert(bag, k)` in a loop copies nothing. Since v15 a copy is *shared until written* (copy-on-write): the copy site only marks the array shared, in constant time, and whichever side next writes to it copies it first. A program can't tell the difference: in the table, "copied" means the new place behaves as its own array.

| Situation | Copied? | Why |
| --- | --- | --- |
| `let ys = xs;` or `ys = xs;` | Yes | `ys` gets its own array |
| A place used as a struct-literal field or array-literal element (`Wrap { data: xs }`) | Yes | The new struct or array gets its own copy |
| `return p;` where `p` is a parameter, a `for … of` variable, or part of one | Yes | Otherwise the caller's argument, or the iterated array, would be shared |
| `return xs;` where `xs` is a local or part of one | No | The local is about to go away, so the array simply moves |
| Passing an argument, `f(xs)` | No | Parameters are immutable, and nothing else runs during the call |
| Calling a method, `x.m()` | No | `self` is passed like any argument; a `mut self` method edits `x` itself, in place |
| `swap(a, b)` | No | The two places exchange their contents (section 5) |
| A `Ref` (v16) | Never | The handle is copied; every copy shares the box |
| A fresh value: literal, call result, string concatenation | No | Nothing else refers to it |

A function's tail value follows the same rules as `return`: a body ending in `p` copies, and one ending in `xs` moves. "From a place" looks through `if` and block tails: `let ys = if c { xs } else { [0] };` copies. When the value comes from a local that is never used again, the compiler may move it instead of copying; no program can tell the difference.

Copies are deep. Copying an `i64[][]` copies every inner array, and copying a struct copies the arrays inside it. Strings inside are shared, since they are immutable. With copy-on-write (v15) a deep copy happens a level at a time: writing `g[0][1]` after `let h = g;` copies `g`'s outer array and the one inner array written, and leaves the other inner arrays shared.

**Strings.** Immutable. `.len` gives the length in bytes as `i64`. `s[i]` reads one byte as `u8`, bounds-checked, with `i` of any integer type (v14). `s.to_bytes()` (v14) gives the bytes as a new `u8[]`. `+` concatenates two strings into a new one, and `s += t` is `s = s + t` (v17, milestone 122). `==` and `!=` compare contents. `<`, `<=`, `>` and `>=` compare bytewise: the first differing byte decides, compared as an unsigned value, and a proper prefix is smaller (`"ab" < "abc"`). This is the order of C's `memcmp` and needs no Unicode tables.

**Inference.** Local inference only.

1. A `let` without an annotation takes the type of its initializer.
2. Function parameters, return types and struct fields are always annotated.
3. Numeric literals use the expected type when there is one: the variable's annotation, the parameter type at a call, the declared return type, the other operand of a binary operator, the other bound of a range, or the target of an assignment.
4. With no expected type, integer literals are `i64` and float literals are `f64`.
5. An integer literal that doesn't fit its type is a compile error. An integer literal never becomes a float, or vice versa.
6. A unary `-` applied directly to a numeric literal is folded into the literal before the range check, so `-2147483648` is a valid `i32` and `-9223372036854775808` a valid `i64`. A negated `u8`, `u32` or `u64` literal is an error (E0402), and so is `-x` on a value of an unsigned type (E0404). A float literal takes `f32` where `f32` is expected (v7), and is then the `f32` nearest its value.

```
let a = 5;            // i64
let b: i32 = 5;       // i32
let c = b + 1;        // i32: 1 takes its type from b
let d = 2.5;          // f64
let e: f64 = 5;       // error: integer literal where f64 expected
```

**Operator typing**

| Operators | Operands | Result |
| --- | --- | --- |
| `+` `-` `*` `/` `%` | Two numbers of the same type | That type |
| `+` | Two strings | `string` |
| `<` `<=` `>` `>=` | Two numbers of the same type, or two strings | `bool` |
| `==` `!=` | Two values of the same type | `bool` |
| `&&` `\|\|` | Two `bool` | `bool` |
| `!` | `bool` | `bool` |
| unary `-` | `i32`, `i64`, `f32` or `f64` | Same type |
| `&` `\|` `^` | Two integers of the same type (v7) | That type |
| `<<` `>>` | An integer, and an amount of the same type (v7) | That type |
| `~` | An integer (v7) | Same type |

**Equality on every type.** `==` and `!=` compare deeply:
- **Primitives and strings:** as before.
- **Structs:** field by field.
- **Enums:** same variant, then payload by payload.
- **Arrays:** same `.len`, then element by element.
- **Option:** `None == None`, and `Some(a) == Some(b)` when `a == b`.

Equality costs time proportional to the values compared. `f64` keeps IEEE semantics, so a `NaN` anywhere makes the values unequal, even a value compared with itself.

**`Eq` and `Key`** (v17, milestone 144; v18, milestone 167). Equality is the prelude's derivable interface `Eq { fun eq(self, other: Self): bool; }`, written `T: Eq` as before and still a reserved name; `Key`, which extends it, `interface Key: Eq { fun hash(self, mut h: Hasher); }`, is what map keys and set values need. Numbers, `bool`, `char`, `string` and handles compare inline; on any other type `a == b` is `a.eq(b)` and `a != b` is `!a.eq(b)`. A struct or enum is `Eq` or `Key` only when it asks, `with Eq` or `with Key` (`with Key` implies `with Eq`), getting both from the prelude's blocks: fields compared in order, hashed in the same order, so equal values hash equal. Or it implements them by hand, `impl Eq for T { … }` and `impl Key for T { … }`, used for `==`, `!=`, maps and sets wherever the type appears, a field of another type included; a type implementing `Key` must be `Eq` too (E0422), and a type's own methods named `eq` or `hash` don't count. The prelude's types are built in: numbers, `bool`, `char` and `string` are `Eq` and `Key`; `f32` and `f64` are `Eq`, by IEEE rules (`NaN != NaN`), but never `Key`; arrays, tuples, `Option`, `Result`, `Map`, `Set` and `()` are `Eq` or `Key` when their parts are; extern handles and `CPtr` are both, by address; `Ref`, tasks, channels, actor handles, functions and interface values are neither. `==` on a type that isn't `Eq` is E0422 when it didn't ask, and E0451 when a part isn't, naming the path. **`Hasher`** is the prelude's running hash: `Hasher.new()`, `h.add(x)` for any `x: Key`, and `h.finish(): u64`: SipHash-1-3 (v18, milestone 204), keyed by a secret the runtime draws for each process, so nobody can choose keys that collide. A hash value differs between runs (fixed in a deterministic test run, section 9): never store or send one. Nothing else depends on the key, since maps keep insertion order and compare regardless of order, so it can't change a program's results. `assert_key_laws(a, b)` panics unless `a` equals itself and, when `a == b`, both hash the same.

**Operand types.** In `==`, `!=` and the comparisons, an operand that can't be typed on its own (a number literal, `None`, `[]`, or a list or `Some` around one, and (v17, milestone 96) a call of a generic function whose type parameters appear only in its result, `text<M>(s): View<M>`) takes its type from the other operand, whichever side it is on. `assert_eq` and `assert_ne` (v16) type their two operands the same way. So `o == None`, `None == o` and `xs == []` work. If neither side can be typed alone (`None == None`), it is E0412. A number literal never takes a non-numeric type this way: `"a" == 1` is E0404.

**Characters** (v14). A `char` literal is one character between single quotes, `'a'`, `'é'`, `'😀'`, or an escape: `'\n'`, `'\t'`, `'\r'`, `'\0'`, `'\\'`, `'\''`, `'\"'`, `'\u{1F600}'` (one to six hex digits naming a scalar value). Anything else is E0112.
- **Comparison:** `==` and `<` compare code points, so `char` is `Ord`. There is no arithmetic (E0404).
- **Casts:** `c as u32` (or `i32`, `i64`, `u64`) gives its code point, and `b as char` gives the character of a byte `b: u8` (v18; narrower integer types and other integers are E0469). `n.try_into<char>()` and `char_from(n: u32)` give an `Option<char>`. `char` casts to nothing else (E0408).
- **Strings:** `s.chars()` iterates a string's characters (a `Chars` iterator), decoding UTF-8; each invalid sequence gives U+FFFD and moves one byte on, so it never fails. `s.char_count()` counts them. `for b of s` still loops over bytes.
- **Methods:** `c.is_alphabetic()`, `is_digit()` (`0`–`9`), `is_whitespace()`, `is_upper()`, `is_lower()`, `to_upper()` and `to_lower()` cover ASCII and Latin-1 (U+0000–U+00FF) only: other characters are neither letters nor cased, and `ß`, `ÿ` and `µ` keep their case.
- **Names:** `char_from` and `Chars` are the prelude's, visible like `range` unless a module declares its own.

**Casts.** `expr as T` converts between numbers only where nothing can be lost (v18): a type to itself; an integer to an integer whose range holds the source's (`u8 as i16`, `i32 as i64`, never signed to unsigned); an integer to a float that holds every value exactly (`i8`, `i16`, `u8`, `u16` to `f32`, any integer up to 32 bits to `f64`); `f32 as f64`; and a `char` to `u32`, `i32`, `i64` or `u64`, or a `u8 as char`. A number literal takes the target type when it fits (`0 as u64`; `300 as u8` is E0402), and an integer literal that a float holds exactly becomes one (`1 as f64`). A type parameter bounded by `Num` or `Float` casts when the cast is lossless at every type its bound admits (`x as f64` for `T: Float`); an unbounded `T` is E0408. Any other numeric cast is E0469, whose help names the method to write: a conversion that can lose information names the loss.
- **`try_into<U>()`**: `Option<U>`, `Some` exactly when `U` holds the value exactly (a float must be whole and in range for an integer; an integer must be representable in a float; NaN stays NaN between floats). From any integer, `try_into<char>()` is `Some` for a Unicode scalar value.
- **`truncate<U>()`**: integers to integers only; keeps the low bits, two's complement (`300.truncate<u8>()` is 44, `(-1).truncate<u8>()` 255).
- **`saturate<U>()`**: the nearest value of `U` within its range. An integer clamps (`300.saturate<u8>()` is 255); a float to an integer rounds toward zero, clamps, and NaN becomes 0; to a float, the nearest value, finite values clamped to the finite range.
- **`round<U>()`**, **`floor<U>()`**, **`ceil<U>()`**: round the named way, then convert; `round` takes halves away from zero (`2.5.round<i64>()` is 3, `(-2.5).round<i64>()` −3). A result the target can't hold, or NaN to an integer, panics with `conversion out of range` at the method; `saturate` is the clamping form. `round` also gives an integer's nearest float (`n.round<f64>()`, how an `i64` becomes an `f64`), `f64`'s nearest `f32`, and between integers is exact or panics. `floor` and `ceil` take a float.
- Each takes its target as a type argument, or from the expected type (`let b: u8 = n.truncate();`; E0412 when neither gives it), works on every number and on type parameters bounded by `Num` or `Float` at their instances, and is E0408 on a pair it doesn't convert (`truncate` from a float, `floor` from an integer). `bool` cannot be converted; write `if b { 1 } else { 0 }`.

**Names of types.** A local — a `let`, a parameter, a `for` variable or a `match` binding — may not be named like a struct, an enum or `Option` (E0311). So a name before `.` is always unambiguous: a type, meaning a variant or associated function follows, or a local, meaning a field or method follows.

**Methods.** An `impl T { … }` block attaches functions to a struct or enum `T` declared anywhere in the file; a type may have several `impl` blocks, and no `impl` may target a built-in type (E0305). Method names are unique per type (E0310). Two members of a type may share a name only when every use shows which one is meant (v17, milestone 123): a method may be named like a field (`p.name` is the field, `p.name()` the method) unless the field's declared type is a function type, since `q.area(5)` could then call either; and a method with `self` may be named like a variant (`Shape.Circle(1.0)`, and `Shape.Circle` as a value, are always the variant; `s.Circle()` the method), but an associated function may not. The pairs that can't be told apart are E0310.

- **`self` methods.** A method whose first parameter is `self` is called on a value: `v.len_sq()`. `self` is an immutable, borrowed parameter of type `T`: storing or returning it copies it, like any parameter.
- **`mut self` methods.** A method whose first parameter is `mut self` changes the caller's value in place: `v.scale(2.0)`. The receiver must be a place whose root is `let mut` (E0501), not inside a string (E0506), and not an array being iterated (E0507); it may be a variable, a field or an element (`r.min.shift(1.0)`, `ps[1].scale(2.0)`). Inside, `self`, its fields and its elements can be assigned, and `self` can be replaced whole.
- **Associated functions.** A function in an `impl` without `self` is called on the type: `Vec2.new(1.0, 2.0)`.
- **No aliasing.** An argument of a `mut self` call may not be the receiver's own storage (E0508): a place rooted at the receiver's root variable whose type is passed by pointer (a struct, enum, `Option`, array, interface value or type parameter), written directly or as the tail of an `if`, block or `match`. In `v.add(v)` the method would see its own edits through `o`; `v.add(v.inner)`, `g.set(g.values)` and `v.add(if c { v } else { w })` are rejected the same way. Copy first: `let w = v; v.add(w);`. Any other mention is allowed (v15): calls on the receiver, `g.tanh(g.add(a, b))`, whose results are fresh values; operators; and reads of numbers, `bool`, `char` and strings, `g.scale(x, g.lr)`, which are passed by value.

Calling a method that doesn't exist, a `self` method on a type or an associated function on a value is E0415. Strings and `Option` have no methods; arrays have only the built-ins below.

**Growable arrays.** Every array has three built-in methods:

| Method | Effect |
| --- | --- |
| `xs.push(v)` | Appends `v`; `xs.len` grows by one |
| `xs.pop()` | Removes and returns the last element as `Some(x)`, or `None` if `xs` is empty |
| `xs.clear()` | Removes every element; `xs.len` becomes 0 |

They follow the `mut self` rules: `xs` must be a place whose root is `let mut` (E0501), not inside a string (E0506), and not an array being iterated by `for … of` (E0507). `push` stores its value like an assignment does — copied from a place, moved on last use — and reads it before the array grows, so `xs.push(xs[0])` simply appends a copy of the first element (E0508 doesn't apply). `pop` moves the element out. Indexing is still bounds-checked against the current `.len`, and copying an array copies its current elements.

**`Result`** (v4). `Result<T, E>` is predeclared in every module as `enum Result<T, E> { Ok(T), Err(E) }`, an ordinary generic enum.
- **Construction:** `Ok(x)` and `Err(e)` are written without a prefix, like `Some`. The type argument the payload doesn't give comes from the expected type (`let r: Result<i64, string> = Ok(1);`); `Ok(1)` alone is E0412.
- **Everything else** is as for any enum: `match` with `Ok(x)` and `Err(e)` patterns (exhaustiveness, E0417), `==` when `T` and `E` are `Eq`, copies and layout.
- **Names:** `Result`, `Ok` and `Err` are built-in names: declaring one is E0302, and a local named `Result` is E0311. A local named `Ok` or `Err` shadows the constructor, as one named `Some` does. Messages print the type as `Result<i64, string>`.

**A `Result` isn't dropped silently** (v18, milestone 175). An expression statement whose type is `Result`, at any arguments (a call, a method call, an `if` or `match` used as a statement), is E0467: handle it with `?` or `match`, or drop it on purpose with `let _ = e;`. A block's tail is its value, not a statement, and only `Result` is checked (an `Option` or a struct holding a `Result` may be dropped); the types are those written, so a `T` some instance makes a `Result` isn't checked.

**`Error`** (v18, milestone 179). The prelude's `interface Error: Display {}` is what a program's errors share: met only by `impl Error for T {}` (with `impl Display for T`), visible in every module unless a module declares its own `Error`. `Result<T, Error>` holds any error, `?` converts a concrete one into it (and an `Error`-extending interface value too), and `"{e}"`, `to_string(e)` and `e.show()` show it through its type's `Display`; a type pattern (`p: ParseError`) gets the concrete error back. `string` implements it, so `Result<_, string>` converts with `?`, as do std's error types: `json.JsonError`, `json.DecodeError`, `toml.TomlError`, `time.TimeError` and `crypto.CryptoError`, each shown as its message.

**The `?` operator** (v4). `e?` evaluates `e` once.
- **On `Result<T, E>`:** its type is `T`. If `e` is `Err(x)`, the enclosing function returns `Err(x)` at once. The function must return `Result<U, E>` with the same `E`, or `Result<U, F>` where `F` is an interface that `E`, a concrete type, satisfies, or an interface that `E`, an interface type, converts to (v6, section 4): then the error is converted into an `F` (v5).
- **On `Option<T>`:** its type is `T`. If `e` is `None`, the function returns `None`. The function must return an `Option`.
- **In a closure**, it returns from the closure, whose return type must be written or expected (E0412 otherwise, as for `return`).
- **Inference** (v17, milestone 140): where `e?` is expected to be a `T`, `e` is expected to be a `Result<T, _>` (in a function returning a `Result`; the error type is left open, since it may convert) or an `Option<T>`, so a generic call's type arguments can come from it: `let p: Person = json.decode(v)?;`.
- **Misuse is E0424**, with help: `?` on any other type, in `main`, in a function returning something else, on an `Option` in a function returning `Result`, or with error types that neither match nor convert.
- **Early return:** on failure nothing after the `?` runs, and a value it was part of is never stored, exactly as for `return`.

**String, `Option` and `Result` helpers** (v4). These methods each take `self`, never change it, and return a new value.

| Receiver | Method | Result |
| --- | --- | --- |
| `string` | `s.lines()` | `string[]`: split at `\n`, one trailing `\r` removed from each line, and no empty last line after a final `\n` (`"a\nb\n"` gives `["a", "b"]`, `""` gives `[]`) |
| `string` | `s.split(sep)` | `string[]`: the parts between occurrences of `sep`, empty parts kept (`"a,,b"` gives `["a", "", "b"]`); an empty `sep` panics with `split with an empty separator` |
| `string` | `s.trim()` | `string`: without leading and trailing ASCII whitespace (space, `\t`, `\n`, `\r`) |
| `string` | `s.contains(sub)`, `s.starts_with(p)`, `s.ends_with(p)` | `bool`; true for an empty argument |
| `string` | `s.slice(from, to)` | `string`: the bytes `from..to`; panics (at the call) unless `0 <= from <= to <= s.len` |
| `string` | `s.to_bytes()` (v14) | `u8[]`: a new array of the string's bytes |
| `string` | `s.chars()`, `s.char_count()` (v14) | the characters (`Chars`, yielding `char`) and their count |
| `string[]` | `xs.join(sep)` | `string`: the elements with `sep` between them; `""` for `[]` |
| `Option<T>` | `o.unwrap_or(d)` | `T`: the value, or `d` |
| `Option<T>` | `o.ok_or(e)` | `Result<T, E>`: `Ok` with the value, or `Err(e)` |
| `Option<T>` | `o.map(f)` | `Option<U>` |
| `Option<T>` | `o.is_some()`, `o.is_none()` | `bool` |
| `Option<T>` | `o.and_then(f)` (v17) | `Option<U>` for `f: fun(T): Option<U>`: `f` of the value, or `None` |
| `Result<T, E>` | `r.unwrap_or(d)` | `T`: the `Ok` value, or `d` |
| `Result<T, E>` | `r.ok()` | `Option<T>` |
| `Result<T, E>` | `r.map(f)`, `r.map_err(f)` | `Result<U, E>`, `Result<T, F>` |
| `Result<T, E>` | `r.is_ok()`, `r.is_err()` | `bool` |
| `Result<T, E>` | `r.and_then(f)` (v17) | `Result<U, E>` for `f: fun(T): Result<U, E>`: `f` of the `Ok` value, or the `Err` unchanged |
| `Option<T>`, `Result<T, E>` | `x.unwrap()` (v9) | `T`: the `Some` or `Ok` value; otherwise panics (at the method name) with `unwrap on None`, or `unwrap on Err: ` and the error printed (v12) |
| `Option<T>`, `Result<T, E>` | `x.expect(msg)` (v9) | `T`, as `unwrap`, but the panic's message is `msg`, a `string` evaluated even when it isn't needed |

Strings are bytes: every position is a byte offset, and `slice` can cut a multi-byte character, giving a string of exactly those bytes. A closure argument is typed as for the array helpers (`o.map(fun(x) = "{x}")` is an `Option<string>`). `map` means the array, `Option` or `Result` helper according to the receiver. `parse_i64(s).ok_or("bad number")?` turns a missing value into an error and returns it. There is no `unwrap()`: handle a failure with `match`, `?` or `unwrap_or`.

**Array helpers** (v3). Every array `xs: T[]` also has these methods. Each takes `self`: the receiver never changes, and every array it returns is new.

| Method | Arguments | Result |
| --- | --- | --- |
| `xs.map(f)` | `f: fun(T): U` | `U[]`: `f` applied to each element, in order |
| `xs.filter(f)` | `f: fun(T): bool` | `T[]`: the elements for which `f` is true, in order |
| `xs.fold(init, f)` | `init: A`, `f: fun(A, T): A` | `A`: `f(…f(f(init, xs[0]), xs[1])…)`, left to right |
| `xs.sort_by(less)` | `less: fun(T, T): bool` | `T[]`: sorted so that `less` holds between neighbours or neither way, stably (equal elements keep their order) |
| `xs.sort()` (v15) | — | `T[]` for a `T: Ord`: ascending, stably, through `<` (E0422 or E0451 otherwise; floats sort with `xs.sort_by(f64.total_order)`) |
| `xs.any(f)` / `xs.all(f)` | `f: fun(T): bool` | `bool`, stopping at the first element that decides |

`T` comes from the receiver, and the other type arguments (`U`, `A`) are inferred as for any generic call, so a closure argument needs no written types: `xs.map(fun(x) = "{x}")` is a `string[]`, its result type coming from the closure's body. Arity and argument types are checked at the call (E0405, E0403). `f` runs once per element it is given, in order; a panic inside it stops the program as usual. If `less` is inconsistent, `sort_by` still returns every element exactly once, in some order. The helpers chain: `people.filter(fun(p) = p.age >= 18).map(fun(p) = p.name)`. Functions named `map`, `filter` and so on in a program are unaffected: the helpers exist only as array methods.

**Generic declarations.** `fun`, `struct`, `enum` and methods can take type parameters.
- **Scope:** a type parameter is a type inside its declaration (fields, payloads, signature and body), and a method also sees its `impl`'s parameters. It may not be named like a type, a module or another parameter of the same declaration, nor a method's like its `impl`'s (E0316).
- **Bounds:** `T: Eq`, `T: Ord`, interfaces (below), or several (`T: Shape + Eq`). Anything else is E0315. `Ord` extends `Eq` (v18): `T: Ord` allows `==` too.
- **`Float`** (v15, milestone 53): a prelude bound satisfied by `f32` and `f64` only (E0419 otherwise, with help to write `2.0` or convert), allowing everything `Num` does. `std.math`'s float functions take a `T: Float`.
- **Operator bounds** (v15; generic since v18, milestone 171): the prelude's interfaces `Add<R = Self>`, `Sub<R = Self>`, `Mul<R = Self>`, `Div<R = Self>`, `Rem<R = Self>` (each `fun op(self, other: R): Self;`), `Neg` (`fun neg(self): Self;`) and `Num` (all six at `Self`). `T: Mul` (that is, `Mul<T>`) allows `a * b` with a `T` on the right, `T: Mul<f64>` `a * 2.0`, and so on, each giving a `T`; `T: Num` allows every arithmetic operator, `==` and `<`, and any integer literal as a value of `T` (`let mut s: T = 0;`, `x * 2`; v15, milestone 58), as `T: Float` also allows any float literal (`x * 0.5`); a float literal where `T` is only `Num` is E0401. Each instance gives the literal its own type, and a literal that doesn't fit one is E0402 at the literal, naming the instance (`300` in `big<u8>`). The numbers satisfy all of them at themselves (`i64: Mul<i64>`) as built-ins, and `string` and every array type satisfy `Add` (arrays: v17, milestone 122); other types satisfy them only through their blocks (section 5), a block keeping the bound's promise only when its result is the type itself (E0422 otherwise, saying what it gives), and only the numbers satisfy `Num` (E0419). An interface whose own item is named `mul` gives no operator. They are visible in every module unless the module declares the name, like `Iter`. Through a bound, each instance uses the built-in operator at a number, string or array type, panics included, and calls the type's block otherwise.
- **What a bare `T` allows:** being stored, moved, copied, passed, returned and put in arrays, structs and enums. `==` needs `T: Eq`, `<` needs `T: Ord`, and a method call needs an interface bound that declares it. Using `==` or `<` without its bound is E0404, with help suggesting the bound.
- **What `T` never allows:** arithmetic without an operator bound (v15), casts, `.len`, or indexing. Those need a concrete type or a bound. A `T` prints (v12), each instance by its concrete type's rules.
- **Checked once:** a generic body is checked once, with its parameters abstract, so every instance compiles.

**Using a generic.**
- **Type arguments:** a generic type is written with all its arguments in type positions: `Stack<i64>`, `Option<Vec2>`, `Pair<string, f64>`. A wrong count, or a bare `Stack` in a type position, is E0418.
- **Written type arguments** (v18): optional, at generic calls (`pick<string>([])`), function values (`bigger<i64>`), a method's own parameters (`xs.map<string>(f)`), associated functions and built-in constructors of generic types (`Stack<i64>.new()`, `Chan<Job>.new(4)`, `Ref<f32>.new(1.5)`), struct literals (`Pair<u8> { a: 1, b: 2 }`) and variants (`Shape<f64>.Circle(r)`, and through `Option`'s name, `Option<i64>.None`, `Option<u8>.Some(7)`). They bind their parameters before inference runs; `_` leaves one to inference (`conv<_, u8>(x, 3)`). The list gives every parameter or is E0418 (`` `two` takes 2 type arguments but 1 was given ``), as is a list on something without type parameters (a plain function, a local, a field); an argument that misses its bound is E0419 (E0451 for a derivable interface), and one the code contradicts is E0403 at the argument it contradicts.
- **Inference:** at a call, a struct literal, a variant or an associated function, the type arguments not written are inferred. They come first from the expected type, then from the arguments that can be typed alone, then from the ones that can't (number literals, `None`, `[]`, and (v17) calls only their result can type, associated functions included (`Cmd.none()` for `fun none(): Cmd<M>` in `impl<M> Cmd<M>`) — a call whose result's type parameter only such arguments could fix, `column([text("a")])`, a list or `Some` of them, and a tuple holding one, `("a", text("a"))`, count too), so `max(x, 3)` with `x: u8` makes `3` a `u8`, a struct literal's fields take their types from each other in any order (`App { init: (0, Cmd.none()), update: step }`), and `pair(text("a"), v)` with `v: View<Msg>` makes `text`'s `M` a `Msg`. A method call whose receiver is such a call, when the method's result mentions the receiver type's own parameters, waits too, and passes its expected type to the receiver: the parameters the expected type fixes in the result give the receiver's type, the rest left open (v17). When the method returns the receiver's own type (`fun pad(self, n: i64): Node<M>` in `impl<M> Node<M>`): `fun a(): Node<Msg> = leaf("x").pad(8).pad(2);`, `[leaf("x").pad(8)]` where a `Node<Msg>[]` is expected; when it returns one of them (milestone 148), as `unwrap` and `expect` on `Option<T>` and `Result<T, E>` do: `let p: Person = json.decode(v).unwrap();`, `json.decode(v).unwrap() == q`, and with `fun first(self): T` in `impl<T> Pair<T>`, `let n: u8 = zeros().first();`. In a list literal with no expected type, when the first element is such a call, the first element that can be typed alone gives the element type (v17): `[text("a"), button("+", Msg.Inc)]` is a `View<Msg>[]`. A method's `impl` parameters come from its receiver: `s.push(1)` on a `Stack<string>` is E0401.
- **Can't infer:** a parameter still undecided is E0412, with help to annotate: `let s: Stack<i64> = Stack.new();`.
- **Bounds at a use:** a type argument that doesn't satisfy its bound is E0419 (`Num`, `Float`, `Eq` on a parameter or a `Ref`), E0422 (a struct or enum that didn't ask for `Eq` or `Ord`) or E0451 (a part with no `derive` block, named by its path: `largest([[1]])` fails because `i64[]` is not `Ord`). `Ord` holds for the integers, `char`, `string`, `()`, tuples of `Ord` elements, type parameters declared `Ord`, and a struct or enum that asks or implements it, below.
- **`Ord`** (v15, milestone 59; an interface since v18, milestone 168): order is the prelude's derivable interface `interface Ord: Eq { fun cmp(self, other: Self): Ordering; }`, returning the prelude's `enum Ordering { Less, Equal, Greater }`, and still a reserved bound name. `a < b` is `a.cmp(b) == Ordering.Less`, `a <= b` is `!= Greater`, `a > b` is `== Greater` and `a >= b` is `!= Less`. A struct or enum is `Ord` only when it asks, `with Ord` (which implies `with Eq`), getting the prelude's blocks: fields compared in declaration order and variants by declaration order, then payloads, the first unequal part deciding. Or it implements it by hand, `impl Ord for T { fun cmp(self, other: T): Ordering … }`, used for `<`, `sort`, `min`, `max` and tuples wherever the type appears; such a type must be `Eq` too (E0422), and a `cmp` method outside an `impl Ord` block doesn't order it. `<` on a struct or enum that isn't `Ord` is E0422 when it didn't ask, and E0451 when a part isn't, naming the path. The integers, `char`, `string` and `()` are `Ord`, and tuples are when their elements are; `bool`, arrays, `Option`, maps, sets and handles aren't. **Floats** keep `<`, `<=`, `>` and `>=`, by IEEE rules, but aren't `Ord`: `NaN` is neither below nor above anything. `f64.total_order(a, b)` and `f32.total_order(a, b)` are built-in associated functions, `fun(f64, f64): bool`, true when `a` comes before `b` in IEEE 754's total order (`-NaN < -inf < … < -0.0 < 0.0 < … < inf < NaN`); they can be called or passed, `xs.sort_by(f64.total_order)`. A float in an ordered type is E0451, with help naming them. `Ordering`'s variants are written `Ordering.Less`, like any enum's; a module's own `Ordering` means its own, and the prelude's prints as `$prelude.Ordering` in messages.

**Interfaces.**
- **Declaring one:** `interface Shape { fun area(self): f64; fun name(self): string; }` lists methods (with a `self` or `mut self` receiver) and associated functions (without one), each ending in `;`. Their signatures use concrete types, the interface's own type parameters (below) and `Self` (v6, below); an item has no type parameters of its own. An item name appears once (E0310).
- **Extending** (v18): `interface Greeter: Named { … }` extends `Named` (several: `interface I: J, K`). A type implementing `Greeter` must implement `Named` too (E0422 at the `impl` or `with`), and `with Greeter` asks for `Named` as well; a bound `T: Greeter` gives `Named`'s items and meets a bound `T: Named`; a default body sees them through `Self`. It is a type like any interface (v18, milestone 179): a `Greeter` value's table holds the items of every interface it extends, transitively and each once, ancestors first in declaration order, then its own, each filled by its own interface's block or default, so `g.name()` works on a `Greeter` value. An ancestor with an associated function or an item using `Self` keeps it from being a type, as its own would (E0425, E0427). E0464 is retired.
- **Generic interfaces** (v6): `interface Source<T> { fun next(mut self): Option<T>; }` takes type parameters as a struct does, with bounds (`interface Sorted<T: Ord>`); a parameter named like a type, twice, or `Self` is E0316. It is always written with its arguments, `Source<i64>`, except that a parameter may have a default (v18, milestone 171), `interface Add<R = Self>`, which may name `Self` and the parameters before it and which every parameter after it must have too (E0201): a use that leaves trailing arguments out takes their defaults, `Self` standing for the bounded parameter or the block's type (`impl Add for Vec2` is `Add<Vec2>`, `T: Add` is `Add<T>`). A bare `Source` or the wrong number is E0418, and an argument that misses its parameter's bound is E0419. A type satisfies `Source<X>` when it satisfies the interface with `X` substituted, and a call through `S: Source<X>` is checked against the substituted signature. A generic interface is a type wherever a plain one is, written with its arguments (`Source<i64>`, `Source<T>[]`), and its values follow every rule of interface values below at those arguments: a value converts to `Source<i64>` when it satisfies `Source<i64>`, `s.next()` is checked against the substituted signature, and a `Source<i64>` is not a `Source<string>` (E0403).
- **Iterators** (v6; v18, milestone 225): the prelude declares `interface Iter<T> { fun next(mut self): Option<T>; }`, a built-in name (E0302). A type is an iterator only through a block, `impl Iter<i64> for Count { fun next(mut self): Option<i64> { … } }`; a method merely named `next` doesn't count, so no type becomes loopable by accident. `for x of e` loops over any `e` that isn't an array, string, view or channel end and is: a struct, enum or extern type with one `Iter` block (the loop calls the block's `next`, even beside a type's own `next`, which `x.next()` still calls, INT-15); an `Iter<T>` value or a value of an interface extending it (`interface Source<T>: Iter<T>`); or a type parameter bounded by either. The helpers (`map`, `filter`, `collect`, …) take the same. Anything else is E0411, whose help names the missing block when the type has a `next` of the right shape (section 5).
- **`Self`** (v6): in an interface's signatures and default bodies, `Self` is the type that satisfies the interface, and can be written wherever a type can: `other: Self`, `: Self`, `Self[]`, `Option<Self>`. It means that even in a module that declares a type named `Self`; outside interfaces, `impl` blocks (v10) and struct and enum declarations (v13, below), a written `Self` is an unknown type (E0305), and an interface parameter can't be named `Self` (E0316). A type `C` satisfies an item that mentions `Self` when its method has `C` in those places (`Money` meets `fun less(self, other: Self): bool` with `fun less(self, other: Money): bool`; for a generic type, `Self` is it at its arguments, `Stack<i64>`), and a mismatch is E0422, showing the signature with `Self` replaced. Through a bound `T: Ordered`, `Self` is `T`: `a.less(b)` needs `b: T`, `a.max(b)` is a `T`, and an associated `fun zero(): Self` is called as `T.zero()`. An interface any of whose methods mentions `Self` outside its receiver is a bound only: written as a type (`Ordered`, `Ordered[]`, a field) it is E0427, naming the method, because two `Ordered` values could hold different types (E0425 comes first for an interface with an associated function). An interface value satisfies such an interface, as a type argument, when its own items match with `Self` replaced by its interface type.
- **Bounds with arguments** (v6): a bound's arguments may name the declaration's other type parameters, `fun collect<T, S: Source<T>>(src: S): T[]`. A parameter that no argument or expected type fixes is read off the methods of the type a bound parameter has: `collect(Count { … })` gives `S = Count`, then `T = i64` from `Count.next`, and this repeats until nothing new is learnt (`T` from `S`, `S` from `H: Holder<S>`). A parameter still unknown is E0412; one that the arguments fix differently fails the bound (E0422). The same holds for a parameter declared as an interface value: `fun count<T>(s: Source<T>): i64` called as `count(Count { … })` reads `T = i64` off `Count.next`, then converts the argument to `Source<i64>`.
- **`Self` in `impl` blocks** (v10): inside `impl T { … }`, in signatures and bodies, `Self` is `T` as written there, with the `impl`'s parameters: `Point`, or `Stack<T>` in `impl<T> Stack<T>`. It can be written wherever `T` can: as a type (`other: Self`, `: Self`, `Self[]`, `Option<Self>`), as a struct literal's name (`Self { x: 0, y: 0 }`), and before `.` for an associated call or a variant (`Self.origin()`, `Self.Red`). In a struct's field types and an enum's payload types (v13), `Self` is likewise the type being declared, with its parameters: `struct Tree<T> { value: T, kids: Self[] }` means `kids: Tree<T>[]`, and `enum List { Cons(i64, Self), Nil }` is recursive as if `List` were written; a struct holding `Self` directly is E0307, as for its own name. In a function outside these, `Self` is an unknown type (E0305).
- **Default methods** (v5): a method, or (v18) an associated function, may end in `= expr;` or a block instead of `;`. Its body is checked once, with `self` of a hidden type that has only the interface's items, so it can call them on `self` and nothing else (no fields, E0410). `Self` names that hidden type (v6), so a default can take and return it: `fun max(self, other: Self): Self = if self.less(other) { other } else { self };`. An `impl` of the interface may leave a defaulted method out, and then the default is used for that type, or define it, with exactly the item's signature, to replace it. A default is called through an interface value, a bound, a qualified call (`Shape.describe(c)`), or directly on a type that implements the interface (v18, below). An associated function may have a default body too (v18), reached through a bound (`T.pair()`), a qualified call or an associated call on an implementing type, and replaced by an `impl` the same way.
- **Implementing one** (v18): a type implements `Shape` only through a block `impl Shape for Circle { … }`, which defines every item without a default, each with exactly the item's signature, `Self` replaced by the type (`fun area(self): f64`, `fun make(r: f64): Circle`), and may define defaulted ones too. A missing item, one with another signature, a method of the block's own type parameters, or a method the interface doesn't declare is E0422 at the block, its help listing each. Having methods of the right names isn't enough: wherever `T: Shape` is checked, a value converts to `Shape`, or `Shape.area(x)` is called, the type needs the block, else E0422 with help to add it. A generic type writes its parameters as in its own `impl` blocks, bounds included, and the interface's arguments may use them or be concrete: `impl<T> Source<T> for Stack<T> { … }`, `impl<T: Ord> Sorted<T> for Stack<T> { … }`, `impl From<string> for Path { … }`. A generic interface may be implemented once for each argument list (`From<string>` and `From<i64>` for one `Path`); a second block for the same interface and arguments is E0461. The interface may be another module's (`impl shapes.Shape for Circle`) and the type another module's (`impl Shape for geo.Circle`), but the block must be written in the module that declares the interface or the one that declares the type, and a type built from several program types only in the interface's module (below), so no two packages can both implement one interface for one type; elsewhere it is E0459. The type is a struct, an enum, an actor or an extern type here; built-in types are below. A block's methods are as visible as its interface. A type parameter meets an interface when its own bounds declare every item the same way. **Still by name:** derivable interfaces (`json.Json`) and indexing's `index` and `set_index` are met by a type's own methods of the right names and shapes, and naming one in `impl … for` is E0459. Every other built-in behaviour (`Eq`, `Key`, `Ord`, `Display`, `Debug`, `Iter`, the operators, `Error`, the collection literals) is met only through an `impl` (v18, milestone 225 for `Iter`). `Num` and `Float` are met only by the number types.
- **Built-in types** (v18, milestone 169): a block may be for any other type that can hold a value: the numbers, `bool`, `char`, `string`, `()`, arrays, tuples, `Option`, `Result`, `Map` and `Set`, at concrete arguments (`impl Show for f64[]`, `impl Show for Option<Point>`, `impl Show for (string, i64)`) or at parameters written after `impl`, with bounds, every one appearing in the type: `impl<T: Show> Show for T[] { … }`, `impl<A: Show, B: Show> Show for (A, B) { … }`. Not a function type, an interface type, a `Ref`, a task, a channel, an actor or a bare parameter (`impl<T> I for T`) (E0459); a struct's or enum's block names exactly its own parameters, in order (E0214 otherwise). `Self` in the block is the type. Its home is the interface's module, or the module that declares the one program type the block's type is built from (`impl Show for Point[]`, `impl Show for (Point, Point)` in `Point`'s module), else E0459. A type built from two or more program types (`(Point, Line)`, `Result<Point, Line>`) has only the interface's module, so two libraries can never both write its block: wrap them in a struct of your own to implement it elsewhere (v18, milestone 223). The prelude decides `Eq`, `Key` and `Ord` for built-in types: a program's block for them is E0459. Two blocks of one interface that could cover one type (`impl<T: Show> Show for T[]` and `impl Show for Point[]`, or either and a `derive` block) are E0461: there is no specialization, and a generic interface has one block per built-in type whatever its arguments. Such a block meets bounds, gives `x.item()` (a built-in method of the same name wins, as a type's own does) and `I.item(x)`, and converts the type to an interface value; its items left out take the interface's defaults. Blocks for built-in types don't make an interface derivable. The prelude's derive blocks count the same way: `3.cmp(4)`, `"a".eq("b")`.
- **Calling an item** (v18): `Shape.area(c)` (or `m.Shape.area(c)`) calls item `area` of `Shape` on `c`, whose type must implement `Shape` (E0422), whatever else `c`'s type has by that name; the other arguments follow. `c.area()` calls `c`'s type's own method `area` when it has one; otherwise the item of the one interface the type implements that declares a method `area` and is in sight at the call (declared in the calling module, in a module it imports, or the prelude, and visible there): the block's method, or the interface's default. None is E0415; two or more is E0460, naming two, with help to qualify the call.
- **Derivable interfaces** (v17, milestones 141–142). A `derive` block, `derive [<params>] I for X { items }`, gives interface `I`'s items to the types `X` covers: one type written with the block's parameters (`i64`, `T[]`, `Option<T>`, `Map<K, V>`), or every `struct`, `enum` or `tuple` whose fields, payloads or elements meet `I` in turn (recursive types included). It is written only in the module that declares `I` (E0452 elsewhere), two blocks of one interface may not both cover a type (E0452), and it defines exactly `I`'s items with `Self` replaced by `X` (E0422 otherwise). An interface with blocks is *derivable*: a type meets it through the block covering it, the block's parameters meeting their bounds (`derive<T: Show> Show for T[]`); anything else is E0451, naming the path to the first part no block covers (E0450 for `json.Json`). **Asking** (v18): a block for one type covers that type, and the tuple, handle and prelude types' blocks cover them, without asking; but a struct or enum of a program or of `std` meets a derivable interface only when it asks: `struct Site { … } with json.Json`, `enum E { … } with Show, Eq`, or `impl I for T = derive;` (in the module of `I` or of `T`), or when it implements it by hand, `impl I for T { … }`. Otherwise it is E0422, with help to add `with`. `with I` needs a block for every struct or every enum, as the type is (E0462 otherwise, naming what the blocks cover); its fields or payloads that don't mention a type parameter must meet `I`, checked at the declaration (E0451, E0450 for JSON), and a generic type meets it at each argument list whose parts do. Asking twice is E0461; deriving and writing one interface for one type is E0463. `Eq` is still met without asking (v18, until `Key` arrives); `with Eq` is accepted and checked. A block's items are reached only through the interface, a bound `T: I`; they never become methods of `X` (`5.show()` stays E0415). Each body is checked once, like a default method's, `Self` in a struct, enum or tuple block knowing only `I`'s items. A fourth kind, `derive I for handle` (v17, milestone 143), covers every `extern type` handle and `CPtr`. A hand-written `impl I for T { … }` replaces the blocks for that type entirely (v18); a type's own method of an item's name doesn't count, except for `Eq`'s `eq` and `hash`, which still may be a struct's or enum's own. Inside a struct or tuple block, `derive.fields(x, ctx, f)` is an `R[]`, `f(ctx, name, field)` for each field in declaration order (a tuple's names are `"0"`, `"1"`, …), and `derive.build(ctx, f)` is a `Result<Self, E>` built from `f(ctx, name, at, of): Result<F, E>` per field, the first `Err` returned; inside an enum block `derive.variant(x, ctx, f)` is the variant's name and its payloads through `f` as a `(string, R[])`, `derive.variants()` each variant's name and payload count as a `(string, i64)[]`, and `derive.build_variant(name, ctx, f)` an `Option<Result<Self, E>>`, `None` for an unknown name. In a struct, tuple or enum block `derive.all(a, b, f)` (v17, milestone 144) is `true` if `f(name, x, y): bool` holds for every pair of corresponding fields (an enum's: `false` for different variants, else every payload pair), stopping at the first `false`; `derive.fold(x, init, f)` threads `init` through `f(acc, name, part): A` for each field (an enum's: each payload of the variant at hand, named by the variant) and is the last `acc`, allocating nothing; in an enum block `derive.index(x)` is the variant's position, an `i64`; in a handle block `derive.address(x)` is the handle's address, a `u64` (v17, milestone 143). `f` names a function of the same module generic over one `F` bounded by `I`, called at each field's type; anything else, or an intrinsic outside a block or in the wrong kind of block, or a local named `derive` inside a block, is E0453.
- **Bounds on `impl` blocks** (v15): `impl<T: Num> Grid<T> { … }` writes bounds on the block's parameters with a function's syntax, and every method of the block has them, added to the type's own. A type may have several `impl` blocks with different bounds, such as `impl<T: Num> Grid<T>` for arithmetic and `impl<T> Grid<T>` for the rest; a method defined in two blocks is E0310 as before. A bounded method used on an instance that misses the bound is E0419: `Grid { data: ["a"] }.total()`.
- **Using one as a bound:** `fun total<T: Shape>(xs: T[]): f64`, combined freely with `Eq`, `Ord` and other interfaces (`T: Shape + Eq`), or from another module (`T: shapes.Shape`). In the body, `x.area()` and, for an associated item, `T.make(…)` are allowed when some bound declares them, and are checked against its signature. Calling something no bound declares is E0421, with help suggesting a bound. Two bounds of one parameter may declare the same name only with the same signature; otherwise the bound list is E0423.
- **At a use:** a type argument that doesn't implement an interface bound is E0422, with help to add the `impl`; for a type parameter or an interface value as the argument, it names each missing or mismatched item.
- **Names and visibility:** interfaces share their module's item namespace (E0302) and follow the visibility rules of section 6. Through a bound, each instance calls the concrete type's own method, so there is no dispatch at run time.

**Interface values** (v5).
- **The type:** an interface whose items are all methods is a type, written by its name (`Shape`, `shapes.Shape`) wherever a type can be: `Shape[]`, `Option<Shape>`, fields, payloads, parameters, return types, function types and type arguments. A generic interface takes its arguments (v6, above); a plain one takes none (E0418). An interface with an associated function can still be a bound, but used as a type it is E0425, naming the associated function.
- **Conversion,** the one implicit conversion: where the expected type is exactly an interface `I`, a value of a type that implements `I` (v18) becomes an `I`; a type that doesn't is E0422, with help to add the `impl`. An expected type reaches a value through `let` annotations, arguments, struct-literal fields, array elements, payloads (`Some(c)`, `Ok(c)`), assignments, `return` and tail values, and the branches of `if` and `match`. A numeric literal takes its default type (`i64`, `f64`) and converts.
  - **Not inside other types:** a `Circle[]` is not a `Shape[]`, and an `Option<Circle>` is not an `Option<Shape>` (E0403).
  - **Between interfaces** (v6; v18, milestone 223): a value of interface type `J` converts to another interface `I` only when `J` extends `I`, at any depth (`interface Pet: Named` makes a `Pet` a `Named`); having items of `I`'s names and signatures doesn't count, so `Label { fun name(self): string; }` doesn't become a `Named` (E0422: only an interface that extends `Named` converts to it). `Iter` and the derivable interfaces, still met by their items' names, convert by items. The value inside keeps its concrete type, so a type pattern on the `I` still matches it (`d: Dog`), and each method behaves as it did through `J`, including a default `J` supplied. The same interface at other arguments doesn't convert (`Source<i64>` to `Source<string>`, E0403). In a generic instance whose type argument is an interface value, converting that `T` to `I` works the same way.
  - **Type parameters:** `T` converts to `I` when its bounds satisfy `I`; each instance converts its concrete type.
  - **No expected type, no conversion:** `let s = c;` keeps `c`'s type, and `[c, sq]` with no expected type is E0403.
- **Calls:** `s.area()` calls the concrete type's method, checked against the interface's signature. A `mut self` method needs a mutable place (E0501, E0502, E0508). The value inside has no reachable fields (E0410).
- **Copies:** an interface value holds its own copy of the value inside. Assigning, passing or storing one copies that value deeply, as for an array, so a `mut self` call through one interface value never changes another; move on last use applies.
- **Equality, order and printing:** none. `==` is E0404, so an interface type doesn't satisfy `Eq` or `Ord` (E0419), and a struct, enum, array or `Option` holding one isn't `Eq` (E0451, v17). Interface values print as the value inside them (v12).
- **As a type argument:** `I` satisfies the interface `I` itself, so `total_area<Shape>(shapes)` works and calls through the values. It satisfies another interface only if `I` extends it (v18, milestone 223), never by declaring the same items.

**Function types** (v3). `fun(A, B): R` is the type of a function value taking `A` and `B` and returning `R`; `fun(A)` returns nothing. Two function types are the same when their parameter and return types are. A return type takes any `[]` after it, so `fun(i64): i64[]` returns an array, and an array of functions is written `(fun(i64): i64)[]`; messages print types the same way.
- **Equality:** function values have no `==` (E0404), and a struct, enum, array or `Option` that holds one anywhere isn't `Eq` (E0451 with the path, v17; E0419 before).
- **Order, printing and casts:** function values have no `<`, print as `<fun>` (v12), and can't be cast (E0408).
- **Copying:** a function value is immutable, so a copy is the same value.
- **Generics and interfaces:** a function type can be a type argument (`apply<T>(f: fun(T): T, x: T)`), a field, an element or a payload.
- **A function type is an interface** (v18, milestone 196), with one method, `call`, the call. Every function value has a type of its own, the struct of what it captured (a named function's has none), which implements its signature's interface; a value of type `fun(A): R` is any function of that signature, which one hidden, as an interface value hides its type. So:
  - **As a bound**, `<F: fun(i64): i64>` takes the exact function: an argument that is a closure literal, a named function, or an immutable local initialised with one without a written type makes `F` that function's own type, so the instance is that function's and calls it directly. Any other function value (a parameter, a `let mut`, an annotated local) makes `F` `fun(i64): i64` itself. A bound parameter in the call's result type is always `fun(…)`, so an own type never leaves its instance. Own types print as their signature and the function: `fun(i64): i64 {closure 1 in main}`, `fun(i64): i64 {double}`.
  - **As something an interface requires**, `interface Pricing: fun(i64): i64 {}`: a function value converts to it when every other interface it requires declares no items; its values are called as functions (`rule(x)`) and have the other interfaces' methods.
  - **Implemented by a struct or enum:** `impl fun(i64): i64 for Scale { fun call(self, x: i64): i64 = x * self.k; }` makes a `Scale` callable (`s(2)`), usable for a `<F: fun(i64): i64>` bound, and convertible to `fun(i64): i64` where one is expected, as a value converts to an interface.
  - **Values still meet as their signature:** two function values meeting in branches, elements or a reassignment are both `fun(A): R`, as before. A value that isn't such a function, passed for one, is E0422.

**Functions as values** (v3).
- **Named functions:** a function used without a call is a value of its function type: `let f = double;`, `apply(double, 3)`, `m.f` from an imported module (visibility applies, E0313).
- **Associated functions** are values too: `let make = Point.at;`, `Circle.make`.
- **Generic functions** need an expected function type to fix their type arguments: `let m: fun(i64, i64): i64 = max;`. Otherwise it is E0412.
- **Variants** (v17, milestone 94): a variant with payloads, written without a call, is a value of type `fun(P1, …): E` — `Msg.Named` is `fun(string): Msg`, `Shape.Rect` is `fun(f64, f64): Shape` — and so are `Some`, `Ok` and `Err` (unless a local or function has the name). A variant of a generic enum takes its type arguments from the expected function type, its result or its parameters (`let s: fun(i64): Option<i64> = Some;`, `xs.iter().map(Some)` where the element type is known); with nothing to fix them it is E0412. A variant without payloads stays a value of its enum (`Msg.Add`). Variant values follow the names variants already have (`E.V`, `m.E.V`, through named imports of the enum) and the enum's visibility.
- **Not values:** methods with a receiver (call them on a value), externs and the intrinsics. Using one as a value is E0406, with help.

**Calls** (v3). `e(args)` calls any expression `e` of function type: a local, a parameter, an element (`fs[0](3)`), a call's result (`make()(3)`). `s.f(x)` calls method `f` if the type has one, otherwise field `f` if it holds a function; a method and a field never share a name (E0310), so this is never ambiguous. Arity (E0405) and argument types (E0403) are checked against the function type. Calling a value that isn't a function is E0406. A local of function type shadows a function of the same name, as any local does.

**Closures** (v3). `fun(x) = x + k` and `fun(x: i64): i64 { … }` are function values written inline: a function without a name. An `= e` body takes the longest expression, so `sort(xs, fun(a, b) = a > b)` ends at the `)`.
- **Parameter types** are written, or come from an expected function type in the same position (`let g: fun(i64): i64 = fun(x) = x + 1;`, or a parameter of a called function). Otherwise it is E0412, with help to write the type. In a generic call, a closure whose parameter types aren't written is typed last, so `apply(fun(x) = x + 1, 41)` makes `T` an `i64` first.
- **The return type** is written, or the expected one, or the body's type. A closure that uses `return` needs its return type written or expected (E0412).
- **Its own function:** `return` leaves the closure, and `break` and `continue` reach only loops inside it (E0504 otherwise).
- **Captures:**
  - A closure captures every local, parameter or `self` of an enclosing function that its body mentions.
  - Each is copied when the closure is created, in order of first mention, by the copy rules above; arrays are always deep-copied.
  - The closure reads its copies and never changes them: assigning a captured variable, one of its fields or elements, calling a `mut self` method or `push`/`pop`/`clear` on it, or `swap`ping it is E0511, with help to return a value instead.
  - So changing the outer variable later doesn't change the closure, and calling the closure never changes the caller's variables.
  - A closure's own `let mut` locals are ordinary.
- **No recursion:** a closure has no name, so it can't call itself; use a named function.

**Generic recursion.** A generic struct or enum may contain itself only with its own parameters unchanged: `enum List<T> { Nil, Cons(T, List<T>) }` is fine, and the indirect-slot rule (section 7) applies to each instance. Instantiation must end: a program whose instances nest more than 64 deep, such as `f<T>` calling `f<T[]>` or a `struct S<T>` holding `S<T[]>`, is E0420. Recursion at the same arguments (`count<T>` calling `count<T>`) never counts. Generic code copies exactly where concrete code would, because each instance is concrete (Array copies, above).

**Mutability.** Bindings are immutable by default. Assigning to any place requires the variable at its root to be declared `let mut`. That covers the variable itself, a field (`p.x = 1.0`), an element (`xs[0] = 1`) and any chain of these (`shapes[i].pts[j].x = 2.0`), and both places of a `swap` (section 5). Because every variable owns its data, a variable declared without `mut` never changes after it is initialized.

**`mut` parameters** (v18). `fun bump(mut c: Counter, by: i64) { c.n = c.n + by; }` changes its caller's place: the call writes `bump(mut c, 2)`, and `c` is changed when the call returns. Inside the function a `mut` parameter is assignable, like `mut self`; any other use (returning it, storing it, capturing it) reads a copy, so the caller's place never escapes the call. The argument must be written `mut` exactly where the parameter is (E0443), be a mutable place: a `let mut` variable, a `mut` parameter, `mut self`, or a field or element of one (E0501, E0502), and have exactly the parameter's type (E0403); no other argument of the call may be the same place, overlap it, or (for a value passed by pointer) be rooted in it, and a `mut self` call's argument may not be rooted in its receiver (E0508). A number or string read out of it is a fresh value. Functions, methods, interface items, `impl` blocks and default bodies may have `mut` parameters, and an `impl` must have them where its interface does (E0422); they are passed by pointer through direct calls, bounds and interface values. A `spawn` of a call with a `mut` argument is E0438, an actor's message with a `mut` parameter E0439, and a function with one can't be a value (E0406), since function types can't spell `mut`; closures take none (E0209). An extern's `mut` parameters are C's pointers (section 8).

## 5. Expressions and statements

Evaluation is strictly left to right: operands, call arguments and struct-literal fields are evaluated in source order. The only exceptions are `&&` and `||`, which skip their right operand when the left decides the result.

**Scopes, `spawn` and `select`** (v17; scopes v18, milestone 195). `scope s { body }` runs its body in the current task; every task `s.spawn` starts finishes before the scope does. At the body's end the scope waits for its tasks, then runs the body's `defer`s, and its value is `Ok` with the body's. The first failure — a task's panic, a task returning `Err`, or a panic in the body — cancels every other task of the scope and the body with `.SiblingFailed`; the scope waits for them, and its value is `Err` with that failure (cancellations it caused aren't failures). Leaving the body early (`return`, `?`, `break`, `continue`) cancels the scope's tasks with `.Parent` and waits for them before the body's `defer`s run and the exit goes on. A failure or cancellation unwinds the current task only as far as the scope that catches it. Cancelling is invisible to the cancelled task: it notices at its next pause point (a channel operation, `join`, `select`, `sleep`, a socket wait) or preemption point and unwinds as a panic does, silently, running its `defer`s; a task waiting at a pause point is woken to do so; a task in C stops once it is back in Lugha. A task's panic doesn't end the program: it fails its scope. **An `independent` scope** (milestone 198) doesn't fail when a task does: the failure is printed to standard error (``task `name` failed: …``) or given to the scope's handler, in the failing task once it has stopped, and the other tasks and the body run on; `join` then sees `Stopped.Failed` or `.Panicked`. A failure of the body itself still cancels the tasks and fails the scope. **`limit n`** makes `s.spawn` wait, a pause point, while `n` of the scope's tasks run. **A deadline** (milestone 199) is the earlier of the scope's own and its enclosing scope's (the task's scope, for a task's body). When it passes, the body and every task of the scope are cancelled with `.Deadline`, as for a failure, even in an `independent` scope; the scope waits for them, and its value is `Err` with `f.kind` `.Deadline` and `f.task` the scope's name, unless a failure came first. The scheduler notices a deadline where sleepers wake and at preemption points, so a busy task stops too. **`shield within ms { body }`** holds cancellation off while its block runs, for at most `ms`: a cancellation arriving meanwhile waits, waits inside the block aren't cut short, and the cancellation applies at the next pause point after the block, or wherever the task is once `ms` has passed (nested shields keep the earlier end). A panic inside a shield isn't held off: it unwinds past it. A shield entered while its task already unwinds (in a `defer` running for a cancellation) is cut off the same way: its block unwinds, running its `defer`s, the code after the shield runs, and the first unwinding goes on; a shield whose type isn't `()` cut off so has no value to go on with, and ends the program with exit 101, as a panic while unwinding does. A scoped task's panic isn't printed as it happens: its scope reports it, a root task's as ``task `name` panicked: …``. The root scope's tasks finish before the program does, once `main` returns; a failure among them cancels the rest and `main`, then ends the program with exit code 101. `s.spawn(f(a, b))` evaluates `a` and `b` at once, in the spawning task, then queues a new green thread that runs the call; the spawning task goes on. A receiver or callee that is itself a call (`make().run()`) is evaluated at once too. Everything else the call mentions is captured by copy, exactly as by a closure (section 4), so later changes in the spawning thread never reach it. A `select` evaluates every arm's channel, sent value and `after` duration once, top to bottom. If some `recv` or `send` arms can go on at once, one of them is picked pseudo-randomly (the same way on every run), so no arm starves; otherwise `default` runs if there is one; otherwise the thread waits until an arm can go on or the `after` time has passed. A closed channel's `recv` arm can always go on (with `None`), and a `send` arm on a closed channel panics.

**Assignment order.** `place = value` and `place op= value` first evaluate the place's index expressions, root outward, bounds-checking each; then the value (for `op=`, the place's current value, then the right operand); then the write. So in `xs[i] = e`, an `e` that copies `xs` keeps the array as it was before the write (v15 spells this out; it is what every version did). `push`'s argument is likewise evaluated before the array is changed.

**Operator overloading** (v15; through blocks since v18, milestone 171). `a op b` where `a` is a struct, enum or extern type calls the method of a block of `a`'s type for the operator's interface: `+` `Add`'s `add`, `-` `Sub`'s `sub`, `*` `Mul`'s `mul`, `/` `Div`'s `div`, `%` `Rem`'s `rem`, and unary `-a` `Neg`'s `neg`.
- **Blocks:** `impl Mul<f64> for Vec2 { fun mul(self, k: f64): Vec2 = … }` gives `v * 2.0`; `impl Add for Vec2` is `Add<Vec2>`, giving `v + w`. A block's method may return any type but `()`, and the operator's result is that type: `impl Mul for Meters { fun mul(self, o: Meters): Area }` makes `a * b` an `Area`. A method of the type's own named `add` is a method, never an operator.
- **Several right-hand types** [INT-05]: a type has one block per right-hand type, `impl Mul<f64> for V` beside `impl Mul for V` (a second for the same type is E0461), and the right operand picks one by its exact type, with no conversion (not even to an interface value), so nothing is ranked; a literal must fit exactly one (`2` fits the integer types, `2.0` the float ones), else E0457 says how to write it; no fit is E0404, listing the right-hand types there are. `v.mul(2.0)` and `Mul.mul(v, 2.0)` choose the same way, and so does `v *= 2.0`.
- **A built-in type on the left** [INT-04]: `impl Mul<V> for f64 { fun mul(self, v: V): V }` makes `2.0 * v`, and any type may be on the left the same way (`impl Add<V> for string`). Such a block may be written in the module that declares the right-hand type, besides the usual homes (section 4), else E0459, so each pair of types has one place that defines it. A literal on the left must fit exactly one number type with a block for the right operand (E0457). `impl f64 { … }` itself is E0466.
- **Compound assignment:** `a += b` is `a = a + b` through the block `b` fits, the place evaluated once; the block's result must have the place's type (E0403): `k *= v` with `k: f64` and `impl Mul<V> for f64` is E0403.
- **Missing:** an operator on a type without a fitting block is E0404, with help naming the block to write. An interface value has no operator blocks (`interface Scaled: Mul<f64>` can't be a value type: `mul` uses `Self`, E0427): `s * 2.0` is E0404, with help to call `s.mul(2.0)`. Numbers and strings keep the built-in operators, and have no methods to call: `5.add(3)` is E0415.
- **Comparisons:** `<`, `<=`, `>` and `>=` call `cmp` (section 4, user-defined order, v15), and `==` and `!=` call `eq` (section 4, `Eq`, v17).
- **Indexing** (v17, milestone 155): `a[i]` calls `fun index(self, i: I): T` and `a[i] = v` calls `fun set_index(mut self, i: I, v: T)` (two parameters, no result), on a struct, enum, interface value or a `T` whose bounds declare the method. The two are independent; each qualifies only with exactly that shape, and a method with type parameters of its own never does. Several indexes are one tuple: the parser reads `a[x, y]` as `a[(x, y)]` for every type, so a type takes them as `fun index(self, at: (i64, i64)): f64` (`let (r, c) = at;`), and a map with tuple keys is written `m[x, y]`; `a[x,]` is E0201, and the formatter writes `a[(x, y)]` as `a[x, y]`. The index is checked against the parameter (E0403; a literal takes its type). A write is a `mut self` call on `a`: `a` must be a mutable place (E0501), the aliasing rules hold (E0508, E0510), and the place's indexes, then `i`, then `v` are evaluated before the call. What `index` gives is a copy, so a change to it (v17, milestone 156) — `a[i] op= v`, a write inside it (`a[i].x = v`, `a[i][j] = v`) or a `mut self` call on it (`a[i].push(x)`, whose result is kept) — reads it, changes the copy and hands the copy back: `let mut e = a.index(i); e.x = v; a.set_index(i, e);`. The place's indexes are evaluated once, root outward, then `index` is called, then the change runs (evaluating `v`), then `set_index`. It needs a fitting `set_index` (E0411 otherwise) and follows its rules (E0501 on the root). `[ ]`s nest in any order: `g[i][j].x = v` reads and hands back at each one, and `m[k][i] += 1` on a map of such types changes the entry in place. `swap(a[i], b)` and `out a[i]` (v17, milestone 157) swap or fill the copy and hand it back after the call; `for x of mut a[i] { … }` loops over the copy and hands it back on every way out of the loop, as a `defer` around it would (its end, `break`, `continue`, `return`, `?`; not a panic), and changing `a` in the body is E0507, since the hand-back would overwrite it. Without a fitting method, a read or a write is E0411, with help naming the shape. Arrays, strings and maps keep their built-in indexing and have no `index` method.
- **Unchanged:** `&&`/`||`, `!` and the bit operators are not overloadable.

**Arithmetic.** Integer `+`, `-`, `*` and unary `-` panic on overflow: if the true result doesn't fit the type, the program stops with `integer overflow` instead of continuing with a wrong value. Integer `/` and `%` panic when the divisor is zero, and also for `MIN / -1`, which overflows. `%` takes the sign of the dividend, like C and Rust. Compound assignments such as `+=` are checked the same way, including `%=` (v9), which panics on a zero divisor as `%` does. Shifts (v7) lose the bits shifted out and never panic for overflow; `>>` copies the sign bit for `i32` and `i64` and fills with zeros for the unsigned types; a shift amount that is negative, or at least the type's width (8, 32 or 64), panics with `shift out of range` at the operator. `&=`, `|=`, `^=`, `<<=` and `>>=` mean `x = x op y` with the place evaluated once, as `+=` does, and need an integer.

**Maps and sets** (v17, milestone 101). `Map<K, V>` maps keys to values and `Set<T>` holds distinct values; both are built in, like `Option` (a module's own `Map` or `Set` still means its own). A literal names the type (v18, milestone 178): `Map { "ann": 31, "bo": 40 }` is a `Map<string, i64>`, `Set { 1, 2, 3 }` a `Set<i64>`, and the empty ones `Map {}` and `Set {}` take their types from where they are used or from written type arguments, `Map<string, i64[]> {}` (E0412 with nothing to go on). Keys are expressions: `Map { name: 1 }` uses the variable `name`, and a bare name nothing defines is E0301 with help to write `"name"`. Entries are typed like an array literal's elements, evaluated left to right. Two equal literal keys in one literal are warning W0105; the later one wins. `#{…}` is gone (E0213).
- **Collection literals** (v18): braces after a type name hold `field: value` for a struct, `key: value` entries for a type that implements the prelude's `FromEntries<K, V>` (`fun from_entries(keys: K[], values: V[]): Self`), and items for one that implements `FromItems<T>` (`fun from_items(items: T[]): Self`); the literal is the call `T.from_entries([k, …], [v, …])` or `T.from_items([a, …])`, with the literal's type arguments and expected type. `Map` implements `FromEntries` and `Set` `FromItems`, and any program type may: `Bag { "apple", "pear" }`. In the module that declares such a type its braces are still its fields, which is how it builds itself; elsewhere they are entries or items. `{}` on a type implementing both is `FromEntries`. The wrong form, items for a struct or a `FromEntries` type, entries for a `FromItems` type, or a key that isn't a field name, is E0470. In a condition such a literal needs parentheses, as a struct literal does (E0205): `if m == (Map { "a": 1 }) { … }`; mixing entries and items is E0201.
- **Values:** like arrays, maps and sets copy on assignment and passing (copy-on-write, so a copy is cheap until one side changes), change only through `let mut` or `mut self`, and move on their last use. Share one through `Ref<Map<K, V>>`.
- **Keys** (v18): `K`, and a set's `T`, must be `Key` (above): written (`Set<f64>`) or inferred (`Map { 1.5: 1 }`), a key that isn't is E0422 or E0451, a float named as such (`NaN != NaN` would lose the entry); a key that mentions a type parameter is checked where the map is used, every map operation asking `K: Key`. Keys are hashed through `Key`, consistently with `==`.
- **Order:** iteration, printing, `keys()` and `values()` follow insertion order; giving a key a new value keeps its place; removing a key and adding it again puts it last.
- **Maps:** `m.len`; `m[k]` (panics `no key "z" in the map` at the `[` when `k` has no value); `m[k] = v` adds or replaces; inside an existing entry, `m[k]` is a place (v17, milestone 156), as `xs[i]` is: `m[k] += v`, `m[k].x = v`, `m[k][i] = v`, `m[k].push(x)`, `swap(m[a], m[b])` and `for x of mut m[k]` change the value where it is, and a missing key panics `no key "z" in the map` at the `[`, as a read does (to add a key, assign it first); `m.get(k): Option<V>`; `m.has(k): bool`; `m.insert(k, v)`; `m.remove(k): Option<V>`; `m.clear()`; `m.keys(): K[]`; `m.values(): V[]`; `for (k, v) of m` and `m.iter()`, each entry a tuple.
- **Sets:** `s.len`; `s.has(x)`; `s.insert(x): bool` (true when new); `s.remove(x): bool`; `s.clear()`; `for x of s` and `s.iter()`; `a.union(b)` (`a`'s values, then `b`'s new ones), `a.intersect(b)`, `a.minus(b)`.
- **Collecting:** `it.collect_map()` on an iterator of `(K, V)` and `it.collect_set()` on any other; a later key's value wins, a repeated value is kept once.
- **Loops:** `for … of m` visits a copy, so the body may change `m` (`m[k] = …`) without changing what the loop visits. `for … of mut` over a map or set is E0442: loop over `m.keys()` and assign `m[k]`.
- **Equality and printing:** `==` compares entries regardless of order (`Map { "a": 1, "b": 2 } == Map { "b": 2, "a": 1 }`); a map is `Eq` when its values are. They print as their literals: `Map { "ann": 31, "bo": 40 }`, `Set { 1, 2, 3 }`, `Map {}`, `Set {}`.
- **Implementation:** written in Lugha in the prelude (`Map`, `Set`, a compact table in insertion order over arrays, open addressing), hashing keys with `Key`'s `hash` (v17, milestones 143–144): SipHash-1-3 under the process's key (v18, milestone 204) over the same structure `==` compares, written in derive blocks; each value feeds `u64` words, one SipHash block each, and a string its bytes eight at a time (the last word zero-padded), then its length, so fields split differently hash differently. Two built-ins only the prelude can name serve it: `$hash.key(i)`, the key's word, and `$hash.chunk(s, i)`, eight of a string's bytes. A key type's own `eq` and `hash` are used if it has them. Maps and sets hash their entries in any order, as `==` compares them, so equal maps and sets are one key however they were built.

**Tuples** (v14). `(A, B, …)` is a tuple type of two or more elements, and `(a, b, …)` a tuple value; `(x)` is just `x`, and `(a,)` is E0201. A tuple is a value like a struct whose fields are named `0`, `1`, …: `t.0` reads an element and `t.0 = v` changes one in a `let mut` tuple (an index past the end is E0410), and `t.1.0` reaches into a nested tuple. Assigning, passing and returning copy it by the copy rules. `==` compares element-wise; `<`, `<=`, `>` and `>=` compare lexicographically, the first unequal element deciding, and need every element `Ord` (E0451 otherwise, naming the element), so a tuple of `Ord` elements is `Ord`. Functions can return several values: `fun divmod(a: i64, b: i64): (i64, i64) = (a / b, a % b);`. A tuple prints as `(1, "a")`, its strings quoted. `Pair` (v13) is gone: `zip` yields tuples.
- **Patterns:** `(p, q, …)` matches a tuple element by element, nesting like any pattern; bare names inside it bind. A tuple has one constructor, so `match` checks exhaustiveness through its elements (E0417, W0102).
- **Destructuring:** `let (q, r) = divmod(7, 2);` and `for (i, w) of xs.iter().zip(ws.iter()) { … }` bind each element. Their patterns must match every value (bindings, `_`, `mut`, and tuples of those); anything that could fail is E0433, with help to use `let … else` or `match`. `let mut (x, y)` is not a thing: write `let (mut x, mut y)`.


**Iterator sources and helpers** (v10). These make and use iterators, the values `for … of` steps through (`Iter<T>`, above):

| Expression | Gives | Yields |
| --- | --- | --- |
| `xs.iter()` | `ArrayIter<T>` | the elements of a copy of `xs`, in order |
| `s.bytes()` | `Bytes` | the bytes of `s`, as `u8` |
| `range(a, b)` | `Range` | `a`, `a + 1`, … up to `b − 1` (`i64`s; nothing if `a >= b`) |

Every iterator `it` that yields `T` (a value whose type has `fun next(mut self): Option<T>`, an `Iter<T>` value, or a type parameter bound by `Iter<T>`) has these helpers, unless its type declares a method or item by that name, which wins:

| Helper | Gives | Meaning |
| --- | --- | --- |
| `it.map(f)`, `f: fun(T): U` | `Mapped<T, U>` (v17; `Map` before), yielding `U` | `f` of each value |
| `it.filter(f)`, `f: fun(T): bool` | `Filter<T>` | the values `f` is true for |
| `it.take(n)` | `Take<T>` | the first `n` values (`n` an `i64`); after them it pulls nothing more |
| `it.skip(n)` | `Skip<T>` | every value after the first `n` |
| `it.enumerate()` | `Enumerate<T>`, yielding `Indexed<T>` | each value with its position: `p.i` (from 0) and `p.value` |
| `it.collect()` | `T[]` | every value, in order |
| `it.count()` | `i64` | how many values |
| `it.fold(init, f)`, `f: fun(A, T): A` | `A` | the values folded from `init` |
| `it.any(f)`, `it.all(f)` | `bool` | whether some or every value passes `f`, stopping at the first that decides |
| `it.find(f)` | `Option<T>` | the first value `f` is true for |
| `it.sum()` | `T` | the total, for a number type `T` only (E0415 otherwise) |
| `a.zip(b)` (v13) | `Zip<A, B>`, yielding `(A, B)` (a `Pair<A, B>` before v14) | `a`'s and `b`'s values side by side, ending with the shorter |
| `a.chain(b)` (v13) | `Chain<T>` | `a`'s values, then `b`'s (`b` must yield `T`, E0422 otherwise) |
| `it.take_while(f)` (v13) | `TakeWhile<T>` | values while `f` holds; after the first that fails, nothing more is pulled |
| `it.skip_while(f)` (v13) | `SkipWhile<T>` | every value from the first `f` fails for |
| `it.flat_map(f)`, `f: fun(T): I` (v13) | `FlatMap<T, U, I>`, yielding `U` | each value's iterator `I` (yielding `U`) in turn; for an array, return `xs.iter()` |
| `it.min()`, `it.max()` (v13) | `Option<T>` | the first smallest or largest value, `None` when empty; `T` must be `Ord` (E0422, E0451) |
| `it.position(f)` (v13) | `Option<i64>` | the index, from 0, of the first value `f` holds for |

- **Lazy:** the five adapters (`map` to `enumerate`) only wrap their iterator; nothing runs until a loop or a consumer pulls values, so they work on endless iterators: `range(10, 1000000000).filter(f).take(3)` tests only as many numbers as it needs.
- **Copies:** a helper takes its iterator by copy, as `for … of` does, so `let m = it.map(f);` leaves `it` unchanged; `xs.iter()` holds its own copy of `xs`.
- **Arguments:** closures are typed from the receiver as for the array helpers, and `take`/`skip` take an `i64`, a negative one taking or skipping nothing. Arrays keep their eager helpers: `xs.map(f)` is an array, `xs.iter().map(f)` an iterator.
- **Through the chain** (v17, milestone 150): a closure whose result can't be typed alone (`fun(x) = None`) takes it from what the chain is expected to make: `collect` expected to be `E[]`, `collect_set` `Set<E>` and `collect_map` `Map<K, V>` want elements `E` (`(K, V)`), which `filter`, `take`, `skip`, `take_while` and `skip_while` pass back unchanged and `map` gives its closure as the result type: `let n: Option<i64>[] = xs.iter().map(fun(x) = None).filter(f).collect();`. Only the helpers do this; a type's own `map` or `collect` sees nothing.
- **Panics:** `sum` uses `+`, so it panics on overflow, located in the prelude.
- **Names:** these are Lugha in the prelude. `range` and the types `ArrayIter`, `Bytes`, `Range`, `Mapped`, `Filter`, `Take`, `Skip`, `Enumerate` and `Indexed`, and (v13) `Zip`, `Chain`, `TakeWhile`, `SkipWhile` and `FlatMap`, are visible in every module, like `Result`, unless the module declares the name; they print with their module, `$prelude.Map<i64, string>`.

**Methods on integers** (v7, v10). Every integer type (`i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`) has these built-in methods:

| Method | Gives | Meaning |
| --- | --- | --- |
| `a.wrapping_add(b)`, `a.wrapping_sub(b)`, `a.wrapping_mul(b)` | `a`'s type | the result modulo 2^width; never panics |
| `a.count_ones()` | `i64` | how many bits are 1 |
| `a.leading_zeros()` | `i64` | zero bits above the highest 1 (the width for 0) |
| `a.trailing_zeros()` | `i64` | zero bits below the lowest 1 (the width for 0) |
| `a.checked_add(b)`, `a.checked_sub(b)`, `a.checked_mul(b)`, `a.checked_div(b)` | `Option` of `a`'s type | `None` where the operator would panic: on overflow, a zero divisor, or `MIN / -1` (v10) |
| `a.saturating_add(b)`, `a.saturating_sub(b)`, `a.saturating_mul(b)` | `a`'s type | the result clamped to `MIN … MAX` (v10) |
| `a.rotate_left(n)`, `a.rotate_right(n)` | `a`'s type | the bits rotated by `n`, an `i64`, modulo the width: a negative `n` rotates the other way (v10) |
| `a.kb()`, `a.mb()`, `a.gb()` (v18) | `a`'s type | `a` kilobytes, megabytes or gigabytes in bytes, in powers of 1024 (`1.kb()` is 1024); an overflow panics as `*` does; not on `i8` or `u8`, which can't hold one kilobyte |

`b` has `a`'s type, and a literal `b` takes it (E0403 for another type, E0405 for the wrong count).

**A float's bits** (v17, milestone 108). `x.to_bits()` gives the IEEE 754 bits of a float exactly — a `u64` for `f64`, a `u32` for `f32`, NaN payloads and the sign of `-0.0` kept — and `f64.from_bits(b)` / `f32.from_bits(b)`, called on the type keyword, read them back; `b` is exactly `u64` / `u32` (a literal takes the type; E0403 otherwise, E0405 for the wrong count, E0415 for another member). `as` converts a value, never its bits.

**Integer limits** (v10). `I.MIN` and `I.MAX` are the smallest and largest value of integer type `I`, of type `I`: `i8.MIN` is −128, `u64.MAX` is 18446744073709551615. Only these two names follow an integer type keyword (E0415 otherwise); other type keywords take none. Any other method on an integer, and these names on a float or `bool`, is E0415. A literal receiver with no expected type is `i64`: `(5).count_ones()` is `2`. The wrapping methods are the explicit way to wrap; `+`, `-` and `*` still panic on overflow.

Float arithmetic follows IEEE 754: overflow gives an infinity and division by zero gives an infinity or NaN, never a panic. Explicit casts are the one place values wrap: narrowing with `as` truncates (section 4).

**Method calls.** `x.m(a, b)` evaluates `x`, then `a`, then `b`; `xs.push(v)` evaluates the place `xs`, then `v`. For a `mut self` call the place `x` runs like an assignment's place (v15): its index expressions first, bounds-checked, then the arguments, then the place itself is reached and the method runs. Calls on the same receiver inside the arguments finish first, and the outer call sees their effects.

**`match` evaluation.** A `match` evaluates its scrutinee once, then tests the arms from top to bottom and runs the first that matches, outside in for nested patterns; a guard runs only once its pattern has matched. A variant's payloads are evaluated left to right when it is built.

**Compound assignment.** `x op= e` means `x = x op e`, except that the place `x` is evaluated once. So `xs[next()] += 1` calls `next()` once.

**Evaluation of `==`.** Both operands are evaluated left to right, then compared. Structs and arrays are compared in field or element order, stopping at the first difference. A comparison never panics.

**Changing what a statement writes.** An assignment, a `swap` and a `mut self` call each fix their place before the rest of the statement runs: the value, the arguments, and the place's own indexes. The rest may not change anything that contains that place (E0510), because growing, shrinking or replacing an array would leave the write aimed at an array that is no longer there. `xs[0] = { xs.push(2); 9 };`, `xs[{ xs.clear(); 0 }] = 1;` and `g[0][0] = { g[1] = [5]; 1 };` are rejected (any two indexes might be equal). For a `mut self` receiver (v15), which is reached again after the arguments, a change counts only when an array index lies between the changed place and the receiver: `gs[0].f(gs.clear())` and `self.layers[i].f(self.reset())` are rejected, while `self.g.f(self.norm(x))` and `a.b.f(a.reset())` are valid. Changing the place itself, a place at the same depth, or a sibling is fine, and reading is always fine: `xs[0] = { xs[1] = 5; 9 };`, `p.items[0] = { p.n += 1; 7 };`, `xs[i] = xs[i] + 1;` and `xs = { xs.push(1); xs };` are valid. Make the change in its own statement instead.

**Variable declarations.** Every variable must be initialized. A variable is in scope from the end of its declaration to the end of its enclosing block, so `let x = x + 1;` refers to an outer `x`. Shadowing is allowed, both in nested blocks and in the same block.

**if / else.** `if` is an expression. The condition must be `bool`. With an `else`, both branches must have the same type, which becomes the type of the whole `if`. Without an `else`, the type is `()`. An expected type flows into both branches, so `let x: i32 = if c { 1 } else { 2 };` is valid. With none, a branch that can't be typed alone — `[]`, `None`, an untyped closure, a result-only generic call — takes the other branch's type: `if c { [] } else { [1] }` is an `i64[]`, either way round; in a `match`, such arms wait for the first arm that can be typed (v17). A literal branch (`255`, `[0]`) likewise takes a typed sibling's type when it could have it, so `if c { b - 1 } else { 255 }` is a `u8` for `b: u8`.

```
let sign = if x < 0 { -1 } else if x == 0 { 0 } else { 1 };
```

**Blocks and tail values.** A block `{ stmts tail }` runs its statements, then evaluates `tail` and yields it as the block's value. A block with no tail has type `()`. Variables declared inside a block go out of scope at its closing brace. A block-like statement in the middle of a block must have type `void`, because a value there would be silently thrown away; the checker reports it as an error.

**while.** Evaluates the condition before each iteration; stops when it is `false`.

**Conditional patterns** (v9). Each tests one pattern, typed as a `match` arm's (or-patterns and `mut` included), with no exhaustiveness check; the scrutinee is a condition, so a struct literal needs parentheses.
- **`if let p = e { a } [else b]`** evaluates `e` once; if `p` matches it runs `a` with `p`'s bindings in scope, otherwise `b` (or nothing). It is an `if`: with `else` both branches agree on a type and it is a value; without, it is `()`. `else if let` chains. An irrefutable pattern (`if let x = e`) is allowed.
- **`while let p = e { body }`** evaluates `e` and tests `p` before each iteration, running `body` with the bindings while it matches; `break` ends the loop and `continue` goes to the next test.
- **`let p = e else { … };`** evaluates `e`; if `p` matches, its bindings are in scope for the rest of the block. Otherwise the `else` block runs, and it must not finish normally: it ends in `return`, `break`, `continue`, `panic(…)` or `exit(…)`, or is otherwise of type `never` (E0429). `let x = e else { … };` reads `x` as a pattern, a binding, so it always matches; `let .Empty = e else { … };` tests a variant. `mut` goes inside the pattern: `let .Some(mut n) = o else { return; };`.

**for over a range.** `for i in a..b { body }` runs with `i` = `a`, `a+1`, …, `b-1`. The range is half-open, and empty when `a >= b`. `a` and `b` must have the same integer type, which becomes the type of `i`. Both bounds are evaluated once, before the first iteration. `i` is a fresh immutable binding on each iteration. It desugars to:

```
{
    let __end = b;
    let mut __i = a;
    while __i < __end {
        let i = __i;
        body
        __i += 1;   // also run on continue
    }
}
```

**for over an array.** `for x of xs { body }` binds each element in order. `xs` is evaluated once and is not copied. `x` is an immutable binding to the element, so assigning to `x` is an error; write `xs[i]` with a range loop to modify elements. The loop holds the array as it was when the loop started, and the body may change `xs` (assign it, push to it, pass it as `mut`): a write copies the array the loop holds, so the loop goes on over what it started with, as a map's or set's loop does (v18, milestone 224; before, a change was E0507). Only `for x of mut xs` guards `xs`, since it writes each element back (E0507). Strings are not iterable in v0.

**for over `mut`** (v13). `for x of mut e { body }` loops over `e` itself rather than a copy. `e` must be a variable, field or element (E0502) rooted in a `let mut` local or `mut self` (E0501); a string is E0506.
- **Over an array,** `x` is each element in turn, not a copy: `x = v`, `x += 1`, `x.field = v` and `mut self` calls on `x` change the array. `x` is still a borrowed value, so storing or returning it copies. The body may copy `e` (`let c = xs;`): later writes to `x` change `e` only, never the copy.
- **Over an iterator,** `next` is called on `e` itself, so after a `break` the iterator stands where the loop left it and a later loop resumes it. `x` is an immutable value, as without `mut`.
- **Guard:** in either case the body may not assign to `e`, grow or shrink it, or call its `mut self` methods (E0507), since the loop is using it; it may read it.

**for over a string** (v10). `for b of s { body }`, where `s` is a `string`, binds each byte of `s` in order, as a `u8`, like `for b of s.bytes()`; strings never change, so nothing in the body can disturb it.

**for over an iterator** (v6). `for x of e { body }`, where `e`'s type implements `Iter<T>` (section 4), evaluates `e` once into a hidden mutable copy (moved instead when `e` is a local's last use), then calls `next` on that copy before each iteration: `Some(v)` runs the body with `x` bound to `v`, and `None` ends the loop. `x` is immutable and holds its own value. `break` ends the loop and `continue` goes to the next call of `next`. Because the loop works on a copy, `e` itself never changes, and the body may assign to it; looping twice over the same variable starts from the beginning both times. As in every `for` header, a struct literal needs parentheses: `for n of (Count { n: 0, end: 3 }) { … }`. It desugars to:

```
{
    let mut $it = e;
    while true {
        match $it.next() {
            .Some(x) => { body }
            .None => { break; }
        }
    }
}
```

**loop** (v17, milestone 152). `loop { body }` runs `body` again and again until a `break` targets it, or a `return`, `?`, `panic` or `exit` leaves it. It is an expression, like `if`: `break value;` gives the loop that value, and every `break` targeting one loop gives the same type (E0403 at the value otherwise); the loop's expected type flows into each value (`let o: Option<i64> = loop { break None; };`), and with none the first `break`'s value decides. A plain `break;` gives `()`. A `loop` that no `break` targets never ends: its type is `never`, so it counts as returning (section 6) and can stand in for any type. Its body has type `()` (E0403 for a tail value). As a statement it needs no `;`, like any block-like one, and its value, if any, must be `()` (E0505). Only a `loop` gives a value: `break value` out of a `while` or `for` is E0514, since those can end without a `break`. `loop` is a keyword.

**Labels** (v17, milestone 152). `name:` directly before `loop`, `while`, `while let` or `for` labels that loop: `outer: for row of grid { for x of row { if x == 0 { continue outer; } } }`. `break name;`, `break name value;` and `continue name;` reach the loop with that label, running the defers of every block they leave, innermost first, as a plain `break` does. Labels live apart from variables: after `break`, a name is the label only when an enclosing loop has that label, and otherwise the start of a value (`break x;` breaks with `x`); after `continue`, a name is always a label. A label that no enclosing loop has is E0512, and labels don't reach into closures or spawned calls, whose bodies are their own functions. Nested loops can't share a label (E0513); loops side by side may. A labelled `loop` is also an expression: `let first = search: loop { … break search x; … };`. In a collection literal, `Map { a: loop { … } }` is the entry `a`, not a label.

**break and continue.** Without a label, apply to the innermost enclosing loop. Using either outside a loop is a compile error (E0504).

**defer** (v17, milestone 151). `defer e;`, `defer place op value;` (any assignment, checked as that statement) or `defer { … }` runs `e` when the block holding it is left, on every way out after the `defer` was reached: the block's end, `return`, `?` returning, `break` and `continue` (through every block between the jump and its loop). Several run last first, innermost block first. `e` is evaluated when it runs, reading variables as they are then (`let mut n = 1; defer println(n); n = 2;` prints 2), with the names in scope at the `defer` (a later `let` shadowing one doesn't change what it reads). A block's tail and a `return` value are computed first and keep the value they had (`defer xs.push(4); xs` gives `xs` without the 4). A `defer` not yet reached doesn't run, and one inside a deferred block runs at that block's end. `e` must have type `()` (E0456), so a `Result` can't be dropped by accident: `defer { let _ = dir.remove_file(p); }` discards one on purpose. A `return`, `?`, `break` or `continue` that would leave `e` is E0455, since its block is already being left; loops and closures inside `e` may use them. A closure's or a spawned call's defers run when it returns, and a panic runs every pending one as it unwinds (section 6); a stack overflow and a second panic during unwinding don't. `defer` is for closing files and sockets on an early `?`:

```lugha
for path in paths {
    let f = dir.open(path)?;
    defer f.close();            // at the end of each iteration
    out.push(f.read_line()?);   // an Err here still closes f
}
```

**Expression statements.** Any expression followed by `;` is a statement, and its value is discarded. Discarding the result of any call is allowed. Adding `;` after a would-be tail expression turns it into a statement, so `{ x * x; }` is a `()` block. When a value was expected, the checker reports this as "remove this semicolon" and points at it.

**Panics.** A panic prints `panic: <message> at <file>:<line>:<col>` to stderr and exits with code 101. Panics come from integer overflow (message `integer overflow`, including `MIN / -1`), integer division by zero (`division by zero`), a shift out of range (`shift out of range`; v7), out-of-bounds indexing (`index out of bounds: the length is N but the index is I`, located at the `[`), an out-of-range `slice` (`slice out of bounds: from F to T of a string of length L`, at the method name; v4), `split` with an empty separator (located in the prelude, `<prelude>:…`; v4), a negative repeat-literal count, too-deep recursion (`stack overflow`), `unwrap` on a `None` or an `Err` (`unwrap on None`, `unwrap on Err: <error>` with the error printed, v12) and `expect(msg)` likewise (message `msg`), each located at the method name (v9), and the `panic(msg)` intrinsic. An arithmetic panic's location is its operator: the `+` in `x + 1`, the `+=` of a compound assignment, the `-` of a negation. A stack overflow is the one panic without a location — it prints just `panic: stack overflow` — because the runtime can't tell which call went too deep. They cannot be caught. After the message, a panic unwinds its task (v18, milestone 194): frame by frame, each runs the `defer` statements pending where it was (section 5), innermost first, and lets go of what it holds, then the program ends with 101 — or, in an actor, only the actor ends (section 9). A panic in a spawned task ends the program once that task has unwound; the other tasks don't unwind. A panic while unwinding (in a `defer`) prints its message and ends the program at once. A stack overflow doesn't unwind, and a panic never unwinds into C: one in a callback unwinds the callback, then ends the program.

**Intrinsics.** These names are built into the compiler, not declared in source. They accept arguments that ordinary functions can't, which is why they are special. Inside a derive block, `derive.fields`, `derive.build`, `derive.variant`, `derive.variants`, `derive.build_variant`, `derive.fold`, `derive.index`, `derive.address` and `derive.all` are intrinsics too (v17, milestones 142–144; section 4); elsewhere `derive` is an ordinary name.

| Intrinsic | Accepts | Effect |
| --- | --- | --- |
| `panic(msg)` | `string` | Panics with `msg` |
| `assert(c)` (v16; a prelude function since v18) | `bool` | Panics with `assertion failed` unless `c` holds, at the call (`@caller_location`) |
| `assert_eq(a, b)` (v16; prelude, `assert_eq<T: Eq>`, since v18) | two values of one type with `==`, inferred as for any generic call | Panics with `assert_eq failed: left <a>, right <b>` unless `a == b`, each value in its debug form |
| `assert_ne(a, b)` (v16) | the same | Panics with `assert_ne failed: both <a>` if `a == b` |
| `skip(reason)` (v17) | a `string` | In a test, ends it at once and reports it skipped with `reason`; outside a test, panics with `skip outside a test` |
| `to_string(x)` | A value with `Display` | Returns its text, as `console.print` writes it; a `string` is itself |
| `debug(x)` (v18) | Any value | Writes `[file:line] <x as written> = <x's debug form>` to stderr and returns `x`; E0465 in an `-O2` build of the program's own package (the main file's), while a dependency's `debug(x)` is just `x` there, printing nothing (v18, milestone 224) |
| `swap(a, b)` | Two places of the same type | Exchanges their contents |
| `parse_i64(s)` | `string` | `Some(n)` if `s` is an optional `-` followed by decimal digits and fits in `i64`, else `None`; no `+`, spaces, `_` or hex (v3) |
| `parse_u64(s)` | `string` | `Some(n)` if `s` is decimal digits only and fits in `u64`, else `None`; no sign (v10) |
| `parse_f64(s)` | `string` | `Some(x)` if `s` is an optional `-`, digits, an optional `.` and digits, and an optional exponent (`e`/`E`, optional sign, digits), else `None`; `"3"` gives `3.0`, while `".5"`, `"5."`, `"inf"` and `"nan"` give `None` (v3) |
| `string_from_utf8(data)` | `u8[]` | `Result<string, string>`: a string of exactly those bytes if they are valid UTF-8 (no overlong forms, surrogates, code points above U+10FFFF or truncated sequences), else `Err("invalid UTF-8 at byte N")` with the first bad sequence's offset; the only way from bytes to a `string` (v14) |
| `include_bytes(path)` | a string literal | `u8[]`: the contents of the file `path` names, read when the program is compiled (v17, milestone 128; below) |
| `include_str(path)` | a string literal | `string`: the same, as text; the file must be UTF-8 (E0448) |

**Gone from programs** (v18, milestones 180 and 182): `print`, `println`, `eprint`, `eprintln`, `read_line`, `read_stdin_bytes`, `flush`, `read_file`, `write_file`, `append_file`, `read_bytes`, `write_bytes`, `append_bytes`, `args` and `exit`. Each reached outside the program without being given the means to, so a call is E0471, naming the capability to take instead (Capabilities, section 6); the names stay reserved (E0302). Their behaviour lives on as the methods of `Console` and `Dir`, and the arguments as a parameter of `main`; a program ends by returning from `main`.

**Included files** (v17, milestone 128). `include_bytes(path)` and `include_str(path)` build a file's contents into the program: lughac reads the file while checking (so `lughac check` and the language server report the errors too) and the compiled program holds the bytes, read-only, without reading anything when it runs. Rebuilding reads the file again. The argument is a string literal (not a constant or interpolation) holding a path relative to the folder of the `.la` file that names it, with `/` between folders; `.` and `..` may appear. The file must lie inside its package's folder — the folder of the `lugha.toml` that governs the file, or the main file's folder when there is none — after symbolic links are resolved, so no path, `..` or link reaches the builder's other files; and it must be a regular file of at most 16 MiB (16 777 216 bytes). Anything else is E0447, saying why: not one string literal, empty, absolute, holding `\`, outside the package's folder, missing, not a file, unreadable or too large. `include_str` of bytes that aren't UTF-8 is E0448, naming the first bad byte. Both calls are constant expressions (section 6), so a `const` may hold one; and like any value, `let mut b = LOGO; b[0] = 1;` changes only `b`. The bundled `std` modules and the prelude include nothing.

**Files** (v4). Paths are relative to the current directory, exactly as given; a path containing a NUL byte is an `Err`. Files are read and written as bytes, with no newline translation, and a whole file is read into memory. Every failure is a value (`Err` or `None`), never a panic.

**How values print** (v12; v18, milestone 170). A value has two texts. Its **`Display`** text is what `print`, `println`, `eprint`, `eprintln`, `to_string` and interpolation write: `interface Display { fun show(self): string; }`, implemented by the numbers, `bool`, `char` and `string`, and by a struct or enum only through `impl Display for T { … }`, never derived. Printing a value whose type lacks it is E0422, with help to write the block or show the debug form; on a bare `T` it names the bound `T: Display`. An interface value has it when its interface declares `show(self): string` or extends `Display` (as `Error` does), called through its table. Arrays, tuples, `Option`, `Result`, maps, sets, `()`, functions and handles have no `Display`. Its **debug form**, `x.debug()`, is the value as code writes it, for every type without asking: `interface Debug { fun debug(self): string; }`, from the prelude's derive blocks, or a type's own `impl Debug for T { … }` wherever the type appears (`with Debug` is E0463). `debug(x)` writes it to stderr with its place, and panics show values in it.

| Value | `Display` (`print`) | Debug form (`x.debug()`) |
| --- | --- | --- |
| a number or `bool` | as below | the same |
| a `string` | its bytes | quoted: `"a\"b"` |
| a `char` | its character, as UTF-8 (v14) | quoted: `'a'` |
| a struct or enum with `impl Display` | what `show` returns | its structure, below |
| a struct | — | `Point { x: 1, y: 2 }`, or `Point {}` without fields |
| an enum value, `Option`, `Result` | — | `Circle(1.5)`, `Empty`, `Some(5)`, `None`, `Ok(1)`, `Err("no")` |
| an array, a tuple | — | `[1, 2, 3]`, `[]`, `(1, "a")` |
| a map, a set | — | `Map { "a": 1 }`, `Set { 1, 2 }` |
| `()` | — | `()` |
| an interface value | its `show`, when it declares one | the value inside it |
| a function, a `Ref`, a task, a channel, an actor, a handle, a C union | — | `<fun>`, `Ref(…)`, `<task>`, `<chan>`, `<actor Counter>`, `<W>`, `<union>` |

- **Parts:** the fields, payloads and elements inside a debug form are in their own debug forms: a `string` is quoted, `"`, `\`, newline, tab and carriage return written `\"`, `\\`, `\n`, `\t`, `\r`, other bytes below 0x20 as `\u{XX}`, and every other byte as itself; a `char` is quoted too (`'a'`, `'\n'`, `'\''`). A type's `Display` is never used inside a debug form.
- **Names:** a struct shows its bare name, without its module path or type arguments (`Stack { items: [3] }`), and an enum value only its variant.
- **`Display` and `Debug`** are visible in every module, like `Iter`, unless the module declares the name. Both work as bounds (`T: Display` gives `x.show()`); `x.debug()` needs no bound, on any `T`. A type's own method named `debug` wins over the debug form, as own methods do.
- **Depth:** a recursive enum shows its full depth. Values are trees, so showing one always ends.

**Interpolated strings** (v3). Each `{expr}` is evaluated left to right and formatted exactly as `print` would write it (floats as below); the parts are joined into one new `string`. A value interpolates its `Display` (v18; any value v12–v17); `"{x.debug()}"` interpolates the debug form.

**Format specifiers** (v13). An interpolation may end in `:` and a specifier, `{expr:spec}`, which changes how its value is written. The lexer splits the specifier off at the first `:` outside every `(`, `[` and `{` of the expression and outside strings, and takes everything up to the closing `}`; an expression that needs such a `:` goes in parentheses. A specifier is `[[fill]align][#][0][width][.precision][kind]`:
- **`align`:** `<` left, `>` right, `^` centre (an odd gap's extra character goes right). Without one, numbers align right and everything else left. **`fill`**, written before an alignment, is any character other than `{` and `}` (v15; ASCII only before); the default is a space.
- **`width`:** at most 1024, counted in characters (Unicode scalar values) since v15 (milestone 63), in bytes before: `{"Zoë":<6}` adds three spaces. Shorter text is padded to it; longer text is never cut. A character counts as one whatever its displayed width; there are no grapheme clusters or East Asian widths.
- **`#`:** with a kind, prefixes `0x`, `0b` or `0o`. **`0`:** pads a number with zeros after its sign and prefix (`{-42:05}` is `-0042`, `{255:#06x}` is `0x00ff`), and is ignored when an alignment is given.
- **`.precision`:** floats only, at most 1024: that many decimals, as C's `%.*f` (`{3.14159:.2}` is `3.14`). Without it a float prints as `print` writes it.
- **`kind`:** integers only. `x` hex, `X` upper-case hex, `b` binary, `o` octal; a negative value prints its two's complement at its type's width (`{-1 as i8:b}` is `11111111`).
- **Other values** take only fill, alignment and width, applied to the text `print` would write: `{p:>20}` right-aligns a struct.
- **Errors:** an empty specifier, an unknown kind, trailing text, a width or precision over 1024, a brace fill, precision on a non-float, a kind on a non-integer, `#` without a kind, or `0` on a non-number is E0431 at the specifier.

**`format`** (v14). `format(template, a, b, …)` returns a `string`: the template's text with each hole replaced by the next argument, written as an interpolation would write it. The template must be a string literal, read raw (section 2). In it, `{}` is a hole, `{:spec}` a hole with a format specifier as above, and `{{` and `}}` are literal braces. `format("{} has {:>4} items", "box", 12)` is `"box has   12 items"`. Any value that prints may be an argument; `format` is the only function that takes any number of them. Errors (E0432): a template that isn't a literal, `{x}` or any other text inside a hole, a `{` never closed, a lone `}`, or a number of holes unlike the number of further arguments. A bad specifier is E0431, as in an interpolation.

`swap(a, b)` exchanges what two places hold, like `let t = a; a = b; b = t;` but without copying anything: swapping two arrays exchanges them in constant time, which makes double buffering cheap (`swap(ys, buf)`). Both arguments must be places of the same type (E0502, E0403), each following the assignment rules: the root is `let mut` (E0501), not inside a string (E0506), and not an array being iterated (E0507). The places are evaluated left to right, and elements are bounds-checked, before either is written. A place can't be swapped with a part of itself (E0509): `swap(n, n.kids[0])` would store `n` inside itself. Places that might be the same are fine, since swapping a place with itself changes nothing: `swap(xs[i], xs[j])`, `swap(p.a, p.b)` and `swap(a, a)` are all valid.

Floats print as the shortest decimal that round-trips, in Lugha float-literal syntax: always a `.` with at least one digit on each side, so `2.0` prints as `2.0`, not `2`. Values with magnitude at or above `1e16`, or below `1e-5` and nonzero, use an exponent: `1.0e16`, `2.5e-7`. Infinities and NaN print as `inf`, `-inf` and `NaN`; `-0.0` prints as `-0.0`. `bool` prints as `true` or `false`. `u8` prints as a number, not a character. `u32` and `u64` print as unsigned decimals (v7). An `f32` prints by the same rules as the shortest decimal that reads back as the same `f32`, so `0.1` prints `0.1`, not the digits of its `f64` value.

## 6. Functions and program structure

A v0 program is a single `.la` file containing top-level items: functions, extern declarations and structs. There are no top-level variables or statements; v17 adds constants (below), but still no variables.

**Constants** (v17, milestone 109). `[private | public] const NAME: T = value;` declares a module-level constant. The type is always written. Visibility, `m.NAME`, named imports, aliases and re-exports work as for functions; a constant shares its module's names with functions (E0302 for a clash), and a local may shadow it. The value is any expression of type `T` that is *pure* (milestone 147): nothing it runs, directly or through the functions, methods, function values and interface values it reaches, may reach outside the program. Reaching outside means the intrinsics `print`, `println`, `eprint`, `eprintln`, `flush`, `read_line`, `read_stdin_bytes`, `read_file`, `read_bytes`, `write_file`, `append_file`, `write_bytes`, `append_bytes`, `args`, `exit` and a `CPtr`'s `view` (C's memory); anything in std that uses the operating system (files, the network, the clock, the environment, random numbers, processes); calling an `extern` function; and `spawn` and `Chan.new` (so no tasks, channels or actors). Everything else is pure: locals, loops, recursion, `if`, `match`, closures, interpolation, arrays, maps, sets, `Ref` and `panic`. A generic function is judged at the type arguments the constant uses (`total(squares)` may be pure while `total(files)` isn't). A constant that isn't pure is E0445, whose help names the path: `` `load` calls `read_file`, which reads files ``. Nothing is evaluated while compiling (decision 212). Its type holds plain data: numbers, `bool`, `char`, `string`, `()`, and arrays, structs, tuples and enums of those (E0445 for functions, interfaces, `Ref`, `Task`, `Chan`, actors, handles, maps and sets). Every constant is computed once, before `main` (and before a test runs), each after those it reads — in its value or in any function it reaches — in any order of declaration, but not in a circle (E0446, the circle shown, through functions too); an overflow, division by zero or failed index panics then, at its expression. A constant is read like a variable and copied like one — `let mut xs = PRIMES; xs.push(1);` leaves `PRIMES` as it was — but is never assigned (E0501), changed in place, passed `mut` or swapped. There are no mutable globals: shared state lives in a `Ref` or an actor.

**Item order doesn't matter.** A function can call a function declared later in the file, and a struct or enum can use a struct or enum declared later. The type checker runs in two passes to support this:

1. Collect every struct name and function signature into a global table.
2. Resolve struct field and enum payload types, check for structs that contain themselves through structs alone (E0307), and mark the enum payload slots that lead back to their enum (indirect slots, section 7).
3. Check each function body against the global table.

**Modules (loading).** A program can span several files (v2, milestone 10; being built).
- **Files are modules.** A file is a module named by its path from the main file's directory, without `.la`, with `/` written as `.`: `shapes/circle.la` is `shapes.circle`.
- **Loading.** `import shapes.circle;` loads `<root>/shapes/circle.la`, where the root is the main file's directory. Each file is loaded once, however many files import it, so import cycles are fine.
- **Errors.** A missing file is E0312. These are E0314: importing the same module twice in one file, two imports whose last names clash, or an import named like one of the file's own items. Importing the main file is E0319.
- **Each file is checked for its own errors.** Invalid UTF-8 (E0110) and lexical and syntax errors in any module are reported against that module's file.
- **Order and cycles.** The passes above run over all modules together, so an item may be used before its declaration across files too.

**Packages** (v8). A package is a folder of modules that a program, or another package, lists in its manifest.
- **The manifest** is `lugha.toml`, in the main file's folder for the program and in a package's folder for that package; both are optional. lughac reads a small subset of TOML: blank lines, comments from `#` to the end of a line, a `[packages]` table and (v11) a `[package]` table, each at most once and in either order, and their lines:
  - under `[packages]`, `name = "path"` or (v11) `name = { path = "path", version = "R" }`, where `name` is a Lugha identifier, `path` a double-quoted string without `"` or `\`, relative to the manifest's folder with `/`, and the inline table's keys come in either order, `path` required and `version` optional; and (v18, milestone 188) `allow = ["ffi"]`, the one permission, in the program's own manifest only (E0321 elsewhere) — see **The `ffi` permission** below;
  - under `[package]` (v11), `version = "MAJOR.MINOR.PATCH"`, and (v15) `name = "ident"` and `source = "path"`, each at most once. A `name` is a Lugha identifier; a `source` is a relative path with `/`, not empty, with no `..`, so a package's modules never come from outside it.
  - under `[link]` (v16, milestone 87), lists of double-quoted strings on one line, `key = ["…", …]` (a trailing comma allowed), each key at most once: `libs` (C libraries, linked as `-l<name>`), `paths` (folders searched for libraries), `include` (folders searched for the headers of this package's C files), `c` (C files compiled into the program, ending in `.c`) and `pkg-config` (names given to the system `pkg-config`). A library or `pkg-config` name uses letters, digits and `_+.-` and doesn't start with `-`; a path is relative, with `/`, not empty, with no `..`. Section 8 says what they do.

  Anything else, a table twice, a name listed twice, an empty path, or a malformed version or requirement is E0321, at its line of `lugha.toml`; a path whose folder doesn't exist is E0322.
  ```
  # lugha.toml
  [packages]
  geo = { path = "libs/geo", version = "1.2" }
  ```
- **Versions** (v11). A version is `MAJOR.MINOR.PATCH`: three decimal numbers without leading zeros (`0` itself is fine) and without suffixes. A requirement is `MAJOR`, `MAJOR.MINOR` or `MAJOR.MINOR.PATCH`, the missing parts 0, and it accepts the compatible versions: those at or above it that keep every part up to its first non-zero written part (its last written part when all are 0). So `"1.2"` accepts 1.2.0 up to, but not including, 2.0.0; `"0.3"` accepts 0.3.x; `"0.0.4"` accepts only 0.0.4; `"0"` accepts 0.x.y. Once every module is loaded, each requirement in every manifest read is checked against the listed package's declared version; a package that no module imports has its manifest read for this. A version that doesn't meet a requirement, or a package that declares none, is E0324 at the requirement's line, naming the range and the version found. The program's own `[package]` version is allowed and unchecked. A git entry's version is its tag's, so its requirement isn't checked against the package's `[package]` version.
- **Git packages** (v14). An entry `name = { git = "URL", version = "R" }` names a package in a git repository; `path` and `git` are exclusive, and `git` needs `version`.
  - **URLs:** only `https://host/…` and `file:///…`. A URL with a user name or password, whitespace or control characters, or another scheme (`ssh://`, `git@host:…`, `git://`, `ext::…`) is E0321: credentials belong in git's own credential helper, never in a manifest.
  - **Resolving:** lughac runs `git ls-remote --tags` and takes the highest tag `vMAJOR.MINOR.PATCH` that `R` accepts, clones it (`--depth 1`, no submodules) into a scratch folder of the cache, and moves it into place as `<cache>/git/<escaped URL>/<commit>`, without its `.git`. A package's own git entries are resolved the same way. Packages from git otherwise behave like folder packages.
  - **The cache** is `$LUGHA_CACHE`, else `$XDG_CACHE_HOME/lugha`, else `$HOME/.cache/lugha`, each used only when it is an absolute path; with none, lughac stops with an internal error. A folder in place is used as is.
  - **The `ffi` permission** (v18, milestone 188): a package other than the program's own whose modules declare an `extern fun` or `extern type`, or whose manifest has a `[link]` table, runs C code, which can do anything the process can; it builds only when the program's `lugha.toml` grants it, `name = { …, allow = ["ffi"] }`. A grant covers the granted package and every package reachable from it through its manifests' entries. Reading C's memory through `p.view` (section 8) needs the grant too, since the program vouches for its length: an ungranted package is E0333 at its first `p.view`, found by the type checker, the only stage that knows the receiver is a `CPtr`; holding or passing a `CPtr`, and taking or indexing a view, need none. `lugha.lock`'s `[ffi]` table lists packages by their `extern` and `[link]` only. An ungranted one is E0333 at its first declaration, with help naming the program's entry to change (its own, or the dependency that brings it in); the check runs after loading, before anything in `[link]` is compiled or `pkg-config` runs. The program's own code and the bundled prelude and std need no grant. A package with `ffi` is fully trusted: the permission is documented as such, and C code isn't bound by capabilities.
  - **`lugha.lock`**, beside the `lugha.toml` governing the build (beside the main file when there is none; v17, milestone 97: before, always beside the main file), records each git package's URL, version and commit in the manifest's subset (a `[locked]` table of `name = { git = "…", version = "…", commit = "<40 hex digits>" }`), then (v18, milestone 188) an `[ffi]` table naming each package that may run C code and the grant that lets it, `zlib = { through = "sqlite" }` — a record for people and diffs, which no check reads. lughac writes it when a build resolves something new, through a temporary file and a rename; commit it with the program. One lock serves every program of a package, in `examples/`, `test/` or `src/` alike. A lock beside the main file is still read when the manifest's folder has none, and is then written beside the manifest (the old file is left alone). A locked entry whose requirement still accepts its version is used exactly: from the cache, or cloned again at its tag, whose `HEAD` must be the locked commit (E0326 otherwise, with help to run `lughac update`). A malformed lock line is E0321.
  - **Commands:** `lughac update prog.la` ignores the lock, re-resolves every git package, and rewrites the lock. `--offline` on `build`, `run` and `check` runs no `git`: a package not locked and cached is E0325.
  - **Failures:** no `git`, a failing `git` command (with the first lines of its message), no matching tag, or a malformed commit is E0325 at the entry.
  - **How `git` runs:** from an argument list, never a shell, with `--` before every URL and folder, and with `GIT_TERMINAL_PROMPT=0` (no password prompts), `GIT_ALLOW_PROTOCOL=https:file`, a low-speed timeout, hooks off (`core.hooksPath=/dev/null`), `core.fsmonitor=false`, `submodule.recurse=false` and `core.symlinks=false`, so a symlink in a package is checked out as a plain file holding its target's name and a package can't make lughac read files outside it. The tag lughac clones is rebuilt from the parsed version. The user's own git configuration (proxies, credential helpers) still applies.
  - **What runs:** lughac only reads a package's `.la` files, `lugha.toml` and the files its `include_bytes`/`include_str` calls name (v17, milestone 128; inside the package's folder, section 5); nothing in a package runs while building. A built program runs a package's code, `extern` calls included, like any code. lughac trusts git's commit ids and does no hashing of its own, and it can't protect against a package that is malicious when run, a compromised host, or a repository too large for the disk.
- **Package layout** (v15).
  - **`source`:** a package's modules start in its folder joined with `source` (its folder without one). With `source = "src"`, `src/array.la` of package `glowworm` is `glowworm.array`; module paths and symbols are as if the file sat at the package's top. A `source` folder that doesn't exist is E0322.
  - **`name`:** a manifest that lists a package with a declared name under another key is E0327 at the listing, with help to list it by its name and to use an import alias (`import glowworm.array as gw;`) for a short one. A package without `name` is listed under any key, as before.
  - **The program's manifest:** the `lugha.toml` beside the main file, as before; when there is none, the nearest one in a parent folder. The lookup stops, finding nothing, at a folder that holds `.git` or is the home folder (`$HOME`), after checking that folder itself; so a repository's root manifest is found, and one in a shared folder above a repository or the home folder (`/tmp`, `/`) never is. A found manifest's `[packages]` are the program's, with paths relative to its folder.
  - **The implicit package:** when the program's manifest declares `name = N`, the program may import that package as `N` (`import N.array;`), with its `source`, as if listed. So a library's `examples/` and `tests/` programs need no manifest of their own. `N` may not also be a folder or `.la` file beside the main file (E0320).
  - **Programs inside the package** (v17, milestone 97): a program whose manifest is its package's own, such as one in `test/`, `tests/` or `examples/`, belongs to that package for visibility: it sees the package's items without `public`, as the package's modules do. `private` items stay private to their module.
  - **A package's file as its module** (v17, milestone 97): `lughac test` and `lughac check` build a file inside its package's `source` folder as that package's module (`glow/src/diff.la` → `glow.diff`), imported by a hidden entry rather than being a main file: modules of the package may import it (no E0319), even in a cycle, and it needs no `main`. `build` and `run` still take it as a main file.
  - **The entry** (v17, milestone 99): a package's `lib.la` at its module root (its `source` folder, or its folder without one) is its entry. `import N;` binds it as `N` (`import N as k;`, `import N { X, y };` and `public import N;` too); its module path is `N`, its symbols `lugha_fn_N/f`, and a test in it is named `N.name`. It is an ordinary module: `import N.module;` still reaches the others, with the same visibility. `import N;` for a package without `lib.la` is E0312, its help naming both remedies; the entry has one name, so `import N.lib;` is E0312 too (a `lib/` folder's modules, `N.lib.x`, are unaffected). `std` has no entry. There is no manifest key for it: renaming a package moves no file.
  - **One file, one module:** a file reached under two module paths, such as `src/cli.la` as `cli` from `src/main.la` and as `tool.cli` through the implicit package, is E0328 at the second import.
  - **Run:** `lughac run glowworm/examples/basics.la` works from any folder; the file argument is required.
- **The standard library** (v15, milestone 53). `import std.<module>;` loads a module bundled with `lughac`, written in Lugha, the same in every build of a given compiler version. It is read and checked only when a file imports it, so a program that imports nothing from `std` is unchanged, and of an imported module only the functions and methods the program reaches (by calls, bounds, interface values or as function values) are compiled (milestone 60); there is no way to opt out, and none is needed. Std modules follow the normal rules: their `public` items, `import std.math as m;`, `import std.math { sqrt };`. `std` is reserved: a manifest entry or `[package] name` called `std` is E0321, a folder `std/` or file `std.la` beside an importing module is E0320, and an unknown module (`import std.maths;`) is E0312, with help listing the std modules. Diagnostics inside a std module name its file `<std>/math.la`. The modules are listed under "The standard library" below.
- **Imports.** In `import a.b.c;` written in a module of package P (the program itself is the root package): if P's manifest lists `a`, the import loads `b/c.la` from `a`'s folder, and the module is `a.b.c`; otherwise it loads `a/b/c.la` from P's own folder, as before. So inside package `geo`, `import util.names;` is geo's own `geo.util.names`, and a program's `util.names` is a different module. `import geo;` naming only a package is E0312, with help to name a module.
- **Not transitive.** A package's manifest is seen only by that package's modules: a program that wants `mathx` lists it itself, even if a package it uses lists `mathx` too.
- **One name, one folder.** Across all the manifests of a build, a package name must mean one folder (compared after resolving `.` and `..`), or it is E0323 at the later entry. Because the root package's modules have no package prefix, no package name may also be a folder or `.la` file in the main file's folder, and inside a package a listed name may not also be one of that package's own folders or files: either is E0320.
- **Loading.** Every file is read once, so packages may import each other. A package has no main file; `main` in one is E0304, and importing the program's main file is E0319, as for any module.
- **Visibility.** Another package sees only what is `public` (below).

**Green threads** (v17, milestone 91). `s.spawn` starts a task, a green thread, in scope `s` (milestone 195); they run on the program's worker threads (v18, milestone 210): one per CPU the process may use, or `LUGHA_WORKERS=n` (1 to 256; anything else ends the program at start with `panic: LUGHA_WORKERS must be from 1 to 256`). The main thread is the first; the others start at the first spawn, so a program that never spawns runs one thread. Tasks on different workers run at the same instant, and a task may move from one worker to another when it waits or is preempted. There are still no data races in Lugha code: a value reaching another task is a copy or marked shared (section 7), and a `Ref` never leaves its task. The order in which different tasks' effects happen, and so their output, isn't fixed; one `println` always prints its line whole. A deterministic or shuffled run (`lughac test`, section 9) uses one worker.
- **Switching:** a green thread runs until it waits — `join` on an unfinished task, `send` or `recv` that can't go on, a `select` that waits, `task.sleep`, `task.yield_now` — or is preempted. Waiting threads wake in the order they began to wait; sleepers in the order of their deadlines. A new thread starts after the threads already ready. With several workers (milestone 211) each worker keeps its own queue of ready threads, in this order; an idle worker steals half of a busy one's; and a thread woken by a running one (a send, a join, an actor's reply) runs next on its waker's worker, at most three times in a row, so two threads passing messages stay on one core. Threads passing messages on different channels, or calling different actors, don't wait on each other (milestone 212): each channel, actor and task keeps its own lock. Neither do threads spawning in different scopes (milestone 213): a scope counts its tasks in and out, and its owner waits for them, under its own lock. A pinned thread (milestone 214: one that touched a pinned handle, or in a `pinned` scope) runs only on its worker: woken, it goes to that worker's own queue, which no other worker takes from, and the worker takes from it in turns with its other ready threads. A timer or socket event meant for a wait that has already ended is dropped, never delivered to the thread's next wait. The order says which thread a worker takes first, not which finishes first. With one worker the order is exactly as stated.
- **Time** (milestone 203): every timer the runtime keeps (sleeps, `after`, deadlines, shields, socket timeouts) reads its monotonic clock, real time in a program; under `lughac test` a plain test's is virtual (section 9).
- **Preemption:** in a program that uses `spawn`, every function entry and every loop iteration counts down a step budget; when it runs out (every 4096 steps) the running thread lets the ready ones run first. There are no signals and no timers, so on one worker a program's interleaving is the same on every run given the same input and timing of sleeps. Programs without `spawn` compile exactly as before.
- **Blocking calls:** sockets (`Net`, milestone 110) never block: a thread waiting to accept, connect, read or write is parked until its socket is ready, and the others run. The runtime's other waiting work (v18, milestone 206) — a `Dir`'s and a `File`'s operations but `close`, `read_line`, name lookups (`resolve`, and `connect`'s and `listen`'s), `Proc`'s `run` from start to exit — runs on a helper thread while its thread is parked. Helpers are OS threads started one at a time when a job finds none idle, at most 16 (`LUGHA_HELPERS=n`, 1 to 256; any other value ends the program at start with `panic: LUGHA_HELPERS must be from 1 to 256`), none before process lockdown, so the kernel limits them as it does the program; each blocks every signal, and works only on copies of its inputs, never on the program's values. At most 256 jobs are given out at once; a thread wanting to give one more waits its turn. A thread cancelled while its job runs stops waiting at once; the job finishes (a started program is still waited for) and its results are dropped. In a deterministic run (section 9), and inside a C callback, the work runs in place, so nothing else runs meanwhile and no virtual time passes. Writing to the console, the environment, clocks, randomness and plain `extern` calls still run on the program's thread; an `extern blocking fun`'s calls run on a helper (v18, milestone 207; section 8). When no thread is ready, the program waits for a socket, a helper's job or the next sleeper.
- **The end:** when `main` returns, the root scope's tasks finish, then the program exits with `main`'s code (milestone 195). A panic in a task fails its scope (section 6); one the program never catches — in `main`, or in a root-scope task — ends it with its message and exit code 101.
- **Ctrl-C** (milestone 199): the first SIGINT cancels the root scope's tasks and `main` with `.Shutdown`, whether or not the program took anything for signals; they unwind at their next pause or preemption point, running their `defer`s, and the program exits with 130 once `main` has unwound. In a program that never started a task, the first SIGINT is held until `main`'s next pause (any wait, which starts the scheduler), where it cancels `main` the same way, so its `defer`s run; no preemption checks are added to such programs, so a loop that never pauses goes on (v18, milestone 226). A second SIGINT exits with 130 at once.
- **Deadlock:** when no green thread can run, none is sleeping, none waits on a socket or a helper's job and one waits, the program panics with `panic: deadlock: every green thread is waiting` (no location), exit 101. Then (milestone 200) one line per task waiting on a channel names it, how it waits and where the channel was made: ``  `worker` waits to receive on the channel made at main.la:3:20`` (`to send on`, `in a select, on`; an actor as ``actor `Name` ``, `main` as `` `main` ``).
- **Actors** (milestone 92) are tasks of their scope (milestone 201) that take messages from a bounded mailbox one at a time: messages from one caller in the order sent, from several in the order they arrive. A panic inside an actor's method unwinds the actor's stack, running its `defer`s (v18, milestone 194), ends the actor and fails its scope as a task's panic does; the caller waiting for that message's result, and every later caller, panics with ``actor `Counter` panicked`` at its call; a call to an actor that has ended otherwise (stopped, or cancelled with its scope) panics with ``actor `Counter` has stopped``. A stack overflow inside an actor still ends the program. Tasks keep the rule above. Actors waiting on each other are a deadlock.

**The standard library** (v15). Version 15 has `std.math` and `std.random` (and had `std.env`, `std.process` and `std.fs`, which became the `Env`, `Proc` and `Dir` capabilities in v18, milestones 181 and 182); version 17 adds `std.task`, `std.time`, `std.net` (the `Net` capability since milestone 183), `std.tls`, `std.crypto`, `std.http`, `std.toml` and `std.json`.

`std.math` (milestone 53). The float functions take and return any `T: Float` (`f32` or `f64`) and follow IEEE 754: they never panic, `sqrt(-1.0)` is `NaN`, `ln(0.0)` is `-inf` and `pow(0.0, -1.0)` is `inf`.

| Function | Result |
| --- | --- |
| `pi(): f64`, `e(): f64` | π and e (top-level constants are a later feature) |
| `sqrt(x)`, `exp(x)`, `ln(x)`, `log10(x)` | square root, eˣ, natural and base-10 logarithm |
| `pow(x, y)` | xʸ |
| `sin(x)`, `cos(x)`, `tan(x)` | in radians |
| `atan2(y, x)` | the angle of the point (x, y), in −π..π |
| `floor(x)`, `ceil(x)` | rounded down, up |
| `round(x)` | to the nearest integer value, halves away from zero (`round(-2.5)` is `-3.0`) |
| `abs<T: Num>(x)` | `-x` for a negative `x`; the minimum integer panics with `integer overflow`, as `-x` does |
| `min<T: Ord>(a, b)`, `max<T: Ord>(a, b)` | the smaller, larger; `a` when they are equal |
| `clamp<T: Ord>(x, lo, hi)` | `x` limited to `lo..=hi`; panics (`clamp: the lower bound is above the upper bound`) when `lo > hi` |

An integer goes to a float function through a cast: `math.sqrt(n as f64)`; `math.sqrt(2)` is E0419.

`std.random` (milestone 54). A `random.Rng` is a pseudo-random generator, xoshiro256\*\* (Blackman and Vigna) seeded through splitmix64. It is fast and statistically strong but **not cryptographically secure**: never use it for keys, tokens, passwords or anything an attacker must not predict. There is no global generator: a program makes one and passes it where it's needed; every method takes `mut self`, so the generator must be `let mut`.

| Function or method | Result |
| --- | --- |
| `seeded(seed: u64): Rng` | a generator whose numbers depend only on `seed`: one seed gives one sequence on every platform, at `-O0` and `-O2`, and in every v15 compiler |
| `from_entropy(entropy: Entropy): Rng` | a generator seeded with `entropy.next_u64()` (v18, milestone 181): from the real `Entropy`, different on every run |
| `r.next_u64(): u64` | the next 64 random bits |
| `r.uniform(): f64` | uniform in [0, 1), with 53 random bits; for `f32`, `r.uniform() as f32` |
| `r.below(n: i64): i64` | uniform in `0..n`, without bias; panics unless `n > 0` |
| `r.between(lo: i64, hi: i64): i64` | uniform in `lo..hi`, half-open like `for i in lo..hi`; panics unless `lo < hi` |
| `r.normal<T: Float>(mean: T, std: T): T` | a normal (Gaussian) value, by Box–Muller, in the arguments' type |
| `r.chance(p: f64): bool` | `true` with probability `p` |
| `r.choice<T>(xs: T[]): Option<T>` | a uniformly chosen element, `None` for an empty array |
| `r.shuffle<T>(xs: T[]): T[]` | a copy of `xs` in a uniformly random order (Fisher–Yates); `xs` is unchanged |

**Folders and files** (`Dir`; `std.fs` from milestone 65 until v18 moved it onto `Dir`, milestone 182). The methods of a `Dir` (section 6), paths relative to it. Errors are the system's message, as for `read_file`; a path holding a NUL byte is `Err("path contains a NUL byte")` (or `false`). Nothing deletes recursively.

| Method | Result |
| --- | --- |
| `read_file(path)`, `write_file(path, text)`, `append_file(path, text)` | whole files as text (section 6) |
| `read_bytes(path): Result<u8[], string>` | the whole file as bytes (v14) |
| `write_bytes(path, data)`, `append_bytes(path, data): Result<i64, string>` | like `write_file` and `append_file`, writing exactly `data` (v14) |
| `list(path): Result<string[], string>` | the entries' names sorted by their bytes, without `.` and `..`; a name that isn't UTF-8 is `Err("a name in <path> is not valid UTF-8")` |
| `make_dir(path): Result<(), string>` | makes one folder (mode 0777 less the umask); its parent must exist |
| `make_dirs(path): Result<(), string>` | makes the folder and any missing parents; already existing is fine |
| `remove_file(path): Result<(), string>` | removes a file or symbolic link; a folder is `Err` |
| `remove_dir(path): Result<(), string>` | removes an empty folder; a non-empty one is `Err` |
| `rename(from, to): Result<(), string>` | `rename(2)` (v17, milestone 115): within one file system it replaces `to` in one step, so anyone opening `to` sees the old file or the new one, never neither; across file systems it is `Err` (`Invalid cross-device link`), never a copy. Write a new version beside the old, then rename it over, to save data safely |
| `set_mode(path, mode: i64): Result<(), string>` | `chmod(2)` (v17, milestone 115), following symbolic links; `mode` from 0 to 4095 (`0o7777`), else `Err`. a leading zero is still decimal (`0600` is six hundred, W0107): write `0o600` |
| `sync(path): Result<(), string>` | `fsync(2)` (v17, milestone 116): a file's data, or a folder's entries, written out to the disk so they survive a power cut; `Err` (the data may not be there) with the system's message. A file is saved durably with `write_file("f.tmp", data)`, `sync("f.tmp")`, `rename("f.tmp", "f")`, `sync(".")` — the last makes the rename durable |
| `size(path): Result<i64, string>` | the size in bytes of the regular file at `path` (v17, milestone 153), from `stat(2)`, following symbolic links; nothing is opened. A folder is `Err("Is a directory")`, any other kind `Err("not a regular file")`, a missing or unreadable path `Err` with the system's message. Check it before `read_bytes` to refuse a file too big to read whole |
| `exists(path)`, `is_dir(path)`, `is_file(path)`: `bool` | whether something, a folder or a regular file is at `path`, following symbolic links; never an error |
| `open(path): Result<File, string>` | a `File` for reading (v17, milestone 146) |
| `create(path): Result<File, string>` | a `File` for writing, emptied first or made (mode 0666 less the umask) |
| `append(path): Result<File, string>` | a `File` for writing at its end (`O_APPEND`), made if missing |

**File streams** (v17, milestone 146). A `File` (a prelude interface since v18, milestone 182, so a fake `Dir` can give fake files; `write_text` is written over `write`) is a shared handle to one open file, like `net.Conn`: every copy is the same stream, and nothing closes one but `close`, so a file left open stays open until the program exits. Each is opened close-on-exec, for reading or for writing, never both; there is no seeking. Opening, reading and writing run on a helper thread (v18, milestone 206), so a FIFO or a terminal that waits stalls only its own green thread; each read or write costs a hand-over to the helper and back, about 20 µs. Using a writing `File` to read is `Err("not open for reading")`, the reverse `Err("not open for writing")`, and any use after `close` is `Err("closed")`.

| Method | Result |
| --- | --- |
| `f.read(max): Result<u8[], string>` | up to `max` bytes (1 to 16777216, else `Err("max must be from 1 to 16777216")`): what `read_line` read ahead first, else one `read(2)`; `[]` at the end of the file |
| `f.read_line(): Result<Option<string>, string>` | the next line without its `\n` and one `\r` before it, as `read_line()` reads stdin: a last line without `\n` is still a line, `None` at the end; read in 64 KiB pieces, with no length limit. A line that isn't UTF-8 is `Err("line N is not valid UTF-8")` (counting the lines `read_line` gave) and is skipped |
| `f.write(bytes: u8[]): Result<(), string>` | every byte, straight to the file (`write(2)` until done): nothing is buffered, so nothing is lost if the program stops without closing |
| `f.write_text(s: string): Result<(), string>` | `s`'s bytes, as `write` |
| `f.close()` | closes the file and every copy of it; closing twice is fine |

**Using another module.** `import shapes.circle;` binds the name `circle` in that file only.
- **Calls:** `circle.area(c)` calls function `area` of the module.
- **Types:** `circle.Circle` names its type in annotations; `circle.Circle { r: 1.0 }` builds it; `circle.Circle.new()` calls an associated function; `circle.Shape.Round(c)` and `circle.Shape.Flat` build variants.
- **Methods:** a method call on a value from another module (`c.area()`) works like any method call.
- **Errors:** an unknown module name is E0301; an unknown function or type in a known module is E0301 or E0305.

**Named imports and re-exports** (v11). An import may list items after its path:
- **`import a.b { X, y };`** imports module `a.b` as before (binding `b`), and binds each listed name in this file to that module's item: a function, extern, struct, enum or interface, written without a prefix (`X { … }`, `y(1)`, `X.f()`, `X.V`, `T: X`). Each must be an item of the module (E0301) visible here (E0313). A listed name the file also declares is E0302; one listed twice, or named like an imported module, is E0314.
- **`public import a.b { X, y };`** does the same, and makes `X` and `y` items of this module, visible to every package: a module that imports this one writes `m.X` and `m.y`, and may re-export them again. A re-exported item must itself be `public` (E0318). `private import` is E0201.
- **Same items:** a named import is the item itself, not a copy: `shapes.Vec2` and `vec.Vec2` are one type, calls reach one function, and types print with the declaring module's path (`mathx.vec.Vec2`). A name imported without `public` is the file's own: another module writing `m.X` for it is E0313. Re-exports don't change manifests: a program uses `shapes.Vec2` without listing `mathx`, and still can't `import mathx.vec;` unless it lists `mathx` itself.

**Aliases and whole-module re-exports** (v14).
- **`import a.b as c;`** binds the module as `c` instead of `b`; `b` is not bound. Aliases clash like any import name (E0314), so `import a.util as u1; import b.util as u2;` is how two `util`s share a file.
- **`import a.b { X as Y };`** binds the item as `Y` only. With `public`, the item is re-exported as `Y`.
- **`public import a.b;`** (or `public import a.b as v;`) makes the module a member of this one, visible to every package. A file that imports this module as `m` names `v`'s items `m.v.X`: calls `m.v.f()`, types `m.v.T`, literals `m.v.T { … }`, associated calls `m.v.T.f()`, variants `m.v.E.V`, bounds `T: m.v.I`, and function values `m.v.f`. Only `public` items are reached this way (E0313). One re-export is followed: a module re-exported by a re-exported module is reached through its own import.
- **Same items:** as with named imports, `m.v.X` is the item itself, so keys, types and symbols are the declaring module's.

**Glob re-exports** (v17, milestone 100).
- **`public import a.b { * };`** re-exports every `public` item of `a.b` — functions, externs, extern types, structs, enums and interfaces — as items of this module, exactly as if each were listed by name: `public import raw { * };` in `gtk` lets a program write `import gtk { GtkWidget, gtk_box_new };` or `gtk.gtk_box_new()`. Items `a.b` doesn't declare `public` are left behind. Like a named import, it also binds `b` in its file.
- **Chained:** items `a.b` itself re-exports by name, or with its own `{ * }`, come along; modules it re-exports whole (`public import x;`) stay members (`m.b.x`, not `m.x`). A cycle of globs adds each item once.
- **Only re-exports:** `*` needs `public` (`import a.b { * };` is E0201: list the items or import the module) and stands alone in its braces (`{ *, X }` is E0201).
- **Clashes:** a name a glob brings that the module already has — its own item, another import's name, another glob's — is E0314 at the `*`, naming the name and both sources, unless both are the same item. Nothing is shadowed silently.
- **Same items:** a glob-re-exported name is the declaration itself, as for named imports; the editor's hover, Go to Definition, completion, references and rename reach the declaration.

**Names.** Each module has its own namespace for functions, structs, enums, interfaces and externs, so two modules may each declare `Point`; duplicates within a module are a compile error. Intrinsics and the built-in names `Option`, `Some`, `None`, `Eq` and `Ord` are visible in every module, and no program may declare them (E0302). Methods live in their type's namespace, so a function `dist` and a method `Point.dist` can coexist. No local may be named like a type or an imported module (E0311, section 4). Local variables may shadow function names. Names resolve from the innermost scope outward: block scopes, then function parameters, then the module's own items and named imports (v11), then imported module names. In messages a type from another module is shown with its full module path (`shapes.circle.Circle`); a main-file type keeps its bare name. `main` belongs in the main file (E0304 elsewhere).

`std.task` (v17, milestone 91). Pausing the running green thread; `spawn`, `Task`, `Chan`, `Sender` and `Receiver` need no import.

| Function | Effect |
| --- | --- |
| `sleep(ms: i64)` | pauses this green thread for at least `ms` milliseconds while the others run; `ms <= 0` is `yield_now()` |
| `yield_now()` | lets every ready green thread run first, then goes on |

`std.time` (v17, milestones 115 and 145). Calendar dates; no zone names, no formatting by locale, no parsing. The moments and the system's offset come from a `Clock` (section 6; milestone 181).

| Function | Gives |
| --- | --- |
| `civil(ms: i64, offset_s: i64): Result<Civil, TimeError>` | moment `ms` (any `i64`, before 1970 too) as a date and time seen `offset_s` seconds east of UTC (v17, milestone 145) |
| `from_civil(c: Civil, offset_s: i64): Result<i64, TimeError>` | the moment a date and time at that offset names, the inverse of `civil`; `c.weekday` is ignored |

`Civil { year, month, day, weekday, hour, minute, second, ms }`, every field `public` and `i64`, is a date on the proleptic Gregorian calendar (year 0 is 1 BC) and a time of day: month 1–12, day 1–31, weekday 1 (Monday) to 7 (Sunday) as in ISO 8601, hour 0–23, minute and second 0–59 (no leap seconds), ms 0–999. Offsets are in seconds, as the system gives them, so historical offsets that aren't whole minutes round-trip. A `TimeError { message }` (printing as its message) is an offset beyond ±26 hours, a field out of its range or a date that doesn't exist (`no day 31 in month 2 of 2026`), a moment past `i64`, or one the system can't give a local time for. Nothing panics on any input: `civil(i64.MIN, 0)` is `-292275055-05-16 16:47:04.192`.

**The network** (`Net`; `std.net` from milestone 110 until v18 made it the prelude's `Net`, milestone 183). TCP sockets; UDP sockets, multicast and network interfaces (milestone 163, below). The functions are `Net`'s methods and the connections' types the prelude interfaces `Listener`, `Conn` and `Udp`. Waiting pauses only the waiting green thread (above). Addresses are `"host:port"` or `"[v6]:port"`; port `0` listens on any free port. Names resolve through the system's resolver, which blocks the OS thread while it looks one up. Failures are `Err` with a reason (the system's message, `"closed"`, `"timed out"`); nothing here panics. `Listener` and `Conn` share their socket between copies, so closing one copy closes them all; closing twice is harmless; nothing closes a socket but `close`, so an unclosed one stays open until the program exits.

| Function or method | Effect |
| --- | --- |
| `listen(addr: string): Result<Listener, string>` | listens (reusing the address, a backlog of 128) |
| `connect(addr: string): Result<Conn, string>`, `connect_timeout(addr, ms)` | connects, trying each address the name gives |
| `resolve(name: string): Result<string[], string>` | every address the resolver gives for `name` (no port), IPv4 and IPv6 as text (`"93.184.215.14"`, `"2606:4700::1"`), in its order, each once; an IP address gives itself; `Err` with the resolver's reason (milestone 121) |
| `l.accept(): Result<Conn, string>` | the next connection |
| `l.addr(): string`, `c.addr(): string`, `c.peer(): string` | `host:port` of this end or the other; `""` once closed |
| `l.set_timeout(ms)`, `c.set_timeout(ms)` | an `accept`, read or write waiting longer is `Err("timed out")`; `0` waits forever (the default) |
| `c.read(max: i64): Result<u8[], string>` | what has arrived, 1 to `max` bytes (`max` from 1 to 16777216), waiting for some; `[]` once the other end has finished |
| `c.write(bytes: u8[]): Result<(), string>` | every byte, waiting while the connection is full; a closed peer is `Err`, never a signal |
| `l.close()`, `c.close()` | closes the socket |

**UDP** (milestone 163). `udp_bind(addr): Result<Udp, string>` binds a UDP socket (`"0.0.0.0:0"`, `"[::]:5353"`) that no other socket may share; `udp_bind_shared(addr)` sets `SO_REUSEADDR` and `SO_REUSEPORT` first, so sockets that also share may bind the same port (mDNS's 5353 beside Avahi). Sharing is a separate function because other programs of the same user can then bind the port and receive its unicast datagrams. A `Udp` shares its socket between copies, like `Conn`; waits park only their green thread, and `set_timeout(ms)` makes `send_to` and `recv_from` give `Err("timed out")`. A socket hears only the multicast groups it joined itself (Linux's `IP_MULTICAST_ALL` is turned off), so `leave` stops it.

| Method or function | Effect |
| --- | --- |
| `u.send_to(bytes, addr): Result<(), string>` | one datagram (may be empty) to an IP address and port (`"192.168.1.20:5353"`, `"[ff02::fb%eth0]:5353"`); a name is `Err("address must be an IP address with a port: …")`, since looking one up would block the thread |
| `u.recv_from(max): Result<(u8[], string), string>` | the next datagram and its sender as `ip:port` (`[ip%interface]:port` for IPv6), waiting for one; `max` from 1 to 65536; a longer datagram is `Err("datagram of N bytes is longer than max")` and is lost, never cut short. A sender's address can be forged |
| `u.join(group, iface)`, `u.leave(group, iface)` | joins or leaves a multicast group on an interface named `iface` (`""` for the system's choice); an IPv4 group needs an IPv4 socket and an IPv6 group an IPv6 one; not a multicast address, or no such interface, is `Err` |
| `u.set_multicast_ttl(ttl)` | routers multicast sent from here may cross (IPv6's hop limit), 1 to 255 |
| `u.set_multicast_loop(on)`, `u.set_multicast_interface(iface)` | whether multicast sent here reaches this machine's sockets; which interface it leaves by |
| `u.addr()`, `u.set_timeout(ms)`, `u.close()` | as `Conn`'s |
| `interfaces(): Result<NetInterface[], string>` | the machine's interfaces, each once: `NetInterface { name, index, up, multicast, addresses }`, addresses as text (link-local IPv6 with `%name`) |

No broadcast, connected UDP sockets, source-specific multicast or mDNS.

`std.tls` (v17, milestone 112; over any `Conn` since v18, milestone 184). TLS through the system's OpenSSL (3.2 or newer since milestone 161), linked only into programs that load `std.tls` (directly or through `std.http`); building one without `libssl-dev` is E0332. OpenSSL works on memory buffers and `std.tls` carries the bytes over the `Conn` underneath, so TLS runs over whatever a `Net` gives, a fake one included: the C side never opens, reads or waits on a socket or a file. Certificates are always verified — the chain against the system's trusted roots and the host name (or IP address) — and nothing turns that off; `Options { ca_pem, timeout_ms, address }` (`options()` for the defaults) adds the certificates in `ca_pem`, PEM text, to trust; `address` (milestone 121), when not `""`, is an IP address (`"127.0.0.1"`, `"::1"`; anything else is `Err("address must be an IP address: …")`) that `connect_with(net, addr, opts)` connects to at `addr`'s port instead of resolving `addr`'s host, which is still sent as SNI and must still be named by the certificate — like curl's `--resolve`: resolve once with `net.resolve`, check the addresses, connect to the one checked. TLS 1.2 is the oldest version spoken. `connect(net, addr)` and `connect_with(net, addr, opts)` connect through the `Net` and give a `SecureConn`; `listen(net, addr, cert_pem, key_pem)` listens through it and gives a `SecureListener` (`accept` runs the handshake, which only the accepting thread waits for; `accept_secure()` gives the connection as a `SecureConn`); `client(conn, host, opts)` and `server(conn, cert_pem, key_pem)` run TLS over a connection made any other way, closing it on failure. A `SecureConn` is a `Conn` and a `SecureListener` a `Listener` (`peer()` and `addr()` are the connection's underneath; `close` sends `close_notify` without waiting for the reply). Certificates and keys are PEM text — the chain with the server's first, an unencrypted key (an encrypted one fails rather than prompting) — read by the caller, through a `Dir`; the only file `std.tls` reads is the system's trusted roots. A verification failure is `Err("certificate verify failed: …")` with OpenSSL's reason; a timeout is `Err("timed out")`. Waits are the `Conn`'s: with a real one only the green thread waits. A peer closing without `close_notify` reads as the end of the stream. OpenSSL's objects aren't counted and are freed by `close`. No client certificates or session resumption.

**Connections by key** (milestone 161). `Keys { secret, accept }` holds this side's 32-byte Ed25519 secret seed (as `std.crypto` makes it) and the 32-byte public keys it accepts. `listen_keys(net, addr, keys)`, `connect_keys(net, addr, keys, timeout_ms)` and, over any `Conn`, `client_keys(conn, keys, timeout_ms)` speak TLS 1.3 only (which encrypts the client's key on the wire) with raw public keys (RFC 7250) and no certificates: each side proves its key, and each accepts only a peer whose key is on its list, checked by OpenSSL in the handshake. A server refuses a client with no key, a certificate or a key not on its list, and `accept` never returns it; a client refuses a server key not on its list with `Err("peer key not accepted")`. Because TLS 1.3's client finishes its handshake before the server checks the client's key, a client the server refuses gets its `Conn` and then an error on its first read. No host name is sent or checked. An empty `accept` refuses everyone; a key of the wrong length is `Err("secret key must be 32 bytes")` or `Err("accepted key {i} must be 32 bytes")`. `listener.set_accept(keys)` replaces a by-key listener's list for the connections it accepts next (`Err("not a by-key listener")` on a certificate listener, `Err("closed")` once closed); `conn.peer_key()` on a `SecureConn` (from `accept_secure()` on the server's side) gives the key the peer proved (`[]` on a certificate connection or once closed). Keys from files, `std.http` by key and a callback that decides are not included.

`std.crypto` (v17, milestone 113). SHA-256, Ed25519 (RFC 8032), random bytes, constant-time comparison, HKDF-SHA256 (RFC 5869), ChaCha20-Poly1305 sealing (RFC 8439) and X25519 key exchange (RFC 7748; both milestone 160) and HPKE (RFC 9180; milestone 162) through the system's OpenSSL (3.2 or newer since milestone 161), linked only into programs that load `std.crypto` (`-lcrypto`; E0332 without `libssl-dev`). Keys, signatures and digests are plain bytes, and programs never see an OpenSSL object. Nothing panics on any input: mistakes are `CryptoError` (`BadLength(what, wanted, given)`, `OpenSsl(reason)`, `NotAuthentic` — sealed bytes that don't match their key, nonce and associated data — and `BadKey(why)` — a low-order X25519 peer key; `e.message()` reads one), or `false`.

| Function | Gives |
| --- | --- |
| `sha256(bytes)` | the 32-byte digest |
| `Sha256.new()`, `h.update(bytes)`, `h.finish()` | a hash fed in pieces; `finish` gives the digest and starts over; copies share one running hash; nothing needs closing (OpenSSL's state goes with the last copy) |
| `ed25519_keypair(entropy: Entropy): Result<(u8[], u8[]), CryptoError>` | a 32-byte secret seed drawn from `entropy` (v18, milestone 181; a fake gives the same key every time) and its 32-byte public key |
| `ed25519_public(secret)` | the public key of a 32-byte seed (`BadLength` otherwise) |
| `ed25519_sign(secret, message)` | the 64-byte signature |
| `ed25519_verify(key, message, signature): bool` | `true` only for a valid signature; `false` for anything else — wrong lengths, a malformed key — never an error |
| `equal(a, b): bool` | the same bytes, compared in time that depends only on the length; `false` at once for different lengths |
| `hkdf_sha256(key, salt, info, length)` | `length` bytes (1 to 8160, `BadLength` otherwise) of key material derived by HKDF-SHA256, RFC 5869 (extract then expand; an empty salt is the RFC's default; milestone 120) |
| `chacha20poly1305_seal(key, nonce, plaintext, aad)` | the ciphertext with the 16-byte tag appended (RFC 8439); `key` 32 bytes, `nonce` 12 (`BadLength` otherwise), `plaintext` and `aad` up to 1073741824 bytes, either may be empty; a key and nonce must never seal twice (`entropy.bytes(12)` is safe for about 2³² messages per key) |
| `chacha20poly1305_open(key, nonce, sealed, aad)` | the plaintext, only when the tag matches; `NotAuthentic` (and no plaintext) for any changed byte or wrong key, nonce or `aad`; `sealed` at least 16 bytes |
| `x25519_keypair(entropy: Entropy): Result<(u8[], u8[]), CryptoError>` | 32 secret bytes drawn from `entropy` (v18, milestone 181) and their 32-byte X25519 public key |
| `x25519_public(secret)` | the public key of 32 secret bytes (any 32 bytes are a secret) |
| `x25519_shared(secret, peer_public)` | the 32-byte shared secret (RFC 7748), the same from either side; `BadKey` for a low-order peer key (an all-zero result); derive keys from it with `hkdf_sha256` |
| `hpke_seal(public, info, plaintext, aad)` | HPKE (RFC 9180) base mode with DHKEM(X25519, HKDF-SHA256), HKDF-SHA256 and ChaCha20-Poly1305, single-shot: the 32-byte encapsulated key, then the ciphertext and its 16-byte tag; a fresh one-time key each time, so no nonce; the sender is anonymous; `public` 32 bytes, `info` at most 1024, `plaintext` from 1 to 1073741824 (OpenSSL refuses an empty one); `BadKey` for a low-order `public` |
| `hpke_open(secret, info, sealed, aad)` | the plaintext, only when `secret`, `info` and `aad` match (`NotAuthentic` otherwise, and no plaintext); `sealed` at least 49 bytes; `BadKey` for a low-order encapsulated key |

The C side wipes its own copies of secrets (`OPENSSL_cleanse`) and OpenSSL clears its key objects; a secret seed handed to the program, and every copy of it, lives in collected memory, which Lugha can't wipe yet. Not included: HPKE's other suites, its PSK and auth modes and its multi-message contexts, other ciphers (AES-GCM) and streaming encryption, password hashing, HMAC, certificates, other algorithms.

`std.toml` (v17, milestone 114). A read-only TOML 1.0 parser written in Lugha, like Python's `tomllib`. `parse(text): Result<Map<string, Value>, TomlError>` gives the document's root table, its keys in the file's order; `parse_with(text, limits)` takes other `Limits` (`limits()`: `bytes` 1048576, `depth` 100 — arrays and inline tables nested, and a key's parts — and `values` 1000000). `Value` is `Text(string)`, `Integer(i64)`, `Float(f64)` (`inf`, `-inf`, `nan` included), `Boolean(bool)`, `OffsetDateTime(Date, Time, offset_minutes)`, `LocalDateTime(Date, Time)`, `LocalDate(Date)`, `LocalTime(Time)`, `Array(Value[])` and `Table(Map<string, Value>)`; `Date { year, month, day }` and `Time { hour, minute, second, nanosecond }` are checked (leap years, a leap second) with no time-zone arithmetic, and fractions past nanoseconds are truncated. The whole of TOML 1.0 is read — every key, string and number form, inline tables, arrays of tables, CRLF — with its rules against defining a key or table twice and extending an inline table or static array. Every mistake is a `TomlError { kind, detail, line, column }` (`kind`: `Syntax`, `Duplicate`, `Immutable`, `Overflow`, `BadValue`, `Limit`; line and column from 1, columns in characters) and `e.message()` reads `a key defined twice: app.name (line 7, column 1)`; nothing panics on any input, and integers past `i64` are `Overflow`. Helpers: `get(table, "a.b")` follows a dotted path (segments aren't parsed as TOML keys), and `as_text`, `as_i64`, `as_f64` (an integer too), `as_bool`, `as_array`, `as_table` and `as_date_time` give an `Option`: `toml.get(doc, "app.name").and_then(toml.as_text)`. No writing, TOML 1.1, struct mapping or file reading (pass `read_file`'s text). Its parts live in `std.$toml_value`, `std.$toml_strings`, `std.$toml_scan` and `std.$toml_parse`, which only bundled sources can import.

`std.json` (v17, milestone 137). Strict RFC 8259 JSON written in Lugha. `parse(text): Result<Value, JsonError>` reads any JSON value; `parse_with(text, limits)` takes other `Limits` (`limits()`: `bytes` 1048576, `depth` 100 — arrays and objects nested — and `values` 1000000). `Value` is `Null`, `Bool(bool)`, `Integer(i64)`, `Float(f64)`, `Text(string)`, `Array(Value[])` and `Object(Map<string, Value>)`: a number without `.`, `e` or `E` that fits `i64` is an exact `Integer` (`-0` is `Integer(0)`), any other a `Float`; objects keep the text's key order. Nothing outside RFC 8259 is accepted — no comments, trailing commas, `NaN`, leading zeros or byte-order mark — and a lone surrogate escape, a key defined twice in one object and a number past `f64`'s range (`1e400`) are errors; underflow gives `0.0`. Every mistake is a `JsonError { kind, detail, line, column }` (`kind`: `Syntax`, `Duplicate`, `Overflow`, `BadValue`, `Limit`, `NotFinite`; line and column from 1, columns in characters) and `e.message()` reads `a key defined twice: a (line 1, column 10)`; nothing panics on any input. `stringify(v)` writes compact text and `pretty(v)` two-space indented text, keys in `Map` order, escaping only `"`, `\` and control characters, floats in their shortest round-trip form (`1.0`, `1.0e300`); both return `Result<string, JsonError>`, `NotFinite` (line and column 0) for a NaN or infinite float, and whatever `parse` gives back reads back equal. Helpers: `get(v, key)` (an object's member), `at(v, i)` (an array's element), `as_text`, `as_i64`, `as_f64` (an integer too), `as_bool`, `as_array`, `as_object`, each an `Option`, and `is_null(v)`: `json.get(v, "user").and_then(fun(u) = json.get(u, "name")).and_then(json.as_text)`. No JSON5, streaming or file reading (pass `read_file`'s text). Its parts live in `std.$json_value`, `std.$json_parse`, `std.$json_write` and `std.$json_from`, which only bundled sources can import.

**`json.encode`** (v17, milestone 139). `encode(x: T): Value` turns any data into JSON, with no annotation: the compiler writes the code per type, as it does for printing and `==`. `T` must satisfy the built-in bound `json.Json` (an interface with nothing to implement: the compiler checks the type's structure), met by every integer and float type, `bool`, `char`, `string`, `()`, arrays, tuples, `Map<K, V>` with `K` `string` or an integer type, `Set<T>`, `Option<T>`, `json.Value`, and structs and enums whose fields and payloads meet it (recursive types too); a generic function passes its parameter through with `T: json.Json`. Anything else — a function or closure, an interface value, `Ref`, `Task`, `Chan`, an actor's handle, an `extern type` handle or `CPtr`, a map with other keys — is E0450, naming the path to the first such part. Shapes: integers are `Integer` (a `u64` past `i64.MAX` a `Float`, rounded); `f32`/`f64` `Float` (NaN and infinities stay, and `stringify` refuses them); `bool`, `string`, `()` (`Null`) as themselves and a `char` a one-character `Text`; arrays, tuples and sets `Array`s (sets in their order); maps `Object`s in map order, integer keys in decimal; `None` is `Null` and `Some(x)` is `x`'s (so `Some(None)` is `Null`); a struct an `Object` of every field, private ones too, in declaration order under its written name; an enum variant without payload its name as `Text`, with one payload `{"Name": value}`, with several `{"Name": [a, b]}`; a `json.Value` itself. `encode` never fails. A struct or enum with its own `fun to_json(self): json.Value` (or `fun from_json(v: json.Value, path: string): Result<Self, json.DecodeError>` for `decode`) uses it instead (v17, milestone 142): `json.Json`'s items are `to_json` and `from_json`, met through `std.$json_value`'s derive blocks (section 4).

**`json.decode`** (v17, milestone 140). `decode(v: Value): Result<T, DecodeError>` is `encode`'s inverse, for the same `T: json.Json`; `T` comes from context — an annotation (`let r: Result<Person, json.DecodeError> = json.decode(v);`), a return type, `?` (`let p: Person = json.decode(v)?;`), or `.unwrap()`/`.expect(…)` (milestone 148: `let p: Person = json.decode(v).unwrap();`), E0412 when nothing fixes it. It reads the shapes `encode` writes: unknown object keys are ignored; a missing field is `None` if its type is an `Option`, an error otherwise; an `Integer` fills any integer type it fits and any number fills `f32`/`f64` (`f32` rounded as `as` does), but a `Float` never fills an integer (so a `u64` past `i64.MAX`, encoded as a `Float`, doesn't decode); a `char` needs a one-character string; a tuple an array of exactly its length; a `Set` an array without repeats; integer map keys must spell an integer of the key's type; an enum a variant's name (no payload) or a one-key object naming a variant with a payload of the right shape; `null` fills only `()` and `None`. Private fields are filled like public ones, without re-checking anything a type's own functions would. `DecodeError { path, detail }` names the first mismatch: `path` joins fields with `.`, indexes with `[i]` and map keys with `["key"]` (`people[2].age`, `scores["ann"]`; empty at the top), and `message()` — also how it prints — is `people[2].age: expected an integer, found a string`, or the detail alone at the top. Details: `expected a string, found an integer`, `missing field age`, `300 doesn't fit u8`, `expected one character, found "ab"`, `expected an array of 2, found 3`, `a repeated element`, `a key that isn't an integer`, `unknown variant Square`, `expected a variant: a name, or an object with one key`, `takes no payload`, `needs a payload`. Nothing panics on any `Value`.

`std.http` (v17, milestone 111). HTTP/1.1 over a `Net` (v18, milestone 183: every function that connects or listens takes it first, `http.get(net, url)`, `http.serve(net, addr, handler)`), written in Lugha: `http://`, and `https://` through `std.tls` (milestone 112). `Request` has `method`, `path` (before `?`, not decoded), `query` (after `?`, raw), `headers` (`Map<string, string>`, names in lower case, a repeated header's values joined with `", "`), `body: u8[]` and `peer`; `Response` has `status`, `headers` and `body`; both have `text()` (UTF-8) and `header(name)` (any case). `text(status, s)`, `json(status, v)` and `bytes(status, b)` make responses; `json` takes a `std.json` `Value` (milestone 138) and writes it compactly as `application/json`, answering a `text/plain` 500 instead when `v` holds NaN or an infinity. JSON helpers (milestone 138): `req.json()` and `res.json()` parse a body with `json.parse`, whatever its `content-type` (a body that isn't UTF-8 is a `Syntax` `JsonError` at line 1, column 1), and `post_json(url, v)` posts `v` as `application/json` (`Err` with the message, before connecting, when `v` can't be written).
- **Serving:** `serve(net, addr, handler: Handler)` (or `serve_with(net, addr, limits, handler)`) serves forever, one task per connection in one scope, keeping HTTP/1.1 connections open between requests (HTTP/1.0 only when asked); it returns only when it can't listen, or with `Err` when a connection panics (the others cancelled first, v18). `Handler` (v18, milestone 197) is `interface Handler: fun(Request): Response + Sendable {}`: a named function, or a closure capturing only sendable values. The server sets `content-length` and, when it closes, `connection: close`; a response header whose name isn't a token or whose value holds a control character is dropped, so no handler can inject lines. A handler's panic ends the program, as any green thread's does.
- **Limits** (`Limits`, `limits()` for the defaults): request line 8192 bytes (414 past it), all headers 8192 bytes and 100 of them (431), body 1048576 bytes (413), 30000 ms for a whole request to arrive and for a write (408 if part of a request came; an idle open connection just closes). Anything malformed is 400: a bad request line, a header name that isn't a token, a control character in a value, a `Content-Length` that isn't one decimal number, both `Content-Length` and `Transfer-Encoding`; another transfer coding than `chunked` is 501, another version than `HTTP/1.0`/`HTTP/1.1` is 505. Each closes the connection; the server goes on. `Expect: 100-continue` is answered before the body is read.
- **HTTPS** (milestone 112): `serve_tls(net, addr, cert_pem, key_pem, handler)` (PEM text since v18, milestone 184) and `serve_tls_with(…, limits, handler)` serve over TLS (a handshake must finish within the limits' timeout); the client accepts `https://` (port 443 by default), verified as `std.tls` verifies, and `request_tls(method, url, headers, body, opts)` takes `tls.Options` to trust more; `request_tls_with(method, url, headers, body, limits, opts)` (milestone 119) takes both: `limits` as given, `opts.ca_pem` trusted, and `opts.timeout_ms`, when not 0, in place of the limits' timeout. Both honour `opts.address` (milestone 121) for `https://` and `http://`, connecting to that IP address at the URL's port and keeping the URL's host in the `host` header.
- **Client:** `get(url)`, `post(url, content_type, body)`, `request(method, url, headers, body)` and `request_with(…, limits)` send one request per connection (`connection: close`) to `http://` or `https://host[:port]/path?query` and read the response by `Content-Length`, `chunked` or to the end, skipping `1xx` responses; bodies up to 16777216 bytes by default. Failures are `Err`: `"bad URL"`, `"response too large"`, `"malformed response"`, `"timed out"`, or the network's message.
- Its reader lives in `std.$http_wire`, which only bundled sources can import.

**Visibility.** Each function, extern, struct, enum, interface, method, associated function and struct field has one of three levels (v8):

| Written | Visible in |
| --- | --- |
| `private` | its own module only |
| nothing | every module of its own package |
| `public` | every package that imports its module |

A program without a `lugha.toml` is one package, so there "nothing" means the whole program, as before v8. A program under its own package's manifest (in `test/`, `examples/`, …) is a module of that package (v17, milestone 97). The prelude's items are visible everywhere.
- **Using what isn't visible** (E0313): naming such an item, calling such a method, or reading or assigning such a field (including through `swap` and `mut self`) is an error. Its message says why: "`helper` is private to module `range`", or "`helper` is not `public`: it is only visible inside package `geo`". A method another package can't see doesn't satisfy that package's interface bounds (E0422).
- **Struct literals:** a literal outside the struct's module needs every field to be visible, so a struct with a `private` field is built through one of its functions (`range.Range.new(1, 5)`). This is how a type protects an invariant such as `lo <= hi`. `==`, copies and passing values are unaffected: they never name a field.
- **Fields and variants:** a field can be no more visible than its struct; variants share their enum's visibility.
- **No leaking** (E0318): a declaration may not mention a type less visible than itself, since the code that can use it couldn't name that type. A declaration that isn't `private` may not mention a `private` type in its signature (parameters, return type, a field's type, a variant's payloads, an interface item), looking inside arrays, `Option` and function types. A `public` declaration (v8) may mention only `public` structs, enums and interfaces, looking also inside type arguments (`Box<Inner>`), interface types and a generic function's bounds.

**Functions.** Declared with `fun`, typed parameters, and an optional `: type` return annotation. Omitting the annotation means `()` (v17; `void` before).

**Generic functions.** A generic function is declared once and compiled once for each distinct list of type arguments the program uses (section 9). One that nothing calls is checked but not compiled. An `extern fun` can't be generic, which the grammar enforces.

**Expression bodies.** A function whose body is a single expression can be written Kotlin-style, with `=` and a semicolon instead of braces. `fun f(...): T = e;` means exactly `fun f(...): T { e }`, so every rule for blocks and tail values applies. The return type follows the same rule as block bodies: it must be written, and omitting it means `()`. This keeps signatures readable without looking at the body, and lets the checker collect every signature before it checks any body.

```
fun square(x: i64): i64 = x * x;

fun is_even(n: i64): bool = n % 2 == 0;

fun max(a: i64, b: i64): i64 = if a > b { a } else { b };

fun greet(console: Console, name: string) = console.println("hello, " + name);

fun clamp(x: i64, lo: i64, hi: i64): i64 {
    let low = max(x, lo);
    if low > hi { hi } else { low }
}
```

- Parameters are immutable bindings. To modify one, shadow it with a mutable copy: `let mut n = n;`.
- Arguments are never copied at the call. Structs and arrays are passed by pointer, which is safe because the callee can't modify them and no other code can reach the caller's storage during the call, green threads included (v17). A copy happens only if the callee stores or returns the parameter (section 4).
- Recursion, including mutual recursion, is allowed.
- There are no default parameters, overloading, variadics or nested functions.

**Return checking.** A function whose return type isn't `()` must produce a value on every path. Its body passes if its tail expression has the return type, or if the body *definitely returns* by these rules:

- `return e;` definitely returns.
- A block definitely returns if any statement in it does. Statements after it are unreachable and reported as a warning.
- `if/else` definitely returns if both branches do. `if` without `else` never does.
- `while` and `for` loops never do, even `while true`.
- A `loop` that no `break` targets does (v17, milestone 152): it never ends.

A `panic(...);` statement also counts as definitely returning, because it never returns. A block that definitely returns can stand in for a value of any type, so `if x < 0 { return 0; } else { x }` has type `i64`.

This rejects a few correct programs, such as a function whose only exit is a `return` inside `while true`. Write that loop as `loop { … }`, which counts (v17), or add a final `panic("unreachable");`. `return` stays available for early exits. In a function returning `()`, `return;` (the same as `return ();`) is allowed and falling off the end is an implicit return.

**Entry point.** Every program defines `main`, without type parameters, returning `()` or an `i32` (the process exit code; 0 for `()`). Its parameters are what the runtime supplies (v18, milestone 180), matched by type, with any names, in any order, each at most once: `string[]`, the command-line arguments with the program's path first; and the capabilities `Console`, `Fs`, `Clock`, `Env`, `Proc`, `Entropy` (milestone 181) and `Net` (milestone 183). Anything else is E0304: `main(args: string[], console: Console): i32`, `main(out: Console)` and `main()` are all fine, `main(n: i64)` and `main(a: string[], b: string[])` aren't. The user's `main` is emitted as `lugha_fn_main`, like every Lugha function, and the compiler emits a real C `main` that initializes the runtime, makes the values `main` asks for, and calls it (see section 9). A `@test` function takes capabilities the same way (not the arguments): safe stand-ins, or the real ones in an integration test (milestone 185; "Tests", section 9).

**Capabilities** (v18, milestone 180). A program touches the world outside it only through values it is given: `main` takes them, and passes them on as ordinary parameters and fields, so a function's signature says what it may do (`fun price(order: Order): Money` touches nothing). They are prelude interfaces, visible in every module unless a module declares its own name, met only by `impl` (so a fake for a test is a type that says it implements one), and may be sent to tasks and actors:
- **`Console`:** `read_bytes(): Result<u8[], string>`, everything left on stdin (`Ok([])` at its end), a failed read being `Err("stdin: <the system's text>")` (milestone 228), and `flush()`, sending out what `print` wrote so far so a long-running program's line shows at once (a failure is ignored, as `print`'s are) (milestone 182); `print(x)`, `println(x)`, `eprint(x)`, `eprintln(x)`, each taking any `Display` value (it converts where the parameter expects the interface, as any value does; `println("")` for an empty line), writing its text to stdout or stderr (`eprint` flushes stdout first, so a terminal shows output in program order); `read_line(): Result<Option<string>, string>` (milestone 228), the next line of stdin without its `\n` (and a `\r` before it), a last line without `\n` still returned, `Ok(None)` at the end of input, stdout flushed first so a prompt shows. As with `File.read_line`, a line that isn't UTF-8 is `Err("line N is not valid UTF-8")`, N counting every line read from stdin by any `Console` (one stream for the process), and is skipped, so the next call reads the line after it; a failed read is `Err("stdin: <the system's text>")`, never the end of input. A narrowed console's `read_line` and `read_bytes` give `Ok(None)` and `Ok([])`.
- **`Fs`:** `cwd(): Dir`, the folder the program started in; `dir(path): Result<Dir, string>` (milestone 186), any folder by its path, absolute or from the working folder, with the system's ordinary resolution: the one way to reach beyond a `Dir`.
- **`Dir`:** `read_file(path): Result<string, string>`, the whole file or `Err` with the system's message (`"No such file or directory"`); `write_file(path, text): Result<i64, string>`, replacing the file's contents, `Ok` with the bytes written; `append_file(path, text): Result<i64, string>`, adding to the end and creating the file if needed; and every folder and file operation (milestone 182; "Folders and files", section 9). Paths are relative to the folder. A fake implements the methods the interface leaves without a body; `write_file`, `append_file` and `make_dirs` are written over them.
- **Confinement** (milestone 186): every `Dir` — `fs.cwd()`'s too — reaches only its folder and below. On Linux it holds a directory descriptor, and every path given to it is opened with `openat2` and `RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS`, so the kernel refuses an absolute path, a `..` above the folder and a symbolic link leading out, at each step of the path, a folder swapped for a link mid-lookup included: `Err("path leads outside the folder")` (`false` from `exists`, `is_dir`, `is_file`). Links that stay inside work. Making, removing and renaming act on a path's last part after its folder is opened beneath the `Dir`: that part can't be `.` or `..` (`Err("a path must end in a name, not `.` or `..`")`), and a link is removed itself, never followed. `set_mode` changes the file the kernel resolved inside. Without `openat2` (Linux before 5.6, or a sandbox that blocks it) every operation fails, `Err("this system can't confine a folder: openat2 needs Linux 5.6")`: nothing falls back to unconfined paths. Mount points inside are crossed. `dir(sub): Result<Dir, string>` gives a folder inside as a narrower `Dir`; `read_only(): Dir` the same folder for reading only, every method that would change something `Err("the folder is read-only")` before anything is touched, and a `Dir` from it read-only too: narrowing is one-way. `read_only` binds Lugha code; C code can still write (`ffi`). `path()` names where the folder is now, even after it is moved. A folder's descriptor is closed when its last `Dir` goes, and `fs.cwd()`'s is opened once and kept.
- **`Dir.path(): Result<string, string>`** (milestone 181): the folder's absolute path, or `Err` with the system's message (when it was deleted). The working folder never changes.
- **`Clock`** (milestone 181): `now_ms(): i64`, milliseconds since 1970-01-01 00:00 UTC from the system clock (`CLOCK_REALTIME`), which can jump when the clock is set; `monotonic_ms(): i64`, milliseconds from an unspecified start that never goes back (`CLOCK_MONOTONIC`), for intervals; `local_offset(ms): Result<i64, string>`, the system zone's offset from UTC at moment `ms` in seconds east, daylight saving included (`localtime_r`'s `tm_gmtoff`: `TZ` if set, else `/etc/localtime`, else UTC), or `Err` for a moment the system can't represent. Pausing (`task.sleep`) needs no clock.
- **`Env`** (milestone 181): `get(name): Option<string>`, the variable's value, `None` when it is unset, isn't valid UTF-8, or `name` contains a NUL byte. Variables are read-only.
- **`Proc`** (milestone 181): runs another program and waits for it. **There is never a shell**: the command and each argument reach the program exactly as given (`posix_spawnp`), so `;`, `|`, `$( )`, quotes and globs in them mean nothing. A command without `/` is looked up on `PATH`, one with `/` is a path; whoever controls `PATH` chooses which program a bare name runs, so use a path containing `/` when that matters. The program inherits this one's environment and working folder unchanged and gets only stdin, stdout and stderr: every other descriptor is closed on exec. A NUL byte in the command or an argument is `Err` before anything starts. There is no timeout and no limit on output size. `run(cmd, args): Result<ProcessOutput, string>` runs `cmd` with an empty stdin and captures its output; `run_with(cmd, args, input)` does the same, writing `input` to its stdin; `call(cmd, args): Result<i32, string>` runs `cmd` sharing this program's stdin, stdout and stderr (what this program printed comes first), and gives its exit code. `ProcessOutput` is a prelude struct, `{ code: i32, stdout: string, stderr: string } with Eq`: `code` is the exit status, or `128 + n` when signal `n` ended the program, as shells report it. Any exit code is `Ok`; `Err` means the program couldn't start (`"tool: No such file or directory"`), an argument held a NUL byte, or its output isn't valid UTF-8 (`"stdout is not valid UTF-8"`). stdin, stdout and stderr are served together, so a program that fills one pipe while another is being read can't deadlock.
- **`Entropy`** (milestone 181): the operating system's randomness, for keys, nonces and seeds: `bytes(n): u8[]`, `n` random bytes (a panic for a negative `n`); `next_u64(): u64`, 64 random bits. The real one reads the kernel (`getrandom`, else `/dev/urandom`, else a panic). A seeded generator (`std.random`) needs none.
- **`Net`** (milestone 183): connecting (`connect(addr)`, `connect_timeout(addr, ms)`), listening (`listen(addr)`), name lookup (`resolve(name)`), UDP (`udp_bind`, `udp_bind_shared`) and the machine's interfaces (`interfaces()`); what it opens are the prelude interfaces `Listener`, `Conn` and `Udp`, so a fake `Net` hands out fake connections ("The network", section 9). `std.tls` and `std.http` take the `Net` first wherever they connect or listen, and all their connections go through it (TLS over a `Conn`, milestone 184), so a fake `Net` covers `https://` too.
- **Narrowing** (milestone 187) makes a weaker capability from a stronger one, one-way, as default methods every implementation has, fakes included; each wraps the capability it came from, which nothing outside the prelude can reach again:
  - `net.only("host:port"): OnlyNet` connects to that host (compared without case) and port only; anything else is `Err("this network reaches only host:port")`, and listening and UDP are refused. It resolves the host once through the wrapped `Net`, drops every address that isn't public unicast, and connects to the first public one by that address, so the address checked is the address connected to; none is `Err("host has no public address")`. An IP address at its port (`"93.184.215.14:443"`, `"[2606:4700::1111]:443"`, as `tls.Options.address` connects) is connected to only when a fresh lookup of the host gives it, among the allowed ones (milestone 227); any other is refused. `resolve(host)` gives only the addresses `connect` would use, and `interfaces()` is empty: a narrowed network shows no local interfaces. Not public: IPv4 `0/8`, `10/8`, `100.64/10`, `127/8`, `169.254/16`, `172.16/12`, `192.0.0/24`, `192.0.2/24`, `192.88.99/24`, `192.168/16`, `198.18/15`, `198.51.100/24`, `203.0.113/24`, `224/4` and above; IPv6 every `::/96` address (`::`, `::1`, the old compatible `::a.b.c.d`), `64:ff9b:1::/48`, `100::/64`, `2001::/32` (Teredo), `2001:2::/48`, `2001:10::/28`, `2001:20::/28`, `2001:db8::/32`, `3fff::/20`, `fc00::/7`, `fe80::/10`, `fec0::/10` and `ff00::/8`. An IPv4 address carried by a fixed rule is judged as that address: mapped `::ffff:a.b.c.d`, NAT64's `64:ff9b::a.b.c.d` and 6to4's `2002:AABB:CCDD::/48` (milestone 227). `.allow_private()`, a method of `OnlyNet` itself (not of `Net`, so code given the value as a `Net` can't widen it), allows every address.
  - `net.connect_only()` connects and looks names up, and refuses listening and UDP; `net.listen_only(addr)` listens on `addr` only (`":8080"` any host, that port), and refuses connecting, lookups and UDP.
  - `env.only(names)` gives only the listed variables; any other reads as unset.
  - `console.stdout_only()` and `console.stderr_only()` write one stream: the other's writes do nothing, and reading gives nothing, as at the end of input.
  - Narrowing binds Lugha code; C code (`ffi`) isn't bound.
- **Process lockdown** (milestone 189): once the runtime has made what `main` (or a test) asked for, and before any of the program's code runs, the kernel limits the whole process — C code and every program it starts included — to the capability kinds it took. seccomp refuses, with `EPERM` and never a kill: `io_uring` always (it would bypass the rest); `socket` and `socketpair` without `Net`; `execve`, `execveat`, `fork`, `vfork` and a `clone` making a process (not a thread) without `Proc` (`clone3` answers `ENOSYS`, so the C library uses `clone`). Landlock, without `Fs`, refuses every file access except reading what the held capabilities need: name lookup's configuration, the trusted certificates and the library folders with `Net`; the zone files with `Clock`; everything, read-only, with `Proc` (started programs need their binaries); a test's own folder, read and written. Without `Net`, on Landlock 6.7+, TCP binds and connects are refused too; on 6.12+ signals and abstract UNIX sockets are scoped to the process. With `Fs` the whole file system is open, as SEC-05 says: narrowing to a `Dir` binds Lugha code only. Started programs inherit every limit and nothing can undo one. Each part applies where the kernel supports it and is skipped quietly where it doesn't (Landlock before Linux 5.13, a container blocking seccomp); `LUGHA_LOCKDOWN=report` prints one line saying what applied. The console, the environment, the clocks and randomness aren't limited. Linux only: `lugha_rt_lockdown(kinds)` is the porting point for other platforms.
- `debug(x)`, `panic` and the assertions need no capability. `hpke_seal` and TLS draw their one-time keys from OpenSSL's own generator.

## 7. Memory model

Value types live on the stack; strings, arrays, boxes, closure environments, interface values' boxes, channels, tasks and actor handles live on the heap, where each object counts its holders and is freed when the last one goes (v18, milestone 193; "Counting" below). Programs never free memory explicitly. Before v18 the heap was the Boehm-Demers-Weiser collector's.

**Stack.** Every local variable and parameter gets an `alloca` in its function's entry block, including struct values. Slots are shared: a slot used only inside one match arm, `if`/`else` branch, `if let`/`while let` body, block or loop body is reused by the next such region that needs a slot of the same type, since two of them are never live at once. A function's frame is therefore as large as its largest region, not the sum of them (v17, milestone 135). LLVM's `mem2reg` pass promotes these to registers. Struct parameters are passed as a pointer to the caller's storage (section 9); a Lugha function returning a struct, enum, `Option`, tuple or function value takes a hidden first parameter `ptr sret(T)`, the address of a slot in its caller's entry block, stores its result there and returns `void` (v17, milestone 125: at `-O0` LLVM's fast instruction selector can't lower a struct returned as a value). Inside a function, struct values are split into their fields (milestone 126): a struct `load` or `store` is one per field, field reads and literals use the fields directly, a struct parameter is one parameter per field, and a struct with more than 64 fields (enum payload words counted) is copied by `memcpy` and its fields read from memory; a `bool` result is marked `zeroext`. Functions C calls (`main`, callback entries) and calls into C keep the platform convention (section 8).

**Enums.** An enum value is a stack value `{ i32 tag; payload area }`. Variants are numbered in declaration order (`None` is 0 and `Some` is 1). The payload area has room for the largest variant's payloads laid out as a struct with natural alignment, and the whole value is aligned to the larger of 4 and that alignment. Enum parameters are passed like structs: as a pointer to the caller's storage. Copying an enum copies its tag and payload bytes; arrays in its payloads are shared, never mutated.

**Indirect slots.** A payload slot whose type leads back to its enum through values held inline — struct fields, enum payloads or `Option`'s payload, but not arrays — is an 8-byte pointer to a copy of the value on the heap, allocated with `lugha_rt_alloc` when the variant is built and never mutated afterwards. Every other slot, and the enum value itself, stays inline. `List` is therefore `{ i32 tag; i64 head; ptr rest }`.

**Function values** (v3) are a pair `{ ptr code, ptr env }`, 16 bytes with alignment 8, passed and stored by value. A named function's value has a null environment, and its code is the function's `lugha_ref_` wrapper (section 9). A function's own type (v18) is the same pair; an instance for it calls its code directly. A struct implementing a function interface, converted to the function type, is the pair of its `call` method and its box (a method takes its struct by pointer, as code takes its environment); a function value in an interface value requiring a function type is boxed, and the table's call slot, `lugha_fn_$prelude/$Fun<n>.call<…>`, calls the pair in the box. The function interfaces are the prelude's `$Fun0` to `$Fun8`.

**Interface values** (v5) are a pair `{ ptr table, ptr box }`, 16 bytes with alignment 8, passed and stored by value. The box is a counted object allocated with `lugha_rt_alloc` that holds one value of the concrete type, primitives included. The table is a constant per (concrete type, interface) pair: the type's identity, a function copying a box, then one pointer per method in the interface's declaration order. Copying an interface value adds a holder of its box; before a `mut self` call, a box with other holders is copied through the table's copy function and the copy replaces it in the place that held it. Converting one interface value to another (v6) copies the box as any conversion does and pairs it with the other interface's table for the same concrete type.

**Task stacks** (v17; v18, milestone 205). Each task, `main` included, has its own stack: 8 MiB of address space, carved from regions of 64 stacks reserved with one `mmap` each (`MAP_NORESERVE`), the system committing only the pages it touches; on an address space too small for that, the size halves, down to 1 MiB, before the first stack is made. There are no guard pages, so a program's mappings stay few however many tasks it runs: the number of tasks is bounded by memory, not by the kernel's limit on mappings. A finished task's stack waits in its worker's cache, up to 16 stacks, for the next task that worker starts, its pages kept; a worker with nothing to run gives its cached stacks' pages back (`MADV_DONTNEED`), so an idle program holds no more than the stacks in use (v18, milestone 213). Past the cache, a stack gives its pages back at once and goes to a pool any worker takes from; stacks never move. What a waiting task holds keeps its counts, so it stays alive; a `Task` keeps only the result. The process's own stack is the C stack (section 8).

**Closure environments** (v3). A closure's environment is one counted object, allocated with `lugha_rt_alloc` when the closure is created and laid out as a struct of its captures, each held. It is never written afterwards, so copies of the closure share it, each a holder. A closure that captures nothing has a null environment.

**Generic instances** lay out exactly like the concrete types they stand for: `Stack<i64>` is laid out like a struct with one `i64[]` field, and `List<bool>` gets its own indirect slots.

**Recursion depth.** Function calls use the task's stack, which is finite. A recursion that goes too deep — for example walking a list of a million nodes recursively — panics with `stack overflow`, which unwinds as any panic does, running the pending `defer`s (v18, milestone 205): a task's overflow fails its scope, `main`'s ends the program with exit 101. Every Lugha function's entry (the functions left once the module is optimised, so a function inlined into a loop doesn't check on every turn) compares the stack pointer with the running task's limit, its stack's bottom plus 256 KiB kept for the runtime and the unwinder; below it, `lugha_rt_stack_low` panics, and while that panic unwinds the limit drops to the bottom plus 64 KiB, so the frames' cleanups have room (a second overflow there ends the program, as a panic while unwinding does). On the C stack, the runtime detects an overflow when the kernel's guard area is hit, and ends the program without unwinding. Every Lugha function probes its frame a page at a time as it enters (LLVM's inline stack probes, v17, milestone 135), so a frame of any size reaches the guard area before memory past it; a frame under one page costs nothing extra. Walk very long lists with a loop.

**Heap objects.** All heap memory comes from one runtime function, `lugha_rt_alloc(size)`, zeroed, with a 16-byte header before the object, so pointers and layouts don't change: the function that releases what the object holds (set where it is made: an array's elements, a box's value, a closure's captures), then its count. Objects up to 272 bytes, header included, come from free lists by 16-byte size class and go back to them when freed; larger ones from `calloc`. Small blocks are carved from 64 KiB chunks, each owned by the worker thread that carved it, with that worker's own lists (v18, milestone 209): a worker allocates and frees its own blocks without locks or atomics, and a block freed on another thread goes onto its owner's list for that class with one compare-and-swap, the owner taking such blocks back in one step before carving more. Memory for small blocks is never returned to the system. Strings and arrays start with a length:

| Object | Layout | Pointer held by Lugha code |
| --- | --- | --- |
| `string` | `{ i64 len; u8 bytes[len]; u8 nul }` | Points at `len` |
| `T[]` | `{ i64 len; i64 cap; T elems[cap] }` | Points at `len` |

Elements are laid out with LLVM's natural alignment for `T`. An array's capacity is never visible to programs: literals, repeat literals and copies are full (`cap == len`), and `push` on a full array allocates a new one with double the capacity (at least 4), copies the elements, and stores the new pointer in the array's place — its only owner, so no other pointer sees the old object. String data is always followed by a NUL byte, so it can be passed to C without copying. String literals are emitted as private constant globals with the same layout and a count no change touches, and are never freed.

**Copy-on-write** (v15; by count since v18). A copy site adds a holder and shares the pointer. Before any write into an array — an assignment through it, `push`, `pop`, `clear`, `swap`, a `mut self` call or `for … of mut` — the array is made unique: if it has other holders, or is a constant, it is cloned one level (each of its elements getting one more holder) and the clone, held once and full, replaces it in the place that held it, which lets go of the original. A loop may make arrays unique once, before it starts, instead of at each write (section 9, milestone 56); programs can't tell the difference.

**Boxes** (v16). A `Ref<T>` is a pointer to one counted object holding a `T` in element layout. Boxes are the only objects two Lugha values may share and change (strings are shared but immutable). A box counts as one holder of what it contains, and nothing writes inside it in place: `get` is a copy site and `set` stores like any assignment, letting go of what the box held. So an array whose count is one has one holder.

**Included files** (v17, milestone 128). Each file an `include_str` names is a string literal, laid out and shared as one. Each file an `include_bytes` names is one private constant global `{ i64 len; i64 cap; u8 bytes[len] }` with `cap == len` and a count no change touches, so a write clones it first and nothing writes the read-only memory; nothing is copied when the program starts.

So, as far as programs can tell, every array is owned by exactly one variable, field or element. Only unwritten arrays and strings, which are immutable, are actually shared.

**Counting** (v18, milestones 190–193). Every holder of a heap object is counted, and the compiler places each change:

- **Holders.** A store (`let`, assignment, a field or element, a capture, a box, a channel send) adds a holder. A fresh value (a call's result, a literal) lives in a temporary released at the end of its statement unless a store takes it over; a fresh argument is released as soon as its call returns, and a temporary taken over where it was made needs no slot. A `let` local is released after the last statement of its block that mentions it, or when a `break`, `continue`, `return` or failing `?` leaves it first; a local read for the last time moves, and a function keeping a parameter takes over its caller's holder when the caller is done with it. A pattern's bindings hold their values, except in a `match` arm that never mentions the matched local or parameter: it outlives the arm unchanged, so they borrow from it. An `if` or `match` hands a fresh value through without counting it. Programs can't observe counts.
- **Across tasks.** A count has a shared bit, set on everything a value reaches that is still local as it crosses to another task, by the side handing it over and before the other side can see it: a value sent on a channel, a spawned call's arguments and captures, an actor message's arguments (by the sender); a task's result (by the task, before it finishes); an actor call's answer (by the actor, before it replies); a failing task's error or panic message, a task's name, a cancellation's reason and a scope's failure handler (where the runtime stores them); what an actor stored in its state (by the actor, when it stops); a file's or socket's buffered bytes (when stored, since every copy of the handle shares them). A shared object's count changes atomically; a local one's with plain arithmetic, the inline check being one unsigned compare (v18, milestone 208: the inline load and store are LLVM `unordered` atomics, the same instructions, defined under a race). Each object carries one visit function, which releases what it holds when it is freed and marks it when it is shared, walking a list rather than the stack. A constant, once filled, is immortal with everything it reaches: never counted, never freed (milestone 208).
- **Tasks, channels and actors** are counted: a channel releases what it still buffers, a task the result nobody took, and a running actor holds itself and its state until it ends. When `main` returns under `LUGHA_RC_REPORT`, each unfinished thread lets go of what it holds as a thread, so the count shows only what was lost (with several workers a thread still running then may disturb it); without the report nothing is let go of, as the process ends at once and a thread running on another worker must not find its task freed. A program linking OpenSSL (`std.tls`, `std.crypto`) starts it without its exit-time cleanup, for the same reason.
- **Resources the runtime made.** A `Dir`'s descriptor is closed, and a running hash's OpenSSL state freed, with the last holder. Files and sockets are closed only by `close`.
- **Reports.** `LUGHA_RC_REPORT=1` prints at exit how many objects were made, freed and still live (`2` lists each live one), constants counted as freed; `LUGHA_RC_POISON=1` keeps freed objects as tombstones, so touching one panics.
- **The owner check** (v18, milestone 208). With `LUGHA_RC_CHECK=1` set for both the build and the run (`LUGHA_RC_CHECK=1 lughac run prog.la`), every object not marked shared records the task that made it (1 for `main`, then in the order tasks start), and another task counting or marking it ends the program: `panic: an unshared object reached a second task (made by task 2, used by task 1)`, exit 101. Each count change then goes through the runtime, so checked programs are slower. Built without it, a program run with it still checks, but copies an array on every write. A crossing it catches is harmless on one thread and a use after free once tasks run on several.
- **Not freed.** Cycles through `Ref` boxes, channels and task or actor handles are never freed. A panic unwinds its own task only: what other tasks hold when it ends the program stays, as does what a task holds when the stack overflows.

Two consequences matter for C:

- **Pointers handed to C aren't counted.** An extern C function keeping a Lugha object after it returns is undefined behaviour: nothing counts C's hold, so the object may be freed while C still uses it. Extern functions must not keep references to Lugha objects after they return.
- **Closures handed to C** (v16, milestone 88) are the one exception, made safe by lughac: a closure passed as a callback's user data is copied into a small record holding its environment, so it and everything it captured stay alive while C may call it — until the body of the scope that registered it ends (v18, milestone 215), or until C calls the `release` function lughac gave it, which frees the record. C calling a callback after releasing it, or releasing it twice, is undefined; calling one with `data` after its scope's body ended is dropped, while one without `data`, a function that captures nothing, runs whenever C calls it (section 8; v18, milestone 226). A callback may be called from any thread: one C made gets a C stack of its own on its first callback (v18, milestone 217; section 8). Every C call runs on its worker's C stack (v18, milestones 205 and 210), and a `direct` callback C makes there runs in the calling green thread, which may spawn but not wait. A task's successive C calls may run on different worker threads, unless it is pinned: a C library that needs one thread (GTK, OpenGL, anything keeping thread-local state) declares its handles `pinned`, or is used inside a `pinned` scope (section 4, milestone 214).

**Safety.** Ordinary v0 code cannot produce a dangling pointer, read uninitialized memory, or index out of bounds. The only route to undefined behavior is calling extern C code.

## 8. C interop

Lugha calls C functions through `extern fun` declarations, which give a C function's name and signature without a body. The compiler emits an LLVM function declaration, and the system linker resolves it.

```
extern fun puts(s: string): i32;
extern fun abs(x: i32): i32;
extern fun sqrt(x: f64): f64;
```

**Blocking externs** (v18, milestone 207). `extern blocking fun f(…): R;` (and `extern blocking "symbol" fun f(…)`) declares a C function that may wait — a database query, a C client's network call, a long computation — and runs each call on a helper thread (section 6) while its task waits, so the program's other tasks run meanwhile; a plain extern runs on the program's own thread and stops every task while it runs. `blocking` is a word of its own only right after `extern`. The rules:
- **Arguments cross as for a plain call,** strings, arrays, `out`, `mut` and `ptr` arguments included: the C function gets pointers into the task's values, so the task's wait can't be cut short. A cancellation (a deadline, a sibling's failure, Ctrl-C) is noticed once the call returns, at the task's next pause, as for a plain extern; only the C function's own timeout ends the call sooner.
- **No pinned handles:** a helper thread isn't the thread that made one, so a `blocking` extern can't take or give a pinned handle (E0480).
- **No callbacks:** Lugha code can't run on a helper thread, so a `blocking` extern takes no callback, nor `data` or `release` (E0479). A callback C stored earlier and calls during a `blocking` call ends the program with `panic: a C callback ran on a helper thread, during a `blocking` extern call` (exit 101, no cleanups).
- **Any helper:** each call may run on a different helper thread, so C state tied to a thread (thread-local variables, `errno` from one call to the next) doesn't carry over.
- **Agreement:** every declaration of one C function, renamed ones included, says `blocking` or none does (E0306).
- **In place:** in a deterministic run, before process lockdown and inside a C callback, a `blocking` extern runs as a plain one does.

**Allowed types.** Extern signatures may use only the types below. Structs, arrays, enums, `Option` (except of a handle, v16) and interface values (v5), generic or not, cannot cross the boundary, and a function type only as a callback parameter (v16, below). So C never sees an array, and copy-on-write (v15) needs nothing at the boundary; strings are immutable.

| Lugha type | C type | Passed as |
| --- | --- | --- |
| `i32` | `int32_t` | Value |
| `i64` | `int64_t` | Value |
| `u8` | `uint8_t` | Value |
| `f64` | `double` | Value |
| `u32` | `uint32_t` | Value (v7) |
| `u64` | `uint64_t` | Value (v7) |
| `f32` | `float` | Value (v7) |
| `i8`, `i16` | `int8_t`, `int16_t` | Value, sign-extended (v10) |
| `u16` | `uint16_t` | Value, zero-extended (v10) |
| `bool` | `bool` (C99) | Value, `i1` zero-extended |
| `string` | `const char *` | To C: a pointer to the first data byte, 8 bytes past the header. From C (a result, v16): a copy |
| `Option<string>` (v16) | `const char *` | As `string`; `NULL` for `None` |
| a handle `H` (v16) | `T *` | The pointer |
| `Option<H>` of a handle (v16) | `T *` | The pointer; `NULL` for `None` |
| `fun(…)` (parameter only, v16) | a function pointer | A callback; see below |
| an `extern struct` (v16) | the C struct | By value, as C passes it; see "Passing structs" |
| return type omitted, or `()` | `void` (calls that need the value use `{}`) | — |

The compiler adjusts the string pointer at each call site. C code receives an ordinary NUL-terminated string.

**Text from C** (v16, milestone 89). A `string` or `Option<string>` coming from C — an extern's result, or a callback's parameter — is copied into a new Lugha string: C's bytes up to the first NUL, each invalid UTF-8 sequence replaced by U+FFFD and the copy moving one byte on, exactly as `chars()` decodes. C keeps its memory and Lugha never frees it, so a C function returning text the caller must free leaks unless it is declared returning a handle and freed through another extern. For `Option<string>`, `NULL` is `None`; for a plain `string`, `NULL` panics: `panic: C gave NULL for a \`string\`; declare it \`Option<string>\`` (without a location: the runtime can't tell which call).

**`out` parameters** (v17, milestone 102). `out name: T` in an extern's parameters is C's `T *` that C writes through: `extern fun wasmtime_context_get_fuel(c: Context, out fuel: u64): Option<WasmError>;`. The call writes `out place` for it — `get_fuel(ctx, out fuel)` — where `place` is anything assignable (a `let mut` variable, a field, an element), under the assignment rules (E0501, E0502, E0506, E0507) and of exactly the type `T`. C gets a pointer to a zeroed slot, not the place's old value; after the call returns, what C wrote is stored into the place, the `out` arguments' places in order, each fixed (its indexes evaluated) when its argument is evaluated. `T` is an integer, a float, `bool`, a handle (C's `T **`; C's pointer as is, as for a handle result), `Option` of a handle (`NULL` is `None`: the usual way to declare C's `T **`, since a variable must start with a value) or an extern struct C fills; a `string` is E0409. A missing `out` for an `out` parameter, or `out` for any other argument, is E0443; `out` on a Lugha function's parameter is E0201. `out` is a keyword only there, before a name, so `out` stays usable as a name. The slot lives for the call only: C must not keep the pointer.

**`ptr` and `mut` parameters** (v17, milestone 103). `ptr name: T` is C's `const T *`: the call writes `ptr e` for any expression `e` of type `T`, and C reads a copy through the pointer. `mut name: T` is C's `T *` read and written: the call writes `mut place`, C gets a pointer to a slot holding the place's value, and after the call the slot is stored into the place, as for `out`. The types are `out`'s; a mode at the call that isn't the declaration's (or is missing) is E0443.

**Arrays** (v17, milestone 104). An extern parameter `xs: T[]`, `T` a number, `bool`, a handle or an extern struct (or union), is C's `const T *` to the array's first element, valid during the call; the length goes as its own argument, as C's API spells it. `mut xs: T[]` (written `mut place` at the call) lets C write the elements in place: the array is made unique first, so no copy of it sees the change; its length never changes. Other arrays (`string[]`, Lugha structs) are E0409, and no extern returns an array.

**Unions** (v17, milestone 104). `extern union Name { field: T, … }` declares C's union: C's size (its largest field's, rounded to its largest alignment) and alignment; fields hold plain data only: numbers, `bool`, `char`, and extern structs and unions of those — no handle, `Option` of one or `CPtr` at any depth (E0437, v17 milestone 106), since reading one from another field's bytes would make up a pointer. A literal writes exactly one field (E0444); reading any field reads the same bytes as that field's type, as C does; `u.f = v` writes one. Unions go where extern structs go — fields, arrays, `out`/`ptr`/`mut` — but not by value across the C boundary, nor a struct holding one (E0409: pass it by address or in an array). They have no `==` (E0404: only C knows the live field) and print as `<union>`.

**C memory** (v17, milestone 105). `CPtr` is C's untyped pointer (`void *`, `uint8_t *`): a handle no module declares, usable wherever a handle is (parameters, results, `out`/`ptr`/`mut`, extern-struct fields), compared with `==`, `CPtr.null` being C's `NULL`. Its memory is reached through a view (below): `p.view(n, f)`. Nothing keeps the pointer, and there is no pointer arithmetic. A non-null `CPtr` (or handle) only ever comes from C.

**Views** (v18, milestone 220). `View<T>` reads C's memory and `MutView<T>` reads and writes it: a pointer and a count of `T`s, C's own memory, never copied. `T` is an integer or float type, or an extern struct (or union) holding only those at any depth, so whatever bytes C has are a valid `T` (E0409 otherwise: not `bool`, `char`, a handle or `CPtr`). A `MutView<T>` is accepted where a `View<T>` is expected (the same pointer and count); the reverse is E0403. A view has:
- `v.len`, an `i64`; `v[i]`, bounds-checked as an array's index is (`index out of bounds`); `for x of v`;
- `v[i] = x` (and `+=` and the rest) on a `MutView` only (E0506 on a `View`), written straight to C's memory;
- `v.slice(from, to)`, a smaller view of the same kind, panicking (`slice out of bounds of a view`) unless `0 <= from <= to <= v.len`;
- `v.to_array()`, a copy, a `T[]`; `m.copy_from(xs: T[])` on a `MutView` only (E0415 otherwise), copying `xs` in, panicking unless the lengths match. Only `to_array` allocates.

A view never outlives the call it was given to (E0483): it may be a parameter (of a function, method or closure) or a `let` local, and passed as an argument, but never returned or a function's result type, stored in a field, variant, array, tuple, `Option`, `Result`, box, map, set or channel, captured by a closure, sent to a task or actor (it isn't `Sendable`), or a type argument. The rule is on types, so nothing more is written.

Views come from two places:
- **A callback's parameters.** In a `direct` or `realtime` callback, `View<T>` is C's `const T *, size_t` and `MutView<T>` is `T *, size_t` (two C parameters each): C's buffer itself, valid until the callback returns; writes reach C at once. A `spawned` callback runs after C's call has returned, so it takes `T[]` instead (`const T *, size_t`), a copy made during the call. `T[]` in a `direct` or `realtime` callback, `View<T>` in a `spawned` one, and `Ref<T[]>` in any are E0484, whose help names what to take.
- **A `CPtr`.** `p.view(n, f)` calls `f` with a view of `n` (a `u64`) `T`s at `p`, the kind and `T` written on `f`'s parameter (`fun(bytes: View<u8>) { … }`, E0403 for a function of anything but one view); the call's value is `f`'s. A null `p` gives an empty view when `n` is 0 and panics otherwise (`NULL CPtr given to view`); a `p` not aligned for `T` panics. The program vouches for two things: `n`, checked only against what C says its buffer holds, so check it against C's own size first; and that the memory stays alive, and C's, for as long as `f` runs (a call inside `f` that frees it, or a wait while another task does, leaves the view reading freed memory). The rest is checked: an `n` above `i64::MAX` divided by `T`'s size panics (`view length too large`) before the null and alignment checks, so a view's length is never negative and its size in bytes never wraps. Outside the program's own code `p.view` needs the `ffi` permission (E0333, section 6). An offset is a slice: `p.view(off + n, fun(v: View<u8>) = v.slice(off, off + n).to_array())`.

**Passing structs** (v16, milestone 90). An extern struct goes to and from C — as an extern's parameter or result, or a callback's — the way the target's C calling convention passes it, which is how a C compiler passes it. On `aarch64-unknown-linux-gnu` (v17, milestone 134) that is AAPCS64: a struct of 1 to 4 scalars all `f32` or all `f64` (nested structs flattened) goes in consecutive floating-point registers; any other of up to 16 bytes in one or two general registers; a bigger argument in a copy the caller makes, passed by pointer, and a bigger result through the pointer in `x8`. On x86-64, System V's rules:
- **Over 16 bytes:** in memory. An argument is a copy C receives on the stack (LLVM `byval`); a result is written through a hidden pointer the caller passes first (`sret`).
- **16 bytes or less:** split into eightbytes. One holding only `f32`/`f64` fields goes in an SSE register, any other in an integer register. A struct whose eightbytes don't all fit the registers still free at its argument goes in memory as a whole.
- **What travels** (as clang lowers it): one LLVM value per eightbyte, `i<n>` for the bytes holding data, `float`, `<2 x float>` or `double`; with two eightbytes the low one fills its eight bytes.

| Struct | Passed as |
| --- | --- |
| `{ r: u8, g: u8, b: u8, a: u8 }` | `i32` |
| `{ x: f32, y: f32 }` | `<2 x float>` |
| `{ x: f32, y: f32, w: f32, h: f32 }` | `<2 x float>`, `<2 x float>` |
| `{ i: i64, d: f64 }` | `i64`, `double` |
| `{ f: f32, n: i32 }` | `i64` |
| `{ tag: u8, v: f64 }` | `i64`, `double` |
| `{ offset: V2, target: V2, rotation: f32, zoom: f32 }` (24 bytes) | in memory |

lughac targets only x86-64 Linux, so this is the only convention it implements. Pointers to structs, arrays of structs, packed structs, unions and bit-fields can't cross yet. A program passing no struct by value gives byte-identical IR.

**Callbacks** (v16, milestone 88). A parameter whose type is a function type, `cb: fun(T…): R`, is a C function pointer that C may call, during the extern call or later. Its parameter and result types are numbers, `bool`, handles, `Option` of a handle and extern structs (`R` may be omitted), and its parameters may also be `string` or `Option<string>`, copied from C (v16, milestone 89). C itself runs on the C stack, the process's own (v18, milestone 205): every call of an `extern` switches to it and back, so a task's stack never holds C's frames, and a callback's Lugha code runs there too, checked against the C stack's end. A callback may not wait (a channel, `join`, `sleep`, a socket): another task would run on the same stack, so the runtime panics with `a C callback can't wait: start a task instead`; preemption leaves it be. A callback never returns a string, since C would keep a pointer into Lugha's memory; no arrays, structs, enums or functions go into or out of a callback, and no extern returns one (E0409).
- **`data`:** one of a callback's parameters may be written `data`: the `void *` user data C hands back to it. The extern then has one parameter written `data` too, where C takes that user data, and may have one written `release`, a `void (*)(void *)` that C calls when it drops the callback. An extern has a `data` parameter exactly when one of its callbacks takes `data`, at most one of each, and `release` only with `data` (E0435). Anywhere else, `data` and `release` are E0435.
- **Calls:** callers write neither the `data` nor the `release` argument; lughac passes them. A callback's argument is a function value of the callback's type without `data`. Where the callback takes no `data`, C keeps nothing but a code pointer, so the argument must be a function's name (`f`, `m.f`) or a closure literal that captures nothing (E0436); where it takes `data`, any function value, closures with captures included.
- **Renamed externs:** `extern "symbol" fun name(…)` declares the C function `symbol` under the Lugha name `name`, so one C function can be declared several times with different callback types. Several declarations may share a symbol; `symbol` is a C identifier and doesn't start with `lugha_` (E0306).
- **`direct` and `spawned`** (v18, milestones 215 and 216): every callback says how C's call runs it, before `fun`; one with neither is E0435. **`cb: direct fun(…)`** runs during C's call (below). **`cb: spawned fun(W, data)`** needs `data` and no result (E0435 otherwise: C can't wait for a task's value): C's call to it starts a task, in the scope that was innermost when the extern registering it was called (the root scope in `main`'s body), that calls the closure with C's arguments, and returns to C at once. The task is one of that scope's: counted, waited for, its panic handled by the scope's policy, cancelled with it, and started whatever its `limit n`. One taking a pinned handle runs on the thread C called it from. What it captures crosses to that task, so must be `Sendable` (E0475), and it takes C's buffers as `T[]`, copies, not views (E0484), since C's call has returned before it runs. `direct` and `spawned` are words only there; elsewhere they are names.
- **Captures** (milestone 217): a callback with `data` may run on another thread, a `spawned` one as a task and a `direct` one on a thread C made, so what it captures must be `Sendable` (E0475); a value that isn't (a `Ref`) reaches it through a channel or an actor instead. A function value whose captures aren't known (not a function's name, not a closure written there) isn't.
- **A `direct` callback** runs in the calling task, on the C stack, and can't wait: waiting panics with `a direct C callback can't wait`. A cancellation reaching the task meanwhile waits until it returns to C. A panic in it stops where it would leave for C: C gets the zero value of the callback's result, and the panic is reported by a task of the registering scope (for a callback without `data`, or once that scope's body has ended, the calling task's innermost scope), as that scope's policy says: failing an all-or-nothing scope or the root, printed by an `independent` one, given to `independent(f)`'s handler.
- **The scope's end:** once the body of the scope a callback with `data` was registered in finishes, is left or unwinds, its callbacks stop (one without `data` captures nothing and runs whenever C calls it, milestone 226): a call C makes after that is dropped (a `spawned` one starts nothing, a `direct` one returns the zero value without running), and the closure's captures are let go of then, or at C's `release` if that comes first.
- **`realtime fun(…)`** (milestone 219): a third word, a `direct` callback that the compiler checks, with every Lugha function it calls (prelude and std included), never allocates or waits (E0482): no strings, arrays, maps, sets, boxes (`Ref.new`, a conversion to an interface value) or capturing closures built, no array element written (it may copy the array first), no task, scope, `select`, channel operation (each takes the channel's lock), `join`, sleep, actor call or capability I/O, no `blocking` extern, and no call through a function value or an interface value, whose code can't be checked. Plain externs are trusted, as an extern declaration vouches for C; a panic's message may be built (an error path). It can't take `string` or `Option<string>`, which C's call copies; it takes C's buffers as views, which copy nothing (`to_array` is refused: it allocates). A `RingSender`'s `try_send` is its way out. A call takes no lock, with `data` or without (milestone 229): it checks that its scope still runs and holds its captures through a counter in the callback's record. Dropping the last holder of a value may still free it, as reference counting does anywhere: a call during which its scope ended may be the last holder of the captures.
- **Threads C made** (milestone 217): a thread that isn't the program's (not the main thread, a worker or a helper) gets, on its first callback, a record and a C stack of its own, aligned and guarded as a worker's, kept until the thread exits. Registering a `realtime` callback sets one record and stack aside (milestone 226): a thread's first real-time call takes it with one compare-and-swap, allocating nothing and taking no lock, and the next registration sets another aside; a second new thread calling before then makes its own, as any first callback does. A `direct` callback runs there and can't wait: anything that would (a full channel's `send`, `recv`, `join`, `task.sleep`, an actor call) panics with `a direct callback can't wait on a thread C made`, which is reported as a direct callback's panic is. Operations that don't wait (an unbounded channel's `send`) run as usual, and objects it makes come from the system allocator. A `spawned` callback from there starts its task on the workers; one taking a pinned handle panics, as that handle's thread is C's. While any thread C made lives, or any callback with `data` is registered, a program whose tasks all wait isn't declared deadlocked: C may still call.
- **Driving a C library's loop:** a `spawned` callback runs when its task gets a worker. A library that delivers events from a call the program makes repeatedly (`SDL_PollEvent`, `glfwPollEvents`, `g_main_context_iteration`, `curl_multi_perform`) is driven from a Lugha loop that yields between calls, `while window_poll(w) { task.yield_now(); }`, and its `spawned` callbacks run in between. A C loop that never returns (`gtk_main()`) holds its thread, so callbacks spawned for that thread wait until it does.
- **No `unsafe`:** as for any extern, a declaration that doesn't match the C function — a callback of the wrong type, `data` in the wrong place — is undefined behavior; the declaration is the one place a program vouches for C.

```
extern type GtkWidget;
extern "g_signal_connect_data" fun on_clicked(button: GtkWidget, signal: string,
    handler: spawned fun(GtkWidget, data), user: data, destroy: release, flags: i32): u64;

let (clicks, clicked) = Chan<i64>.unbounded();
on_clicked(button, "clicked", fun(b: GtkWidget) { clicks.send(1); }, 0);   // spawned: a task per click
```

**Rules**

- Extern functions are called exactly like Lugha functions.
- Variadic C functions such as `printf` cannot be declared in v0. Use the `print` and `println` intrinsics instead.
- The extern name is used as the C symbol name unchanged, or the string after `extern` when there is one (v16). Lugha functions themselves are emitted with a `lugha_fn_` prefix, so they never collide with C symbols. Methods and associated functions are `lugha_fn_<Type>.<name>` (for example `lugha_fn_Point.dist`); `.` can't appear in a Lugha name, so these can't collide either. A method of an `impl I for T` block (v18) adds `@` and the interface as written, its `.`s as `/`: `lugha_fn_Circle.area@Shape`, `lugha_fn_Path.from@From<string>`, `lugha_fn_Circle.area@geo/Shape`; a block for another module's type adds `@` and the block's module path too (`lugha_fn_geo/Circle.area@Shape@shapes`). `@` can't appear in a Lugha name, so a block's method never collides with the type's own.
- A package's modules are named by paths that start with the package name (v8), so their symbols do too: function `area` of `geo`'s module `shapes` is `lugha_fn_geo/shapes/area`. Diagnostics in a `lugha.toml` name that file relative to the main file's folder (`lugha.toml`, `libs/geo/lugha.toml`).
- Named imports and re-exports (v11) are resolved by the checker to the declaring module's items, so they add no symbols: `shapes.add` calls `lugha_fn_mathx/vec/add`.
- Items of a module other than the main file put the module's path before their name, joined with `/`: function `area` of module `shapes.circle` is `lugha_fn_shapes/circle/area`, and its method `Circle.area` is `lugha_fn_shapes/circle/Circle.area`. A name contains neither `/` nor `.`, so a module function (`lugha_fn_a/b/c`) can't collide with a method (`lugha_fn_a/b.c`, method `c` of type `b` in module `a`).
- A generic instance adds its type arguments as they print, with module paths joined by `/`: `lugha_fn_max<i64>`, `lugha_fn_twice<fun(i64): i64 {closure 1 in main}>` for a function's own type (v18), `lugha_fn_a/b/max<a/b/Point>`. A method's go on its type, then its own: `lugha_fn_Stack<string>.push`, `lugha_fn_Stack<string>.tag<f64>`. `<` can't appear in a Lugha name, so instances can't collide with anything else.
- A variant used as a value (v17) is `{ lugha_ref_<Enum>.<Variant>, null }`, the enum printed with its type arguments and module path joined by `/` (`lugha_ref_Shape.Circle`, `lugha_ref_Option<i64>.Some`, `lugha_ref_$prelude/Result<i64, string>.Ok`): a wrapper taking the environment, then the payloads (deep ones copied, as a function stores a parameter), building the variant with the indirect-slot rules; emitted once. It can't collide with a method's wrapper: a method may not share a variant's name (E0310).
- A function or method that keeps a parameter holding arrays (v17, milestone 149) has a second instance, `lugha_fn_<symbol>.$own` (`lugha_fn_insert.$own`, `lugha_fn_Bag.with.$own`), in which its kept parameters are its own: their last-use stores move. Only direct calls (`f(…)`, `m.f(…)`, `x.m(…)`, `T.f(…)`, bound calls once monomorphised) call it, sharing each kept argument they still need and handing over the rest; function values, interface tables, derive blocks, printing and every other path call `lugha_fn_<symbol>`, which shares what it stores. `$` can't appear in a program's names, so the suffix can't collide.
- A function used as a value (v3) is called through `lugha_ref_<symbol>`, a wrapper that takes an environment pointer first and calls `lugha_fn_<symbol>`: `lugha_ref_double`, `lugha_ref_max<i64>`, `lugha_ref_Point.at`. It is emitted once, when first needed.
- An interface value's table (v5) is `lugha_vt_<Type>.<Interface>`, with names printed as in other symbols (`lugha_vt_Circle.Shape`, `lugha_vt_Stack<i64>.geo/Sized`), emitted once per pair a program converts. Its identity slot points at `lugha_type_<Type>`, its copy slot at `lugha_copy_<mangled type>_box` (`lugha_copy_6Circle_box`), and each method slot at the method itself: methods take their receiver as a pointer, so a call passes the box. Copying an interface value goes through `lugha_copy_<mangled interface>` (`lugha_copy_5Shape`). A generic interface (v6) prints with its arguments: `lugha_vt_Count.Source<i64>`, `lugha_copy_11Source<i64>`. A type pattern compares the table's identity slot with `lugha_type_<T>`.
- A default method (v5) compiles once per type that uses it, through a table or a bound: `lugha_fn_<Interface>.<method><Type>` (`lugha_fn_Error.report<ParseError>`); that type's table slot, bound calls and (v10) calls on the type itself use it. A generic interface's default (v6) compiles once per type and interface arguments, the arguments printed on the interface: `lugha_fn_Source<i64>.skip<Count>`. A converted `?` error is boxed on the failure branch, like any conversion. A program without interface values has none of these.
- Printing a value that isn't a number, `bool` or `string` (v12) calls `lugha_show_<type>(value) -> string`, named like the copy helpers (`lugha_show_5Point`, `lugha_show_i64_arr`) and emitted once, only when used: it calls the type's `show`, or builds the built-in form with `lugha_rt_str_concat` and quotes strings with `lugha_rt_str_quote`. A generic type's `show` is listed as an instance wherever such a value is printed, inside other values and through type parameters too. An interface type's helper, built at the end of lowering, compares the value's identity slot with each concrete type the program has a table of for that interface and prints the box's value by its rules; dispatch tables are unchanged. A function value prints the literal `<fun>`. `unwrap()` on a `Result` builds its panic message in the failing branch from the error's text. Programs printing only numbers, `bool` and strings, and calling no `unwrap()` on a `Result`, give byte-identical IR.
- `Self` in `impl` blocks and in struct and enum declarations (v10, v13) is replaced by the parser with the type it stands for, so the checker and codegen see the type's own name.
- `for x of mut e` (v13) works on `e`'s place: over an array, `x` names the element itself (an `i1` occupies one byte, so `bool` elements alias too); over an iterator, `next` is called on `e`'s own storage instead of a hidden copy. Since v15 the element (or iterator) is reached afresh through `e`'s place, made unique, at each use, with the index expressions of `e` evaluated once before the loop.
- **Copy-on-write places** (v15): a write lowers in two phases. The plan evaluates the place's index expressions and bounds-checks them; the commit walks from the root slot, loading each array on the path, testing its shared flag (`cap < 0`) and, if set, cloning it and storing the clone back into the slot, then forms the element address. Assignment plans, evaluates the value, then commits; `swap` plans both places before committing either.
- **Owning arrays once per loop** (v15, milestone 56): before any loop, the compiler scans its condition and body. A variable from outside the loop qualifies when the loop mentions it only as the root of a written place whose new value holds no array (`=`, `op=`, `push`, `pop`, `clear`, a method on a value without arrays), or in reads whose value holds no array (numbers, `bool`, `char`, strings, `.len`, structs without arrays). Any other mention — a copy, an argument or return, a capture, a method call on a value holding arrays, `swap`, `for … of`, `match` — disqualifies it. For a qualifying variable, each array the loop writes through whose path is fixed (every index a literal or a variable the loop never assigns or binds, with no calls; a `for … of mut` variable is never fixed) is made unique before the loop. An index then out of bounds skips that step instead of panicking. Writes inside the loop to such a path load the array without testing its flag: nothing in the loop can share it again.
- The v13 iterator helpers are Lugha in the prelude's third file, compiled only when reached like the others. `flat_map` keeps its current inner iterator in a 0- or 1-element array, so `next` steps it in place.
- A format specifier (v13) is parsed once by the checker (E0431) and again by codegen, which passes its parts to `lugha_rt_fmt_int`, `lugha_rt_fmt_float` or `lugha_rt_pad`, from a runtime fragment `format.c`. Programs without specifiers give byte-identical IR.
- `format` (v14) splits its template into text and holes in the checker and again in codegen, then lowers like an interpolated string with the arguments as its parts. Programs without `format` give byte-identical IR.
- `Proc` (v18; `std.process` before) is prelude code over one built-in, `$os.start(cmd, args, input, mode): Result<(i32, u8[], u8[]), string>` (named `$os.spawn` until `spawn` became a keyword in v17; `lugha_rt_spawn`: `posix_spawnp`, `pipe2(O_CLOEXEC)` pipes, a `poll` loop, `SIGPIPE` blocked while writing, `waitpid`); the outputs become strings through `string_from_utf8`. The runtime opens its own files close-on-exec (`fopen` mode `"e"`, `O_CLOEXEC`). Changes to `runtime/process.c` need the user's security review, like `modules/git.rs`.
- `Dir` (v18; `std.fs` before) is prelude code over the `$os.dir_*` built-ins of `runtime/dir.c` (milestone 186), each taking the folder's handle (a collected object holding the descriptor, closed by a finalizer) and a path opened beneath it with `openat2`: `dir_read_bytes`/`dir_read_text`, `dir_write`, `dir_list` (names as bytes, sorted in Lugha), `dir_make`/`dir_remove`/`dir_rename` (`mkdirat`, `unlinkat`, `renameat` on the last part, beneath its opened folder), `dir_kind` (0 none, 1 file, 2 folder, 3 other), `dir_size`, `dir_chmod` (`fchmodat2` with `AT_EMPTY_PATH` on the resolved descriptor, else its `/proc/self/fd` entry), `dir_sync`, `dir_file` (a stream's descriptor), `dir_path` (`/proc/self/fd`'s link), `dir_sub`; `dir_cwd` and `dir_open` open a folder with ordinary resolution for `Fs`. `make_dirs` is Lugha over `make_dir` and `is_dir`.
- `Env` (v18; `std.env` before) is prelude code over `$os.env(name): Option<u8[]>` (`lugha_rt_env`, `getenv`, a copy); the bytes become strings through `string_from_utf8`, so C never decides what UTF-8 is.
- `std.random` (v15) is Lugha code except `$os.seed()`, which calls the runtime's `lugha_rt_os_seed() -> u64`, and `$float.from_f64(x)`, which is a no-op at `f64` and `fptrunc` at `f32`; both built-ins can be named only in bundled sources.
- A std module (v15) is embedded in `lughac` (`lugha::STD`) and joins the program's offset space like a file when imported; its items' symbols carry its path, `lugha_fn_std/math/sqrt<f64>`. A `$float.<op>` built-in at `f32` or `f64` lowers to LLVM's intrinsic at that width (`llvm.sqrt.f64`, `llvm.log` for `ln`, `llvm.round`, `llvm.atan2`, …), so `std.math` makes no call into libm. Programs that import nothing from `std` give byte-identical IR.
- An overloaded operator (v15) lowers to a call of the method, `lugha_fn_Vec2.add(self, other)` with a pointer to a copy of the left operand as for any `self` method, or through the interface's table for an interface value. A generic body's operator on `T` is lowered per instance: at a number or string type it is the built-in operator, so `add<i64>` is a plain checked `add`; at a struct or enum type, the method call. A `0` or `1` of type `T` is the instance's constant (`0.0` at `f64`). Programs without overloaded operators give byte-identical IR.
- A type with several methods of one operator (v17, milestones 158–159) names all but the one taking the type itself by their right-hand type, `lugha_fn_V.mul(f64)`; `impl f64 { … }`'s are `lugha_fn_f64.mul(V)`, with the module's path when not the main one. A type with one method per operator keeps `lugha_fn_V.mul`.
- `a[i]` on a type with `index` (v17, milestone 155) is the method call `lugha_fn_Grid.index(self, i)`, and `a[i] = v` the `mut self` call `lugha_fn_Grid.set_index(self, i, v)` on `a`'s place, each as any method call (through the interface's table for an interface value); `a[x, y]` is the tuple `(x, y)` built as any tuple. A change through `[ ]` (milestone 156) on a map's entry finds the entry's slot with the prelude's `$slot` and writes into the map's values array; on a type it holds the place's indexes and the element in hidden locals, calls `index`, makes the change on the copy, then calls `set_index`; for `for … of mut` (milestone 157) that call is a pending exit action of a block around the loop, lowered at each of its exits as a `defer` is.
- An index (v14) is evaluated at its own integer type and sign- or zero-extended to `i64` before the bounds check, so an `i64` index lowers exactly as before.
- A `char` (v14) is an LLVM `i32`, compared unsigned. An integer `as char` selects U+FFFD unless the value, widened to `i64`, is a scalar value. `chars()`, `char_count()`, `char_from` and the classification and case methods are Lugha in the prelude's fourth file.
- A tuple type (v14) is the generic struct `$tuple<n>` with fields `0`, `1`, …, so it lowers as a struct: an LLVM named struct spelt as the type prints, `%"(i64, string)"`, with struct copy, equality and print helpers, and symbols such as `lugha_fn_max<(i64, i64)>`. `<` calls one `lugha_cmp_<type>(a, b) -> i32` helper per tuple type. A tuple pattern tests each field's address, with no tag. `for (i, w) of e` becomes a loop over a hidden variable whose body starts `let (i, w) = …;`.
- Converting a `J` value to `I` (v6) calls `lugha_up_<J>.<I>(table) -> table`, which compares the value's table with each `J` table the program has and returns the matching `I` table; its body is built after everything else, when every table is known. That `I` table is derived from the table the value was built with: `lugha_vt_<C>.<J>.<I>` has `C`'s identity and copy and, for each `I` item, the function `C`'s `J` table has, so methods behave as through `J`; derived tables keep the original interface (`<C>.<J>.<K>` for a chain) and converting back to it gives the original table. A program that never converts between interfaces has no `lugha_up_` functions and no derived tables. (The approved draft put one slot per target interface in each table instead; tables are emitted while lowering, before every conversion is known, so PRP-054 uses lookup functions.)
- The v7 operators and methods: `& | ^` are `and`/`or`/`xor`; `<<` is `shl` and `>>` is `ashr` for `i32`/`i64` and `lshr` for the unsigned types, each after an `icmp uge` of the amount against the width that panics (`shift out of range`); `~x` is `xor x, -1`; the wrapping methods are plain `add`/`sub`/`mul`; the counting methods are `llvm.ctpop`, `llvm.ctlz` and `llvm.cttz` (zero not poison), widened to `i64`. A program that uses none of them has none of this code.
- The v10 integer methods: `checked_add/sub/mul` read the overflow flag of `llvm.{s,u}{add,sub,mul}.with.overflow` and select `None` or `Some`; `checked_div` divides by 1 instead when the divisor is 0 (or the operands are `MIN` and −1) and selects `None` then, so it never traps; `saturating_add/sub` are `llvm.{s,u}{add,sub}.sat`, and `saturating_mul` selects `MIN` or `MAX` on overflow; rotations are `llvm.fshl`/`llvm.fshr` of the value with itself, the `i64` count truncated to the width; `I.MIN`/`I.MAX` are constants.
- `if let`, `while let` and `let … else` (v9) test their pattern as an arm of the arm chain does, jumping on a mismatch to the `else` branch, the loop's exit, or the `else` block; `let … else` declares its bindings in the enclosing block's scope.
- `unwrap` and `expect` (v9) are prelude methods since v18 (milestone 173), in `impl<T> Option<T>` and `impl<T, E> Result<T, E>`, marked `@caller_location` so their panic points at the caller's method name. **`@caller_location`** (v18): a marked function or a type's own method takes three hidden trailing parameters, the caller's file (a C string), line and column; every call passes its own place as constants, or, inside a marked function, the place that function was given, so a chain reports the first unmarked caller. `panic(…)` inside a marked function reports that place; the compiler's own panics (overflow, an index out of bounds) and unmarked callees' keep their own. A marked function is never a value (E0406), and an interface's item, a derive block's, an actor's message, `main` and a test can't be marked (E0210).
- An or-pattern (v9) lowers in the arm chain: each alternative that fails tries the next, and every alternative stores a name's value into the arm's one local for it; a `match` with an or-pattern or a `mut` binding never uses the flat `switch`. A `mut` binding is a local of its own, deep-copied when its type is.
- An iterator loop (v6) stores the iterator in a hidden local and calls its `next` with that local's address: directly for a struct or enum (its own method, or a bound default's instance in a generic instance), and through the table for an interface value.
- An actor's message handler (v17, milestone 92) is `lugha_actor_<Type>.<method>(state, args, result)`, `<Type>` the handle's type as it prints (a generic actor's instance, `Cache<string, i64>`; v18): it reads the arguments from a counted block, one field each in element layout, calls the state's method on the state and stores the result. An interface table built for handles (v18, milestone 202) holds `lugha_actor_<Type>.<method>$send(box, params…)` in each slot: it takes the handle from the box, copies the arguments into a block and sends the message, waiting for its result if it has one.
- Each constant (v17) is a global `lugha_const_<key>` (the key's module path joined with `/`), zeroed, filled by `lugha_const_init()` (which runs the functions the values call, milestone 147), which the C `main` (and a test executable's) calls right after `lugha_rt_init`; a read loads the global.
- A `spawn` site (v17) starts `lugha_task_<enclosing>.<n>(code, env, result)`, numbered per enclosing function from 0 like closures: it calls the closure the call was wrapped in and stores its result, in element layout, at `result`; a result that is an `Err` is then handed to `lugha_rt_task_failed` as an `Error` value (milestone 195).
- A scope (v18, milestone 195) keeps its runtime record in its function's frame. Within its body, a landing pad's cleanup runs only the body's `defer`s and releases, after `lugha_rt_scope_caught`, then goes on past the body instead of resuming: the unwind stops there. Every way out of the body waits for its tasks before the body's `defer`s run: its end through `lugha_rt_scope_end`, an early exit through `lugha_rt_scope_leave`.
- A closure's body (v3) is `lugha_closure_<enclosing>.<n>`: the enclosing function's symbol without its prefix, then the closure's number in that function from 0 (`lugha_closure_main.0`, `lugha_closure_main.0.0` for a closure inside it, `lugha_closure_max<i64>.0` in an instance). Its environment comes first, as for every function value's code.
- The array helpers (v3) are generic functions of a prelude written in Lugha, bundled with the compiler and checked with every program, under the module path `$prelude`, which no program can name: `lugha_fn_$prelude/map<Person, string>`. Each is compiled only at the types a program uses. `xs + ys` and `xs += ys` on arrays (v17, milestone 122) call `$concat<T>` the same way: a copy of `xs` with every element of `ys` pushed.
- The iterator helpers and sources (v10) are Lugha in the prelude too: `it.map(f)` calls the prelude's `iter_map` with `it` converted to `Iter<T>` (its type's table, as for any conversion), `xs.iter()` calls `array_iter`, and `it.sum()` calls the `iter_sum_<T>` of its element type. Like every prelude item, each function, instance and method (`lugha_fn_$prelude/Range.next`) is compiled only when the program reaches it. `for b of s` is the array loop over the string's bytes, 8 bytes past the string pointer.
- An extern follows the visibility rules of section 6, and the same C function may be declared in several modules only with identical signatures (E0306); it is one symbol. A renamed extern (v16) may give it another signature: LLVM declares the symbol once, and each call uses its own declaration's function type and C extensions.
- A callback (v16) is a C-ABI function lughac emits. Without `data`, each call site gets `lugha_cfn_<n>`, which calls the function value the site stores in the global `lugha_cbv_<n>` before the call (the value at a site never changes, E0436). With `data`, each callback C signature gets one trampoline, `lugha_cb_<signature>` (`lugha_cb_fun(GtkWidget, data)`), which holds `{ code, env }` from its `data` argument for the call (`lugha_rt_cb_hold`); a `spawned` one's, `lugha_cbq_<signature>`, copies C's arguments and the closure into a block and starts a task (`lugha_rt_cb_queue`) running `lugha_cbq_fun(<params>)` (v18, milestone 215); every entry stops a panic, giving C a zero value (`lugha_rt_cb_caught`); the `data` argument is `lugha_rt_cb_pin(code, env)` and a `release` argument `lugha_rt_cb_release`. Both convert `Option` of a handle and C's extensions as an extern call does. Programs without handles or callbacks give byte-identical IR.
- Runtime library functions use the `lugha_rt_` prefix. Neither prefix is a prefix of the other, so a Lugha function can never collide with a runtime function, whatever its name.
- Extern names starting with `lugha_` are reserved and rejected with an E03xx error, so an extern declaration can't collide with either.
- Extern C code must not keep a Lugha string after the call returns, and must not write through the pointer (section 7).
- C's `libc` and `libm` are always linked, so their functions can be declared directly.
- **Other C libraries** (v16, milestone 87) are named in a `[link]` table of `lugha.toml` (section 6); a package's table needs the program's `ffi` grant (v18, milestone 188). The tables of the program and of every package with a module in the program apply, program first, then packages in the order they are loaded; a package listed but never imported adds nothing. Each `paths` folder becomes `-L`, each library `-l<name>`, each once in order of first appearance, after `-lgc -lm` and every object, so a library comes before the ones it uses.
  - **`pkg-config`:** when `build`, `run` or `test` links, a table's names go to `pkg-config --libs`, and to `pkg-config --cflags` when that table lists C files. The program is `$PKG_CONFIG` if set, else `pkg-config`, started without a shell. Only `-l…`, `-L…` and `-pthread` are taken from `--libs`, and only `-I…`, `-D…`, `-pthread` and exactly `-mfpmath=sse`, `-msse` and `-msse2` (GTK's on x86-64, which only restate that target's defaults; v17) from `--cflags`, after the table's own lists; any other flag, a missing `pkg-config`, or one that fails is E0329 at the `pkg-config` line.
  - **C files** are each compiled by `cc -c` with their own package's `include` folders and `--cflags` only, never with the runtime, and linked into the program. One that doesn't compile is E0330 at its `c` line, with the compiler's message. They follow the rules above: C can't call Lugha functions, and a C file must not keep a Lugha string.
  - **The object cache** (v17, milestone 127): C files' objects, and the runtime's (compiled alone with `cc -c`, a variant per TLS and crypto), are kept in `objects/` under the cache root git packages use (`$LUGHA_CACHE`, else `$XDG_CACHE_HOME/lugha`, else `~/.cache/lugha`). One is reused only when the source's bytes, its flags, `cc --version`'s output, lughac's version, the object itself and every header the compile read (listed by `cc -MD`) are unchanged; anything else recompiles. A problem with the cache never fails a build: the file compiles as before. Deleting `objects/` is always safe. The cache is trusted as the git package cache is.
  - **Failures:** when a program with `[link]` input fails to link, that is E0331 with the linker's message (exit 1). Without `[link]` input a failing link is an internal error (exit 2), and the command is exactly the one in section 9.
  - **Checking runs nothing:** `lughac check` and the language server check a `[link]` table (E0321, and E0322 for a `paths` or `include` folder or `c` file that doesn't exist) but never start `pkg-config` or the C compiler.
  ```
  # lugha.toml
  [link]
  libs = ["raylib"]
  paths = ["vendor/lib"]
  c = ["shim/colors.c"]
  include = ["vendor/include"]
  pkg-config = ["gl"]
  ```

## 9. Compilation model

The compiler, `lughac`, is written in Rust with inkwell. It turns one `.la` file into one native executable, in six stages. Each stage stops the pipeline if it reports an error.

1. **Lex.** Source text → tokens with spans.
2. **Parse.** Tokens → AST. Recursive descent for items and statements, Pratt parsing for expressions.
3. **Check.** Resolve names, compute a type for every expression, check every rule in sections 4–6. Output: the AST plus a side table mapping each expression to its type.
4. **Lower.** Typed AST → LLVM IR via inkwell, one LLVM function per Lugha function. Run `module.verify()` afterwards; a verifier failure is a compiler bug, not a user error.
5. **Optimize and emit.** Run LLVM's pass pipeline (`default<O0>` or `default<O2>`), then write an object file with `TargetMachine`.
6. **Link.** Invoke the system C compiler, which also compiles the runtime: `cc prog.o lugha_rt.o -lm -o prog`. The compiler is `cc`, or (v17, milestone 132) what `$LUGHA_CC` names: its first space-separated word is the program and the rest go before lughac's arguments (`LUGHA_CC="zig cc"` runs `zig cc -c …`). It runs from an argument list, never a shell, so quotes and `$` are ordinary characters. A compiler that isn't there is exit 2 naming it; a `$LUGHA_CC` that isn't UTF-8 is exit 2. Cached objects are kept per compiler (its words and `--version`). The runtime source is embedded in `lughac` and written to a private temporary file for each build, compiled at `-O2`, and its object is kept between builds. With `[link]` tables (v16, section 8), their C files are compiled first, each on its own, and their objects follow `prog.o`; their `-L` folders, libraries and `-pthread` follow `-lm`.

**Lowering notes.** These are the places where Lugha semantics need specific IR:

- **Overflow checks:** lower `+`, `-` and `*` to the `llvm.sadd.with.overflow`, `llvm.ssub.with.overflow` and `llvm.smul.with.overflow` intrinsics (the `u` versions for `u8`). Each returns the result plus an overflow flag; branch to a panic call when the flag is set. Unary `-x` is a checked `0 - x`. At `-O2`, LLVM removes checks it can prove never fire.
- **Division:** compare the divisor to zero (and check `MIN / -1`) before `sdiv`/`srem`; branch to a panic call on failure.
- **Bounds checks:** load the length header, compare with `icmp ult` (an unsigned compare catches negative indexes too), branch to a panic call on failure.
- **Short-circuit `&&` and `||`:** separate basic blocks joined by a `phi`.
- **if expressions:** lower each branch to its own basic block, as for statements, and merge a result other than `()` with a `phi` in the join block. A branch that definitely returns contributes no incoming edge.
- **Float-to-int casts:** the `llvm.fptosi.sat` intrinsic gives the saturating behavior in section 4.
- **Opaque pointers:** every `load`, `store` and `getelementptr` needs its element type, taken from the checker's type table.
- **Array copies:** generate one deep-copy function per type that contains arrays (for example `lugha_copy_i64_arr_arr`, or `lugha_copy_5Point_arr` — struct names are length-prefixed so they can't be confused with `_arr` or primitive names; the `lugha_copy_` prefix is disjoint from `lugha_fn_` and `lugha_rt_`), and call it to clone a shared array before a write (v15). A copy site itself only marks arrays shared: `or` of the flag into `cap`, through the struct, interface box and closure copies that reach them. For arrays of plain values, a clone is one `lugha_rt_alloc` plus `llvm.memcpy`; for arrays of arrays, the helper copies the outer array and marks the inner ones.
- **JSON** (v17, milestones 139–141): `json.encode` and `json.decode` call `Json`'s items, `x.$to_json()` and `T.$from_json(v, path)`, which come from `std.$json_value`'s derive blocks: `$derive Json for i64 { … }`, `$derive<T: Json> Json for T[] { … }`, and `$derive Json for struct`/`enum`/`tuple` for every struct, enum and tuple whose parts meet `Json`. A block's items are compiled like a generic method of a hidden owner, once per type, `lugha_fn_std/$json_value/Json@struct.$to_json<Point>`; inside a struct, enum or tuple block the field intrinsics (`$derive.fields`, `build`, `variant`, `variants`, `build_variant`, bundled sources only) are unrolled into one call per field at that field's type. `$derive` blocks are written only in bundled sources, in the interface's own module.
- **Equality** (v17, milestone 144): numbers, `bool`, `char` and handles compare inline and strings through `lugha_rt_str_eq`; any other `==` calls its type's own `eq` or the instance of its `Eq` block, `lugha_fn_$prelude/Eq@struct.eq<P>`, whose `derive.all` reads both values in place and stops at the first difference. Prelude and std instances are internal, so LLVM inlines them.
- **Struct, enum and array arguments:** pass a pointer to the caller's storage rather than the value itself, since the callee can't modify it.
- **`?`** (v4): the operand goes in a slot and its tag picks the branch. The failing branch builds the function's own `Err(e)` (or `None`) from the payload and returns it; the passing branch loads the `Ok`/`Some` payload.
- **`defer`** (v17, milestone 151): emits nothing where it is written; each block keeps the `defer` statements reached so far, with the names then in scope. Every exit lowers the pending ones inline, innermost block first and last first: the block's end after its tail, `return` and a failing `?` (every block of the function, after the value is built), `break` and `continue` (the blocks inside the loop). A statement reached by several exits is emitted at each. A value leaving while defers are pending is copied first (arrays only marked shared, section 4). Liveness counts a local mentioned in a deferred statement as used at its block's end, so nothing below the `defer` hands it over.
- **Unwinding** (v18, milestone 194): at each statement's end, the calls made directly in it that may unwind (Lugha functions, function values, the runtime's functions; not count operations, intrinsics or C) get a landing pad if the frame then holds anything to clean up: a `landingpad` (cleanup only, personality `__gcc_personality_v0`) followed by what a `return` there would run — the pending `defer`s and the frame's releases — then `resume`. A call that releases its fresh arguments itself on return gets a pad releasing them first. The calls become `invoke`s once struct results and split parameters are rewritten. Every function has unwind tables (`uwtable`). The C `main` gives `lugha_fn_main`'s call a pad releasing `main`'s arguments; a callback's entry gives its calls one ending the program (`lugha_rt_unwound_to_c`). A panic is a forced unwind (`_Unwind_ForcedUnwind`) whose stop function ends it at the task's first frame (a green thread's boot frame is marked outermost).
- **`loop`** (v17, milestone 152): a head block (where `continue` goes, and a preemption point like every loop head) and an exit block (where `break` goes). A `loop` with a value gets a result slot in the entry block; `break value` stores into it, after copying the value as any store does, before the defers it leaves run, and the exit loads it. A `never` loop's exit block is unreachable. Labels are resolved by the checker; each jump goes to its loop's head or exit.
- **Text and option helpers** (v4) are methods of the prelude's blocks for each type (v18, milestone 172): `impl string { fun trim(self) … }`, `impl<T> Option<T> { … }`, `impl<T, E> Result<T, E> { … }`, `impl char { … }`, called as methods, with a hidden owner per block (`lugha_fn_$prelude/$string.trim`); `slice` is built in and calls `lugha_rt_str_slice`. A prelude method or function without type parameters is compiled only when user code reaches it, as generic ones are. Only the prelude writes such blocks (E0466 in a program).
- **`Result`** (v4) is declared in the prelude, so it is the generic-enum machinery throughout; its LLVM type and helpers are named as it prints (`%"Result<i64, string>"`).
- **Array helpers** (v3): `xs.map(f)` calls the instance of `map` in the prelude's `impl<T> T[] { … }` (v18, milestone 172; a free function `map(xs, f)` before), the receiver passed as any method's is; `sort` is in `impl<T: Ord> T[]` and `join` in `impl string[]`. The prelude is Lugha source like any other (`sort_by` is a bottom-up merge sort between two buffers), so it needs no special lowering; `--emit=tokens|ast` leaves it out, and an error inside it is reported as an internal compiler error.
- **`Ord`** (v15; v18): `<` on a struct or enum calls its `cmp`: the `impl Ord` block's method, or the instance of the prelude's derive block for its type, which compares part by part through `derive.order` and returns −1, 0 or 1 as `i64`, stopping at the first nonzero part (an enum's tags first, by declaration position); the result's tag is compared with `Less` and `Greater` by variant. Tuples are ordered through the same derive block. Through `T: Ord`, each instance resolves `cmp` for its type; numbers, `char` and `string` compare inline. `f64.total_order` flips the bits of a negative float's other bits (`bits ^ 0x7fff…`) and compares the results as signed integers.
- **Shared boxes** (v16): `Ref.new` is `lugha_rt_alloc` and a store; `get` a load and a copy site; `set` a store by the store rules; `update` a call of the function value on a copy, then a store; `is` a pointer comparison. A `Ref` is a plain pointer to the copy helpers, which never look inside it. Printing a box (`lugha_show_<T>_ref`) asks `lugha_rt_show_enter(box)` whether it is already on the runtime's stack of boxes being printed: if so it writes `Ref(…)`, otherwise it writes the contents and pops with `lugha_rt_show_leave()`.
- **Compiled when reached** (v15, milestone 60): the prelude's and std's non-generic functions and methods are listed in the instance list like generic ones, from the uses in the program's bodies and in other listed bodies, and only listed ones are declared and defined. A function used as a value counts as a use, and so does converting a value to an interface for each method its table holds. A program's own modules and its packages are compiled whole.
- **`it.sum()`** (v15, milestone 61) calls the prelude's `iter_sum<T: Num>` instance for the element type (`lugha_fn_$prelude/iter_sum<i64>`), one generic body instead of one per number type.
- **Calling a function value** (v3): extract its code and environment, and call the code indirectly with the environment as a hidden first argument (after the result slot when it returns a struct, v17), then the arguments as for any call (aggregates by pointer).
- **Monomorphization:** after checking, the checker lists every generic function and method instance reachable from the non-generic code, at concrete type arguments, and every concrete type those instances use (E0420 when they nest too deep). Codegen compiles each instance from its one body by substituting the type arguments into the types the checker recorded; it still never infers a type. Generic structs and enums get one LLVM type per instance, named as they print (`%"Stack<i64>"`), and their own copy and equality helpers (`lugha_copy_10Stack<i64>`).
- **Methods:** ordinary functions whose first parameter is the receiver. A `self` receiver is passed like a struct argument; a `mut self` receiver is the address of the caller's place itself, so the method's writes land there.
- **`match`:** an LLVM `switch` on the tag for enums and on the value for integers and `bool`s; a chain of `lugha_rt_str_eq` tests for strings. Payload bindings load from the payload area at constant offsets. An exhaustive `match` without `_` ends in `unreachable`. A `match` with a nested pattern or a guard is lowered arm by arm instead: each arm tests its pattern outside in (loading indirect payloads through their pointer), binds, runs its guard, and on any failure branches to the next arm.

**Runtime library.** A small C file, `lugha_rt.c`, is compiled once and linked into every program. The compiler calls only these functions:

| Function | Purpose |
| --- | --- |
| `lugha_rt_init(argc, argv)` | Keeps `argc`/`argv` for `args()`, installs the stack-overflow handler for the C stack (an alternate signal stack and a `SIGSEGV` handler that reports only faults at the process stack's end; task stacks check at each function's entry) and the Ctrl-C handler (SIGINT: the first cancels the root scope, a second exits with 130), and reads the scheduling mode from `LUGHA_SCHED` (`deterministic`: virtual time; `shuffle:SEED`: virtual time and seeded picks of the ready task and `select` arm; milestone 203) the helper threads' limit from `LUGHA_HELPERS` (milestone 206) whether the owner check runs from `LUGHA_RC_CHECK` (milestone 208) and the number of workers from `LUGHA_WORKERS` (milestone 210), then fills the hash key `lugha_rt_hash_key` (two `u64`s read inline by generated code): from the operating system's randomness (`getrandom`, else `/dev/urandom`), or in a deterministic run fixed, derived from the shuffle seed (milestone 204). The runtime may use randomness that can't affect a program's results, as the hash key can't; the C `main` calls it first, since a C macro can't be called from IR |
| `lugha_rt_main(argc, argv, entry)` (v18, milestone 205) | The C `main`'s one call: takes a pooled stack for `main`, notes where C's calls start on the process's stack, runs `entry(argc, argv)` there and returns its exit code |
| `lugha_rt_c_call(thunk, frame)` (v18, milestone 205) | A call of a C function: runs `thunk(frame)` on the C stack (in place when already there), with the C stack's limit for any callback's Lugha code. Generated code makes, per call site, the thunk `lugha_c_<n>(frame)`, which takes the arguments from the frame, makes the call with its attributes and stores the result there |
| `lugha_rt_c_blocking(thunk, frame)` (v18, milestone 207) | A call of an `extern blocking fun`: gives `thunk(frame)` to a helper thread and waits, uncancellably, for it to return; in place, as `lugha_rt_c_call`, where no helper may be used |
| `lugha_rt_cb_enter()` / `lugha_rt_cb_leave()` (v18, milestones 207 and 215) | The first and last calls of every C entry into Lugha (callbacks): on a helper thread it ends the program, since Lugha code can't run there; between them no cancellation point unwinds |
| `lugha_rt_ring_new(n, size, file, line, col)` / `lugha_rt_ring_try_send(ring, value)` (v18, milestone 218) | `Ring<T>.new(n)`, a channel with a ring's writing side; `try_send` on its `RingSender`: 0 sent, 1 full, 2 closed, without a lock; readers waiting are woken through the poller's eventfd |
| `lugha_rt_cb_run(body, frame)` (v18, milestone 217) | Every C entry's call of its body: in place on the program's threads; on a thread C made, on that thread's own C stack, made on its first callback |
| `lugha_rt_cb_hold(pinned, env)` (v18, milestone 215) | Before a callback with `data` runs: its captures, a new holder, in `env`, and 1; 0 once its scope's body has ended |
| `lugha_rt_cb_queue(pinned, entry, block, pins)` (v18, milestone 215) | A `spawned` callback's call: a task in its scope running `entry(NULL, block, …)`, `block` handed over; dropped once the scope's body has ended; `pins`: on the calling worker |
| `lugha_rt_cb_task_panicked(text)` / `lugha_rt_cb_caught(pinned)` (v18, milestone 215) | A callback task's panic, handled by its scope's policy; a panic stopped at a direct callback's entry, reported by a task of the registering scope |
| `lugha_rt_stack_mask`, `lugha_rt_stack_low()` (v18, milestones 205 and 210) | A stack's size less one: every stack Lugha code runs on (a task's, `main`'s, each worker's C stack) is aligned to its size, so a function's entry tests `(sp & lugha_rt_stack_mask) < 256 KiB` with no memory load and nothing kept per thread; and what it calls below that: `panic: stack overflow`, which unwinds, its cleanups allowed down to 64 KiB |
| `lugha_rt_alloc(size)` | Allocate a zeroed counted object, held once (`lugha_rt_rc_alloc(size, visit)` also sets what releases its contents) |
| `lugha_rt_str_from_c(text, nullable)` (v16) | A copy of C's NUL-terminated text as a string, invalid UTF-8 replaced; `NULL` stays `NULL` when `nullable`, else panics |
| `lugha_rt_task_spawn(entry, code, env, size)` (v17) | A new green thread running `entry(code, env, result)`, `result` being `size` bytes the task holds; returns the task |
| `lugha_rt_task_join(task)` (v17) | Waits for the task; the address of its result |
| `lugha_rt_scope_enter(record, name, policy, handler, env, limit, within, until, pinned, panic_handler)` (v18) | Starts a scope, its record in the caller's frame: the current task's innermost; its deadline `within` ms on or at `until` monotonic ms (-1: none), never later than its enclosing scope's; `pinned` (1, milestone 214) keeps it on one worker; `panic_handler` (milestone 215), for `independent(f)`, gives a callback task's panic to the handler as a `Failure<E>` |
| `lugha_rt_pin_here()` (v18, milestone 214) | Before each call of an extern taking or giving a pinned handle: the running task stays on its worker from now on |
| `lugha_rt_scope_remaining(record)` (v18) | `s.remaining()`: the milliseconds left, at least 0, or -1 without a deadline |
| `lugha_rt_shield_enter(ms)`, `lugha_rt_shield_exit()` (v18) | A shield's block starts (cancellation held off for at most `ms`) and ends, however it ends |
| `lugha_rt_shield_caught()`, `lugha_rt_shield_lost()`, `lugha_rt_shield_end()` (v18) | A shield's landing pad stopped an unwind: a cut-off gives the first unwind back, any other is kept; a shield with a value was cut off while unwinding (exit 101); past the shield, the kept unwind goes on and a held cancellation applies |
| `lugha_rt_task_spawn_in(record, entry, code, env, size)` (v18) | `lugha_rt_task_spawn`, the task started in that scope |
| `lugha_rt_scope_end(record)`, `lugha_rt_scope_leave(record)` (v18) | The body's end: waits for the scope's tasks; an early exit: cancels them with `.Parent` first, and ends the scope |
| `lugha_rt_scope_caught(record)` (v18) | A landing pad stopped an unwind at the scope: a panic fails the scope, then its tasks are cancelled and waited for |
| `lugha_rt_scope_exit(record, table, box, panic)` (v18) | Ends the scope: 0, or 2 with a failed task's `Error` (its table and box), or 3 with a panic's message; a cancellation from outside goes on unwinding |
| `lugha_rt_scope_root()`, `lugha_rt_scope_root_finish()`, `lugha_rt_scope_root_unwound()` (v18) | The root scope's record; once `main` returns, waits for its tasks (exit 101 if one failed); once `main` unwound from a cancellation, the same wait |
| `lugha_rt_task_failed(table, box)` (v18) | The current task returned `Err`: it fails its scope with that `Error` |
| `lugha_rt_task_join_status(task, reason, text, table, box)` (v18) | Waits for the task (a pause point): 0 with its value ready, 1 cancelled (its reason and text), 2 failed (its error), 3 panicked (its message) |
| `lugha_rt_task_cancel(task, why)`, `lugha_rt_task_name(task, text)` (v18) | `t.cancel(why)`, `t.name(text)` |
| `lugha_rt_cell_new(a, b, c, d)`, `lugha_rt_cell_get(cell, i)`, `lugha_rt_cell_set(cell, i, v)`, `lugha_rt_cell_bytes(cell)`, `lugha_rt_cell_set_bytes(cell, bytes)` (v18) | A cell: a counted runtime object of four integers and a byte array, the shared state of the library's handles (a file's descriptor and read-ahead, a socket's descriptor and timeout); every copy of a handle holds the one cell |
| `lugha_rt_task_yield()` / `lugha_rt_task_sleep(ms)` (v17) | `task.yield_now()` and `task.sleep(ms)` |
| `lugha_rt_task_preempt()` (v17) and the step budget (milestone 210) | The countdown generated code decrements at function entries and loop heads: the top word of the running stack, `(sp | lugha_rt_stack_mask) - 15`, so it moves with its task; at zero it calls `lugha_rt_task_preempt`, which refills it and yields |
| `lugha_rt_task_go(task)` (milestone 210) | Queues a task `lugha_rt_task_spawn_in` made, once generated code has set its result's visit, its name and an actor's record: until then no worker can run it |
| `lugha_rt_print_begin()`, `lugha_rt_print_end()` (milestone 210) | Around `println`'s text and newline: stdout's lock, so a line prints whole though other workers print too |
| `lugha_rt_task_select(cases, n, timeout, has_default, file, line, col)` (v17) | `select` over `n` cases `{ chan, kind, data, ok }` (kind 0 receive, 1 send); the case that ran, −1 for `default`, −2 for `after` |
| `lugha_rt_chan_new(cap, elem, file, line, col)` (v17), `lugha_rt_chan_unbounded(elem, file, line, col)` (v18) | A channel of `elem`-byte values, buffering `cap` (a negative `cap` panics), or growing its buffer as it fills; it keeps where it was made, for deadlock reports; generated code gives both ends the one channel |
| `lugha_rt_chan_try_send(c, value)` (v18) | `tx.try_send(v)`: 0 sent, 1 full, 2 closed; unless sent, the value stays the caller's |
| `lugha_rt_chan_send(c, value, file, line, col)` / `lugha_rt_chan_recv(c, out)` / `lugha_rt_chan_try_recv(c, out)` / `lugha_rt_chan_close(c, file, line, col)` (v17) | A channel's operations on values in element layout; receives return 1 for a value, 0 for closed (or, for `try_recv`, nothing ready); `try_recv` gives 1 with a value, 0 when empty, 2 once closed and empty (v18) |
| `lugha_rt_actor_new(state, name, capacity)`, `lugha_rt_actor_run(actor)`, `lugha_rt_actor_started(task, actor)` (v18) | A new actor holding `state` (a counted block), its mailbox holding `capacity` messages; its loop, the closure of the task generated code starts in the scope, so the scope's policy meets its failure; the task noted as the actor's, which its scope stops once the scope's other tasks are done |
| `lugha_rt_actor_call(a, handler, args, size, file, line, col)` / `lugha_rt_actor_cast(a, handler, args, file, line, col)` (v17) | Queue a message; a call waits and returns its result's address, and panics in the caller if the actor stopped or panicked |
| `lugha_rt_actor_stop(a, file, line, col)` (v17) | `stop()`, queued past the mailbox's limit |
| `lugha_rt_cb_pin(code, env)` / `lugha_rt_cb_release(pinned)` (v16) | A copy of a function value C can keep as a callback's user data, registered in the calling task's innermost scope (v18, milestone 215); its captures let go of when that scope's body ends or C releases it, the record freed when C does |
| `lugha_rt_panic(msg, file, line, col)` | Print the panic message and `exit(101)` |
| `lugha_rt_str_concat(a, b)` | Implement string `+` |
| `lugha_rt_panic_bounds(len, index, file, line, col)` | The out-of-bounds panic, with the length and index |
| `lugha_rt_str_eq(a, b)` | Implement string `==` and `!=`; returns 1 or 0 as `i32` |
| `lugha_rt_str_cmp(a, b)` | Implement string `<`, `<=`, `>`, `>=`; returns −1, 0 or 1 as `i32` (v2) |
| `lugha_rt_array_grow(array, elem_size)` | A copy of a full array with double the capacity (at least 4), for `push`; the copy is unshared |
| `lugha_rt_print_*` / `lugha_rt_to_string_*` | One per primitive type (plus `lugha_rt_print_str`), for the intrinsics; `bool` and `u8` arguments are passed zero-extended to 32 bits; `u32` goes to the `u64` functions zero-extended (v7), as does `u16`, and `i8`/`i16` go to the `i64` ones sign-extended (v10) |
| `lugha_rt_str_quote(s)` | `s` quoted and escaped, for strings inside a printed value (v12) |
| `lugha_rt_os_seed()` | 64 bits of OS entropy for the real `Entropy` (v18; `std.random`'s `from_os()` before): `getrandom`, else `/dev/urandom`, else a panic |
| `lugha_rt_env(name, out)` | `Env` (v18; `std.env` before): a variable's bytes, or 0 when unset or the name holds a NUL |
| `lugha_rt_dir_cwd` / `lugha_rt_dir_open` / `lugha_rt_dir_sub` / `lugha_rt_dir_read_bytes` / `lugha_rt_dir_read_text` / `lugha_rt_dir_write` / `lugha_rt_dir_list` / `lugha_rt_dir_make` / `lugha_rt_dir_remove` / `lugha_rt_dir_rename` / `lugha_rt_dir_kind` / `lugha_rt_dir_size` / `lugha_rt_dir_chmod` / `lugha_rt_dir_sync` / `lugha_rt_dir_file` / `lugha_rt_dir_path` | `Dir` (v18, milestone 186): every path opened beneath the folder's descriptor with `openat2` and `RESOLVE_BENEATH \| RESOLVE_NO_MAGICLINKS`, as above |
| `lugha_rt_lockdown(kinds)` | Process lockdown (v18, milestone 189): seccomp and Landlock for the capability kinds the entry took (`Fs` 1, `Net` 2, `Proc` 4, `Clock` 8, a test's `Dir` 16), best effort; called by the C `main` and the test entry after making the capabilities |
| `lugha_rt_file_read(fd, max, out, error)` / `lugha_rt_file_write(fd, bytes, error)` / `lugha_rt_file_close(fd)` | `File`'s (v18; `std.fs`'s before) file streams (v17, milestone 146): one read, a write of every byte, a close, on a descriptor `lugha_rt_dir_file` opened |
| `lugha_rt_spawn(cmd, args, input, mode, code, out, err, error)` | `Proc` (v18; `std.process` before): runs a program without a shell (mode 0 shares stdio, 1 captures with an empty stdin, 2 also writes `input`), and gives its code and output bytes, or 0 and why it couldn't start |
| `lugha_rt_fmt_int(v, bits, signed, kind, alt, fill, align, width, zero)` / `lugha_rt_fmt_float(v, f32, precision, fill, align, width, zero)` / `lugha_rt_pad(text, fill, align, width)` | Format specifiers (v13): an integer (sign- or zero-extended to `i64`), a float (as `f64`), or any other value's text, padded; all arguments are plain numbers |
| `lugha_rt_print_newline()` | The newline `println` adds |
| `lugha_rt_args()` | A new `string[]` of the program's arguments (v3) |
| `lugha_rt_parse_f64(s, out)` | Parse `s` into `*out`; return 1, or 0 for `None` (v3). `parse_i64` and `parse_u64` are prelude functions since v15 (milestone 62) |
| `lugha_rt_str_slice(s, from, to, file, line, col)` | `s.slice(from, to)`: a new string of those bytes, or the bounds panic (v4) |
| `lugha_rt_read_stdin_bytes` (v18, milestone 228: 1 and the bytes, or 0 and why a read failed) / `lugha_rt_str_to_bytes` / `lugha_rt_str_from_bytes` | Bytes (v14): stdin's bytes, a string's bytes; and (v15) checked bytes as a new string, behind the prelude's `$string.from_bytes`. The UTF-8 check behind `string_from_utf8` and `s.chars()` is one prelude decoder, `utf8_decode` (milestone 62) |
| `lugha_rt_print_char` / `lugha_rt_to_string_char` / `lugha_rt_char_quote` | A `char` written as UTF-8, as a string, and quoted for a built-in form (v14) |
| `lugha_rt_read_line(number, out, error)` | `read_line()`: 1 with the line's number and bytes, 0 at the end of input, -1 and why a read failed (v4; v18, milestone 228: bytes, which the prelude checks with `string_from_utf8`) |
| `lugha_rt_eprint(text, newline)` / `lugha_rt_exit(code)` | `eprint`/`eprintln` (values formatted first) and `exit`, each flushing stdout first (v4) |

The runtime writes through C's `stdout` stream, so output from `print`/`println` and from libc functions such as `puts` appears in program order.

The C `main(argc, argv)` emitted by the compiler calls `lugha_rt_main(argc, argv, lugha_entry)` (v18, milestone 205), which runs the internal `lugha_entry(argc, argv)` as the `main` task on a pooled stack and returns its exit code; `lugha_entry` calls `lugha_rt_init(argc, argv)`, then `lugha_fn_main()`, and returns the exit code. An interpolated string lowers to the `lugha_rt_to_string_*` functions and `lugha_rt_str_concat`.

**Command-line interface**

| Command | Effect |
| --- | --- |
| `lughac build prog.la [-o prog]` | Compile and link an executable |
| `lughac run prog.la [-- args…]` | Build to a temporary file and run it, passing `args` to the program (v3) |
| `lughac check prog.la` | Lex, parse and type-check only; no codegen or linking (type checking from milestone 3). A file without `fun main` is checked as a library (v17): in its package's `source` folder as that package's module, elsewhere as the module named after the file (`diff.la` → `diff`), so modules it imports may import it back (v17); `lughac test` loads files the same way |
| `lughac skill [PAGE]` | Print the Lugha skill's overview or one page; `--list`, `--search TEXT`, `--export DIR`; see below (v18, milestone 230) |
| `lughac update prog.la` | Re-resolve every git package to its newest matching tag and rewrite `lugha.lock` (v14); print `zlib now runs C code, through `sqlite`'s ffi grant` for each package the update newly lets run C code (v18) |
| `lughac init [folder] [--lib] [--name N]` | Start a program, or with `--lib` a library, in `folder` (default: the current one); see below (v16) |
| `lughac lsp` | Serve the Language Server Protocol on stdin and stdout, for editors; see below (v16) |
| `lughac explain [CODE]` | Explain an error or warning code, or list every code; see below (v16) |
| `lughac fmt [paths…] [--check]` | Rewrite `.la` files in the canonical style; `--check` only lists the files that would change; see below (v16) |
| `lughac test [paths…] [--filter TEXT]… [--exact] [--integration] [--timeout SECS] [--list] [--json] [--shuffle[=SEED]]` | Run (or list) the `@test` functions of `.la` files, each in its own process; see below (v16) |
| `--offline` | On `build`, `run` and `check`: run no `git`; every git package must be locked and cached (v14) |
| `lughac build --emit=tokens\|ast\|ir prog.la` | Print an intermediate stage and stop; with several modules, `tokens` and `ast` print each after a `; module <name>` line (`main` for the main file) |
| `--diagnostics=human\|json` | Diagnostic format for `build`, `run` and `check`; `human` is the default |
| `-O0` / `-O2` | Optimization level; `-O0` is the default |
| `--cpu=LEVEL` | On `build`, `run` and `test` (v17, milestone 130): the x86-64 level the program's code is built for, one of `x86-64` (the default: every 64-bit Intel and AMD CPU), `x86-64-v2`, `x86-64-v3`, `x86-64-v4` or `native` (this machine's own CPU and features; the executable may not run on another). Any other value is exit 2. The runtime and `[link]` C files are compiled by `cc` at its default either way. Before v17 every build used `native` |
| `--target=TRIPLE` | On `build` only (v17, milestone 133): the platform the executable is for, `x86_64-unknown-linux-gnu` (the default) or `aarch64-unknown-linux-gnu` (Linux on ARM64, the baseline ARMv8-A; `--cpu` with it is exit 2). The C compiler is then `cc` or `aarch64-linux-gnu-gcc`, unless `$LUGHA_CC` names one (section 8). Any other value is exit 2. |

Exit codes: 0 on success, 1 when the program has errors, 2 for bad command-line usage, an internal compiler error, or output lughac couldn't write (a reader that stops early, as `lughac skill | head` does, is not a failure).

**`lughac init`** (v16, milestone 69) makes `folder` if needed and writes, for a package named `N`:

| File | Program | Library (`--lib`) |
| --- | --- | --- |
| `lugha.toml` | `[package]` with `name = "N"`, `version = "0.1.0"`, `source = "src"` | the same |
| `src/main.la` | `import greeting;` and `fun main()` printing `greeting.hello("world")` (v17, milestone 129; `Hello from N!` before) | — |
| `src/greeting.la` | `fun hello(who: string): string`, giving `Hello, <who>, from N!` (v17) | — |
| `src/lib.la` | — | the entry (v17; `src/N.la` before), with a module doc and `public fun greet(who: string): string`, giving `Hello, <who>, from N!` |
| `examples/hello.la` | — | `import N;` (v17; `import N.N;` before) and a `main` printing `N.greet("world")`, runnable at once through the implicit package (section 6) |
| `test/greeting_test.la`, `test/N_test.la` | `import N.greeting;` and a `@test fun greets()` checking `greeting.hello("world")` (v17) | `import N;` and a `@test fun greets()` checking `N.greet("world")` (v17) |
| `.gitignore` | `/main`, the executable `lughac build src/main.la` writes | `/hello` |

It prints `created <path>` for each file, then how to run the result and (v17, milestone 129) how to test it: `test it with: lughac test <folder>`, or `lughac test` for the current folder. `lughac test` passes at once, and `lughac fmt` changes none of the files.
- **The name** is `--name`, else the folder's last component once `.` and `..` are resolved (`lughac init` in `~/oyster` makes `oyster`). It must be an identifier that is neither a keyword nor `std`; otherwise it is exit 2 with a suggestion: `` `my-shell` isn't a valid package name `` and help `` try `lughac init my-shell --name my_shell` ``.
- **Nothing is overwritten.** A `lugha.toml` already in the folder stops `init` before it writes anything (exit 2). Any other file that exists is kept, reported as `kept <path> (already exists)`, and `init` still succeeds.
- An I/O failure is exit 2 with the system's message; files written before it stay. `init` runs no `git`, uses no network and never asks a question.

**`lughac lsp`** (v16, milestone 70) speaks the Language Server Protocol (3.17) on stdin and stdout; an editor starts it. It publishes the diagnostics `lughac check` would report, computed from the editor's text of every open document (unsaved changes included) and the disk for the rest, 200 ms after the last change or at once on opening and saving.
- **What is checked:** a file with `fun main` checks itself. A module checks through the main files the client lists in its `lugha.entries` setting (relative `.la` paths inside the workspace folder, sent as `initializationOptions` or through `workspace/didChangeConfiguration`) and through every main file the server has already checked that loaded it; with neither, it is checked alone and E0303 is left out.
- **Diagnostics** carry the code, `source: "lughac"`, the message with the label and `help:` on their own lines, and secondary labels as related information; ranges count UTF-16 units. Each lands in the file it belongs to; a manifest error lands in `lugha.toml`; errors inside `<std>` modules have no file and are dropped.
- **Safety:** it is always offline (as `--offline`: no `git`), writes no file (not `lugha.lock` either) and opens no port. A panic inside the compiler is caught and shown as one internal error at the top of the file; the server keeps running.
- **Hover** (milestone 71): `textDocument/hover` shows what is under the cursor as a Lugha code block — the smallest expression or declared name there, a name winning a tie:
  - a `let`, parameter, `for` or `match` binding: `let total: i64` (`let mut n: i64`);
  - a function or method, at its declaration or a use: its header as written, a method's with its type, `fun Circle.area(self): f64`;
  - a struct field at its declaration: `x: f64`;
  - a module's name, in an `import` or before `.` (v17, milestone 98): `module geo.shapes`, followed by its module doc (section 2);
  - an expression: its type, printed as diagnostics print types.

  Notes follow the block: `instance: max<f64>` for a generic call, `converted to Shape` for a value converted to an interface. Hover uses what the checker found even when the program has errors; an expression the checker couldn't type, whitespace, comments and keywords have none (`null`). In a file with a syntax error, the program the parser recovered is checked (section 3), so everything but the broken statements has a hover; only while a module doesn't lex (an unclosed string) is there none. A hover first runs a check still waiting for typing to pause.
- **Go to Definition** (milestone 72): `textDocument/definition` answers the declaration of the smallest name under the cursor: a variable or parameter → its binding; a function or method (at a call, a value or the declaration itself) → its declaration; `s.area()` on an interface value → the interface's method; a type name (an annotation, a literal, an `impl`, a construction) → its struct, enum or interface; `p.x` and a literal's `x:` → the field; a variant → its declaration in the enum; a module in an `import` or a prefix (`geo` in `geo.f`) → the module's file, from its top. A declaration inside `std` or the prelude has no file, so there is none (`null`); so for anything else under the cursor. As hover, it uses what the checker found even with errors, and runs a pending check first.
- **Completion** (milestone 73): `textDocument/completion`, also triggered by `.`. After `.`: a value's fields and methods, the built-in methods and prelude helpers its type accepts (the iterator helpers for an iterator), a module's items except another module's `private` ones, and a type's variants and associated functions. Otherwise: the locals and parameters declared earlier in the enclosing function, the module's functions and types, imported modules, the intrinsics, `Some`, `None`, `Ok`, `Err` and the keywords. Each item has a kind and a detail as hover shows it; the editor filters and orders them. Nothing is offered in a comment or a string. Completion reads the current text's check, partial while it has a syntax error (milestone 75); only while a module doesn't lex does it read the last check of the file that parsed, and then it uses that check's positions only within the leading text the check and the current text share, and a receiver written past that point is looked up by name among the locals, imports and types. Otherwise the list is empty, never a guess.
- **Signature help** (milestone 74): `textDocument/signatureHelp`, triggered by `(` and `,`: the signature of the innermost call the cursor is inside (found by scanning back to its unclosed `(`, skipping strings, comments and other brackets), with the parameter it is on. A declared function or method shows its header as written; a generic one whose instance the last check that parsed knew at that call shows it filled in (`fun max(a: f64, b: f64): f64`) with the written header as documentation; variants and built-in methods show their types (`Circle(f64)`, `fun push(i64)`); intrinsics show their call form from the table above (`read_file(path)`). The callee is resolved as completion resolves a receiver. A grouping parenthesis, a closed call, a string or a comment has none (`null`).
- **Documentation** (milestone 77): a declaration's doc comment (section 2) follows its header in hover, completion items carry it, and signature help shows its description with the active parameter's `@param` text. Hover also answers on a type's or variant's name: `struct Point`, `enum Shape`, `interface Area`, `Circle(f64)`, with their docs. std and the prelude are documented.
- **Snippets** (milestone 78), for a client that supports them: completing a function, method, associated function or variant with a payload inserts its call with a placeholder per parameter, `max(${1:a}, ${2:b})`, unless a `(` already follows; after `.` on a value whose type is an enum, `Option` or `Result`, a `match` item replaces `value.` with a `match value { … }` holding every variant and a placeholder per payload slot. The VS Code extension adds fixed snippets for declarations and control flow.
- **Quick fixes** (milestone 79): `textDocument/codeAction` answers each E0417 among the request's diagnostics with **Add missing arms** — one arm per missing pattern, every one the checker found, each written in full (`Rect(_, _)`, `Some(Dot)`), preferred — and **Add a `_` arm**; a match on integers, strings or interface values gets only the second. Each new arm's body is `panic("not handled")`; the arms go before the match's closing `}` at its arms' indentation, with a comma added after the last existing arm if needed. Only a current check of text that parses answers.
- **More quick fixes** (milestone 82), each marked preferred and only from a check of the current text that parsed: on E0501, when the place's root name was declared by a `let` without `mut` in the same file, **Make `x` mutable** inserts `mut ` after that `let` (parameters, loop variables, `self` and pattern bindings get none); on W0103, when one variant of the enum the binding matches differs from it only in case, or is the only closest within 2 edits, **Change to `V`** replaces the binding with it; on E0301, when the undefined name is a std module (`math`, `random`, `env`, `fs`, `process`) followed by `.`, **Add `import std.m;`** inserts the import after the file's last import, or first with a blank line after it.
- **References** (milestone 83): `textDocument/references` answers every place written that resolves to the same declaration as the name under the cursor (at a use or the declaration) — locals, parameters, functions, methods, types, fields, variants, interface methods, modules — in every file the server has checked (open files, their imports, the mains in `lugha.entries` and mains already checked), with the declaration when the request includes it. A `std` or prelude name lists its uses; an intrinsic (`println`) has no declaration and answers `null`.
- **Rename** (milestone 83): `textDocument/prepareRename` answers the name's range, and `textDocument/rename` one edit per place References finds, after applying them in memory and checking every affected main again: each renamed place must resolve to the renamed declaration, nothing else may, and no file may gain an error. Refused, with the reason as the error message: no name under the cursor; a module (it would rename a file); a name from `std` or the prelude; a new name that isn't an identifier or is a keyword; a module-level item that isn't `private`, declared outside a main file, that no checked main imports; and anything the self-check rejects ("renaming `b` to `a` would make a use point elsewhere").
- **Formatting** (milestone 81): `textDocument/formatting` (Format Document) formats the whole document from the editor's text, unsaved changes included, exactly as `lughac fmt` would format the file; the answer is one edit replacing the document, or none when it is already formatted. The request's tab and space options are ignored: the style has none. A document with syntax errors gets no edits and no message (its diagnostics show the errors); one that fails the formatter's own check gets no edits and one error message (`window/showMessage`). There is no range or on-type formatting, and the server still writes no file: the editor applies the edit. Format on save is the editor's setting; the VS Code extension makes itself Lugha's default formatter.
- **Semantic tokens** (v17, milestone 154): `textDocument/semanticTokens/full` answers every token of the document from the editor's text, in LSP's relative encoding with UTF-16 columns; there are no `range` or `delta` requests. The legend uses only LSP's predefined names. Types, in order: `namespace`, `type`, `struct`, `enum`, `interface`, `typeParameter`, `parameter`, `variable`, `property`, `enumMember`, `function`, `method`, `decorator`, `keyword`, `comment`, `string`, `number`, `operator`; modifiers, in order: `declaration`, `mutable`, `defaultLibrary`, `documentation`.
  - The lexer gives every span: keywords (`true`, `false`, `self` too) are `keyword`; built-in type names (`i64`, `string`, …) `type` + `defaultLibrary`; numbers `number`; strings, `char` literals, every piece of an interpolated string and its format specifiers `string`; operators `operator`; comments `comment`, a doc comment with `documentation`; `@` and the name after it `decorator`. Brackets, `,`, `;`, `:` and `.` have none. A token over several lines is sent as one per line.
  - Names are what the check resolved them to: a module (each segment of a path) `namespace`; a struct, enum or interface by its kind, `extern type` `type`, a variant `enumMember`, a type parameter `typeParameter`, a field `property`, a function `function`, a method or associated function `method`, a parameter `parameter`, any other binding or a constant `variable` (+ `mutable` for `let mut`). A name at its own declaration adds `declaration`; one declared in `std` or the prelude adds `defaultLibrary`, as do an intrinsic's call (`println(`) and `Some`, `None`, `Ok`, `Err` when nothing in the program declares them. A name the check couldn't resolve has no token, never a guess.
  - Names come from the current check, as hover's do; while the text doesn't lex (an unclosed string being typed), only the text before the first lex error has tokens, its names from the last check that parsed where the two texts still agree. A document the server doesn't hold answers `null`.
- **Explanations** (milestone 86): each diagnostic with a code carries `codeDescription.href` `lugha-explain:<CODE>.md`; the VS Code extension shows that document, from `lughac explain`, in a read-only tab, also in untrusted folders since it runs no program code (and **Lugha: Explain Error Code** opens one by hand).
- **Syntax errors** (milestone 75): while a module has one, the server publishes only the syntax errors (and the loader's other errors); what the checker finds in the recovered program is used by the features above, not shown, since a dropped statement makes later names look undefined.
- **Exit codes:** 0 after `shutdown` and `exit`; 1 after `exit` without `shutdown`; 2 when the connection breaks (a malformed message, a closed stream). Requests other than `initialize`, `shutdown`, `textDocument/hover`, `textDocument/definition`, `textDocument/completion`, `textDocument/signatureHelp` and `textDocument/codeAction` get `MethodNotFound`.

**`lughac explain`** (v16, milestone 86) prints the explanation of a code — `lughac explain E0501` (any case) — in Markdown: its title, what it means and why, an example that reports it and the example fixed. With no code it lists every code with its title, one per line (`E0501  Assignment to an immutable binding`). An unknown code is exit 2 (`error: no code E9999; run \`lughac explain\` for the list`). Every code of the table below has an explanation, and every example in them that a single file can show is checked against the compiler by the tests. After the diagnostics of any command in the human format, one line names their codes in order of first appearance: `For more about E0501, run \`lughac explain E0501\`.`, or for several `For more about E0403 and E0501, run \`lughac explain <code>\`.`; the JSON format has no such line.

**`lughac fmt`** (v16, milestone 80) formats each path given (default: the current folder); a folder means every `.la` file under it, in name order, skipping hidden folders, `target/` and symbolic links. A file is rewritten only when its text changes, through a temporary file and a rename. With `--check` nothing is written and each file that would change prints as `would reformat <path>`.
- **The style:** 4-space indents; `{` on the line of what opens it; one statement, field, variant, method or `match` arm per line, and a block's contents on their own lines (`{}` when empty), except that an `if` with `else` whose every branch is a lone value, with no comment inside, stays on one line when it fits in 100 characters, `let y = if c { 1 } else { 2 };` (v17; a longer one breaks every branch); one space around binary operators, `=` and `=>`, after `,` and `:`; a call, array, tuple, struct literal, parameter list or `import { … }` stays on one line when it fits in 100 characters, else breaks one element per line with a trailing comma, except that a last element spanning lines (a closure, a block) stays on the line of the opening `(`, and (v17, milestone 95) a call's last argument that is an array or struct literal "hugs" the call — `column([` one element per line `])` — when the call doesn't fit but its first line, up to that `[` or `{`, does; a group is measured with what follows it up to the first place that can break, so a function's header stays whole while its body breaks; a function's `= expr;` body stays an expression body; a comma after each arm but one whose body is a block — kept there too when the next arm starts with `(` or `-`, which would otherwise continue the block (`{ 1 } (a, _)` reads as a call); parentheses only where precedence needs them (and around a struct literal in a condition); at most one blank line between items or statements, kept where written, none at a block's start or end; literals and names exactly as written (`0xFF`, `1_000`, escapes, `Self`). It has no options.
- **Comments:** a comment on its own line prints above the item, statement, field, variant, method or arm after it, at its indentation; a comment after one on its line stays at the end of that line. A comment anywhere else — inside an expression, a parameter list, a header — leaves that whole statement or item as written (only re-indented).
- **Safety:** before writing, the output is parsed again; its syntax tree (positions ignored) and its comments' texts must equal the original's. Otherwise nothing is written and it is an internal error, exit 2. Formatting formatted code changes nothing.
- **Exit codes:** 0; 1 when `--check` found a file to change or a file has syntax errors (printed as diagnostics; the file is left alone and the others are still formatted); 2 when a path can't be read or written or a file fails the safety check.

**`lughac test`** (v16, milestone 84) runs tests:

```
fun area(r: f64): f64 = 3.0 * r * r;

@test
fun area_of_unit() {
    assert(area(1.0) > 2.0);
    assert_eq(area(0.0), 0.0);
}
```

- **Tests:** `@test` before a top-level `fun` makes it a test, and `@test(integration)` (v18, milestone 185) an integration test; any other argument is E0210. A test has no type parameters and returns nothing (E0434); its parameters are capabilities, matched by type as `main`'s, each at most once (E0434 otherwise; v18, milestone 180):
  - **A plain test** may take `Console`, `Dir`, `Clock`, `Env`, `Entropy` and `Net`, and is given stand-ins (milestone 185): the real `Console`, captured as below; a `Dir` of a fresh empty folder made for the test and removed after it, passed or failed, confined by the kernel like any `Dir` (milestone 186; links the test makes inside can't lead out either); a `Clock` stopped at 2000-01-01T00:00:00Z (`now_ms()` 946684800000, `monotonic_ms()` 0, `local_offset` `Ok(0)`); an empty `Env`; an `Entropy` giving splitmix64's stream from seed 0, the same in every test and run; a `Net` of real sockets on the loopback interface only (`127.0.0.1`, `::1` and `localhost`; anything else `Err("a test's network is this machine only: …")`, `interfaces()` empty). A plain test runs in its folder. Asking for `Fs` or `Proc` is E0434, with help to take a `Dir` or mark the test `@test(integration)`.
  - **An integration test** may take `Fs` and `Proc` too, and is given the real `Console`, `Fs`, `Clock`, `Env`, `Proc`, `Entropy` and `Net` (and the same fresh `Dir` if it asks); it runs in the folder `lughac test` was started in, and only under `--integration`: without it, it is reported `skipped (integration: run with --integration)`.
  - Before v18 a test took no parameters at all.

  Nothing may call a test or use it as a value (E0434). `@` starts an attribute, and `@test` is the only one before a `fun` here: any other name, or `@test` before anything but a `fun`, is E0210. Tests may be in any file, beside the code they test (reaching its `private` items) or in their own files through imports. `lughac build` and `run` leave tests out entirely.
- **Which:** each path given (default: the current folder) is a file, or a folder meaning every `.la` file under it in name order, skipping hidden folders, `target/` and symbolic links. Each file holding `@test` is checked and built with its imports into a test executable, needing no `main` (one it has is ignored); the files of one folder governed by a `lugha.toml` share one executable (v17, milestone 124), each the module named by its file's stem (`test/diff.la` → `diff`, so another test file may `import diff;`), and a file with errors is reported alone while the folder's others still build together. A file outside any package is built alone. With `--filter`, only files holding a test it selects (found by parsing) are built. A test is named `module.name`, the module being the file's path without `.la`, folders joined by `.` (`src/geo.la` → `src.geo.area_of_unit`) — or, for a file in its package's `source` folder, its module path (`glow/src/diff.la` → `glow.diff.patches`, v17), the file then being built as that module (section 6). `--filter TEXT` runs only the tests whose name contains `TEXT`.
- **Running:** each test runs as its own process of its file's executable, told which test by the environment variable `LUGHA_TEST` and its fresh folder by `LUGHA_TEST_DIR` (v18), so a panic, overflow, bounds error or stack overflow fails only that test. Up to one test per CPU runs at once. Every global constant of an executable is computed before its test runs, so a constant whose computation panics fails every test sharing that executable (v17, milestone 124). A test still running after `--timeout` seconds (default 10, wall-clock time) is killed and fails.
- **Time and order** (v18, milestone 203): a plain test runs deterministic, told so by `LUGHA_SCHED=deterministic`: its time is virtual, starting at 0 and moving only when every task waits and no socket has anything ready, when it jumps to the earliest timer (a sleep, `select`'s `after`, a scope's deadline or `remaining()`, a shield's end, a socket or UDP timeout) without waiting; a test of a retry after thirty seconds takes no time. Its `Clock` stays the fake one, which timers never read. With `--shuffle=SEED` (a `u64`; `--shuffle` alone draws one and prints `shuffle seed N` first) it also runs shuffled (`LUGHA_SCHED=shuffle:SEED`): each time a task is picked to run, it is picked at random among the ready ones, as `select`'s ready arm is, from a generator seeded with `SEED`, so a test depending on one order fails under some seeds, the same way every time; a failing one's line says `FAILED (seed N)` (`FAILED (exit code 3, seed N)`), and `--json` results carry `"seed":N`. An integration test runs on real time, in order, as `lughac run` does. `LUGHA_SCHED` set to anything else, or not at all, is that normal mode. The executables live in a fresh temporary folder, removed afterwards; they are started directly, never through a shell.
- **Output:** one line per test, in file then declaration order: `test geo.area_of_unit ... ok`, or `... FAILED` followed by the panic (`panic: assert_eq failed: left 4, right 5 at geo.la:13:5`) and, after `--- output ---`, what the test printed (captured, shown only on failure); `... FAILED (timed out after 10 s)`; `... FAILED (exit code N)` for a test that called `exit(N)`; `... skipped (reason)` for a test that called `skip(reason)` (v17). Then a blank line and `N tests: P passed, F failed` (with `, S skipped` when any were, and `, K files didn't compile` when a file had errors, v17), or just `0 tests`.
- **Skipping** (v17, milestone 97): `skip(reason: string)` ends the test at once and reports it skipped, counted apart; skipped tests don't fail the run. A test that panicked never counts as skipped, nor one that exits with the skip status without calling `skip`. Outside a test, `skip` panics (`skip outside a test`). A file with errors prints its diagnostics to stderr and its tests don't run; the others still do.
- **Exit codes:** 0 when every test passed (also with none); 1 when a test failed or a file has errors; 2 for lughac's own errors (a test executable that can't be started, a temporary folder that can't be made).
- **For tools** (milestone 85): `--list` names every test the paths hold, by parsing alone (nothing is checked or built, so a file with a type error still lists its tests), as `test <name> <file>:<line>:<col>` lines — the test's name — or with `--json` as `{"test":"geo.area_of_unit","file":"geo.la","line":5,"col":5}`. A run with `--json` prints, as each test finishes, `{"test":…,"file":…,"line":…,"status":"passed"|"failed"|"timed_out"|"skipped" (v17),"message":…,"at":{"file":…,"line":…,"col":…},"output":…,"ms":…}` — `message` and `at` from the panic, absent when it passed (`at` also when the panic has no location) — (for a skip, `message` is the reason) and finally `{"summary":{"passed":P,"failed":F,"skipped":S,"uncompiled":K}}` (`skipped` and `uncompiled`, the files with errors, from v17); diagnostics stay on stderr, and the exit codes are the same. `--filter` may be given several times (a test runs when any matches), and `--exact` makes each one match a whole name. Lines and columns are 1-based, columns in bytes.
- **In VS Code** (milestone 85): the extension lists every test of the workspace folders in the Testing view with a ▶ beside each `@test`, refreshed when a `.la` file changes, and runs them with `lughac test --json --exact`; results show at the failing line. It does so only when asked (or under the Testing view's continuous run), never in a folder VS Code doesn't trust, and starts only `lugha.path`, without a shell.

**Diagnostics.** Errors print the file, line and column, the offending source line, a caret under the span with its label, and any secondary labels, in a rustc-like layout rendered by the `codespan-reporting` crate. Each diagnostic is followed by a blank line, and lines carry no trailing whitespace. The checker reports as many errors as it can find instead of stopping at the first one.

Internal errors — an unreadable file, a failed link — have no code and exit with 2. They print as `error: <message>`, with a source snippet when they have a location. A crash inside the compiler itself is reported the same way, as `error: internal compiler error: <message>` with a note asking for a bug report, and also exits with 2.

**Error codes.** Every diagnostic has a stable code, grouped by stage. A code is never reused or renumbered once released; its message wording may improve.

| Range | Stage |
| --- | --- |
| E01xx | Lexing: bad characters, unterminated strings, invalid escapes, out-of-range literals |
| E02xx | Parsing: unexpected tokens, missing delimiters |
| E03xx | Names and scopes: undefined or duplicate names, recursive structs |
| E04xx | Types: mismatches, invalid operators, bad casts, wrong argument counts |
| E05xx | Mutability and control flow: assignment to immutable bindings, missing returns, `break` outside a loop |
| W01xx | Warnings: unreachable code |

Name and type errors, reported by the checker:

| Code | Error | Example |
| --- | --- | --- |
| E0301 | Undefined name | `count + 1` with no `count` in scope; a call to an undefined function |
| E0302 | Name defined more than once, or a function named like an intrinsic | two `fun area`; `fun println()`; `import vec { add };` in a file declaring `add` |
| E0303 | No `main` function | (not under `lughac test` nor `lughac check`) |
| E0304 | `main` with the wrong signature, generic, or outside the main file | `fun main(argc: i32): i32`; `fun main(a: string[], b: string[])`; `fun main<T>()`; a `main` in a module |
| E0305 | Unknown type name | `let p: Point = 1;` with no `Point` |
| E0306 | Reserved extern name, or one C function declared with two signatures | `extern fun lugha_rt_alloc(size: i64): i64;` (§8); `extern "not a name" fun f();` |
| E0307 | Struct contains itself | `struct A { b: B }` with `struct B { a: A }` — hold it in an array instead (`A[]`) |
| E0308 | Field declared twice | `struct P { x: i64, x: f64 }` |
| E0309 | Variant declared twice | `enum E { A, B, A }` |
| E0310 | Method declared twice, or named like a member it can't be told from | `impl Q { fun area(self) {} }` where `Q`'s field `area` holds a function |
| E0311 | A local named like a type or an imported module | `let Shape = 3;`, `fun f(Point: i64)` |
| E0312 | Module not found | `import shapes.cirlce;` with no `shapes/cirlce.la`; `import std.maths;`; `import geo;` when package `geo` has no `lib.la`, and `import geo.lib;` when it has |
| E0313 | An item, method or field not visible here | another module's `private` item: `range.helper()`, `r.lo`, `range.Range { lo: 1, hi: 5 }`; another package's item without `public`; `shapes.Vec2` when `shapes` imported `Vec2` without `public`; `shapes.vec.helper()` through a re-exported module when `helper` isn't `public` |
| E0314 | Import clash | `import a.util;` and `import b.util;` in one file; an import named like an item; `import vec { add, add };`; `import a as u; import b as u;` |
| E0315 | Unknown bound | `fun f<T: Hash>(x: T)`; a struct as a bound |
| E0316 | Bad type parameter name | `fun f<T, T>()`, a parameter named like a struct |
| E0317 | Retired in v8 (was: `public` before packages existed); never reported, never reused | |
| E0318 | A declaration exposing a less visible type | `private struct S {}` then `fun make(): S`; `public fun a(): Inner` where `Inner` isn't `public`; `public import util { helper };` where `helper` isn't `public` |
| E0319 | The main file imported | a module writing `import main;` |
| E0320 | A package name that is also a local folder or file | `geo` in `lugha.toml` while `geo/` sits next to `main.la`; `import std.math;` beside a `std.la` |
| E0321 | A malformed `lugha.toml` or `lugha.lock` | `geo = libs/geo` (no quotes); `[deps]`; `geo` listed twice; `std = "lib"` or `name = "std"`; `{ git = "ssh://h/g", version = "1" }`; `{ git = "https://me:pw@h/g", version = "1" }`; `git` without `version`; a lock commit that isn't 40 hex digits; under `[link]`, `flags = ["-Wl,-x"]`, `libs = ["-lx"]`, `paths = ["/usr/lib"]` |
| E0322 | A manifest path that doesn't exist | `geo = "libs/goe"` with no such folder; a `[link]` `paths`/`include` folder or `c` file that isn't there |
| E0323 | One package name for two folders | two manifests list `mathx` with different folders |
| E0325 | A git package that can't be fetched | no `git`; a URL that fails; no tag meeting `version`; `--offline` with the package not locked and cached |
| E0326 | A git package that doesn't match `lugha.lock` | the locked tag now gives another commit |
| E0327 | A package listed under another name than it declares | `gw = { path = "../glowworm" }` when glowworm's `[package]` says `name = "glowworm"` |
| E0328 | One file imported as two modules | `src/main.la` importing `cli` and `tool.cli` when `tool`'s `source` is `src` |
| E0329 | `pkg-config` failed for a `[link]` table | `pkg-config = ["gtk4"]` without `pkg-config` installed, or without gtk4; `pkg-config` giving `-Wl,-rpath,/x` |
| E0330 | A `[link]` C file that doesn't compile | `c = ["shim.c"]` with a syntax error in `shim.c` |
| E0331 | Linking failed with `[link]` input | `libs = ["raylib"]` with no raylib installed |
| E0332 | A program needs OpenSSL 3.2 or newer, which isn't installed | building a program that loads `std.tls`, `std.http` or `std.crypto` on a machine without `libssl-dev`, or with an OpenSSL older than 3.2 |
| E0333 | A package declares C code without the `ffi` permission | a package's `extern fun` or `extern type`, its `[link]` table, or its `p.view` of C's memory, when the program's `lugha.toml` grants neither it nor a package that depends on it `allow = ["ffi"]` |
| E0324 | A package version that doesn't meet a requirement, or a requirement on a package without a version | `geo = { path = "libs/geo", version = "1.2" }` when geo's `lugha.toml` says `version = "2.0.0"`, or nothing |
| E0401 | Numeric literal of the wrong kind for its expected type | `x + 2.5` where `x` is `i32` |
| E0402 | Integer literal out of range for its type | `let b: u8 = 256;`; `x + 300` in a `fun big<T: Num>` called at `u8` |
| E0403 | Type mismatch | `let n: i32 = ok;` where `ok` is `bool`; `if`/`else` of different types |
| E0404 | Operator applied to invalid operand types | `r == s` on boxes, with help; `a < b` on a struct without a `cmp` of the right shape; `true + 1`, `-u` where `u` is `u8`, `u16`, `u32` or `u64`; `a == b` on a `T` without `Eq` (with help to declare it); `s + 1` where `s`'s type has no `add` of the operator shape, `a + a` on a `T` without `Add` or `Num` |
| E0405 | Wrong number of arguments | `add(1)` for `fun add(a: i64, b: i64)` |
| E0406 | Not a function, or something that can't be a value | `let f = bump;` for a function with a `mut` parameter (v18); `let step = 2; step(1);`; `let a = Circle.area;` (a method), `let p = println;`, an extern |
| E0407 | Retired in v17 (was: `void` used as a value, before `()` was one); never reported, never reused | |
| E0410 | No such field | `s.size` on a `string`; `(5).len`; `p.z` or `Point { z: 1.0 }` on a `Point` without `z` |
| E0411 | Not indexable or not iterable | `n[0]` where `n` is an `i64`; `p[0]` or `p[0] = 1` where `p`'s type has no `index` or `set_index` of the indexing shape; `for c of 5`; a `for … of` over a type whose `next` isn't `fun next(mut self): Option<T>` |
| E0412 | Cannot infer an array's element type, `None`'s type, or a type argument | `let xs = [];`, `let x = None;`, `let s = Stack.new();`, `let m = max;` — annotate it, `let xs: i64[] = [];`, or write the type arguments, `Stack<i64>.new()` (v18) |
| E0413 | Struct literal missing fields | `Point { x: 1.0 }` |
| E0414 | Struct literal field given twice | `Point { x: 1.0, x: 2.0, y: 0.0 }` |
| E0415 | No such variant or method | `Shape.Circel(1.0)`, a `.Square` pattern on a `Shape`, `p.size()`, `xs.push(1)` |
| E0416 | Wrong number of payloads | `Shape.Rect(1.0)`, `Rect(w) => …` |
| E0417 | Non-exhaustive `match` | `match s { Circle(r) => r }` with more variants; an integer `match` without `_` |
| E0418 | Wrong number of type arguments | `Stack<i64, i64>`, a bare `Stack` as a parameter type; `pair<u8>(1, "x")` for `fun pair<A, B>`; `plain<i64>(1)` (v18) |
| E0419 | Type argument doesn't satisfy its bound | a `Ref`, or a type holding one, as a `T: Eq`; `sum(["a"])` with `T: Num`, `math.sqrt(2)` (`i64` isn't `Float`), or a method of an `impl Grid<T: Num>` on a `Grid<string>` |
| E0420 | Generic instantiation too deep | `fun f<T>(x: T) { f([x]); }`; `struct S<T> { xs: S<T[]>[] }` |
| E0421 | A method no bound declares | `s.perimeter()` in `fun f<T: Shape>(s: T)` |
| E0422 | A type that doesn't implement an interface, or an `impl` that doesn't match it | `total_area([Square { side: 1.0 }])` without `impl Shape for Square`; `let s: Shape = 1;`; a `Named` value where a `Pet` is expected, without `sound`; an `impl Shape for Square` missing `area` |
| E0423 | Two bounds declare one item differently | `T: Sized + Measured` where both declare `size` with different types |
| E0424 | `?` where it can't return | `?` on an `i64`; in `main`; on an `Option` in a function returning `Result`; error types that neither match nor convert |
| E0425 | An interface with an associated function used as a type | `fun f(x: Factory)` where `Factory` declares `fun make(): Factory;` |
| E0426 | A type pattern that can't apply | `c: Circle` on an `i64`; `s: Shape` (an interface); `d: Dot` when `Dot` doesn't implement the interface |
| E0427 | An interface that uses `Self` used as a type | `let xs: Ordered[] = …;` where `Ordered` declares `fun less(self, other: Self): bool;` |
| E0429 | A `let … else` whose `else` block can finish | `let Some(n) = o else { println("no"); };` |
| E0434 | A misused test | `@test fun t(n: i64) {}`, `@test fun t(): i64 = 1;`, a generic test, `@test fun t(fs: Fs) {}` (`Fs` and `Proc` only with `@test(integration)`); calling a test or using it as a value |
| E0435 | A malformed callback declaration | `extern fun f(cb: direct fun(i64), u: data);` (nothing takes `data`); `extern fun f(cb: spawned fun(data));` (no `data` parameter); `release` without `data`; `fun f(x: data)` outside an extern |
| E0436 | A function with captures passed to a callback without `data` | `sort(xs, fun(a, b) = a * k < b)` where `sort`'s comparator is `fun(i64, i64): i32` |
| E0439 | An actor used against its rules | `c.count` through a handle; a message's `mut` parameter (v18); `Counter { count: 0 }` outside a spawn; `fun f<T>(self)` in an actor; `fun stop(self)` in an actor (a `Ref` crossing its boundary: E0475, v18); `actor A mailbox 0` (milestone 201) |
| E0438 | A `spawn` that changes a copy, or a malformed `select` | `spawn xs.push(1)`, `spawn bump(mut c)` (v18), `spawn p.bump()` with `mut self`; `select { default => 1 }`; two `default`s or `after`s; `after` with `default`; `m = s.recv()` with `s` not a `Chan` |
| E0440 | `..base` of another type | `P { x: 1.0, ..q }` where `q` is a `Q` |
| E0441 | Retired in v18 (a float as a map key or set value; now E0451) | never reported |
| E0442 | `for … of mut` over a map or set | `for (k, v) of mut m { … }` |
| E0443 | `out` or `mut` missing or misplaced in a call | `bump(c)` for `mut c: Counter` (v18); `get_fuel(ctx, fuel)` for `out fuel: u64`; `put(out n)` where `n` isn't `out` |
| E0444 | A union literal writing other than one field | `U { a: 1, b: 2.0 }`; `U {}` |
| E0445 | Not a constant | a call, closure, `if`, block, interpolation or collection literal in a constant's value; a constant of type `Ref<T>`, a function, a task, a channel, an actor, a handle, a map or a set |
| E0446 | A constant that depends on itself | `const A: i64 = B; const B: i64 = A;` |
| E0447 | A file that can't be included | `include_bytes("../../secret")` leaving the package; a missing file; one over 16 MiB; `include_str(PATH)` |
| E0448 | `include_str` of a file that isn't UTF-8 | `include_str("logo.png")` |
| E0450 | A type that can't be turned into JSON | `json.encode(fun() {})`; `struct Job { run: fun() } with json.Json`; `Map<bool, i64>`; a type parameter without `T: json.Json` |
| E0449 | Retired in v17 (was: an extern struct by value on ARM64, before lughac followed ARM's rules for one); never reported, never reused | |
| E0451 | A type that doesn't meet a derivable interface | `show(job)` where `Show` has no `derive` block for `job`'s field `run: fun(i64): i64`; a type parameter without the bound |
| E0452 | A derive block in the wrong module, or overlapping another | `derive Show for i64 { … }` where `Show` is imported; `derive Show for Option<T>` beside `derive Show for Option<i64>` |
| E0453 | A field intrinsic used wrongly | `derive.fields(…)` outside a derive block or in a `for enum` block; an `f` that isn't generic over one `F: I`; a local named `derive` in a block |
| E0454 | Retired in v18 (a type's own `eq` without `hash`; now `impl Eq` and `impl Key`) | never reported |
| E0455 | A `return`, `?`, `break` or `continue` leaving a deferred statement | `defer { return; }`; `defer { let n = load()?; }`; `while c { defer { break; } }` |
| E0456 | A deferred statement with a value | `defer dir.remove_file(p);` (a `Result`); `defer n + 1;` |
| E0457 | An operand that fits more than one arithmetic method | `v * 2` where `V` has `Mul<i64>` and `Mul<u8>`; `2 * v` with `impl Mul<V> for i64` and `impl Mul<V> for u8` |
| E0458 | Retired in v18 (arithmetic in `impl f64 { … }`; now `impl Mul<V> for f64`, E0459 and E0466) | never reported |
| E0459 | An `impl I for T` written where it may not be (v18) | `impl shapes.Shape for geo.Square` in a third module; `impl Show for bool` outside `Show`'s module; `impl Show for fun(i64): i64`; `impl<T> Show for T`; `impl Eq for f64[]`; `impl Display for P` |
| E0460 | A method name two implemented interfaces share (v18) | `c.describe()` where `Circle` implements `Named` and `Shown`, which both declare `describe` |
| E0461 | An interface implemented twice for one type (v18) | two `impl Shape for Square` blocks; `with Show, Show`; `impl<T: Show> Show for T[]` and `impl Show for Point[]` |
| E0462 | An interface that can't be derived for this type (v18) | `enum E { … } with Show` where `Show` has blocks for structs only; `with Display` |
| E0463 | An interface both derived and implemented (v18) | `struct P { … } with json.Json` beside `impl json.Json for P { … }` |
| E0464 | Retired in v18 (milestone 179; was: an interface that extends another, used as a type); never reported, never reused | |
| E0465 | `debug` in an optimized build (v18) | `debug(x)` in the program's own code built with `-O2` (a dependency's prints nothing instead) |
| E0466 | Methods for a type this module doesn't declare (v18) | `impl f64 { … }`; `impl shapes.W { … }` |
| E0467 | A discarded `Result` (v18) | `save("");` as a statement, where `save` returns a `Result` |
| E0468 | A variant name used as a binding in a pattern (v18) | `None =>` on an `Option`; `.Some(Empty)` on an `Option<Shape>` |
| E0469 | A cast that can lose information (v18) | `big as u8` for `big: i64`; `2.5 as i64`; `n as f64` for `n: i64`; `n as T` for `T: Num` |
| E0470 | Braces that can't build their type (v18) | `Point { 1, 2 }`; `Set { "a": 1 }`; `Map { 1, 2 }`; `Bag { "a" }` in `Bag`'s own module |
| E0471 | Touching the world without a capability (v18) | `println(x)` (take `console: Console`); `read_file(p)` (take `fs: Fs`); `args()`; `exit(1)` |
| E0472 | A scope or task handle kept (v18) | `fun f(s: Scope): Scope = s;`; `[s.spawn(f())]`; `struct K { t: Task<i64> }`; a closure capturing `s` |
| E0473 | `spawn` through something that isn't a scope (v18) | `x.spawn(f())` with `x: i64` |
| E0474 | A task or an actor started without a scope (v18) | `spawn work()`; `spawn Counter { count: 0 }` (milestone 201) |
| E0475 | A value that can't cross to another task (v18) | `s.spawn(f(r))` with `r: Ref<i64>`; `Chan<Ref<i64>>.new(1)`; a closure capturing a `Ref`, spawned; `impl Console for S` with `S` holding a `Ref`; an unbounded `T` sent |
| E0476 | Claiming `Sendable` (v18) | `impl Sendable for P {}`; `struct P { … } with Sendable`; `auto interface` outside the prelude |
| E0477 | A scope setting that doesn't exist, said twice, or two deadlines (v18) | `scope s quickly { … }`; `scope s limit 1 limit 2 { … }`; `scope s within 10 until 20 { … }` (milestone 199) |
| E0478 | A task's error that its scope can't hold (v18) | `s.spawn(f())` with `f` failing with `string` in a `scope<SyncError> s` |
| E0479 | A `blocking` extern takes a callback (v18) | `extern blocking fun each(f: direct fun(i64));` |
| E0480 | A `blocking` extern takes or gives a pinned handle (v18) | `extern blocking fun load(w: Window): i64;` with `extern type Window pinned;` |
| E0482 | A `realtime` callback allocates or waits (v18) | `on_note(fun(n: i64) { let s = "note {n}"; })` for `realtime fun(i64, data)`; a `realtime` callback taking a `string` |
| E0483 | A view outlives its call (v18) | `fun back(v: View<u8>): View<u8> = v;`; `Some(v)`, `[v]`, a field `View<u8>`, a closure capturing `v`, `id(v)` for a generic `id` |
| E0484 | A callback takes C's buffer the wrong way (v18) | `direct fun(f32[], data)` (take `View<f32>`); `Ref<f32[]>` anywhere (take `MutView<f32>`); `spawned fun(View<u8>, data)` (take `u8[]`) |
| E0481 | No longer reported (was: a `spawned` callback taking `Ref<T[]>`; callbacks take views or copies now, E0484); never reused | |
| E0437 | A malformed `extern struct` | `extern struct S { s: string }`; a field of an array, a plain struct, an enum, `Option` (other than of a handle) or a function type; a handle, `Option` of one or `CPtr` in an `extern union`, at any depth; `extern struct S<T> { … }`; `extern struct S {}` |
| E0433 | A pattern that must match every value but might not | `let (1, x) = t;`, `let Some(n) = o;` without `else`, `for (Some(a), b) of …` |
| E0428 | Or-pattern alternatives that bind differently | `.Some(x) \| .None`; `.Count(n) \| .Circle(n)` with `n` an `i64` and an `f64` |
| E0430 | Retired in v18 (a default method two visible interfaces supply; now E0460) | never reported |
| E0431 | A bad format specifier | `"{n:.2}"` with `n: i64`; `"{x:q}"`; `"{f:x}"` with `f: f64`; `"{n:#}"`; `"{s:05}"` with `s: string`; `"{n:2000}"` |
| E0432 | A bad `format` template | `format(t, 1)` with `t` a variable; `format("{} and {}", 1)`; `format("{x}", 1)`; `format("a {")` |
| E0409 | Type not allowed in an extern signature | `extern fun total(xs: i64[]): i64;` — no array, plain struct (an `extern struct` is fine), enum, `Option` (but of a handle or `string`) or interface parameters or returns, and functions only as callback parameters (§8); a callback returning a string, `extern fun f(cb: direct fun(i64): string);` |
| E0408 | Invalid cast or conversion | `ready as i32` where `ready` is `bool`; `x as bool`; `n as T` for a `T` without `Num` or `Float`; `2.5.truncate<u8>()`, `n.floor<i64>()` on an integer (v18) |
| E0501 | Assignment to an immutable binding | `let total = 0; total = 5;`; assigning to a parameter or loop variable; `xs[0] = 1;` where `xs` isn't `let mut`; `for x of mut xs` where `xs` isn't |
| E0502 | Assignment to something that isn't a place | `(a + b) = 3;`; `for x of mut [1, 2]` |
| E0503 | Missing return | a function not returning `()` whose body can end without a value (§6); a stray `;` after the result gets a "remove this semicolon" label |
| E0504 | `break` or `continue` outside a loop | |
| E0505 | Block-like statement with a discarded value | `if big { 100 } else { 1 }` followed by more statements |
| E0506 | Assignment into a string | `s[0] = 104;`, `s.len = 2;`, `for b of mut s` — strings are immutable (§4) |
| E0507 | Changing the array a `for … of mut` loop writes | `for x of mut xs { xs.push(x); }`, an assignment, a `mut self` call or `swap` on it; `it.next()` inside `for n of mut it` |
| E0508 | An argument aliases a `mut self` receiver or a `mut` argument | `v.add(v)` where `add` takes `mut self`; `v.add(v.inner)`, `g.set(if c { g.vals } else { h })`; `both(mut a, mut a)`, `f(mut a, a)`, `p.eat(mut p.x)` (v18) |
| E0509 | A place swapped with a part of itself | `swap(n, n.kids[0])` |
| E0510 | A statement changes what contains the place it writes | `xs[0] = { xs.push(2); 9 };`; for a `mut self` receiver only across an index, `gs[0].f(gs.clear())` (§5) |
| E0511 | Changing a variable a closure captured | `let inc = fun() { count += 1; };` |
| E0512 | Unknown label | `loop { continue nowhere; }`; `a: loop { let f = fun() { loop { break a; } }; }` |
| E0513 | A label already used by an enclosing loop | `a: for i in 0..3 { a: for j in 0..3 { } }` |
| E0514 | `break` with a value out of a `while` or `for` loop | `while c { break 1; }`; `a: for x of xs { loop { break a x; } }` |
| W0101 | Unreachable code (warning) | statements after `return`, `break` or `continue`; reported once per block |
| W0102 | Unreachable `match` arm (warning) | an arm after `_`, or a variant already covered |
| W0103 | Unused `match` binding (warning) | `Some(Emtpy) => 0`, a misspelled variant that became a binding |
| W0104 | `..base` that copies nothing (warning) | `P { x: 1.0, y: 2.0, ..p }` |
| W0105 | A key written twice in a map or set literal (warning) | `Map { "a": 1, "a": 2 }`; `Set { 1, 1 }` |
| W0106 | A `let` holding `()` (warning) | `let x = log();` where `log` has no return type; not `let x: () = log();`, `let x = ();` or `let _ = log();` |
| W0107 | A leading zero that looks octal (warning) | `0600`, `007` (decimal; write `0o600` for octal) |

**JSON diagnostics.** With `--diagnostics=json`, the compiler writes one JSON object per line to stderr, one per diagnostic, and nothing else. Lines and columns are 1-based; columns and offsets count UTF-8 bytes. `label` is the text shown under the primary span, or `null`. `labels` holds secondary spans, and `help` is an optional suggestion. Internal errors have `"code":null`, and `"span":null` when they have no location. `"file"` is the file of the primary span: the main file as given on the command line, or a module's path relative to the main file's directory (`shapes/circle.la`); lines, columns and offsets are within that file. A label in a different file carries its own `"file"` key, which appears only then.

```json
{"severity":"error","code":"E0401","message":"float literal where i32 expected","file":"main.la","span":{"start":{"line":3,"col":17,"offset":49},"end":{"line":3,"col":20,"offset":52}},"label":"expected i32","labels":[{"span":{"start":{"line":3,"col":13,"offset":45},"end":{"line":3,"col":14,"offset":46}},"message":"this operand is i32"}],"help":null}
```

Both formats come from the same internal diagnostic records, so they always report the same errors.

**The Lugha skill** (v18, milestone 230; `lughac spec` before). No model has seen Lugha in training, so the compiler ships readable pages on writing Lugha, built from `skill/` into `lughac`: an Agent Skill folder, `SKILL.md` (frontmatter `name: lugha` and a `description`, then how to work, the rules that surprise newcomers, and a table linking every page) and `concepts/NN-name.md` pages in teaching order, then `library/name.md` reference pages (milestone 233: frontmatter `title`, `summary` and optionally `modules`, the std modules it documents; sections Overview, Reference and Examples; a page over 300 lines starts with a contents list, and the tests check that each names every public item of its `modules` and every prelude method of the blocks it covers), each with frontmatter `title`, `prerequisites` (earlier concepts) and `summary`, and the sections The idea, Example, Rules, Mistakes and Exercises. The pages stand alone: they never refer to this file. `lughac skill` prints `SKILL.md`; `lughac skill PAGE` one page, named by its path without `.md` (`concepts/02-functions`) or its last part without the number (`functions`), in any case (an unknown name is exit 2, suggesting close names; a short name several pages share, such as `collections`, is exit 2, listing their full names); `--list` each page's name and summary in order; `--search TEXT` every line containing `TEXT` in any case, as `page:line: text` (exit 1 when none does, as `grep`); `--export DIR` writes the pages as `DIR/lugha/`, ready to upload as a skill (exit 2 if `DIR/lugha` exists). The options exclude each other and a page (exit 2). Every `lugha` example block in the pages is checked by the tests: one followed by an `output` block runs and must print exactly that, and one marked `lugha error E0xxx` must fail with that code; a block marked `lugha file NAME` (milestone 232; any `LANG file NAME` since milestone 235, such as `c file glue.c` or `toml file lugha.toml`) is written as `NAME` beside the next example's `main.la`, so an example may span modules and link C; `lugha test` (milestone 235) must pass `lughac test`; `lugha part` (milestone 235) is a step's piece of its page's whole program and must appear in it word for word; `output`, `sh`, `text`, `toml` and `c` blocks are shown, not checked. Then come `tasks/name.md` how-tos (frontmatter `title`, `summary` and `uses`, the pages each builds on, which must exist; sections Goal, Steps, The whole program and Going further) and `examples/name.la` programs, each starting with `//` lines saying what it does, its first one the summary `--list` shows, and ending with an `// Output:` comment block holding exactly what it prints when run with no input, which the tests check. An example is named like a page, without `.la`.

## 10. Example programs

These programs are the v0 acceptance tests: a conforming compiler builds and runs each one, and its stdout and exit code must match exactly. Every expected output ends with a newline.

**Hello world** (milestone 4). Prints `Hello, world!`, exits with 0.

```
fun main(console: Console) {
    console.println("Hello, world!");
}
```

**Recursion** (milestone 4). Prints `832040`, exits with 0.

```
fun fib(n: i64): i64 = if n < 2 { n } else { fib(n - 1) + fib(n - 2) };

fun main(console: Console): i32 {
    console.println(fib(30));
    0
}
```

**Arrays, repeat literals and mutability** (milestone 5). Prints `25`, exits with 0. `is_composite` needs `mut` because its elements are assigned, just as `count` and `j` need it to be reassigned.

```
fun count_primes(limit: i64): i64 {
    let mut is_composite = [false; limit + 1];
    let mut count = 0;
    for i in 2..limit + 1 {
        if !is_composite[i] {
            count += 1;
            let mut j = i * i;
            while j <= limit {
                is_composite[j] = true;
                j += i;
            }
        }
    }
    count
}

fun main(console: Console) {
    console.println(count_primes(100));
}
```

**Structs, for-of and casts** (milestone 5). Prints `centroid: 2.0, 1.0`, exits with 0.

```
struct Point { x: f64, y: f64 }

fun centroid(pts: Point[]): Point {
    let mut sx = 0.0;
    let mut sy = 0.0;
    for p of pts {
        sx += p.x;
        sy += p.y;
    }
    let n = pts.len.round<f64>();
    Point { x: sx / n, y: sy / n }
}

fun main(console: Console) {
    let pts = [
        Point { x: 0.0, y: 0.0 },
        Point { x: 4.0, y: 0.0 },
        Point { x: 2.0, y: 3.0 },
    ];
    let c = centroid(pts);
    console.println("centroid: " + to_string(c.x) + ", " + to_string(c.y));
}
```

**Calling C** (milestone 4). Prints `hello from libc` then `1.4142135623730951` on two lines, exits with 0.

```
extern fun puts(s: string): i32;
extern fun sqrt(x: f64): f64;

fun main(console: Console) {
    puts("hello from libc");
    console.println(sqrt(2.0));
}
```

**Enums and match** (milestone 6). Prints `12.56636` then `6.0` then `0.0` on three lines, exits with 0.

```
enum Shape {
    Circle(f64),
    Rect(f64, f64),
    Empty,
}

fun area(s: Shape): f64 = match s {
    .Circle(r) => 3.14159 * r * r,
    .Rect(w, h) => w * h,
    .Empty => 0.0,
};

fun main(console: Console) {
    let shapes = [Shape.Circle(2.0), Shape.Rect(2.0, 3.0), Shape.Empty];
    for s of shapes {
        console.println(area(s));
    }
}
```

**A linked list** (milestone 6). Prints `6`, exits with 0.

```
enum List {
    Nil,
    Cons(i64, List),
}

fun sum(l: List): i64 = match l {
    .Nil => 0,
    .Cons(x, rest) => x + sum(rest),
};

fun main(console: Console) {
    let l = List.Cons(1, List.Cons(2, List.Cons(3, List.Nil)));
    console.println(sum(l));
}
```

**Methods** (milestone 7). Prints `5.0` then `6.0` on two lines, exits with 0.

```
struct Vec2 { x: f64, y: f64 }

impl Vec2 {
    fun new(x: f64, y: f64): Vec2 = Vec2 { x: x, y: y };
    fun len_sq(self): f64 = self.x * self.x + self.y * self.y;
    fun scale(mut self, k: f64) {
        self.x *= k;
        self.y *= k;
    }
}

fun main(console: Console) {
    let mut v = Vec2.new(1.0, 2.0);
    console.println(v.len_sq());
    v.scale(2.0);
    console.println(v.x + v.y);
}
```

**A stack** (milestone 8). Prints `3` then `2` then `empty` on three lines, exits with 0.

```
fun main(console: Console) {
    let mut stack: i64[] = [];
    stack.push(1);
    stack.push(2);
    stack.push(3);
    match stack.pop() {
        .Some(x) => console.println(x),
        .None => console.println("empty"),
    }
    console.println(stack.len);
    stack.clear();
    match stack.pop() {
        .Some(x) => console.println(x),
        .None => console.println("empty"),
    }
}
```

**A rejected enum program** (milestone 6). The checker reports the missing variant, and `lughac` exits with 1:

```
enum Shape { Circle(f64), Empty }

fun area(s: Shape): f64 = match s {
    .Circle(r) => r * r,
};

fun main() {}
```

```
error[E0417]: non-exhaustive match: `.Empty` not covered
  --> main.la:3:27
  |
3 | fun area(s: Shape): f64 = match s {
  |                           ^^^^^^^
  |
  = help: add the missing arms, or `_ => …`

For more about E0417, run `lughac explain E0417`.
```

**A rejected program** (milestone 3). The checker must report this error with its location, and `lughac` exits with 1:

```
fun main() {
    let x: i32 = 5;
    let y = x + 2.5;
}
```

```
error[E0401]: float literal where i32 expected
  --> main.la:3:17
  |
3 |     let y = x + 2.5;
  |             -   ^^^ expected i32
  |             |
  |             this operand is i32

For more about E0401, run `lughac explain E0401`.
```

With `--diagnostics=json`, the same error is the JSON line shown in section 9.

**Patterns and equality** (milestone 9). Prints `big circle` then `square` then `nothing` then `true` then `true` then `true` on six lines, exits with 0.

```
enum Shape { Circle(f64), Rect(f64, f64), Empty } with Eq

fun describe(s: Option<Shape>): string = match s {
    .Some(.Circle(r)) if r > 1.0 => "big circle",
    .Some(.Circle(_)) => "small circle",
    .Some(.Rect(w, h)) if w == h => "square",
    .Some(.Rect(_, _)) => "rect",
    .Some(.Empty) => "empty",
    .None => "nothing",
};

fun main(console: Console) {
    console.println(describe(Some(Shape.Circle(2.0))));
    console.println(describe(Some(Shape.Rect(3.0, 3.0))));
    console.println(describe(None));
    console.println([1, 2] == [1, 2]);
    console.println(Shape.Rect(1.0, 2.0) == Shape.Rect(1.0, 2.0));
    console.println("apple" < "banana");
}
```

**Two modules** (milestone 10). Prints `3.5`, exits with 0. The two files sit in one directory, and `lughac run main.la` builds both.

```
// geometry.la
struct Vec2 { x: f64, y: f64 }

impl Add for Vec2 {
    fun add(self, o: Vec2): Vec2 = Vec2 { x: self.x + o.x, y: self.y + o.y };
}

fun origin(): Vec2 = Vec2 { x: 0.0, y: 0.0 };
```

```
// main.la
import geometry;

fun main(console: Console) {
    let mut p = geometry.origin();
    p = p.add(geometry.Vec2 { x: 1.5, y: 2.0 });
    console.println(p.x + p.y);
}
```

**Generics** (milestone 11). Prints `b` then `true` then `plum` on three lines, exits with 0.

```
struct Stack<T> { items: T[] }

impl<T> Stack<T> {
    fun new(): Stack<T> = Stack { items: [] };
    fun push(mut self, x: T) { self.items.push(x); }
    fun pop(mut self): Option<T> = self.items.pop();
}

fun contains<T: Eq>(xs: T[], x: T): bool {
    for y of xs {
        if y == x { return true; }
    }
    false
}

fun largest<T: Ord>(xs: T[]): Option<T> {
    if xs.len == 0 { return None; }
    let mut best = xs[0];
    for x of xs {
        if x > best { best = x; }
    }
    Some(best)
}

fun main(console: Console) {
    let mut s: Stack<string> = Stack.new();
    s.push("a");
    s.push("b");
    match s.pop() {
        .Some(x) => console.println(x),
        .None => console.println("empty"),
    }
    console.println(contains([1, 2, 3], 2));
    match largest(["pear", "apple", "plum"]) {
        .Some(w) => console.println(w),
        .None => console.println("none"),
    }
}
```

**Interfaces** (milestone 12). Prints `15.0` then `9.0` then `square` on three lines, exits with 0.

```
interface Shape {
    fun area(self): f64;
    fun name(self): string;
}

struct Circle { r: f64 }

impl Shape for Circle {
    fun area(self): f64 = 3.0 * self.r * self.r;
    fun name(self): string = "circle";
}

struct Square { side: f64 }

impl Shape for Square {
    fun area(self): f64 = self.side * self.side;
    fun name(self): string = "square";
}

fun total_area<T: Shape>(shapes: T[]): f64 {
    let mut sum = 0.0;
    for s of shapes {
        sum += s.area();
    }
    sum
}

fun describe<T: Shape>(s: T): string = s.name();

fun main(console: Console) {
    console.println(total_area([Circle { r: 1.0 }, Circle { r: 2.0 }]));
    console.println(total_area([Square { side: 3.0 }]));
    console.println(describe(Square { side: 1.0 }));
}
```

**Arguments and interpolation** (milestone 13). Run as `prog 21`. Prints `21 doubled is 42`, exits with 0.

```
fun main(args: string[], console: Console) {
    let a = args;
    if a.len < 2 {
        console.println("usage: prog <n>");
        return;
    }
    match parse_i64(a[1]) {
        .Some(n) => console.println("{n} doubled is {n * 2}"),
        .None => console.println("usage: prog <n>"),
    }
}
```

**Functions as values** (milestone 14). Prints `6` then `9` on two lines, exits with 0.

```
fun double(x: i64): i64 = x * 2;
fun triple(x: i64): i64 = x * 3;

fun apply(f: fun(i64): i64, x: i64): i64 = f(x);

fun main(console: Console) {
    let fs = [double, triple];
    console.println(apply(fs[0], 3));
    console.println(apply(fs[1], 3));
}
```

**Closures** (milestone 15). Prints `13` then `1` then `20` on three lines, exits with 0.

```
fun adder(k: i64): fun(i64): i64 = fun(x) = x + k;

fun main(console: Console) {
    let add10 = adder(10);
    console.println(add10(3));
    let mut count = 0;
    let show = fun(): i64 = count + 1;
    count = 5;
    console.println(show());
    let twice = fun(f: fun(i64): i64, x: i64): i64 = f(f(x));
    console.println(twice(fun(y) = y + 5, 10));
}
```

**Array helpers** (milestone 16). Prints `ann, cy` then `61` then `cy` on three lines, exits with 0.

```
struct Person { name: string, age: i64 }

fun main(console: Console) {
    let people = [Person { name: "ann", age: 30 }, Person { name: "bo", age: 12 }, Person { name: "cy", age: 31 }];
    let adults = people.filter(fun(p) = p.age >= 18);
    let names = adults.map(fun(p) = p.name);
    console.println(names.fold("", fun(acc, n) = if acc == "" { n } else { "{acc}, {n}" }));
    console.println(adults.fold(0, fun(sum, p) = sum + p.age));
    console.println(people.sort_by(fun(a, b) = a.age > b.age)[0].name);
}
```

**Results and `?`** (milestone 17). Prints `3` then `3 is odd` on two lines, exits with 0.

```
fun half(n: i64): Result<i64, string> = if n % 2 == 0 { Ok(n / 2) } else { Err("{n} is odd") };

fun quarter(n: i64): Result<i64, string> {
    let h = half(n)?;
    half(h)
}

fun main(console: Console) {
    match quarter(12) {
        .Ok(q) => console.println(q),
        .Err(e) => console.println(e),
    }
    match quarter(6) {
        .Ok(q) => console.println(q),
        .Err(e) => console.println(e),
    }
}
```

**Text helpers** (milestone 18). Prints `42` then `a-b-c` then `2` on three lines, exits with 0.

```
fun main(console: Console) {
    let text = "name, age\nann, 30\n\nbo, 12\n";
    let mut total = 0;
    for line of text.lines() {
        let parts = line.split(",").map(fun(p) = p.trim());
        if parts.len < 2 || parts[0] == "name" {
            continue;
        }
        total += parse_i64(parts[1]).unwrap_or(0);
    }
    console.println(total);
    console.println(["a", "b", "c"].join("-"));
    console.println("  hi  ".trim().len);
}
```

**Reading input** (milestone 19). Run with `5`, `7` and `x` on three lines of stdin. Prints `12` then `skipped 1` on two lines, exits with 0.

```
fun main(console: Console) {
    let mut total = 0;
    let mut skipped = 0;
    while true {
        match console.read_line() {
            .Ok(.Some(line)) => match parse_i64(line.trim()) {
                .Some(n) => { total += n; }
                .None => { skipped += 1; }
            },
            .Ok(.None) => { break; }
            .Err(e) => { console.eprintln(e); break; }
        }
    }
    console.println(total);
    console.println("skipped {skipped}");
}
```

**Interface values** (milestone 20). Prints `16.0` then `1.0` then `circle` on three lines, exits with 0.

```
interface Shape {
    fun area(self): f64;
    fun name(self): string;
    fun grow(mut self, k: f64);
}

struct Circle { r: f64 }

impl Shape for Circle {
    fun area(self): f64 = 3.0 * self.r * self.r;
    fun name(self): string = "circle";
    fun grow(mut self, k: f64) { self.r *= k; }
}

struct Square { side: f64 }

impl Shape for Square {
    fun area(self): f64 = self.side * self.side;
    fun name(self): string = "square";
    fun grow(mut self, k: f64) { self.side *= k; }
}

fun total(shapes: Shape[]): f64 {
    let mut sum = 0.0;
    for s of shapes {
        sum += s.area();
    }
    sum
}

fun main(console: Console) {
    let c = Circle { r: 1.0 };
    let mut shapes: Shape[] = [c, Square { side: 2.0 }];
    shapes[0].grow(2.0);
    console.println(total(shapes));
    console.println(c.r);
    let first = shapes[0];
    console.println(first.name());
}
```

**Type patterns** (milestone 21). Prints `big circle` then `circle of radius 1.5` then `a square` on three lines, exits with 0.

```
interface Shape {
    fun area(self): f64;
}

struct Circle { r: f64 }

impl Shape for Circle {
    fun area(self): f64 = 3.0 * self.r * self.r;
}

struct Square { side: f64 }

impl Shape for Square {
    fun area(self): f64 = self.side * self.side;
}

fun describe(s: Shape): string = match s {
    c: Circle if c.r > 10.0 => "big circle",
    c: Circle => "circle of radius {c.r}",
    _: Square => "a square",
    _ => "some shape",
};

fun main(console: Console) {
    let shapes: Shape[] = [Circle { r: 20.0 }, Circle { r: 1.5 }, Square { side: 2.0 }];
    for s of shapes {
        console.println(describe(s));
    }
}
```

**Errors as interfaces** (milestone 22). Prints `42` then `error: not a number: x` then `range error: 500 is out of range` on three lines, exits with 0. The program's own error interface is `Report`: `Error` is the prelude's, which `?` and `"{e}"` use.

```
interface Report {
    fun message(self): string;
    fun report(self): string = "error: {self.message()}";
}

struct ParseError { text: string }

impl Report for ParseError {
    fun message(self): string = "not a number: {self.text}";
}

struct RangeError { n: i64 }

impl Report for RangeError {
    fun message(self): string = "{self.n} is out of range";
    fun report(self): string = "range error: {self.message()}";
}

fun parse(s: string): Result<i64, ParseError> = parse_i64(s).ok_or(ParseError { text: s });

fun check(n: i64): Result<i64, RangeError> =
    if n < 100 { Ok(n) } else { Err(RangeError { n: n }) };

fun read(s: string): Result<i64, Report> {
    let n = parse(s)?;
    let m = check(n)?;
    Ok(m * 2)
}

fun main(console: Console) {
    for s of ["21", "x", "500"] {
        match read(s) {
            .Ok(n) => console.println(n),
            .Err(e) => console.println(e.report()),
        }
    }
}
```

**Generic interfaces** (milestone 23). Prints `3` then `a-b` then `11` on three lines, exits with 0.

```
interface Source<T> {
    fun next(mut self): Option<T>;
}

struct Count { n: i64, end: i64 }

impl Source<i64> for Count {
    fun next(mut self): Option<i64> {
        if self.n == self.end {
            return None;
        }
        self.n += 1;
        Some(self.n)
    }
}

struct Words { xs: string[], at: i64 }

impl Source<string> for Words {
    fun next(mut self): Option<string> {
        if self.at == self.xs.len {
            return None;
        }
        self.at += 1;
        Some(self.xs[self.at - 1])
    }
}

fun collect<T, S: Source<T>>(src: S): T[] {
    let mut s = src;
    let mut out: T[] = [];
    let mut more = true;
    while more {
        match s.next() {
            .Some(x) => { out.push(x); }
            .None => { more = false; }
        }
    }
    out
}

fun main(console: Console) {
    console.println(collect(Count { n: 0, end: 3 }).len);
    console.println(collect(Words { xs: ["a", "b"], at: 0 }).join("-"));
    let mut s: Source<i64> = Count { n: 10, end: 12 };
    console.println(s.next().unwrap_or(0));
}
```

**Iterators** (milestone 24). Prints `1`, `2`, `3`, `0`, `88` on five lines, exits with 0.

```
struct Count { n: i64, end: i64 }

impl Iter<i64> for Count {
    fun next(mut self): Option<i64> {
        if self.n == self.end {
            return None;
        }
        self.n += 1;
        Some(self.n)
    }
}

struct Fib { a: i64, b: i64, left: i64 }

impl Iter<i64> for Fib {
    fun next(mut self): Option<i64> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        let a = self.a;
        self.a = self.b;
        self.b += a;
        Some(a)
    }
}

fun total(it: Iter<i64>): i64 {
    let mut sum = 0;
    for n of it {
        sum += n;
    }
    sum
}

fun main(console: Console) {
    let c = Count { n: 0, end: 3 };
    for n of c {
        console.println(n);
    }
    console.println(c.n);
    console.println(total(Fib { a: 0, b: 1, left: 10 }));
}
```

**`Self`** (milestone 25). Prints `1999` then `2.0` on two lines, exits with 0.

```
interface Ordered {
    fun less(self, other: Self): bool;
    fun max(self, other: Self): Self = if self.less(other) { other } else { self };
}

struct Money { cents: i64 }

impl Ordered for Money {
    fun less(self, other: Money): bool = self.cents < other.cents;
}

struct Version { major: i64, minor: i64 }

impl Ordered for Version {
    fun less(self, other: Version): bool =
        self.major < other.major || (self.major == other.major && self.minor < other.minor);
}

fun largest<T: Ordered>(xs: T[]): T {
    let mut best = xs[0];
    for x of xs {
        best = best.max(x);
    }
    best
}

fun main(console: Console) {
    console.println(largest([Money { cents: 250 }, Money { cents: 1999 }, Money { cents: 5 }]).cents);
    let v = largest([Version { major: 1, minor: 9 }, Version { major: 2, minor: 0 }, Version { major: 1, minor: 10 }]);
    console.println("{v.major}.{v.minor}");
}
```

**Converting between interfaces** (milestone 26). Prints `good dog, rex` then `woof` then `hello, tom` then `meow` on four lines, exits with 0.

```
interface Named {
    fun name(self): string;
}

interface Pet: Named {
    fun sound(self): string;
}

struct Dog { called: string }

impl Named for Dog {
    fun name(self): string = self.called;
}

impl Pet for Dog {
    fun sound(self): string = "woof";
}

struct Cat { called: string }

impl Named for Cat {
    fun name(self): string = self.called;
}

impl Pet for Cat {
    fun sound(self): string = "meow";
}

fun greet(n: Named): string = match n {
    d: Dog => "good dog, {d.called}",
    _ => "hello, {n.name()}",
};

fun main(console: Console) {
    let pets: Pet[] = [Dog { called: "rex" }, Cat { called: "tom" }];
    for p of pets {
        console.println(greet(p));
        console.println(p.sound());
    }
}
```

**More numbers** (milestone 27). Prints `18446744073709551615` then `4000000001` then `0.1` then `true` then `4294967295` on five lines, exits with 0.

```
fun main(console: Console) {
    let big: u64 = 18446744073709551615;
    console.println(big);
    let n: u32 = 4000000000;
    console.println(n + 1);
    let x: f32 = 0.1;
    console.println(x);
    console.println(x as f64 > 0.1);
    console.println((-1).truncate<u32>());
}
```

**Bits** (milestone 28). Prints `true`, `11`, `5`, `5`, `1024`, `-4`, `65280` on seven lines, exits with 0.

```
fun main(console: Console) {
    let flags: u32 = 0b1010;
    console.println(flags & 0b0010 == 0b0010);
    console.println(flags | 1);
    console.println(flags ^ 0xf);
    console.println(~flags & 0xf);
    console.println(1 << 10);
    console.println(-8 >> 1);
    let mut m: u64 = 0xff;
    m <<= 8;
    console.println(m);
}
```

**Hashing** (milestone 29). Prints `12638187200555641996`, `14695981039346656037`, `3`, `3`, `25`, `0` on six lines, exits with 0.

```
fun hash(s: string): u64 {
    let mut h: u64 = 14695981039346656037;
    for i in 0..s.len {
        h = (h ^ (s[i] as u64)).wrapping_mul(1099511628211);
    }
    h
}

fun main(console: Console) {
    console.println(hash("a"));
    console.println(hash(""));
    let x: u32 = 0b1011000;
    console.println(x.count_ones());
    console.println(x.trailing_zeros());
    console.println(x.leading_zeros());
    let m: u8 = 255;
    console.println(m.wrapping_add(1));
}
```

**Packages** (milestone 30). Prints `12.0`, exits with 0. The files sit as their first lines say, and `lughac run main.la` builds them all.

```
# lugha.toml
[packages]
geo = "libs/geo"
```

```
// libs/geo/shapes.la
public struct Circle { r: f64 }

impl Circle {
    public fun area(self): f64 = 3.0 * self.r * self.r;
    fun diameter(self): f64 = 2.0 * self.r;
}

public fun circle(r: f64): Circle = Circle { r: r };
```

```
// main.la
import geo.shapes;

fun main(console: Console) {
    let c = shapes.circle(2.0);
    console.println(c.area());
}
```

In the same layout, a `main.la` that calls `c.diameter()` or writes `shapes.Circle { r: 1.0 }` is rejected with E0313: `diameter` and the field `r` have no `public`, so only package `geo` may use them.

**Or-patterns** (milestone 31). Prints `3.0`, `3.0`, `stop`, `run`, `42` on five lines, exits with 0.

```
enum Shape { Circle(f64), Square(f64), Rect(f64, f64) }

fun size(s: Shape): f64 = match s {
    .Circle(r) | .Square(r) => r * 2.0,
    .Rect(w, h) => w + h,
};

fun kind(cmd: string): string = match cmd {
    "q" | "quit" | "exit" => "stop",
    "h" | "help" => "help",
    _ => "run",
};

fun main(console: Console) {
    console.println(size(Shape.Square(1.5)));
    console.println(size(Shape.Rect(1.0, 2.0)));
    console.println(kind("quit"));
    console.println(kind("ls"));
    match Some(41) {
        .Some(mut n) => {
            n += 1;
            console.println(n);
        }
        .None => {}
    }
}
```

**Unwrapping** (milestone 32). Prints `9` then `5` on two lines, exits with 101 (the last `expect` panics with its message).

```
fun main(console: Console) {
    let n = parse_i64("42").unwrap();
    let m = parse_i64("7").expect("need a number");
    let mut x = n + m;
    x %= 10;
    console.println(x);
    let r: Result<i64, string> = Ok(5);
    console.println(r.unwrap());
    console.println(parse_i64("x").expect("not a number"));
}
```

**Conditional patterns** (milestone 33). Prints `skipped x`, `7`, `321`, `42`, `-1` on five lines, exits with 0.

```
fun double(s: string): i64 {
    let .Some(n) = parse_i64(s) else {
        return -1;
    };
    n * 2
}

fun main(console: Console) {
    let mut total = 0;
    for w of ["3", "x", "4"] {
        if let .Some(n) = parse_i64(w) {
            total += n;
        } else {
            console.println("skipped {w}");
        }
    }
    console.println(total);
    let mut stack = [1, 2, 3];
    while let .Some(top) = stack.pop() {
        console.print(top);
    }
    console.println("");
    console.println(double("21"));
    console.println(double("x"));
}
```

**`Self` in `impl` blocks** (milestone 34). Prints `(3, 4)`, `this is (3, 4)`, `green` on three lines, exits with 0.

```
interface Describe {
    fun name(self): string;
    fun describe(self): string = "this is {self.name()}";
}

struct Point { x: i64, y: i64 }

impl Point {
    fun origin(): Self = Self { x: 0, y: 0 };
    fun plus(self, other: Self): Self = Self { x: self.x + other.x, y: self.y + other.y };
    fun unit(): Self = Self.origin().plus(Self { x: 1, y: 1 });
}

impl Describe for Point {
    fun name(self): string = "({self.x}, {self.y})";
}

enum Light { Red, Green }

impl Light {
    fun toggle(self): Self = match self {
        .Red => Self.Green,
        .Green => Self.Red,
    };
}

fun main(console: Console) {
    let p = Point.unit().plus(Point { x: 2, y: 3 });
    console.println(p.name());
    console.println(p.describe());
    match Light.Red.toggle() {
        .Green => console.println("green"),
        .Red => console.println("red"),
    }
}
```

**More integers** (milestone 35). Prints `32767`, `-1`, `32767`, `255`, `3`, `true`, `18446744073709551615` on seven lines, exits with 0.

```
fun main(console: Console) {
    let a: i16 = i16.MAX;
    console.println(a);
    console.println(a.checked_add(1).unwrap_or(-1));
    console.println(a.saturating_add(100));
    let b: u8 = 200;
    console.println(b.saturating_mul(2));
    let c: u16 = 0b1000000000000001;
    console.println(c.rotate_left(1));
    let d: i8 = i8.MIN;
    console.println(d.checked_div(-1).is_none());
    console.println(parse_u64("18446744073709551615").unwrap());
}
```

**Iterator helpers** (milestone 36). Prints `16,4,36`, `5050`, `63`, `0=104;1=101;2=121;`, `3`, `true`, `5`, `31` on eight lines, exits with 0.

```
fun even(n: i64): bool = n % 2 == 0;

fun main(console: Console) {
    let xs = [3, 1, 4, 1, 5, 9, 2, 6];
    let words = xs.iter().filter(even).map(fun(n) = "{n * n}").collect();
    console.println(words.join(","));
    console.println(range(1, 101).sum());
    console.println(range(10, 1000000000).filter(fun(n) = n % 7 == 0).take(3).sum());
    for p of "hey".bytes().enumerate() {
        console.print("{p.i}={p.value};");
    }
    console.println("");
    let mut vowels = 0;
    for b of "banana" {
        if b == 97 {
            vowels += 1;
        }
    }
    console.println(vowels);
    console.println(xs.iter().any(fun(n) = n > 8));
    console.println(xs.iter().find(fun(n) = n > 4).unwrap());
    console.println(xs.iter().fold(0, fun(a, n) = a + n));
}
```

**Package versions** (milestone 37). Prints `12`, exits with 0. If geo's manifest said `version = "2.0.0"`, or declared no version, the program would be rejected with E0324 at the requirement.

```
# lugha.toml
[packages]
geo = { path = "libs/geo", version = "1.2" }
```

```
# libs/geo/lugha.toml
[package]
version = "1.4.0"
```

```
// libs/geo/shapes.la
public fun area(w: i64, h: i64): i64 = w * h;
```

```
// main.la
import geo.shapes;

fun main(console: Console) {
    console.println(shapes.area(3, 4));
}
```

**Re-exports** (milestone 38). Prints `8`, exits with 0. The program lists only `geo`, and uses `mathx`'s `Vec2` and `add` through `geo.shapes`.

```
# lugha.toml
[packages]
geo = "libs/geo"
```

```
# libs/geo/lugha.toml
[packages]
mathx = "../mathx"
```

```
// libs/mathx/vec.la
public struct Vec2 { public x: i64, public y: i64 }

public fun add(a: Vec2, b: Vec2): Vec2 = Vec2 { x: a.x + b.x, y: a.y + b.y };
```

```
// libs/geo/shapes.la
public import mathx.vec { Vec2, add };

public fun centre(a: Vec2, b: Vec2): Vec2 = add(a, b);
```

```
// main.la
import geo.shapes;

fun main(console: Console) {
    let c = shapes.centre(shapes.Vec2 { x: 1, y: 2 }, shapes.Vec2 { x: 3, y: 4 });
    console.println(shapes.add(c, c).x);
}
```

**Printing values** (milestone 39). Prints `Point { x: 1, y: 2 }`, `Circle(1.5)`, `[Some(1), None]`, `Team { name: "red", members: ["ann", "bo"] }`, `$5.25`, `wallet: Wallet { cash: Money { cents: 525 } }`, `Stack { items: [3] }` on seven lines, exits with 0.

```
struct Point { x: i64, y: i64 }

enum Shape { Circle(f64), Empty }

struct Team { name: string, members: string[] }

struct Money { cents: i64 }

impl Display for Money {
    fun show(self): string = "${self.cents / 100}.{self.cents % 100}";
}

struct Wallet { cash: Money }

struct Stack<T> { items: T[] }

fun dump<T>(console: Console, x: T) {
    console.println(x.debug());
}

fun main(console: Console) {
    console.println((Point { x: 1, y: 2 }).debug());
    console.println(Shape.Circle(1.5).debug());
    console.println([Some(1), None].debug());
    console.println((Team { name: "red", members: ["ann", "bo"] }).debug());
    let m = Money { cents: 525 };
    console.println(m);
    // A debug form shows each part's debug form, never its `Display`.
    console.println("wallet: {(Wallet { cash: m }).debug()}");
    dump(console, Stack { items: [3] });
}
```

**Printing anything** (milestone 40). Prints `Circle { r: 2.0 }`, `Square { side: 3 }`, `<fun>` on three lines, exits with 101 (the last line panics, with the message unwrap on Err: Bad { code: 7 }).

```
interface Shape { fun area(self): f64; }

struct Circle { r: f64 }
impl Shape for Circle { fun area(self): f64 = 3.0 * self.r * self.r; }

struct Square { side: i64 }
impl Shape for Square { fun area(self): f64 = (self.side * self.side).round<f64>(); }
impl Display for Square { fun show(self): string = "square {self.side}"; }

struct Bad { code: i64 }

fun main(console: Console) {
    let shapes: Shape[] = [Circle { r: 2.0 }, Square { side: 3 }];
    // An interface value's debug form is its value's.
    for s of shapes {
        console.println(s.debug());
    }
    console.println((fun(x: i64): i64 = x).debug());
    let r: Result<i64, Bad> = Err(Bad { code: 7 });
    console.println(r.unwrap());
}
```

**`Self` in declarations** (milestone 41). Prints `3` then `Cons(1, Cons(2, Nil))` on two lines, exits with 0.

```
struct Tree<T> { value: T, kids: Self[] }

impl<T> Tree<T> {
    fun size(self): i64 {
        let mut n = 1;
        for k of self.kids {
            n += k.size();
        }
        n
    }
}

enum List { Cons(i64, Self), Nil }

fun main(console: Console) {
    let t = Tree { value: 1, kids: [Tree { value: 2, kids: [] }, Tree { value: 3, kids: [] }] };
    console.println(t.size());
    console.println((List.Cons(1, List.Cons(2, List.Nil))).debug());
}
```

**`for … of mut`** (milestone 42). Prints `[2, 4, 6]`, `a b`, `c d` on three lines, exits with 0.

```
fun main(console: Console) {
    let mut xs = [1, 2, 3];
    for x of mut xs {
        x *= 2;
    }
    console.println(xs.debug());
    let mut words = ["a", "b", "--", "c", "d"].iter();
    let mut first = "";
    for w of mut words {
        if w == "--" {
            break;
        }
        first = first + w + " ";
    }
    console.println(first.trim());
    let mut rest = "";
    for w of mut words {
        rest = rest + w + " ";
    }
    console.println(rest.trim());
}
```

**More iterator helpers** (milestone 43). Prints `[(1, "a"), (2, "b")]`, `[1, 2, 3, 4]`, `[1, 2]`, `[3, 1]`, `[0, 0, 1, 0, 1, 2]`, `Some(1)`, `Some(9)`, `Some(2)` on eight lines, exits with 0.

```
fun main(console: Console) {
    let xs = [1, 2, 3, 1, 9];
    console.println((xs.iter().zip(["a", "b"].iter()).collect()).debug());
    console.println(([1, 2].iter().chain([3, 4].iter()).collect()).debug());
    console.println((xs.iter().take_while(fun(n) = n < 3).collect()).debug());
    console.println((xs.iter().skip_while(fun(n) = n < 3).take(2).collect()).debug());
    console.println((range(1, 4).flat_map(fun(n) = range(0, n)).collect()).debug());
    console.println(xs.iter().min().debug());
    console.println(xs.iter().max().debug());
    console.println((xs.iter().position(fun(n) = n == 3)).debug());
}
```

**Format specifiers** (milestone 44). Prints `3.14`, `|    42|`, `|42    |`, `-0042`, `0xff`, `***ann***`, `11111111` on seven lines, exits with 0.

```
fun main(console: Console) {
    let pi = 3.14159;
    let n = 42;
    console.println("{pi:.2}");
    console.println("|{n:6}|");
    console.println("|{n:<6}|");
    console.println("{-42:05}");
    console.println("{255:#x}");
    console.println("{"ann":*^9}");
    console.println("{-1 as i8:b}");
}
```

**Bytes and indexes** (milestone 45). Run in an empty folder. Prints `wrote 4`, `[104, 105, 0, 255]`, `6`, `Ok("héllo")`, `Err("invalid UTF-8 at byte 1")`, `195 169 104` on six lines, exits with 0.

```
fun main(console: Console, fs: Fs) {
    let data: u8[] = [104, 105, 0, 255];
    match fs.cwd().write_bytes("out.bin", data) {
        .Ok(n) => console.println("wrote {n}"),
        .Err(e) => console.println("error {e}"),
    }
    match fs.cwd().read_bytes("out.bin") {
        .Ok(b) => console.println(b.debug()),
        .Err(e) => console.println(e),
    }
    let text = "héllo";
    let raw = text.to_bytes();
    console.println(raw.len);
    console.println(string_from_utf8(raw).debug());
    let bad: u8[] = [104, 195, 40];
    console.println(string_from_utf8(bad).debug());
    let i: u8 = 1;
    let j: u32 = 2;
    let k: i32 = 0;
    console.println("{raw[i]} {raw[j]} {raw[k]}");
}
```

**Unicode text** (milestone 46). Prints `10 chars, 12 bytes`, `NAÏVE CAFÉ`, `9`, `['é', 'a', '\n']`, `233`, `None` on six lines, exits with 0.

```
fun shout(s: string): string {
    let mut out = "";
    for c of s.chars() {
        out = out + to_string(c.to_upper());
    }
    out
}

fun main(console: Console) {
    let s = "naïve café";
    console.println("{s.char_count()} chars, {s.len} bytes");
    console.println(shout(s));
    console.println(s.chars().filter(fun(c) = c.is_alphabetic()).count());
    console.println(['é', 'a', '\n'].debug());
    console.println('é' as u32);
    console.println(char_from(55296).debug());
}
```

**Tuples** (milestone 47). Prints `3 2`, `0: ann`, `1: bo`, `(1, "a", 2.5)`, `a`, `big two`, `true`, `Some((1, 9))`, `(1, [2]) (1, [9])`, `true`, `7` on 11 lines, exits with 0.

```
fun divmod(a: i64, b: i64): (i64, i64) = (a / b, a % b);

fun main(console: Console) {
    let (q, r) = divmod(17, 5);
    console.println("{q} {r}");
    let names = ["ann", "bo"];
    for (i, w) of range(0, 2).zip(names.iter()) {
        console.println("{i}: {w}");
    }
    let t = (1, "a", 2.5);
    console.println(t.debug());
    console.println(t.1);
    let label = match (q > 2, r) {
        (true, 2) => "big two",
        (true, _) => "big",
        (false, _) => "small",
    };
    console.println(label);
    console.println((1, "b") < (1, "c"));
    console.println(([(2, 1), (1, 9)].iter().min()).debug());
    let mut p = (1, [2]);
    let copy = p;
    p.1[0] = 9;
    console.println("{copy.debug()} {p.debug()}");
    console.println((1, 2) == (1, 2));
    if let (.Some(a), b) = (Some(3), 4) {
        console.println(a + b);
    }
}
```

**Format** (milestone 48). Prints `box has   12 items`, `crate has  340 items`, `3.14 | 0xff | left  |`, `{literal} and (1, 'a')`, `total: 00352` on five lines, exits with 0.

```
// `format` (v14): a template and the values for its holes.
struct Item { name: string, n: i64 }

fun line(it: Item): string = format("{} has {:>4} items", it.name, it.n);

fun main(console: Console) {
    let items = [Item { name: "box", n: 12 }, Item { name: "crate", n: 340 }];
    for it of items {
        console.println(line(it));
    }
    console.println(format("{:.2} | {:#x} | {:<6}|", 3.14159, 255, "left"));
    console.println(format("{{literal}} and {}", (1, 'a').debug()));
    let total = items.iter().map(fun(it: Item): i64 = it.n).sum();
    console.println("total: {format("{:05}", total)}");
}
```

**Import aliases** (milestone 49). Prints `11 22`, `11`, `12`, `27`, `up`, `Vec2 { x: 2, y: 4 }` on six lines, exits with 0. `shapes` re-exports `shapes.vec` whole and `shapes.circle` as `round`.

```
// shapes/vec.la
public struct Vec2 { public x: i64, public y: i64 }

impl Vec2 {
    public fun zero(): Vec2 = Vec2 { x: 0, y: 0 };
}

public fun add(a: Vec2, b: Vec2): Vec2 = Vec2 { x: a.x + b.x, y: a.y + b.y };

public enum Dir { Up, Down }

fun hidden(): i64 = 1;
```

```
// shapes/circle.la
public fun area(r: i64): i64 = r * r * 3;
```

```
// shapes.la
public import shapes.vec;
public import shapes.circle as round;
```

```
// main.la
// Import aliases and whole-module re-exports (v14).
import shapes;
import shapes.circle as c;
import shapes.vec { Vec2 as V, add };

fun main(console: Console) {
    let a = shapes.vec.Vec2 { x: 1, y: 2 };
    let b: shapes.vec.Vec2 = shapes.vec.add(a, V { x: 10, y: 20 });
    console.println("{b.x} {b.y}");
    console.println(add(b, shapes.vec.Vec2.zero()).x);
    console.println(c.area(2));
    console.println(shapes.round.area(3));
    let d = shapes.vec.Dir.Up;
    match d {
        .Up => console.println("up"),
        .Down => console.println("down"),
    }
    let f = shapes.vec.add;
    console.println(f(a, a).debug());
}
```

**Operator overloading** (milestone 52). Prints `Vec2 { x: 4.0, y: 6.0 }`, `Vec2 { x: 3.0, y: 8.0 }`, `15`, `glowworm`, `Vec2 { x: 4.0, y: 6.0 }`, `6`, `0.75` on seven lines, exits with 0.

```
// Operator overloading (v15): `+` calls `add`, `*` calls `mul`, and
// generic code uses `+` through the prelude's `Add` and `Num` bounds.
struct Vec2 { x: f64, y: f64 }

impl Add for Vec2 {
    fun add(self, o: Vec2): Vec2 = Vec2 { x: self.x + o.x, y: self.y + o.y };
}

impl Mul for Vec2 {
    fun mul(self, o: Vec2): Vec2 = Vec2 { x: self.x * o.x, y: self.y * o.y };
}

fun add<T: Add>(a: T, b: T): T = a + b;

fun sum<T: Num>(xs: T[]): T {
    let mut s: T = 0;
    for x of xs {
        s += x;
    }
    s
}

fun main(console: Console) {
    let a = Vec2 { x: 1.0, y: 2.0 };
    let b = Vec2 { x: 3.0, y: 4.0 };
    console.println((a + b).debug());
    console.println((a * b).debug());
    console.println(add(5, 10));
    console.println(add("glow", "worm"));
    console.println(add(a, b).debug());
    console.println(sum([1, 2, 3]));
    console.println(sum([0.5, 0.25]));
}
```

**The standard library** (milestone 53). Prints `1.4142135623730951`, `5.0`, `7`, `3`, `3.0`, `3.141592653589793`, `1024.0`, `-2.0`, `10` on nine lines, exits with 0.

```
// The std library (v15): `import std.math;`, with float functions generic
// over `T: Float` and `abs`, `min`, `max` and `clamp` over any number.
import std.math;

fun hypot<T: Float>(a: T, b: T): T = math.sqrt(a * a + b * b);

fun main(console: Console) {
    console.println(math.sqrt(2.0));
    console.println(hypot(3.0, 4.0));
    console.println(math.max(2, 7));
    console.println(math.abs(-3));
    console.println(math.round(2.5));
    console.println(math.pi());
    console.println(math.pow(2.0, 10.0));
    console.println(math.floor(-1.5));
    console.println(math.clamp(15, 0, 10));
}
```

**Random numbers** (milestone 54). Prints `1546998764402558742`, `0.3789802506626686`, `6`, `13`, `0.3834617912676943`, `false`, `Some("red")`, `[1, 3, 5, 2, 4]` on eight lines, exits with 0. The first two values match the reference xoshiro256\*\* with splitmix64 seeding.

```
// std.random (v15): a seeded generator gives the same numbers on every
// run and platform, which this program's expected output pins down.
import std.random;

fun main(console: Console) {
    let mut rng = random.seeded(42);
    console.println(rng.next_u64());
    console.println(rng.uniform());
    console.println(rng.below(6) + 1);
    console.println(rng.between(10, 20));
    console.println(rng.normal(0.0, 1.0));
    console.println(rng.chance(0.5));
    console.println(rng.choice(["red", "green", "blue"]).debug());
    console.println(rng.shuffle([1, 2, 3, 4, 5]).debug());
}
```

**Copy-on-write** (milestone 55). Prints `[10, 2, 3] [1, 2, 3]`, `[[0, 0], [7, 0], [9]] [[0, 0], [0, 0]]`, `[0.5, 2.5] [0.5, 1.5]`, `[1.0, 3.0] [0.5, 1.5]` on four lines, exits with 0. Every copy keeps its values, exactly as with v14's eager copies.

```
// Copy-on-write (v15): copies share an array until one side writes, and
// every write leaves the other copies as they were.
struct Layer { weights: f64[] }

fun scaled(xs: f64[], k: f64): f64[] {
    let mut ys = xs;
    for y of mut ys { y *= k; }
    ys
}

fun main(console: Console) {
    let mut a = [1, 2, 3];
    let b = a;
    a[0] = 10;
    console.println("{a.debug()} {b.debug()}");

    let mut grid = [[0; 2]; 2];
    let saved = grid;
    grid[1][0] = 7;
    grid.push([9]);
    console.println("{grid.debug()} {saved.debug()}");

    let mut net = [Layer { weights: [0.5, 1.5] }];
    let w = net[0].weights;
    net[0].weights[1] += 1.0;
    console.println("{net[0].weights.debug()} {w.debug()}");
    console.println("{scaled(w, 2.0).debug()} {w.debug()}");
}
```

**Nested calls** (milestone 57). Prints `24.0`, `5`, `[3.0, 1.0, 4.0, 6.0, 24.0]` on three lines, exits with 0: the same as with one `let` per call.

```
// Nested `mut self` calls (v15): an argument may call methods on the
// receiver, and runs before the receiver is reached. Only an argument that
// could be the receiver's own storage is E0508.
struct Tape { vals: f64[] }

impl Tape {
    fun put(mut self, x: f64): i64 {
        self.vals.push(x);
        self.vals.len - 1
    }
    fun add(mut self, a: i64, b: i64): i64 = self.put(self.vals[a] + self.vals[b]);
    fun mul(mut self, a: i64, b: i64): i64 = self.put(self.vals[a] * self.vals[b]);
}

fun main(console: Console) {
    let mut t = Tape { vals: [] };
    let x = t.put(3.0);
    let y = t.mul(t.add(x, t.put(1.0)), t.add(x, x));
    console.println(t.vals[y]);
    console.println(t.vals.len);
    console.println(t.vals.debug());
}
```

**Numbers as T** (milestone 58). Prints `3`, `3.0`, `13`, `1.5`, `3` on five lines, exits with 0.

```
// Numbers as `T` (v15): `as` converts to and from a `T: Num`, and any
// integer literal is a `T: Num` and any float literal a `T: Float`.
fun mean<T: Num>(xs: T[]): T {
    let mut s: T = 0;
    for x of xs {
        s += x;
    }
    s / xs.len.round<T>()
}

fun scale<T: Num>(x: T): T = x * 3 - 2;
fun half<T: Float>(x: T): T = x * 0.5;

fun main(console: Console) {
    console.println(mean([1, 2, 6]));
    console.println(mean([1.0, 2.0, 6.0]));
    console.println(scale(5 as u8));
    console.println(half(3.0 as f32));
    console.println(mean([2.5, 3.5]).saturate<i64>());
}
```

**Ordering** (milestone 59). Prints `gpt`, `gpt mlp bigram`, `true`, `true` on four lines, exits with 0.

```
// User-defined order: `impl Ord` with its own `cmp`, so `<`, `sort`,
// `min` and tuples work on it, though a float field keeps it from
// `with Ord`. `==` stays structural.
struct Run { loss: f64, name: string } with Eq

impl Ord for Run {
    fun cmp(self, o: Run): Ordering =
        if self.loss < o.loss {
            Ordering.Less
        } else if self.loss > o.loss {
            Ordering.Greater
        } else {
            Ordering.Equal
        };
}

fun main(console: Console) {
    let runs = [Run { loss: 2.24, name: "gpt" }, Run { loss: 2.46, name: "bigram" }, Run { loss: 2.40, name: "mlp" }];
    let best = runs.iter().min().unwrap();
    console.println(best.name);
    console.println(runs.sort().map(fun(r: Run): string = r.name).join(" "));
    console.println(runs[0] < runs[1]);
    console.println((runs[1], 0) < (runs[1], 1));
}
```

**Environment** (milestone 64). Prints `true`, `default`, `true` on three lines, exits with 0.

```
// `Env` (v18): read-only environment variables; and the working
// folder's path, from `Fs`.
fun main(console: Console, env: Env, fs: Fs) {
    console.println(env.get("LUGHA_SURELY_UNSET").is_none());
    let home = env.get("LUGHA_SURELY_UNSET").unwrap_or("default");
    console.println(home);
    console.println(fs.cwd().path().is_ok());
}
```

**Directories** (milestone 65). Prints `["a.log", "b.log"]`, `true true false`, `Directory not empty`, `false` on four lines, exits with 0, and leaves nothing behind.

```
// Folders and what exists (v15, through a `Dir` since v18), cleaned up
// afterwards.
fun main(console: Console, fs: Fs) {
    let dir = fs.cwd();
    let _ = dir.make_dirs("runs/2026");
    let a = dir.write_file("runs/2026/b.log", "b");
    let b = dir.write_file("runs/2026/a.log", "a");
    match dir.list("runs/2026") {
        .Ok(names) => console.println(names.debug()),
        .Err(e) => console.println(e),
    }
    console.println("{dir.is_dir("runs")} {dir.is_file("runs/2026/a.log")} {dir.exists("runs/x")}");
    match dir.remove_dir("runs/2026") {
        .Ok(_) => console.println("removed"),
        .Err(e) => console.println(e),
    }
    for name of dir.list("runs/2026").unwrap_or([]) {
        let _ = dir.remove_file("runs/2026/{name}");
    }
    let _ = dir.remove_dir("runs/2026");
    let _ = dir.remove_dir("runs");
    console.println(dir.exists("runs"));
}
```

**Processes** (milestone 66). Prints `0: hello ; not a command`, `["apple", "pear", ""]`, `7`, `true` on four lines, exits with 0.

```
// `Proc` (v18): run a program with an argument list (never a shell),
// wait, and read its exit code and output.
fun main(console: Console, proc: Proc) {
    match proc.run("echo", ["hello", "; not a command"]) {
        .Ok(out) => console.println("{out.code}: {out.stdout.trim()}"),
        .Err(e) => console.println(e),
    }
    match proc.run_with("sort", [], "pear\napple\n") {
        .Ok(out) => console.println(out.stdout.split("\n").debug()),
        .Err(e) => console.println(e),
    }
    match proc.run("sh", ["-c", "exit 7"]) {
        .Ok(out) => console.println(out.code),
        .Err(e) => console.println(e),
    }
    console.println(proc.run("surely-not-a-program", []).is_err());
}
```

**Shared boxes** (milestone 67). Prints `2`, `10 true`, `10 11`, `Ref(Node { value: 1, next: Some(Ref(…)) })` on four lines, exits with 0.

```
// Shared boxes (v16): copying a `Ref` copies the handle, so every copy
// sees one box. Everything else is still a value.
struct Node { value: i64, next: Option<Ref<Node>> }

fun main(console: Console) {
    let count = Ref.new(0);
    let tick = fun(): i64 {
        count.update(fun(n: i64): i64 = n + 1);
        count.get()
    };
    tick();
    console.println(tick());
    let alias = count;
    alias.set(10);
    console.println("{count.get()} {count.is(alias)}");
    let mut copy = count.get();
    copy += 1;
    console.println("{count.get()} {copy}");
    let a = Ref.new(Node { value: 1, next: None });
    let mut first = a.get();
    first.next = Some(a);
    a.set(first);
    console.println(a.debug());
}
```

**The working folder** (milestone 181). Prints `true`, `true`, `Ok("hi")` on three lines, exits with 0, and leaves nothing behind.

```
// The working folder (v18): `fs.cwd()` is where the program started, and
// it never changes; paths are relative to it.
fun main(console: Console, fs: Fs) {
    let here = fs.cwd();
    let _ = here.make_dirs("inbox/2026");
    let wrote = here.write_file("inbox/2026/note.txt", "hi");
    console.println(here.path().is_ok());
    console.println(here.is_file("inbox/2026/note.txt"));
    console.println(here.read_file("inbox/2026/note.txt").debug());
    let _ = here.remove_file("inbox/2026/note.txt");
    let _ = here.remove_dir("inbox/2026");
    let _ = here.remove_dir("inbox");
}
```

**Block comments** (milestone 76). Prints `3`, `7`, `done` on three lines, exits with 0.

```
/* Block comments (v16): anywhere whitespace may be,
   over several lines, and nested. */
fun add(a: i64, /* the other */ b: i64): i64 = a + b;

fun main(console: Console) {
    let x = 1 /* inline */ + 2;
    /* commented out:
    console.println("old");   /* a comment inside */
    */
    console.println(x);
    console.println(add(x, 4));
    console.println("done");
}
```

**Green threads** (milestone 91). Prints `25`, `closure 2`, `60` on three lines, exits with 0.

```
// Green threads (v17): `root.spawn` runs a call concurrently, in the root
// scope `main` takes, and `join` waits for its result. Channels carry
// values between threads; nothing is marked `async`, and `square` is an
// ordinary function.
fun square(n: i64): i64 = n * n;

fun produce(out: Sender<i64>, count: i64) {
    for i in 1..count + 1 {
        out.send(i * 10);
    }
    out.close();
}

fun main(console: Console, root: Scope) {
    let a = root.spawn(square(3));
    let b = root.spawn(square(4));
    console.println(a.join().unwrap() + b.join().unwrap());
    let k = 2;
    let t = root.spawn(fun(): string { "closure {k}" }());
    console.println(t.join().unwrap());
    let (tx, rx) = Chan<i64>.new(2);
    root.spawn(produce(tx, 3));
    let mut sum = 0;
    for v of rx {
        sum += v;
    }
    console.println(sum);
}
```

**Select and deadlock** (milestone 91). Prints `quick: Some("late")`, `timed out`, `None`, `waiting` on four lines, exits with 101 after `panic: deadlock: every green thread is waiting` on stderr, then a line naming `main` waiting on `empty`'s channel.

```
// `select` waits on several channels at once; `after` gives up, and
// `default` never waits. A deadlock is a panic.
import std.task;

fun later(c: Sender<string>, ms: i64) {
    task.sleep(ms);
    c.send("late");
}

fun main(console: Console, root: Scope) {
    let (to_quick, quick) = Chan<string>.new(0);
    let (to_slow, slow) = Chan<string>.new(0);
    root.spawn(later(to_quick, 5));
    root.spawn(later(to_slow, 100));
    select {
        m = quick.recv() => console.println("quick: {m.debug()}"),
        m = slow.recv() => console.println("slow: {m.debug()}"),
    }
    select {
        m = slow.recv() => console.println("slow: {m.debug()}"),
        after(20) => console.println("timed out"),
    }
    let (never, empty) = Chan<i64>.new(0);
    let got = select {
        m = empty.recv() => m,
        default => None,
    };
    console.println(got.debug());
    console.println("waiting");
    empty.recv();
}
```

**Actors** (milestone 92; in scopes since milestone 201). Prints `100`, `true`, `true` on three lines, exits with 0.

```
// Actors (v17; in scopes, v18): state that one green thread owns, reached only through
// messages. A void method is queued; a method with a result waits for it.
// An actor starts in a scope, which stops it once the scope's tasks are
// done; its panic fails the scope.
actor Account {
    balance: i64,
    fun deposit(mut self, amount: i64) { self.balance += amount; }
    fun withdraw(mut self, amount: i64) {
        if amount > self.balance {
            panic("insufficient funds");
        }
        self.balance -= amount;
    }
    fun total(self): i64 = self.balance;
}

fun customer(account: Account) {
    for i in 0..10 {
        account.deposit(5);
    }
}

fun main(console: Console) {
    let r = scope s {
        let account = s.spawn(Account { balance: 0 });
        scope c {
            c.spawn(customer(account));
            c.spawn(customer(account));
        }.unwrap();
        console.println(account.total());
        account.withdraw(500);
    };
    console.println(r.is_err());
    let r = scope s {
        let other = s.spawn(Account { balance: 7 });
        other.stop();
    };
    console.println(r.is_ok());
}
```

**Scopes** (milestone 195). Prints `6`, `fetch: too slow: 50`, `slow: stopped`, `the body of s panicked` on four lines, exits with 0.

```
// Scopes (v18): every task starts in a scope, which waits for it. The
// first failure, a task's `Err` or a panic, cancels the others, which
// unwind at their next pause running their `defer`s, and is the scope's
// value.
import std.task;

fun fetch(ms: i64): Result<i64, string> {
    task.sleep(ms);
    if ms > 10 { Err("too slow: {ms}") } else { Ok(ms * 2) }
}

fun slow(console: Console) {
    defer console.println("slow: stopped");
    task.sleep(10000);
}

// A failing task cancels the body before its `join` returns, so `unwrap`
// only ever sees values.
fun both(a: i64, b: i64): Result<i64, Failure> {
    scope s {
        let x = s.spawn(fetch(a));
        let y = s.spawn(fetch(b));
        x.join().unwrap() + y.join().unwrap()
    }
}

fun main(console: Console) {
    for (a, b) of [(1, 2), (1, 50)] {
        match both(a, b) {
            .Ok(n) => console.println(n),
            .Err(f) => match f.kind {
                .Error(e) => console.println("{f.task}: {e}"),
                .Panic(m) => console.println(m),
                .Deadline => console.println("{f.task}: too late"),
            },
        }
    }
    let r = scope s {
        s.spawn(slow(console));
        task.sleep(1);
        let xs = [1, 2, 3];
        xs[5]
    };
    match r {
        .Ok(n) => console.println(n),
        .Err(f) => match f.kind {
            .Panic(_) => console.println("the body of {f.task} panicked"),
            .Error(e) => console.println(e),
            .Deadline => console.println("{f.task}: too late"),
        },
    }
}
```

**Word count** (milestone 101). Prints `Map { "the": 3, "cat": 1, "and": 2, "hat": 1, "bat": 1 }`, `3`, `5`, `the: 3`, `and: 2` on five lines, exits with 0.

```
fun main(console: Console) {
    let text = "the cat and the hat and the bat";
    let mut counts: Map<string, i64> = Map {};
    for w of text.split(" ") {
        counts[w] = counts.get(w).unwrap_or(0) + 1;
    }
    console.println(counts.debug());
    console.println(counts["the"]);
    console.println(counts.len);
    for (w, n) of counts {
        if n > 1 {
            console.println("{w}: {n}");
        }
    }
}
```

**Dedupe** (milestone 101). Prints `[3, 1, 2]`, `Set { 3, 1, 2 }`, `true`, `Set { 1, 2, 3, 4 }`, `Set { 2, 3 }`, `Set { 1 }`, `true` on seven lines, exits with 0.

```
fun main(console: Console) {
    let mut seen: Set<i64> = Set {};
    let mut firsts: i64[] = [];
    for x of [3, 1, 3, 2, 1] {
        if seen.insert(x) {
            firsts.push(x);
        }
    }
    console.println(firsts.debug());
    console.println(seen.debug());
    console.println(seen.has(2));
    let a = Set { 1, 2, 3 };
    let b = Set { 2, 3, 4 };
    console.println(a.union(b).debug());
    console.println(a.intersect(b).debug());
    console.println(a.minus(b).debug());
    console.println(Set { 1, 2 } == Set { 2, 1 });
}
```

## 11. Milestones and scope

The spec is implemented in five milestones, each ending in a program that runs. Build them in order; each one only adds to the last.

| # | Milestone | Spec subset | Done when |
| --- | --- | --- | --- |
| 1 | Expressions to a binary | Integer literals and arithmetic in `fun main(): i32`; lexer, parser, codegen, linking | `fun main(): i32 { 2 + 3 * 4 }` exits with 14 |
| 2 | Variables, control flow, functions | `let`/`let mut`, assignment, blocks with tail values, `if` expressions, `while`, `for` over ranges, optional condition parentheses, `=` function bodies, calls, recursion; integers only, all treated as `i64` | The program below exits with 55 |
| 3 | Type checker and diagnostics | All primitive types, literal inference, casts, mutability checks, return checking; error codes, `--diagnostics=json`, `lughac check`, "remove this semicolon" | The rejected program in section 10 reports E0401 in both formats |
| 4 | Runtime and C | `lugha_rt.c`, intrinsics, `extern fun`, string literals, integer overflow and division checks (panics) | Hello world and the libc example run |
| 5 | Heap data | Boehm GC, strings, arrays, structs, array copies, `for`-`of`, bounds checks, panics; `lughac spec` | Every program in section 10 passes |
| 6 | Enums (v1) | `enum`, construction, `match` with flat patterns, exhaustiveness, `Option<T>`, recursive enums | The enum programs in section 10 pass (done) |
| 7 | Methods (v1) | `impl`, `self`/`mut self`, associated functions, method calls | The methods program in section 10 passes (done) |
| 8 | Growable arrays (v1) | `push`, `pop`, `clear` on `T[]` | The stack program in section 10 passes (done: v1 complete) |
| 9 | Patterns and equality (v2) | deep `==` on every type, string order, nested patterns, guards, W0103 | The patterns program in section 10 passes (done) |
| 10 | Modules (v2) | `import`, per-module names, `m.f`/`m.T`, `private` (E0313), `public` reserved (E0317), no leaking (E0318), module symbols | The two-module program in section 10 passes (done) |
| 11 | Generics (v2) | type parameters, `Eq`/`Ord` bounds, inference, monomorphization, instance symbols, E0420; `Option<T>` as a predeclared generic enum | The generics program in section 10 passes (done) |
| 12 | Interfaces (v2) | `interface` declarations, interfaces as bounds, structural satisfaction (E0421, E0422, E0423) | The interfaces program in section 10 passes (done: v2 complete) |
| 13 | Input and interpolation (v3) | `args()`, `parse_i64`, `parse_f64`, string interpolation (E0111), `lughac run … -- args` | The arguments program in section 10 passes (done) |
| 14 | Functions as values (v3) | function types, named and associated functions as values, calling values, no `==` on functions, `lugha_ref_*` | The functions-as-values program in section 10 passes (done) |
| 15 | Closures (v3) | closure literals, capture by copy (E0511), parameter inference, `lugha_closure_*`, closures in generic code (E0209) | The closures program in section 10 passes (done) |
| 16 | Array helpers (v3) | `map`, `filter`, `fold`, `sort_by`, `any`, `all`, from a Lugha prelude | The array-helpers program in section 10 passes (done: v3 complete) |
| 17 | Results and `?` (v4) | `Result<T, E>` (in the prelude), `Ok`/`Err`, the `?` operator (E0424) | The results program in section 10 passes (done) |
| 18 | Text and option helpers (v4) | string helpers, `join`, `Option`/`Result` helpers, built-in `slice` | The text-helpers program in section 10 passes (done) |
| 19 | Input and output (v4) | `read_line`, `read_file`, `write_file`, `append_file`, `eprint`/`eprintln`, `exit` | The reading-input program in section 10 passes (done: v4 complete) |
| 20 | Interface values (v5) | interfaces as types, conversion, dynamic calls, copies, `lugha_vt_*` (E0425) | The interface-values program in section 10 passes (done) |
| 21 | Type patterns (v5) | `name: T` and `_: T` in `match`, E0426 | The type-patterns program in section 10 passes (done) |
| 22 | Default methods and error conversion (v5) | interface methods with bodies, `lugha_fn_<I>.<m><T>`; `?` converting into an interface error | The errors-as-interfaces program in section 10 passes (done: v5 complete) |
| 23 | Generic interfaces (v6) | `interface I<T>`, bounds with arguments, inference through bounds, generic interface values | The generic-interfaces program in section 10 passes (done) |
| 24 | Iterators (v6) | `Iter<T>` in the prelude, `for … of` over iterators | The iterators program in section 10 passes (done) |
| 25 | `Self` (v6) | `Self` in interfaces, bounds-only interfaces (E0427) | The `Self` program in section 10 passes (done) |
| 26 | Converting between interfaces (v6) | one interface value to another, and through `?` | The converting-between-interfaces program in section 10 passes (done: v6 complete) |
| 27 | More numbers (v7) | `u32`, `u64`, `f32`: literals, arithmetic, casts, printing, externs | The more-numbers program in section 10 passes (done) |
| 28 | Bits (v7) | `& \| ^ ~ << >>`, the compound forms, `0b` literals, the shift panic | The bits program in section 10 passes (done) |
| 29 | Integer methods (v7) | `wrapping_add`/`sub`/`mul`, `count_ones`, `leading_zeros`, `trailing_zeros` | The hashing program in section 10 passes (done: v7 complete) |
| 30 | Packages (v8) | `lugha.toml`, package imports, per-package manifests, `public` and package visibility, E0320–E0323 | The packages program in section 10 passes (done: v8 complete) |
| 31 | Better patterns (v9) | or-patterns (E0428), `mut` bindings | The or-patterns program in section 10 passes (done) |
| 32 | Unwrapping (v9) | `unwrap`, `expect`, `%=` | The unwrapping program in section 10 passes (done) |
| 33 | Conditional patterns (v9) | `if let`, `while let`, `let … else` (E0429) | The conditional-patterns program in section 10 passes (done: v9 complete) |
| 34 | `Self` everywhere (v10) | `Self` in `impl` blocks; default methods on concrete types (E0430) | The `Self`-in-`impl` program in section 10 passes (done) |
| 35 | More integers (v10) | `i8`, `i16`, `u16`; `checked_*`, `saturating_*`, rotations, `MIN`/`MAX`; `parse_u64` | The more-integers program in section 10 passes (done) |
| 36 | Iterator helpers (v10) | `iter`, `bytes`, `range`, `for b of s`; lazy adapters and consumers | The iterator-helpers program in section 10 passes (done: v10 complete) |
| 37 | Package versions (v11) | `[package]` versions, inline entries with requirements, E0324 | The package-versions program in section 10 passes (done) |
| 38 | Re-exports (v11) | `import m { X }`, `public import m { X }` | The re-exports program in section 10 passes (done: v11 complete) |
| 39 | Printing values (v12) | `Display` and `show`, built-in forms, quoting, printing type parameters | The printing-values program in section 10 passes (done) |
| 40 | Printing anything (v12) | interface and function values, `unwrap on Err: <error>` | The printing-anything program in section 10 passes (done: v12 complete) |
| 41 | `Self` in declarations (v13) | `Self` in struct fields and enum payloads | The `Self`-in-declarations program in section 10 passes (done) |
| 42 | `for … of mut` (v13) | iterators advanced in place; array elements as mutable aliases | The `for … of mut` program in section 10 passes (done) |
| 43 | More iterator helpers (v13) | `zip` (`Pair`; tuples since v14), `chain`, `take_while`, `skip_while`, `flat_map`, `min`, `max`, `position` | The more-helpers program in section 10 passes (done) |
| 44 | Format specifiers (v13) | `"{x:spec}"`, E0431 | The format-specifiers program in section 10 passes (done: v13 complete) |
| 45 | Bytes and indexes (v14) | binary file I/O, `to_bytes`, `string_from_utf8`, `read_stdin_bytes`, any-integer indexes | The bytes-and-indexes program in section 10 passes (done) |
| 46 | Unicode text (v14) | `char`, `'a'`, `chars()`, `char_count()`, casts, `char_from`, classification and case (E0112) | The Unicode-text program in section 10 passes (done) |
| 47 | Tuples (v14) | tuple types and values, `t.0`, patterns, `let`/`for` destructuring (E0433), `zip` → tuples, `Pair` removed | The tuples program in section 10 passes (done) |
| 48 | `format` (v14) | template `format(…)` (E0432) | The format program in section 10 passes (done) |
| 49 | Import aliases and re-exports (v14) | `as` on modules and items, `public import m [as v];`, `m.v.X` | The import-aliases program in section 10 passes (done) |
| 50 | Git packages (v14) | git entries, the cache, `lugha.lock`, `lughac update`, `--offline` (E0325, E0326) | A program using a `file://` repository is fetched, locked, built `--offline`, and a moved tag is caught (`tests/git_packages.rs`; done: v14 complete) |
| 51 | Package layout (v15) | `[package] name` (E0327) and `source`, the program's manifest from a parent folder and its implicit package, E0328, the `..` path fix | A library with `source = "src"` and an `examples/` program run from any folder (`tests/package_layout.rs`; done) |
| 52 | Operator overloading (v15) | `+ - * / %` and unary `-` through `add`/`sub`/`mul`/`div`/`rem`/`neg`, compound assignment, the prelude's `Add`…`Neg` and `Num` bounds, `0`/`1` as `T: Num`, bounds on `impl` blocks | The operator-overloading program in section 10 passes (done) |
| 53 | The std library and `std.math` (v15) | bundled `std.<module>` modules, `std` reserved, `$` built-ins in bundled sources, the `Float` bound, `std.math` | The standard-library program in section 10 passes (done) |
| 54 | `std.random` (v15) | a xoshiro256\*\* `Rng`: `seeded`, `from_os`, `uniform`, `below`, `between`, `normal`, `chance`, `choice`, `shuffle`; runtime `lugha_rt_os_seed` | The random-numbers program in section 10 passes (done) |
| 55 | Copy-on-write arrays (v15) | the shared flag in `cap`, marking at copy sites, cloning before writes, two-phase places, `for … of mut` through the place | The copy-on-write program in section 10 passes; every earlier program prints the same (done) |
| 56 | Owning arrays once per loop (v15) | the loop scan, fixed paths, guarded hoists before every loop kind; writes inside skip the flag test | Every program prints the same; `tests/programs/collections/owning_per_loop/` (done) |
| 57 | Nested `mut self` calls (v15) | E0508 only for arguments that can be the receiver's storage, `mut self` receivers reached after the arguments, E0510 for receivers only across an index | The nested-calls program in section 10 passes (done) |
| 58 | Numbers as `T` (v15) | `as` with `T: Num`/`Float`, any literal as `T`, E0402 per instance | The numbers-as-T program in section 10 passes (done) |
| 59 | User-defined `Ord` (v15) | `cmp(self, other: Self): Ordering`, the prelude's `Ordering`, `<` through `cmp`, tuples, `xs.sort()` | The ordering program in section 10 passes (done) |
| 60 | Lazy std modules (v15) | std functions and methods compiled only when reached, as the prelude's | Every program prints the same; `tests/lazy_std.rs` (done) |
| 61 | One `iter_sum` (v15) | `it.sum()` through `iter_sum<T: Num>` | Every program prints the same (done) |
| 62 | C → Lugha (v15) | `parse_i64`/`parse_u64` and one UTF-8 decoder in the prelude, `$string.from_bytes` | Every program prints the same; `m62/decode.la` matches the C implementation (done) |
| 63 | Widths in characters (v15) | format widths count characters, any character fills | `m63/widths.la` (done) |
| 64 | `std.env` (v15) | `get`, `current_dir`, read-only; `$os.env`, `$os.cwd` | The environment program in section 10 passes (done) |
| 65 | `std.fs` (v15) | folders and what exists; no recursive delete; `$os.list_dir` … `$os.stat_kind` | The directories program in section 10 passes (done) |
| 66 | `std.process` (v15) | `run`, `run_with`, `call`; no shell, `PATH` lookup, inherited environment, fds 0–2 only; `$os.spawn` | The processes program in section 10 passes (done: the v15 TODO complete) |
| 67 | Shared boxes (v16) | `Ref<T>`: `new`, `get`, `set`, `update`, `is`; not `Eq`; cyclic printing | The shared-boxes program in section 10 passes (done) |
| 68 | Changing directory (v16) | `env.set_current_dir(path)`; `$os.chdir` | The changing-directory program in section 10 passes (done) |
| 69 | `lughac init` (v16) | program and library layouts; the name from the folder or `--name`; never overwrites | A program and a library are created, checked and run, and every refusal is exit 2 (`tests/init.rs`; done) |
| 70 | `lughac lsp` (v16) | a language server publishing `check`'s diagnostics for unsaved text; offline, no writes, panics caught; the VS Code extension uses it | `tests/lsp.rs` drives it over stdio (done) |
| 71 | Hover (v16) | `textDocument/hover`: types of expressions, bindings, functions and methods (headers as written), fields; notes for generic instances and interface conversions; also with errors | `tests/lsp_hover.rs` drives it over stdio (done) |
| 72 | Go to definition (v16) | `textDocument/definition`: variables, functions and methods, interface methods, types, fields, variants, modules; none for `std` and the prelude; also with errors | `tests/lsp_definition.rs` drives it over stdio (done) |
| 73 | Completion (v16) | `textDocument/completion`: members after `.` (fields, methods, built-ins, helpers, module items, variants), names in scope, intrinsics, keywords; from the last check that parsed | `tests/lsp_completion.rs` drives it over stdio (done) |
| 74 | Signature help (v16) | `textDocument/signatureHelp`: every callable, the active parameter, generic instances filled in when known, intrinsics in their call forms | `tests/lsp_signature.rs` drives it over stdio (done) |
| 75 | Partial syntax trees (v16) | the parser's recovered program checked by the language server; only syntax errors shown meanwhile; the CLI unchanged | `tests/lsp_partial.rs` (done) |
| 76 | Block comments (v16) | `/* … */`, nested, wherever whitespace may be; E0113 | The block-comments program in section 10 passes (`tests/programs/basics/block_comments/`; done) |
| 77 | Doc comments (v16) | `/** … */` before declarations, Markdown with `@param`/`@returns`; hover, completion and signature help show them; hover on type and variant names; std and the prelude documented | `tests/lsp_docs.rs` (done) |
| 78 | Snippets (v16) | call placeholders and a postfix `.match` from the language server; declaration and control-flow snippets in the VS Code extension, each checked by `lughac check` | `tests/lsp_snippets.rs`, `tests/snippets.rs` (done) |
| 79 | Add missing arms (v16) | quick fixes on E0417: every missing arm in full, or a `_` arm; bodies `panic("not handled")` | `tests/lsp_code_actions.rs` (done) |
| 80 | `lughac fmt` (v16) | the tree-based formatter: the canonical style, comments kept, the output verified, `--check` | Every program in the repository formats, verifies and is idempotent (`tests/fmt_corpus.rs`); `tests/fmt.rs` (done) |
| 81 | Formatting in the editor (v16) | `textDocument/formatting` in `lughac lsp` from the editor's text; quiet on syntax errors, one message on a verification failure; the VS Code extension the default Lugha formatter | `tests/lsp_formatting.rs` (done) |
| 82 | More quick fixes (v16) | `let mut` (E0501), the closest variant (W0103), a std import (E0301), each preferred | `tests/lsp_code_actions.rs` (done) |
| 83 | Find References and Rename (v16) | every use of a name across the checked files; rename refused unless safe, self-checked in memory | `tests/lsp_references.rs`, `tests/lsp_rename.rs` (done) |
| 84 | `lughac test` (v16) | `@test` functions (E0210, E0434), `assert`/`assert_eq`/`assert_ne`, each test in its own process, in parallel, `--filter`, `--timeout` | `tests/test_runner.rs`, `tests/programs/tooling/lughac_test/` (done) |
| 85 | Tests in the editor (v16) | `lughac test --list`/`--json`/`--exact`, repeatable `--filter`; the VS Code Testing view; nothing in untrusted folders | `tests/test_runner.rs`, the extension's `tests.test.ts` (done) |
| 86 | `lughac explain` (v16) | an explanation for every code, its examples checked; the hint after human diagnostics; code links in the editor | `tests/explain.rs` (done) |
| 87 | Linking C libraries (v16) | `[link]` in `lugha.toml`: `libs`, `paths`, `include`, `c`, `pkg-config`; every used package's table; E0329–E0331 | `tests/link_libs.rs` (done) |
| 88 | Callbacks and handles (v16) | `extern type`; functions and closures passed to C (`data`, `release`); `extern "symbol" fun`; E0435, E0436 | `tests/callbacks.rs` (done) |
| 89 | Strings from C (v16) | `string` and `Option<string>` extern results, `Option<string>` arguments, string callback parameters; copies, U+FFFD | `tests/c_strings.rs` (done) |
| 90 | C structs by value (v16) | `extern struct`; passed by value to and from C as on x86-64 System V; E0437 | `tests/extern_structs.rs` (done) |
| 91 | Green threads (v17) | `spawn`, `Task`, `Chan`, `select`, `std.task`; preemption; deadlock; E0211, E0438 | The §10 green-threads programs pass; `tests/programs/concurrency/green_threads/` (done) |
| 92 | Actors (v17) | `actor` declarations, `spawn` of an actor, call and cast, `stop`, `join`, `is`, panics ended in their actor, sharing nothing; E0439 | The §10 actors program passes; `tests/programs/concurrency/actors/` (done) |
| 93 | Struct update syntax (v17) | `S { f: e, ..base }`: Rust's rules; E0440, W0104 | `tests/programs/types/struct_update` (done) |
| 94 | Variants as functions (v17) | `Msg.Named`, `Some`, `Ok`, `Err` as function values; generic ones typed by what's expected | `tests/programs/types/variant_functions` (done) |
| 95 | Formatter hugging (v17) | a call's trailing array or struct literal hugs; headers kept whole | `src/formatter/tests.rs` (done) |
| 96 | Inference from siblings (v17) | result-only generic calls typed by their siblings; `assert_eq`/`assert_ne`, `==`/`!=` | `tests/programs/generics/sibling_inference` (done) |
| 97 | Packages and tests (v17) | package files as modules under `test`/`check`, `test/` inside the package, `check` without `main`, one lock per package, `skip(reason)` | `tests/test_runner.rs`, `tests/package_layout.rs`, `tests/git_packages.rs`, `tests/cli.rs` (done) |
| 98 | Module doc comments (v17) | a file's top `/** */` documents its module; hover on module names; std documented | `tests/lsp_docs.rs`, `lsp/docs.rs` (done) |
| 99 | Package entry modules (v17) | `import N;` imports the package's `lib.la`; `lughac init --lib` writes one | `src/modules/entry_tests.rs`, `tests/package_layout.rs`, `tests/init.rs`, `tests/lsp_docs.rs` (done) |
| 100 | Glob re-exports (v17) | `public import m { * };` re-exports every `public` item of `m`; E0201 for misuse, E0314 for clashes | `src/check/globs.rs`, `tests/programs/modules/glob_reexports/`, `tests/package_layout.rs`, `tests/lsp_definition.rs` (done) |
| 101 | Maps and sets (v17) | `Map<K, V>`, `Set<T>`, `#{ … }` literals; E0441, E0442, W0105; the iterator type `Mapped` | `tests/programs/collections/maps_and_sets/`, the §10 word count and dedupe programs (done) |
| 102 | `out` parameters (v17) | `out` on extern parameters and call arguments; E0443 | `tests/out_params.rs` (done) |
| 103 | `ptr` and `mut` parameters (v17) | `ptr e`, `mut place` | `tests/out_params.rs` (done) |
| 104 | Arrays and unions at the C boundary (v17) | `T[]`, `mut T[]`, `extern union`, E0444 | `tests/c_arrays.rs` (done) |
| 105 | C memory and pointer callbacks (v17) | `CPtr`, `c_read`/`c_write`, callback `T[]`/`Ref<T[]>` | `tests/c_memory.rs` (done) |
| 106 | Nullable handle fields, plain-data unions (v17) | `Option<H>` fields, E0437 for pointers in unions | `tests/extern_structs.rs`, `tests/c_arrays.rs` (done) |
| 107 | The unit type (v17) | `()` type and value, W0106, E0407 retired | `tests/programs/types/unit/` (done) |
| 108 | Float bits (v17) | `x.to_bits()`, `f64.from_bits(b)`, `f32.from_bits(b)` | `tests/programs/numbers/float_bits/` (done) |
| 109 | Global constants (v17) | `const NAME: T = value;`, E0445, E0446 | `tests/programs/constants/globals/` (done) |
| 110 | `std.net` (v17) | TCP, non-blocking sockets in the scheduler | `tests/net.rs` (done) |
| 111 | `std.http` (v17) | HTTP/1.1 server and client, limits | `tests/http.rs` (done) |
| 112 | TLS (v17) | `std.tls`, `https://`, E0332 | `tests/tls.rs` (done) |
| 113 | `std.crypto` (v17) | SHA-256, Ed25519, random bytes, `equal` | `tests/crypto.rs` (done) |
| 114 | `std.toml` (v17) | TOML 1.0 parser, limits, helpers | `tests/toml.rs` (toml-test v2.2.0) (done) |
| 115 | `fs.rename`, `fs.set_mode`, `flush`, `std.time` (v17) | atomic renames, modes, flushing stdout, the clocks | `tests/fs_time.rs` (done) |
| 116 | `fs.sync` (v17) | `fsync` on files and folders | `tests/fs_time.rs` (done) |
| 117 | Octal literals (v17) | `0o600`, W0107 | lexer units (done) |
| 118 | `and_then` (v17) | `Option.and_then`, `Result.and_then` | `tests/programs/errors/and_then` (done) |
| 119 | `request_tls_with` (v17) | `http.request_tls_with`: limits and TLS options | `tests/tls.rs` (done) |
| 120 | `hkdf_sha256` (v17) | `crypto.hkdf_sha256`: RFC 5869 key derivation | `tests/crypto.rs` (done) |
| 122 | Joining arrays (v17) | `xs + ys`, `xs += ys`, `s += t`, arrays satisfy `Add` | `tests/programs/collections/joining` (done) |
| 123 | Shared member names (v17) | a method named like a field or, with `self`, a variant; E0310 reworded | `tests/programs/types/shared_member_names` (done) |
| 124 | Shared test builds (v17) | `lughac test` builds a package folder's test files once | `tests/test_groups.rs` (done) |
| 125 | Struct results through a pointer (v17) | `sret` results for Lugha functions; no language change | `tests/codegen.rs`, codegen units (done) |
| 126 | Struct values split into fields (v17) | fields for small structs, `memcpy` and field loads for big ones, `zeroext` `bool` results; no language change | `tests/codegen.rs`, codegen units (done) |
| 127 | The C object cache (v17) | `[link]` C files' and the runtime's objects kept between builds | `tests/c_cache.rs` (done) |
| 128 | Included files (v17) | `include_bytes`, `include_str`, E0447, E0448 | `tests/programs/modules/includes/`, `tests/includes.rs` (done) |
| 129 | A test folder from `lughac init` (v17) | `test/` with one passing test in both layouts, `src/greeting.la` for a program | `tests/init.rs` (done) |
| 130 | Portable x86-64 executables (v17) | the baseline `x86-64` by default, `--cpu` | `tests/codegen.rs` (no `popcnt` by default), `tests/cli.rs`, `tests/test_runner.rs` (done) |
| 131 | The collector built in (v17) | bdwgc 8.2.12 compiled from embedded source, linked statically; no `-lgc` | `tests/programs/runtime/collector/`, `tests/c_cache.rs`, `tests/cli.rs`, `tests/tasks_runtime.rs` (done) |
| 132 | Choosing the C compiler (v17) | `$LUGHA_CC` for every C compile and the link | `src/link/compiler.rs`, `tests/c_cache.rs`, `tests/cli.rs` (done) |
| 133 | Linux on ARM64 (v17) | `--target aarch64-unknown-linux-gnu`, the AArch64 stack switch, E0449 | `tests/arm.rs` under qemu, `tests/cli.rs` (done) |
| 134 | ARM64 struct passing (v17) | extern structs to and from C by value as AAPCS64 passes them; E0449 retired | `tests/arm.rs` (the `extern_structs` shapes against `aarch64-linux-gnu-gcc`), `src/codegen/c_abi_arm.rs` (done) |
| 135 | Small stack frames (v17) | stack slots shared between match arms, branches and blocks; stack probes, so any overflow panics | `tests/programs/runtime/small_frames/`, `src/codegen/frames.rs` (done) |
| 136 | Finer slot sharing (v17) | slots used one after another inside a block share | `tests/programs/runtime/slot_sharing/`, `src/codegen/frames.rs` (done) |
| 137 | `std.json` (v17) | strict JSON parsing and writing, limits, helpers | `tests/json.rs` (JSONTestSuite) (done) |
| 138 | JSON in `std.http` (v17) | `req.json()`, `res.json()`, `json(status, v)`, `post_json` | `tests/http.rs` (done) |
| 139 | `json.encode` (v17) | any data type to a `json.Value`, the `json.Json` bound, E0450 | `tests/programs/std/json_encode/` (done) |
| 140 | `json.decode` (v17) | a `json.Value` to any data type, `DecodeError` with paths; `?` passes its expected type on | `tests/programs/std/json_decode/` (done) |
| 141 | Derive blocks (v17) | `$derive I for X { … }` in bundled sources; `json.encode`/`decode` written in Lugha on them, unchanged | `tests/programs/std/json_encode/`, `m140/`, `m141/` (done) |
| 142 | Derive blocks for every module (v17) | `derive I for X { … }`, `derive.fields(…)` and the rest, a type's own items override; custom JSON per type; E0451–E0453 | `tests/programs/derive/every_module/` (done) |
| 143 | Hashing on derive blocks (v17) | the prelude's `$Hash`, implied by `Eq`; `derive.fold`, `derive.index`, `derive … for handle`, `derive.address`; equal maps and sets hash equal | `tests/programs/derive/hashing/` (done) |
| 144 | `==` through `Eq` (v17) | `Eq` a derivable prelude interface with `eq` and `hash`, a type's own pair overriding both; `derive.all`; E0451 at `==`, E0454 | `tests/programs/interfaces/eq/` (done) |
| 145 | Calendar dates and the local offset (v17) | `time.civil`, `time.from_civil`, `time.local_offset`, `Civil`, `TimeError` | `tests/programs/std/calendar/` (done) |
| 146 | File streams (v17) | `fs.open`, `fs.create`, `fs.append`, `File` with `read`, `read_line`, `write`, `write_text`, `close` | `tests/programs/std/file_streams/` (done) |
| 147 | Constants computed by pure code (v17) | a `const`'s value any pure expression, calls included; purity found through calls at real type arguments; E0445 with the path; E0446 through functions | `tests/programs/constants/pure/` (done) |
| 148 | Inference through result methods (v17) | a waiting generic call typed through a method whose result mentions its receiver's type parameters: `json.decode(v).unwrap()`, `.expect(…)`, own methods | `tests/programs/generics/result_inference/` (done) |
| 149 | Handing over arguments (v17) | kept parameters, `x = f(x)` as a last use, the owning instance `lugha_fn_<symbol>.$own` called by direct calls | `tests/programs/runtime/handing_over/` (done) |
| 150 | Inference through iterator chains (v17) | `collect`/`collect_set`/`collect_map`'s expected element passed back through `filter`, `take`, `skip`, `take_while`, `skip_while` to `map`'s closure | `tests/programs/generics/chain_inference/` (done) |
| 151 | `defer` (v17) | `defer e;` and `defer { … }` at every exit of their block but a panic, last first; E0455, E0456; `defer` a keyword | `tests/programs/control/defer/` (done) |
| 152 | `loop` and labels (v17) | `loop { }` as an expression (`never` without a `break`), `break value`, labels on `loop`, `while` and `for` for `break` and `continue`; E0512–E0514; `loop` a keyword | `tests/programs/control/loop_and_labels/` (done) |
| 153 | `fs.size` (v17) | a regular file's size from `stat(2)` without opening it; a folder or other kind is `Err` | `tests/programs/std/fs_size/` (done) |
| 154 | Semantic tokens (v17) | `lughac lsp` classifies every token, names by what the check resolved; full documents only; standard legend | `tests/lsp_semantic_tokens.rs` (done) |
| 155 | Indexing your own types (v17) | `a[i]` through `index`, `a[i] = v` through `set_index`, `a[x, y]` as `a[(x, y)]` for every type | `tests/programs/interfaces/indexing/` (done) |
| 156 | Read-modify-write through `[ ]` (v17) | map entries as places, changed in place; `g[i] += v`, `g[i].x = v`, `g[i].push(x)` through `index` and `set_index` | `tests/programs/interfaces/read_modify_write/`, `tests/programs/collections/maps_and_sets/entry_*` (done) |
| 157 | Write-back on a loop's exits (v17) | `for x of mut g[i]`, `swap(g[i], b)`, `out g[i]` through `index`, handed back on every exit but a panic | `tests/programs/interfaces/read_modify_write/loops.la`, `tests/out_params.rs` (done) |
| 158 | Several right-hand types (v17) | `add`…`rem` declared once per right-hand type, chosen by the operand's exact type; E0457 | `tests/programs/interfaces/mixed_arithmetic/` (done) |
| 159 | A number or another module's type on the left (v17) | `impl f64 { fun mul(self, v: V): V }`, `impl shapes.W { … }` beside the right-hand type; E0458 | `tests/programs/interfaces/mixed_arithmetic/left/`, `e0458.la` (done) |
| 160 | Sealing and key exchange (v17) | `crypto.chacha20poly1305_seal`/`_open`, `x25519_keypair`/`_public`/`_shared`; `CryptoError.NotAuthentic`, `BadKey` | `tests/crypto_seal.rs` (done) |
| 161 | TLS by key (v17) | `tls.listen_keys`, `connect_keys`, `Conn.peer_key`, `Listener.set_accept`: mutual TLS 1.3 with raw Ed25519 keys; OpenSSL 3.2 the floor | `tests/tls_keys.rs` (done) |
| 162 | HPKE (v17) | `crypto.hpke_seal`, `hpke_open`: RFC 9180 base mode, X25519/HKDF-SHA256/ChaCha20-Poly1305 | `tests/crypto_seal.rs` (done) |
| 163 | UDP (v17) | `net.udp_bind`, `udp_bind_shared`, `send_to`, `recv_from`, `join`/`leave`, multicast options, `interfaces()` | `tests/udp.rs` (done) |
| 164 | Nominal interfaces (v18) | `impl I for T`, qualified calls `I.m(x)`, E0459–E0461; E0430 retired | `tests/programs/interfaces/nominal` (done) |
| 165 | Deriving on request (v18) | `with I, J`, `impl I for T = derive;`, hand-written impls of derivable interfaces, `json.Json`'s `missing`, defaults on associated functions, E0462–E0463 | `tests/programs/derive/with` (done) |
| 166 | `mut` parameters (v18) | `fun f(mut x: T)`, `f(mut place)`, through bounds and interface values; E0443 and E0508 widened | `tests/programs/functions/mut_params` (done) |
| 167 | `Eq` and `Key` (v18) | `Eq` and `Key` asked for, `interface I: J`, `Hasher`, `assert_key_laws`; E0464; E0441 and E0454 retired | `tests/programs/interfaces/eq_key` (done) |
| 168 | `Ord` (v18) | `Ord: Eq` asked for or written, tuples through a derive block, floats out of `Ord`, `f64.total_order`, `f32.total_order` | `tests/programs/interfaces/ord` (done) |
| 169 | Built-in implementations (v18) | `impl I for f64`, `impl I for Point[]`, `impl<T: B> I for T[]`; method syntax, qualified calls and interface values on built-in types | `tests/programs/interfaces/builtin_impls` (done) |
| 170 | Printing (v18) | `Debug` for every type, `Display` by hand, `x.debug()`, `debug(x)`; E0465 | `tests/programs/text/printing` (done) |
| 171 | Operator interfaces (v18) | `Add<R = Self>` … `Neg` through blocks, the result from the block, interface parameter defaults, E0466; E0458 retired | `tests/programs/interfaces/operators` (done) |
| 172 | The prelude's methods as blocks (v18) | `impl string`, `impl<T> T[]`, `impl char`, `impl<T> Option<T>`, `impl Result<T, E>` in the prelude; the helper table removed | `tests/programs/collections/prelude_methods` (done) |
| 173 | `@caller_location` (v18) | marked functions and methods report their caller's place; `unwrap`, `expect` and the assertions in the prelude | `tests/programs/errors/caller_location` (done) |
| 174 | Sizes (v18) | `kb()`, `mb()`, `gb()` on integers, in powers of 1024 | `tests/programs/numbers/sizes` (done) |
| 175 | Discarded `Result` (v18) | an expression statement of type `Result` is E0467; `let _ = …;` drops one | `tests/programs/errors/discarded_result` (done) |
| 176 | Dotted variants (v18) | `.Some(x)`, `.None`, `.Circle(r)` in every pattern; a bare name always binds; E0212, E0468 | `tests/programs/patterns/dot_variants` (done) |
| 177 | Type arguments and lossless `as` (v18) | `max<i64>(a, b)`, `Chan<Job>.new(4)`, `Pair<u8> { … }`, `Option<i64>.None`; `as` only without loss; `try_into`, `truncate`, `saturate`, `round`, `floor`, `ceil`; E0469 | `tests/programs/generics/type_arguments`, `tests/programs/numbers/conversions` (done) |
| 178 | Collection literals (v18) | `Map { "a": 1 }`, `Set { 1, 2 }`, `Map {}`; `FromEntries` and `FromItems`; `#{…}` removed; E0213, E0470 | `tests/programs/collections/literals` (done) |
| 179 | `Error` and extending interfaces as values (v18) | `interface Error: Display`, `impl Error for string` and std's errors; an interface that extends others is a value type; E0464 retired | `tests/programs/interfaces/extending_values` (done) |
| 180 | Capabilities, first step (v18) | `main` takes `string[]`, `Console`, `Fs`; `Console`, `Fs`, `Dir`; `print`, `println`, `eprint`, `eprintln`, `read_line`, `read_file`, `write_file`, `append_file`, `args`, `exit` removed (E0471) | `tests/programs/capabilities/console_fs` (done) |
| 181 | Capabilities, second step (v18) | `Clock`, `Env`, `Proc` (with `ProcessOutput`) and `Entropy`, taken by `main` and tests; `Dir.path`; `std.env` and `std.process` removed; `std.time`'s clocks, `random.from_os` and `crypto.random_bytes` removed; key pairs take `Entropy` | `tests/programs/capabilities/clock_env_proc_entropy/` (done) |
| 182 | Capabilities, third step (v18) | `std.fs` onto `Dir` (`list`, `make_dirs`, `size`, `rename`, `open`, …, byte files); `File` a prelude interface; `Console.read_bytes`, `flush`; `read_bytes`, `write_bytes`, `append_bytes`, `read_stdin_bytes`, `flush` are E0471; `std.fs` removed | `tests/programs/capabilities/dir_files/` (done) |
| 183 | Capabilities, fourth step (v18) | `Net`; `Listener`, `Conn`, `Udp` prelude interfaces, `NetInterface`; `std.net` removed; `std.tls` and `std.http` take a `Net` | `tests/programs/capabilities/net/` (done) |
| 184 | TLS over any `Conn` (v18) | OpenSSL on memory BIOs; `tls.client`, `client_keys`, `server`; `SecureConn`, `SecureListener`, `accept_secure`; PEM text for certificates, keys and CAs | `tests/tls_over_conn.rs` (done) |
| 185 | Tests' capabilities (v18) | a plain test's stand-ins (`Dir`, `Clock`, `Env`, `Entropy`, `Net`); `@test(integration)` and `lughac test --integration` | `tests/test_capabilities.rs` (done) |
| 186 | `Dir` confinement (v18) | `openat2` with `RESOLVE_BENEATH \| RESOLVE_NO_MAGICLINKS` for every path of every `Dir`, `fs.cwd()`'s too; `fs.dir`, `dir.dir`, `dir.read_only`; `runtime/dir.c` | `tests/programs/capabilities/dir_confinement/`, `tests/dir_race.rs` (done) |
| 187 | Narrowing (v18) | `net.only` (`OnlyNet`, `allow_private`; public addresses by `lugha_rt_net_private`, one lookup), `connect_only`, `listen_only`, `env.only`, `console.stdout_only`/`stderr_only` | `tests/programs/capabilities/narrowing/` (done) |
| 188 | The `ffi` permission (v18) | `allow = ["ffi"]` in the program's manifest, covering dependencies; E0333; `lugha.lock`'s `[ffi]` table; `lughac update`'s new-reach line | `tests/ffi_permission.rs` (done) |
| 189 | Process lockdown (v18) | seccomp (`io_uring`; sockets without `Net`; processes without `Proc`) and Landlock (files without `Fs`, beyond what held capabilities read), inherited by started programs, best effort, `LUGHA_LOCKDOWN=report` | `tests/lockdown.rs` (done) |
| 190 | Reference counting, single-task prototype (v18) | `--memory=rc` / `LUGHA_MEMORY=rc`: a two-word header (drop, count) on every object, counts at copies and stores, releases at statement and block ends, moves on last use, copy-on-write by count, immortal constants; `LUGHA_RC_REPORT`, `LUGHA_RC_POISON`; no tasks, channels, actors or callbacks yet; Boehm stays the default | `tests/rc_programs.rs` (done) |
| 191 | Reference counting across tasks (v18) | under `--memory=rc`: a count's shared bit (atomic counts), marking on send, spawn, actor messages and joins (a work list, not the stack); each object's visit releases or marks; tasks, channels and actor handles counted; unfinished threads let go when `main` returns; `chanping` | `tests/rc_programs.rs`, `tests/programs/concurrency/rc_sharing/` (done) |
| 192 | The memory measurement (v18) | under `--memory=rc`: free lists by size class, releases after a local's last mention, `if` and `match` values handed through; for both models, enum payloads addressed so LLVM keeps them in registers and the runtime compiled at `-O2`; `LUGHA_CALL_COSTS=1` simulates later phases' per-call costs; `bench/measure/pauses.la`; the decision written up in `bench/RESULTS.md` | `tests/rc_programs.rs`, `bench/RESULTS.md` (done) |
| 193 | Reference counting, the only memory model (v18) | the Boehm collector, `--memory` and `LUGHA_MEMORY` removed; callbacks hold their closure's environment until released; `Dir` and hasher handles counted, closed and freed with their last holder; temporaries taken where made and fresh arguments need no slot, `match` arms that leave the matched local alone borrow their bindings, releases read only counted fields | `tests/leaks.rs`, `tests/programs/capabilities/dir_lifetime/`, `tests/callbacks.rs` (done) |
| 194 | Unwinding (v18) | a panic unwinds its task: each frame runs its pending `defer`s and lets go of what it holds, through landing pads on the calls that need them (`__gcc_personality_v0`, a forced unwind ending at the task's start); actors end after unwinding, spawned tasks end the program; a second panic, a stack overflow and C frames don't unwind | `tests/programs/control/unwinding/`, `tests/leaks.rs`, `tests/callbacks.rs` (done) |
| 195 | Scopes and cancellation (v18) | `scope s { … }` as `Result<T, Failure>`; `s.spawn(call)`, bare `spawn` for actors only; `Scope`, `Failure`, `Stopped`, `Reason`; `join()` as `Result<V, Stopped>`, `cancel`, `name`; all-or-nothing failure; cancellation unwinding at pause and preemption points (socket waits included); early exits cancelling with `.Parent`; the root scope `main` and tests take; second-class handles; E0472–E0474 | The scopes program in section 10 passes; `tests/programs/concurrency/scopes/` (done) |
| 196 | Function types as interfaces (v18) | `fun(A): R` as a bound, a required interface and an `impl` target; every function value its own type, implementing its signature's interface; one instance per function passed to a function-bounded parameter, called directly; structs callable through `call` | `tests/programs/functions/as_interfaces/` (done) |
| 197 | The sendable rule (v18) | `auto interface Sendable` and its derive blocks; checks at spawned calls (arguments, captures, result), channels, actors, conversions and `impl`s; capabilities and `Error` require `Sendable`; `T: Sendable`; `std.http`'s `Handler`; the library's handles on runtime cells; E0475, E0476 | `tests/programs/concurrency/sendable/` (done) |
| 198 | Scope policies (v18) | `independent`, `independent(f)`, `limit n`; `scope<E>` and `Failure<E>`; `Failure` a struct with the task's name; task names; a scoped task's entry catching its own unwind; a root task's failure printed in full; E0477, E0478 | `tests/programs/concurrency/scope_policies/` (done) |
| 199 | Time and shutdown (v18) | `within ms`, `until t`, `s.remaining()`; `.Deadline` failures and reasons; deadlines nesting; `shield within ms { … }`, a shield in a `defer` cut off at its deadline; Ctrl-C cancelling the root scope with `.Shutdown`, exit 130; E0211 for `shield` without `within` | `tests/programs/concurrency/deadlines/`, `tests/interrupt.rs` (done) |
| 200 | Channels with two ends (v18) | `Chan<T>.new(n)` and `Chan<T>.unbounded()` giving `Sender<T>` and `Receiver<T>`; `try_send` and `try_recv` as `TrySend`/`TryRecv`; `for x of rx`; `s.merge`; `select` on the ends; deadlock reports naming channels' places | `tests/programs/concurrency/channels/` (done) |
| 201 | Actors in scopes (v18) | `s.spawn(Counter { … })`; scopes stopping their actors after their tasks, cancelling them, and reporting their panics; `join` gone; bounded mailboxes, `mailbox n`; E0474 for an actor | `tests/programs/concurrency/actors/` (done) |
| 202 | Generic actors and actors as interface values (v18) | type parameters on actors, bounded `Sendable`; handles with type arguments, handlers per instance; `impl I for SomeActor`; handles converting to `I` and meeting `T: I`; calls through `I` as messages, defaults inside the actor | `tests/programs/concurrency/actors/` (done) |
| 203 | Deterministic mode and virtual time (v18) | plain tests on virtual time; `--shuffle[=SEED]` picking ready tasks and `select` arms from a seed; `FAILED (seed N)`, `"seed"` in `--json`; integration tests on real time | `tests/test_modes.rs`, `tests/tasks_runtime.rs` (done) |
| 204 | Hash seeding (v18) | SipHash-1-3 in `Hasher`, keyed per process from the operating system's randomness, fixed in deterministic runs; strings by 8-byte words and their length; `$hash.key`, `$hash.chunk` | `tests/hash_seed.rs`, `tests/programs/interfaces/eq/` (done) |
| 205 | Stacks and C calls (v18) | pooled 8 MiB task stacks without guard pages; the entry check after optimising and `stack overflow` panics that unwind; `main` a task (`lugha_rt_main`); every C call on the C stack through per-site thunks (`lugha_rt_c_call`); callbacks unable to wait | `tests/programs/control/unwinding/`, `tests/programs/concurrency/scopes/`, `tests/callbacks.rs` (done) |
| 206 | Helper threads (v18) | a pool of helper threads, started on demand after lockdown, for files and folders, `read_line`, name lookups and `Proc`'s `run`; results through the scheduler's `epoll` set; inline in deterministic runs; `LUGHA_HELPERS` | `tests/helpers.rs` (done) |
| 207 | `extern blocking fun` (v18) | `blocking` after `extern`; calls through `lugha_rt_c_blocking` on a helper thread, uncancellable; E0479 for callbacks, E0306 for disagreeing declarations; `lugha_rt_cb_enter` | `tests/blocking_externs.rs`, `tests/programs/c/blocking/` (done) |
| 208 | Groundwork for many cores (v18) | values marked shared by the side handing them over (task results, actor answers, failures, names, reasons, handlers, actor state, cell bytes); constants immortal once filled; `LUGHA_RC_CHECK=1` (the owner check, at build and run); count accesses `unordered` atomics; waiting through a parking state and a compare-and-swap handoff (`runtime/waiting.c`, plain while one worker runs); the scheduler's state in a `Worker` record; a `Ref` print's boxes per task | `tests/rc_check.rs`, `tests/waiting_runtime.rs`, `tests/programs/concurrency/printing/` (done) |
| 209 | An allocator for many threads (v18) | chunks owned by the worker that carved them, per-worker lists, remote frees by compare-and-swap onto the owner's list per class, taken back before carving; chunks taken with one atomic add; the `Worker` record in thread-local storage (`lugha_rt_worker_enter`); the report's counts per worker | `tests/rc_alloc_threads.rs`, with ThreadSanitizer (done) |
| 210 | Tasks on every core (v18) | worker threads (`LUGHA_WORKERS`, one per CPU; one when deterministic), started at the first spawn after lockdown; one run queue and one lock of atomics and a futex over the scheduler's state, held across switches; an idle loop per worker, one poller in `epoll_wait`, the others asleep, woken for surplus work; `main` returned to the main thread; aligned stacks: the entry check `(sp & mask)` and the budget in the stack's top word; a C stack per worker with a guard; `lugha_rt_task_go`; whole-line `println` | `tests/workers.rs` (with ThreadSanitizer) (done) |
| 211 | A scheduler for many cores (v18) | a 256-slot run queue per worker, half stolen by idle workers; a run-next slot (several workers only), three picks in a row at most; a shared queue for overflow and outside wakes, checked every 61 picks; Go's spinning protocol for waking parked workers; one poller; timers in a heap per worker; the lock no longer held across a switch, passed only between fibers resuming inside sections; the parking handoff's atomics on once workers run | `tests/scheduler_stress.rs` (with ThreadSanitizer), `tests/workers.rs` (done) |
| 212 | Locks of their own for the hot paths (v18) | a lock per channel, actor and task record; scopes, cancelling, deadlines, shields and the job room under one scopes lock; waits park holding their object locks, let go of once saved; claim before moving a value; a 32-bit generation in each wait, timers and socket events claiming only theirs; `select` locking its channels in address order | `tests/scheduler_stress.rs`, `tests/waiting_runtime.rs`, `bench/measure/scaling` (done) |
| 213 | Spawning that scales (v18) | a cache of 16 task stacks per worker, their pages kept until the worker parks; a lock per scope over its tasks, counts, owner's wait and `limit n`'s waiter, after the scopes lock and parent before child; a leaf lock over the live-task list; the fiber count atomic | `tests/spawn_stress.rs` (with ThreadSanitizer), `bench/measure/scaling` (done) |
| 214 | Pinned handles and pinned scopes (v18) | `extern type X pinned`, never `Sendable` (E0475), not in a `blocking` extern (E0480); a task touching one pinned to its worker (`lugha_rt_pin_here`); `scope s pinned`, inherited by the scopes inside, the main thread for `main`; a per-worker inbox for pinned threads, no other worker taking from it, replacing `main`'s return home | `tests/pinned.rs`, `tests/scheduler_stress.rs` (with ThreadSanitizer), `tests/programs/c/pinned/` (done) |
| 215 | Callbacks registered against a scope (v18) | a callback with `data` and no result queued as a task of the scope its extern call ran in, `inline` to opt out; its captures `Sendable` (E0475), no `Ref<T[]>` (E0481); a panic in an inline callback giving C a zero value and failing the scope, cancellation held off meanwhile; callbacks stopping at the scope's body's end, their captures let go of; `independent(f)` handlers getting callback tasks' panics | `tests/scope_callbacks.rs`, `tests/callbacks.rs`, `tests/programs/c/callbacks/` (done) |
| 216 | Direct and spawned callbacks (v18) | `direct` or `spawned` before every extern callback's `fun`, a word only there; neither, or `spawned` with a result or without `data`, is E0435; milestone 215's `inline` removed | `tests/programs/c/callbacks/`, `tests/callbacks.rs`, `tests/scope_callbacks.rs` (done) |
| 217 | Callbacks on threads C made (v18) | C threads recognised; a record and guarded C stack per thread from its first callback, freed at its exit (a pthread key); every C entry a shell calling its body through `lugha_rt_cb_run`; waiting there a panic; system allocation there; tasks it makes ready through the shared queue; no deadlock while C may call; `direct` callbacks' captures `Sendable` | `tests/foreign_threads.rs`, `tests/callbacks.rs`, `tests/programs/c/callbacks/` (done) |
| 218 | Rings (v18) | `Ring<T>.new(n)`, `RingSender<T>` with `try_send`, `close`, `is`; a lock-free single writer at a time, overlap a panic; readers an ordinary `Receiver<T>`, woken through the poller's eventfd; `select` moved to `select.c` | `tests/rings.rs`, `tests/scheduler_stress.rs` (with ThreadSanitizer), `tests/programs/concurrency/rings/` (done) |
| 219 | Real-time callbacks (v18) | `realtime fun(…)`; a check of the typed program from each `realtime` callback through the Lugha functions it calls: allocation, waiting, calls it can't follow; parameters C copies refused; E0482 with the chain of calls | `tests/programs/c/realtime/`, `tests/rings.rs`, `src/check/realtime/` (done) |
| 220 | Views of C's memory (v18) | `View<T>`, `MutView<T>`: index, `len`, `for`, `slice`, `to_array`, `copy_from`; the escape rule (E0483); callbacks taking views or, `spawned`, copies (E0484); `p.view(n, f)`; elements integers, floats or plain extern structs (E0409); `c_read`, `c_write` and callback `Ref<T[]>` removed | `tests/c_memory.rs`, `tests/rings.rs`, `tests/programs/c/views/`, `src/check/views.rs` (done) |
| 221 | Guarded views (v18) | `p.view` needs the `ffi` permission outside the program's own code (E0333); an `n` too large for a view's length panics | `tests/ffi_permission.rs`, `tests/c_memory.rs` (done) |
| 222 | One spelling for a generic block (v18) | `impl<T: B> Grid<T>` the only form of a generic block (E0214); `+` between an interface's parents (E0215) | `tests/programs/interfaces/one_spelling/`, `src/parser/impl_forms.rs` (done) |
| 223 | Nominal rules (v18) | interface values convert only along `extends` (E0422); one home for a block of a type built from several program types (E0459); the names syntax uses reserved (E0302) | `tests/programs/interfaces/nominal_rules/` (done) |
| 224 | Small rules (v18) | one loop rule for arrays, maps and sets; `\u{…}` in strings; `Result<(), string>` from four `Dir` methods; E0465 for the program's own `debug` only | `tests/programs/iterators/loop_rule/`, `tests/cli.rs` (done) |
| 225 | Iterators through `impl` (v18) | `for … of`, the helpers and `Iter<T>` values through `impl Iter<T> for …` only; E0411's help; the prelude's iterators as blocks | `tests/programs/iterators/by_impl/` (done) |
| 226 | Review fixes (v18) | a task-free program's first Ctrl-C cancels `main` at its next pause; callbacks without `data` run after their scope; `realtime` registration sets a thread record aside | `tests/interrupt.rs`, `tests/scope_callbacks.rs`, `tests/foreign_threads.rs` (done) |
| 227 | `net.only`'s gaps (v18) | NAT64 and 6to4 judged by their IPv4 address; more ranges never public; `resolve` filtered; an IP address of the host connects after a fresh lookup | `tests/programs/capabilities/narrowing/`, `tests/net.rs` (done) |
| 228 | `Console` input errors (v18) | `read_line()` and `read_bytes()` give `Result`; a line that isn't UTF-8 is an error and skipped; a failed read is never the end of input | `tests/programs/text/io/`, `tests/helpers.rs` (done) |
| 229 | Lock-free callback hold (v18) | a `direct` or `realtime` call with `data` checks and holds its captures through the record's counter, not the callbacks lock; the last of the scope's end and a running call lets go of them | `tests/foreign_threads.rs` (done) |
| 230 | The Lugha skill (v18) | `skill/` (`SKILL.md`, concepts 1–5) built in; `lughac skill [PAGE]`, `--list`, `--search`, `--export`; `lughac spec` removed | `tests/skill.rs`, `tests/cli.rs` (done) |
| 231 | The skill's concepts 6–10 (v18) | collections, interfaces, generics, iterators and capabilities pages, in `SKILL.md`'s index | `tests/skill.rs` (done) |
| 232 | The skill's concepts 11–14 (v18) | tasks and scopes, channels and actors, modules and packages, and C interop pages; multi-file examples (`lugha file NAME`) in the tests | `tests/skill.rs` (done) |
| 233 | The skill's library pages, first batch (v18) | `library/` text, collections, console and files, numbers, time and randomness; coverage of std modules and prelude methods checked; an ambiguous page name is exit 2 | `tests/skill.rs` (done) |
| 234 | The skill's library pages, second batch (v18) | `library/` net, http, tls and crypto, json and toml, environment and processes; the library checks in `tests/skill_library.rs` | `tests/skill.rs`, `tests/skill_library.rs` (done) |
| 235 | The skill's tasks and examples (v18) | `tasks/` (five how-tos) and `examples/` (five programs ending in their `// Output:`); `lugha part`, `lugha test` and `LANG file NAME` blocks checked | `tests/skill.rs`, `tests/skill_tasks.rs` (done) |

In milestones 1 and 2, every value can be treated as `i64`, so codegen can start before the type checker exists. Until milestone 4 provides `lugha_rt_panic`, integer `+`, `-` and `*` wrap, and `/` and `%` stop the program with a trap on a zero divisor or `MIN / -1`. Milestone 3 then replaces that assumption with real types. The process exit code is the low 8 bits of `main`'s result, so milestone tests use results below 256.

The milestone 2 test program. It uses `i32` throughout so it stays valid once milestone 3 adds real types:

```
fun fib(n: i32): i32 = if n < 2 { n } else { fib(n - 1) + fib(n - 2) };

fun main(): i32 {
    let n: i32 = 3;
    let mut total: i32 = 0;
    for i in 0..n {
        total += i;
    }
    fib(10) + total - n
}
```

**Out of scope for v15 so far** (for later, roughly in order; `std.fs` (directories), `std.process`, `std.env` and Unicode follow-ups are planned for v15):
- a default entry point (`lughac run` without a file), workspaces of several packages, and requiring `name`;
- a separate debug form, parsing values back from text, one-element tuples;
- a package registry, `ssh` URLs, branches or commits as requirements; two versions of one package in a build; full version ranges and pre-release versions;
- re-export chains (`m.v.w.X`), glob imports, and a `--package` flag;
- directories, streams and processes;
- shared references and closures that change their captures;
- explicit type arguments and `.len()`;
- green threads and actors on several OS threads, built-in supervisors, generic actors, actors as interface values, bounded mailboxes, catching a plain green thread's panic, and non-blocking input and output other than sockets (v17 milestone 110 made sockets non-blocking; v18 milestone 206 runs the rest on helper threads).

## 12. Open questions

These choices were made provisionally to keep the spec complete. Each is easy to change before implementation starts.

- [x] **Return type syntax.** `fun f(): i32` (TypeScript) was chosen over `fun f() -> i32` (Rust) so that `:` always introduces a type.
- [x] **Cost of array copies.** Decided in v15: copy-on-write with a sticky shared flag (milestone 55), reversing v0's eager deep copies. v0 judged that most copied arrays are written right away and that copy-on-write needs reference counts. Real programs proved otherwise: firefly's autograd copies arrays to read them (`let g = self.grads[i];`), and a sticky flag needs no counts, at the price of one extra copy when an array's last holder writes it. Measured: firefly's MLP and GPT training run 13–35% faster (1 m 43 s to 1 m 30 s for 6000 GPT steps; 20.1 s to 13.8 s for 1000). Each write tests a flag on every array along its path, which slowed loops that only write (a histogram count loop, a matrix multiply) by up to about 50%; since milestone 56 a loop makes such arrays unique once before it starts, which brings them back to within about 10% of eager copies (matmul 131 ms eager, 173 ms with milestone 55, 133 ms with 56). Writing through the place (`ps[i].pos[0] += 1.0`) is still cheapest.
- [x] **Default integer type.** Decided: unannotated integer literals are `i64`. This matches `.len` and array indexes, so loop counters need no casts, and it pushes the overflow trap out to about 9.2 quintillion. Use `i32` explicitly where memory in large arrays matters.
- [x] **Integer overflow.** Decided: always trap, like Swift, rather than wrapping or Rust's debug-only checks. One behavior in every build, at a cost of a few percent. Explicit wrapping operations can come later.
- [x] **Semicolons.** Required, which keeps the parser simple. TypeScript-style automatic semicolon insertion is possible later but adds edge cases.
- [ ] **Editing a box in place** (v16). `r.update(f)` copies the contents out and back, which costs for big contents; `r.with(fun(mut x: T) { … })` could edit in place if the closure can be kept from reaching the same box.
- [ ] **A bounds-checked byte buffer** (v16). The safe answer to pointer-style code over raw bytes, since Lugha has no pointers.
- [ ] **Literal patterns on `f64`.** Excluded from v1: float equality in patterns invites mistakes. Revisit if needed.
- [x] **Swapping buffers.** Decided (v1.1): the `swap(a, b)` intrinsic. Double buffering with `ys = buf;` copies the whole buffer each time, because `buf` is still in use; copy-on-write and move on last use can't avoid that copy (DECISION-010), but `swap(ys, buf)` exchanges the two arrays without copying.
- [x] **Array growth factor.** Decided: capacity doubles, starting at 4. It was adequate on the stack program and the DECISION-010 programs; revisit only with measurements.
- [x] **Instantiation depth.** Decided (v2): 64, large enough for real code and small enough to fail fast (E0420).
- [ ] **`mut` bindings in `match`.** Bindings are borrowed and immutable; no program has needed `mut` ones yet.
- [x] **Expression-bodied functions.** Decided: adopted (section 6). `fun square(x: i64): i64 = x * x;` drops the braces from one-liners. It is a small parser addition and a second way to write the same function, accepted because one-liners are common and it is pure sugar for `{ e }`.
- [ ] **Boxing primitives** (v5). An `i64` inside an interface value is boxed like a struct, so every interface value has one layout. Small values could be stored inline later, an optimisation that changes nothing visible.
- [x] **Default methods on concrete types** (v5). Decided (v10): `c.describe()` works when exactly one visible interface the type satisfies supplies the default; two are E0430. Changed (v18): only interfaces the type implements count, and two are E0460.
- [ ] **Iterating a copy** (v6). `for n of c` leaves `c` unchanged, which suits values but means a half-used iterator can't be resumed after a `break`. A later version could add `for n of mut c`.
- [x] **One argument per type** (v6). A type satisfies `Source<X>` for at most one `X`, since it has one `next`. That suits iterators but rules out an interface such as `From<T>` implemented once per `T`, which would need methods overloaded by type. Decided (v18): each `impl From<X> for T` is its own block, so a type implements a generic interface once per argument list.
- [ ] **`%=`** (v7). The compound assignments still lack `%=`, which v0 left out. Adding it is one line of grammar if programs want it.
- [x] **Unsigned indexes** (v7). Decided (v14): any integer type indexes; below 0 or past the end panics.
- [ ] **Re-exports** (v8). A package can't yet expose another package's items as its own (`public import`); users import both.
- [ ] **A `--package` flag** (v8). Pointing at a package without editing `lugha.toml`, for a one-off build, could come later as a command-line override.
- [x] **Printing errors** (v9). Decided (v12): `unwrap on Err: <error>` prints the error.
- [ ] **Irrefutable `if let`** (v9). `if let x = e` is allowed silently; a warning could come later if it proves a common mistake.
- [ ] **`xs.iter()` copies the array** (v10). It follows the copy rules, so iterating a variable still used later costs a copy; move on last use avoids it otherwise. A borrowing iterator would need shared references.
- [ ] **Unicode text** (v10). `bytes()` is the only way to iterate a string; iterating characters waits for a decision on text encoding.
- [ ] **Checking the program's own version** (v11). The root package may declare a version that nothing reads. A later `lughac` could print it, or check it against a minimum `lughac` version.
- [ ] **Re-exports and visibility levels** (v11). A re-export is always `public`. A package-level re-export (visible to the package's other modules only) could come later if packages need internal façades.
- [ ] **A `show` of another shape** (v12) is ignored silently, so v11 programs stay valid. A later warning could point out `fun show(self): i64`.
- [ ] **A default entry point** (v15). `[package] main = "src/main.la"` and `lughac run` with no file could come later.
- [ ] **Requiring `name`** (v15). A later version could require it once packages have adopted it.
- [ ] **Git packages and `extern`** (v14). A downloaded package may declare `extern` C functions, which a built program calls like any code. A later version could list a package's externs at `lughac update`, so users see what a package can call.
- [ ] **Module paths in printed names** (v12). A type prints with its bare name, so two modules' `Point`s look the same. A debug form could add the path.
- [ ] **Width counts bytes** (v13). Text with multi-byte characters pads as if it were longer; this waits for a decision on Unicode text.
- [ ] **Format specifiers at run time** (v13). A spec must be written in the string. v14's `format` keeps that rule: its template is a literal, checked when compiling.
