---
title: C interop
prerequisites: [values-and-types, functions, structs-and-enums, match-and-patterns, errors, collections, interfaces, generics, capabilities, tasks-and-scopes, channels-and-actors, modules-and-packages]
summary: Calling C: extern functions and types, linking libraries, blocking calls, callbacks, views and the ffi grant.
---

# C interop

## The idea

Lugha calls C directly. An `extern fun` declares a C function with Lugha
types, and the call goes straight to C with no wrapper to write. A C
library is linked by listing it in `lugha.toml`. C code can do anything the
process can, so a package may run C only when the program grants it, and
the program vouches for what C does.

## Example

```lugha
extern fun puts(s: string): i32;
extern fun strlen(s: string): u64;
extern fun sqrt(x: f64): f64;
extern fun abs(n: i32): i32;

fun main(console: Console) {
    puts("hello from C");
    console.println("{strlen("lugha")} {sqrt(2.0)} {abs(-7)}");
}
```
```output
hello from C
5 1.4142135623730951 7
```

## Rules

- **Declaring:** `extern fun name(params): R;`, named exactly as in C, with
  Lugha types for C's:
  - `i8`…`i64`, `u8`…`u64`, `f32`, `f64` and `bool` are the C types of that
    size;
  - `string` is a `const char *` valid during the call;
  - an array `T[]` of numbers is a pointer to its first item, with the
    length passed as its own argument; `mut xs: T[]` lets C write the items.
- **Text from C:** an extern may return `string` (copied, invalid UTF-8
  replaced) or `Option<string>` when C may return `NULL`.
- **Handles:** `extern type Window;` declares an opaque C pointer type, used
  only through externs. `Option<Window>` allows `NULL`.
- **Structs by value:** `extern struct Point { x: f64, y: f64 }` has C's
  layout and may cross to C by value.
- **Linking** a library or C files is set in `lugha.toml`:
  ```toml
  [link]
  libs = ["sqlite3"]
  pkg-config = ["gtk4"]
  c = ["glue.c"]
  ```
- **Blocking calls:** `extern blocking fun` marks a C function that may take
  a while (a database query, a blocking read). It runs on a helper thread
  while its task waits, so other tasks keep running.
- **Callbacks:** a function parameter is a C function pointer, and says how
  C's call runs it:
  - `cb: direct fun(i64, data)` runs during C's call, and may not wait;
  - `cb: spawned fun(i64, data)` starts a task in the registering scope on
    each call, so it may wait; it needs `data` and returns nothing;
  - `cb: realtime fun(…)` is a `direct` callback the compiler checks never
    allocates or waits, for audio and other real-time threads.
  The `data` parameter carries the closure's captures as C's `void *`
  user data, and `release` frees them when C is done. A callback with
  `data` stops when its scope ends.
- **Views:** a callback may take `View<T>` (read) or `MutView<T>` (write) to
  work on C's buffer in place, bounds-checked, without copying. A view never
  outlives the call.
- **C calling Lugha by name:** an `extern fun` with a body is a Lugha
  function C calls, `extern fun add(a: i64, b: i64): i64 = a + b;` (renamed
  with `extern "c_name" fun …`). It takes and gives what an extern does;
  a `string` it returns is C's own copy, freed with `free()`. It runs as a
  task while C waits, so it may use scopes and channels, but it gets no
  capabilities. `lughac build --shared lib.la` makes `liblib.so` (and
  `lib.h`) from a file with such functions and no `main`.
- **The `ffi` grant:** a dependency that declares `extern` items or has a
  `[link]` table builds only with `allow = ["ffi"]` in the program's
  `lugha.toml`.
- **What C must not do:** keep a pointer to Lugha's memory (a string or an
  array) after the call returns. Copy what C must keep.

```lugha
extern type Db;
extern struct Size { w: i32, h: i32 }

extern fun db_open(path: string): Option<Db>;
extern blocking fun db_query(db: Db, sql: string): i32;
extern fun db_close(db: Db);
extern fun window_size(): Size;
extern fun on_row(db: Db, cb: direct fun(i64, data), user: data);
extern fun on_change(db: Db, cb: spawned fun(i64, data), user: data, free: release);
extern fun on_audio(cb: realtime fun(MutView<f32>, data), user: data);

fun main(console: Console) {
    // Declarations only: a real program links the library with [link].
    console.println("declared");
}
```

## Mistakes

- **A callback that doesn't say how it runs** is E0435: write `direct`,
  `spawned` or `realtime` before `fun`.

```lugha error E0435
extern fun on_tick(f: fun(i64, data), user: data);

fun main() {}
```

- **A type C can't take,** such as a Lugha struct or a `Map`, is E0409:
  use an `extern struct` or a handle.

```lugha error E0409
struct Point { x: f64, y: f64 }

extern fun draw(p: Point);

fun main() {}
```

- **A `realtime` callback that allocates** (a string, an array, a capturing
  closure) is E0482, naming what it does.

```lugha error E0482
extern fun on_audio(cb: realtime fun(MutView<f32>, data), user: data);

fun main() {
    on_audio(fun(out: MutView<f32>) {
        let label = "frame {out.len}";
    });
}
```

## Exercises

1. Declare `getpid` from libc (`extern fun getpid(): i32;`) and print the
   process id.
2. Write the externs for a small C library of your own in `glue.c`, link it
   with `c = ["glue.c"]`, and call it.
3. Declare a C function taking a `direct` callback with `data`, and pass it
   a closure that sends each value it gets on a channel.
