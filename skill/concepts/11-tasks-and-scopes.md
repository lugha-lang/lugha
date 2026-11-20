---
title: Tasks and scopes
prerequisites: [values-and-types, functions, structs-and-enums, match-and-patterns, errors, collections, interfaces, generics, capabilities]
summary: Running work at the same time: scope, spawn, join, cancellation, deadlines and failures.
---

# Tasks and scopes

## The idea

A task is a function call running at the same time as the rest of the
program. Every task belongs to a **scope**: `scope s { … }` starts tasks
with `s.spawn(call)`, and the scope doesn't end until all of them have.
So no task outlives the code that started it, and a failure never goes
unnoticed: if one task fails, the scope cancels the others and reports it.
Tasks are cheap (many thousands are fine) and run on all the machine's cores.

## Example

```lugha
import std.task;

fun fetch(name: string, ms: i64): string {
    task.sleep(ms);
    "{name} done"
}

fun main(console: Console) {
    let result = scope s {
        let a = s.spawn(fetch("slow", 30));
        let b = s.spawn(fetch("fast", 10));
        let first = b.join().unwrap();
        let second = a.join().unwrap();
        "{first}, then {second}"
    };
    match result {
        .Ok(text) => console.println(text),
        .Err(f) => console.println("{f.task} failed"),
    }
}
```
```output
fast done, then slow done
```

## Rules

- **`scope s { body }`** runs the body, then waits for every task started
  in it. Its value is a `Result`: `Ok` with the body's value, or `Err` with
  a `Failure` naming the task that failed (`f.task`) and how (`f.kind`).
- **`s.spawn(f(args))`** starts the call as a task and gives a `Task<T>`.
  The arguments are computed first, by the caller.
- **`t.join()`** waits for the task and gives `Ok(value)`, or `Err` with
  why it stopped (`.Cancelled(…)`, `.Failed(e)`, `.Panicked(message)`).
- **All or nothing by default:** when a task panics or returns `Err`, the
  scope cancels the rest and its body, and the scope's value is that
  failure.
- **`scope s independent { … }`** lets tasks fail alone: the others run
  on, and each failure is seen through its `join()`.
- **Cancelling:** `t.cancel("why")` stops one task. A cancelled task stops
  at its next pause (a wait, a sleep, a loop's turn), running its `defer`s.
- **Deadlines:** `scope s within 500 { … }` cancels everything still running
  after 500 milliseconds. `s.remaining()` gives the time left.
  `shield within 100 { … }` holds cancellation off briefly, for cleanup.
- **Pausing:** `task.sleep(ms)` and `task.yield_now()`, from `import std.task;`.
- **`main` may take `root: Scope`,** a scope as long as the program, for
  tasks or actors that live until the end.
- **What crosses to a task must be `Sendable`:** numbers, strings, arrays,
  structs and enums of those, channels' ends, actors and capabilities are; a
  `Ref` isn't, since two tasks would share it.
- **A `Scope` or `Task` is passed only as an argument:** it can't be
  returned, stored in an array or a field, or captured by a closure. To
  collect the results of many tasks, have each send its result on a
  channel (see channels and actors).

```lugha
import std.task;

fun count(n: i64): i64 {
    let mut sum = 0;
    for i in 0..n {
        sum += i;
    }
    sum
}

fun check(n: i64): Result<i64, string> =
    if n < 0 { Err("negative: {n}") } else { Ok(n) };

fun main(console: Console) {
    let total = scope s {
        let a = s.spawn(count(1_000));
        let b = s.spawn(count(2_000));
        a.join().unwrap() + b.join().unwrap()
    };
    console.println(total.unwrap());

    let failed = scope s {
        s.spawn(check(-1));
        task.sleep(1_000);
    };
    console.println(failed.is_err());

    let late = scope s within 20 {
        task.sleep(10_000);
    };
    console.println(late.is_err());
}
```
```output
2498500
true
true
```

## Mistakes

- **`spawn` without a scope** is E0474: open one, or take `root: Scope`.

```lugha error E0474
fun work(): i64 = 1;

fun main() {
    let t = spawn work();
}
```

- **Keeping a `Task` or `Scope` past its call,** in an array, a field or a
  closure, is E0472.

```lugha error E0472
fun work(n: i64): i64 = n;

fun main() {
    let r = scope s {
        let ts = [s.spawn(work(1))];
    };
}
```

- **Sending a `Ref` to a task** is E0475: send a copy of the value, or keep
  the shared state in an actor.

```lugha error E0475
fun bump(r: Ref<i64>) {
    r.update(fun(n) = n + 1);
}

fun main() {
    let r = Ref.new(0);
    let done = scope s {
        s.spawn(bump(r));
    };
}
```

## Exercises

1. Start ten tasks that each sleep a different time and then send their
   number on a channel, and print the numbers in the order they arrive.
2. Use an `independent` scope where one of three tasks panics, and print how
   each ended.
3. Give a scope a 50 ms deadline around a task that sleeps 1 second, and
   print what the scope gives back.
