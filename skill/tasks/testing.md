---
title: Testing
summary: @test functions, assertions, fakes for capabilities, and running them with lughac test.
uses: [functions, interfaces, capabilities, modules-and-packages]
---

# Testing

## Goal

Test code that depends on the clock and the file system without touching
the real ones: write the code against capability interfaces, pass the real
ones from `main`, and fakes or fresh folders from the tests. `lughac test`
runs every test in its own process and reports what failed.

## Steps

1. **Write the code against capabilities,** taking a `Clock` and a
   `ReadDir` (it only reads) as parameters rather than reaching for them.

```lugha part
fun greeting(clock: Clock): string {
    let hour = (clock.now_ms() / 3_600_000) % 24;
    if hour < 12 { "good morning" } else { "good afternoon" }
}
```

2. **A test is a function marked `@test`.** It passes when it returns
   without panicking; `assert_eq` panics with both values when they differ.

```lugha part
@test
fun counts_lines_in_a_fresh_folder(dir: Dir) {
    dir.write_file("a.txt", "one\ntwo\n").unwrap();
    assert_eq(count_lines(dir.read_only(), "a.txt"), Ok(2));
    assert(count_lines(dir.read_only(), "missing.txt").is_err());
}
```

3. **Fake a capability by implementing its interface.** A fixed clock makes
   the time-dependent code give the same answer every run.

```lugha part
struct FixedClock { ms: i64 }

impl Clock for FixedClock {
    fun now_ms(self): i64 = self.ms;
    fun monotonic_ms(self): i64 = self.ms;
    fun local_offset(self, ms: i64): Result<i64, string> = Ok(0);
}
```

4. **What a test is given:** a `dir: Dir` parameter is a fresh, empty
   folder of its own; a plain test's `Clock`, `Env`, `Entropy` and `Net`
   are fixed, empty, seeded and loopback-only. A test that must touch the
   real system is `@test(integration)`, run by `lughac test --integration`.

## The whole program

```lugha test
fun greeting(clock: Clock): string {
    let hour = (clock.now_ms() / 3_600_000) % 24;
    if hour < 12 { "good morning" } else { "good afternoon" }
}

fun count_lines(dir: ReadDir, path: string): Result<i64, string> {
    let text = dir.read_file(path)?;
    Ok(text.lines().len)
}

fun main(console: Console, clock: Clock, fs: Fs) {
    console.println(greeting(clock));
    match count_lines(fs.cwd().read_only(), "notes.txt") {
        .Ok(n) => console.println("{n} lines"),
        .Err(e) => console.eprintln(e),
    }
}

struct FixedClock { ms: i64 }

impl Clock for FixedClock {
    fun now_ms(self): i64 = self.ms;
    fun monotonic_ms(self): i64 = self.ms;
    fun local_offset(self, ms: i64): Result<i64, string> = Ok(0);
}

@test
fun mornings_and_afternoons() {
    assert_eq(greeting(FixedClock { ms: 9 * 3_600_000 }), "good morning");
    assert_eq(greeting(FixedClock { ms: 15 * 3_600_000 }), "good afternoon");
}

@test
fun counts_lines_in_a_fresh_folder(dir: Dir) {
    dir.write_file("a.txt", "one\ntwo\n").unwrap();
    assert_eq(count_lines(dir.read_only(), "a.txt"), Ok(2));
    assert(count_lines(dir.read_only(), "missing.txt").is_err());
}
```

```sh
lughac test                       # every test in the package
lughac test --filter mornings     # tests whose names contain it
lughac test --shuffle             # in a random order, to catch hidden dependencies
lughac test --integration         # the @test(integration) ones too
```

## Going further

- Record what code prints with a fake `Console` whose state lives in an
  actor (capabilities are `Sendable`, so a fake can't hold a `Ref`).
- Test an HTTP handler without a server: build an `http.Request` and call
  the handler function directly.
- Tests run on virtual time: `task.sleep` in a plain test takes no real
  time, so timeouts and retries can be tested quickly.
- A test whose run left `Ref` cycles for the collector gets a `note:` line
  under its result, naming where the boxes were made; it isn't a failure,
  but a `Weak` for the link back usually removes the cycle.
