---
title: A TCP server
summary: A TCP server with a task per connection, shared state in an actor, timeouts, and a clean stop.
uses: [tasks-and-scopes, channels-and-actors, capabilities, net]
---

# A TCP server

## Goal

Build a line server: each client sends one line and gets it back in upper
case, with the number of requests served so far. Each connection runs in
its own task, the count lives in an actor that every task can reach, slow
clients time out, and the server stops cleanly after a set number of
connections. The program then plays the client too, so it runs anywhere.

## Steps

1. **Listen.** `net.listen` takes `host:port`; port `0` picks a free one,
   and `addr()` tells which.

```lugha part
    let listener = net.listen("127.0.0.1:0").unwrap();
    let addr = listener.addr();
```

2. **Keep shared state in an actor.** Tasks don't share changing data: they
   send the actor messages, and it handles one at a time.

```lugha part
actor Counter {
    served: i64,
    fun next(mut self): i64 {
        self.served += 1;
        self.served
    }
}
```

3. **Handle one connection,** with a timeout so a silent client can't hold
   its task for ever, and `defer` so the connection is always closed.

```lugha part
fun handle(counter: Counter, c: Conn) {
    defer c.close();
    c.set_timeout(5_000);
    let line = match c.read(4096) {
        .Ok(bytes) => string_from_utf8(bytes).unwrap_or("").trim(),
        .Err(_) => {
            return;
        }
    };
    let n = counter.next();
    let w = c.write("{n}: {upper(line)}\n".to_bytes());
}
```

4. **Accept in a loop, a task per connection,** all in one scope: the scope
   doesn't end until every connection's task has.

```lugha part
fun serve(counter: Counter, l: Listener, connections: i64) {
    let r = scope s {
        for _ in 0..connections {
            match l.accept() {
                .Ok(c) => {
                    s.spawn(handle(counter, c));
                }
                .Err(_) => {
                    break;
                }
            }
        }
    };
    l.close();
}
```

## The whole program

```lugha
fun upper(s: string): string = s.chars().map(fun(c) = "{c.to_upper()}").collect().join("");

actor Counter {
    served: i64,
    fun next(mut self): i64 {
        self.served += 1;
        self.served
    }
}

fun handle(counter: Counter, c: Conn) {
    defer c.close();
    c.set_timeout(5_000);
    let line = match c.read(4096) {
        .Ok(bytes) => string_from_utf8(bytes).unwrap_or("").trim(),
        .Err(_) => {
            return;
        }
    };
    let n = counter.next();
    let w = c.write("{n}: {upper(line)}\n".to_bytes());
}

fun serve(counter: Counter, l: Listener, connections: i64) {
    let r = scope s {
        for _ in 0..connections {
            match l.accept() {
                .Ok(c) => {
                    s.spawn(handle(counter, c));
                }
                .Err(_) => {
                    break;
                }
            }
        }
    };
    l.close();
}

fun client(net: Net, addr: string, line: string): Result<string, string> {
    let c = net.connect(addr)?;
    defer c.close();
    c.write("{line}\n".to_bytes())?;
    Ok(string_from_utf8(c.read(4096)?)?.trim())
}

fun main(console: Console, net: Net, root: Scope) {
    let counter = root.spawn(Counter { served: 0 });
    let listener = net.listen("127.0.0.1:0").unwrap();
    let addr = listener.addr();
    root.spawn(serve(counter, listener, 3));
    for line of ["hello", "lugha", "bye"] {
        console.println(client(net, addr, line).unwrap());
    }
}
```
```output
1: HELLO
2: LUGHA
3: BYE
```

## Going further

- Serve for ever: loop on `accept` until it gives `Err`, and stop the
  server from elsewhere with `listener.close()`, which wakes the waiting
  `accept` with `Err("closed")`.
- Read a whole line even when it arrives in pieces: keep reading into a
  buffer until it holds `\n`.
- Encrypt it: `tls.listen(net, addr, cert_pem, key_pem)` gives a listener
  whose connections are encrypted, used the same way (see
  `library/tls-and-crypto`).
- `examples/kv-server.la` is a complete key-value server built this way.
