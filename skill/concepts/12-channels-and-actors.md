---
title: Channels and actors
prerequisites: [values-and-types, functions, structs-and-enums, match-and-patterns, errors, collections, interfaces, generics, capabilities, tasks-and-scopes]
summary: Tasks talking: channels with their two ends, select, and actors that own their state.
---

# Channels and actors

## The idea

Tasks don't share changing data; they pass values. A **channel** carries
values from one task to another: a `Sender` puts them in and a `Receiver`
takes them out, in order. An **actor** is a task that owns some state and
handles one message at a time, so many tasks can use it without ever
seeing it half-changed. Use a channel for a stream of values, and an actor
for state many tasks share.

## Example

```lugha
fun produce(out: Sender<i64>, n: i64) {
    for i in 1..n + 1 {
        out.send(i * i);
    }
    out.close();
}

actor Tally {
    total: i64,
    count: i64,
    fun add(mut self, n: i64) {
        self.total += n;
        self.count += 1;
    }
    fun report(self): string = "{self.count} values, total {self.total}";
}

fun main(console: Console) {
    let (tx, rx) = Chan<i64>.new(4);
    let r = scope s {
        let tally = s.spawn(Tally { total: 0, count: 0 });
        s.spawn(produce(tx, 5));
        for v of rx {
            tally.add(v);
        }
        console.println(tally.report());
        tally.stop();
    };
    console.println(r.is_ok());
}
```
```output
5 values, total 55
true
```

## Rules

- **Making a channel:** `Chan<T>.new(n)` gives `(Sender<T>, Receiver<T>)`,
  holding up to `n` values (0: each `send` waits for a receiver);
  `Chan<T>.unbounded()` has no limit.
- **Sending:** `tx.send(v)` waits while the channel is full. `try_send(v)`
  never waits: `.Sent`, `.Full(v)` or `.Closed(v)`.
- **Receiving:** `rx.recv()` waits for a value: `Some(v)`, or `None` once
  the channel is closed and empty. `for v of rx` loops until then.
- **Closing:** `tx.close()`, or the last `Sender` going away, closes the
  channel. Receivers get what is left, then `None`.
- **No receiver left:** once every `Receiver` of a channel is gone, nothing
  can read it: `send` drops its value instead of waiting, and `try_send`
  gives `.Closed(v)`. A producer whose consumer stopped early still finishes.
- **Both ends are handles:** copying one gives another handle on the same
  channel, so several tasks may send, or receive, on one channel.
- **`select`** waits for the first of several operations:
  ```text
  select {
      v = rx.recv() => …,
      tx.send(x) => …,
      after(100) => …,      // a timeout in milliseconds
      default => …,         // when nothing is ready now
  }
  ```
- **Actors:** `actor Name { fields, methods }` declares one. It starts in a
  scope like a task, `let a = s.spawn(Name { … });`, and the result is a
  handle.
  - A method with no result (`fun add(mut self, n: i64)`) is a message: the
    caller goes on at once.
  - A method with a result (`fun report(self): string`) waits for the
    answer.
  - `a.stop()` ends it after the messages already sent. An actor that has
    ended lets go of its fields, even while handles to it remain.
  - A panic in an actor fails its scope, like any task's.
  - The actor's name as a type means its handle, inside its body too;
    `Self` means its state. So `fun new(): Self = Name { … };` builds a
    state, and a field `peers: Name[]` holds handles of the same actor.
- **Values only:** what goes through a channel or to an actor must be
  `Sendable`, and arrives as the receiver's own copy.
- **A deadlock** (every task waiting on another) ends the program with an
  error naming the waiting tasks.

```lugha
import std.task;

fun ticker(out: Sender<string>) {
    task.sleep(5);
    out.send("tick");
}

fun main(console: Console) {
    let (tx, rx) = Chan<string>.new(1);
    let r = scope s {
        s.spawn(ticker(tx));
        select {
            m = rx.recv() => console.println("got {m.unwrap()}"),
            after(1_000) => console.println("timed out"),
        }
        let (quiet_tx, quiet) = Chan<string>.new(1);
        select {
            m = quiet.recv() => console.println("?"),
            after(10) => console.println("nothing came"),
        }
        match quiet_tx.try_send("a") {
            .Sent => console.println("sent"),
            .Full(_) | .Closed(_) => console.println("not sent"),
        }
    };
}
```
```output
got tick
nothing came
sent
```

## Mistakes

- **`Chan` used as a type** is E0305: the type of an end is `Sender<T>` or
  `Receiver<T>`.

```lugha error E0305
fun take(c: Chan<i64>) {}

fun main() {}
```

- **An actor field that isn't `Sendable`,** such as a `Ref`, is E0475.

```lugha error E0475
actor Store {
    data: Ref<i64>,
    fun get(self): i64 = self.data.get();
}

fun main() {}
```

## Exercises

1. Build a pipeline: one task sends the numbers 1 to 20, a second keeps the
   even ones and passes them on, and `main` prints them.
2. Write an actor `Inventory` holding a `Map<string, i64>`, with `add`,
   `take` (giving `Result<(), string>`) and `count`, and use it from several
   tasks.
3. Use `select` with `after` to wait at most 100 ms for a reply on a
   channel.
