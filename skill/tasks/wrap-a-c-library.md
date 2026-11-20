---
title: Wrapping a C library
summary: Linking C code, declaring its functions and handles, and giving it a safe Lugha API.
uses: [structs-and-enums, errors, c-interop, modules-and-packages]
---

# Wrapping a C library

## Goal

Call a C library from Lugha and hide it behind a small, safe API: callers
use a Lugha type with methods, never a raw handle, and the C object is
always freed. The library here is a tiny one written for the purpose,
`glue.c`, which keeps running statistics; a real one (`sqlite3`, `zlib`) is
linked the same way, by `libs` or `pkg-config` instead of `c`.

## Steps

1. **The C side.** A handle type, a constructor, operations and a free
   function: the usual shape of a C library.

```c file glue.c
#include <stdlib.h>
typedef struct { double sum; long count; double max; } stats;
stats *stats_new(void) { return calloc(1, sizeof(stats)); }
void stats_add(stats *s, double x) {
    if (s->count == 0 || x > s->max) s->max = x;
    s->sum += x;
    s->count++;
}
long stats_count(const stats *s) { return s->count; }
double stats_mean(const stats *s) { return s->count ? s->sum / s->count : 0.0; }
double stats_max(const stats *s) { return s->max; }
void stats_free(stats *s) { free(s); }
```

2. **Link it.** `lugha.toml` beside the program compiles `glue.c` into it.
   For an installed library, write `libs = ["sqlite3"]` or
   `pkg-config = ["sqlite3"]` instead.

```toml file lugha.toml
[link]
c = ["glue.c"]
```

3. **Declare the C functions.** `extern type` names the handle, so a
   `Stats` handle can't be confused with any other pointer. `Option<Stats>`
   is how a C function that may return `NULL` is declared.

```lugha part
extern type CStats;
extern fun stats_new(): Option<CStats>;
extern fun stats_add(s: CStats, x: f64);
extern fun stats_count(s: CStats): i64;
extern fun stats_mean(s: CStats): f64;
extern fun stats_max(s: CStats): f64;
extern fun stats_free(s: CStats);
```

4. **Wrap the handle in a Lugha type.** Callers see `Stats` and its
   methods; the externs stay private to the module.

```lugha part
struct Stats { h: CStats }

impl Stats {
    fun new(): Result<Stats, string> = match stats_new() {
        .Some(h) => Ok(Stats { h: h }),
        .None => Err("out of memory"),
    };
    fun add(self, x: f64) { stats_add(self.h, x); }
    fun count(self): i64 = stats_count(self.h);
    fun mean(self): f64 = stats_mean(self.h);
    fun max(self): f64 = stats_max(self.h);
    fun free(self) { stats_free(self.h); }
}
```

5. **Free it with `defer`,** so it is freed however the function ends.

```lugha part
    let s = Stats.new()?;
    defer s.free();
```

## The whole program

```lugha
extern type CStats;
extern fun stats_new(): Option<CStats>;
extern fun stats_add(s: CStats, x: f64);
extern fun stats_count(s: CStats): i64;
extern fun stats_mean(s: CStats): f64;
extern fun stats_max(s: CStats): f64;
extern fun stats_free(s: CStats);

struct Stats { h: CStats }

impl Stats {
    fun new(): Result<Stats, string> = match stats_new() {
        .Some(h) => Ok(Stats { h: h }),
        .None => Err("out of memory"),
    };
    fun add(self, x: f64) { stats_add(self.h, x); }
    fun count(self): i64 = stats_count(self.h);
    fun mean(self): f64 = stats_mean(self.h);
    fun max(self): f64 = stats_max(self.h);
    fun free(self) { stats_free(self.h); }
}

fun summary(xs: f64[]): Result<string, string> {
    let s = Stats.new()?;
    defer s.free();
    for x of xs {
        s.add(x);
    }
    Ok("{s.count()} values, mean {s.mean()}, max {s.max()}")
}

fun main(console: Console) {
    console.println(summary([3.0, 9.5, 4.5]).unwrap());
    console.println(summary([]).unwrap());
}
```
```output
3 values, mean 5.666666666666667, max 9.5
0 values, mean 0.0, max 0.0
```

## Going further

- Put the externs and `Stats` in a module of their own and make only
  `Stats` and its methods `public`, so no caller reaches the handle.
- Published as a package, the library needs the program's grant:
  `stats = { path = "libs/stats", allow = ["ffi"] }` in `lugha.toml`.
- A slow C call (a query, a blocking read) is `extern blocking fun`, so it
  runs on a helper thread while other tasks go on.
- C calling back into Lugha: `direct`, `spawned` and `realtime` callbacks
  (see `c-interop`).
