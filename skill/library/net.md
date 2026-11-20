---
title: Network
summary: Net, Conn, Listener, Udp: connecting, serving, resolving names, UDP, multicast and narrowing.
---

# Network

## Overview

The network is the `Net` capability: `main` takes `net: Net` and passes it
to the code that needs it. TCP connections are `Conn` values, servers
`Listener`s, and UDP sockets `Udp`. Every operation that waits (connect,
accept, read, write) pauses only its own task, so a server handles each
connection in its own task. Every value here is a shared handle: copies use
the same socket, and closing one closes them all.

## Reference

### Net

| Item | What it gives |
| --- | --- |
| `net.connect(addr)` | `Ok(Conn)` to `"host:port"`, trying each address the name resolves to |
| `net.connect_timeout(addr, ms)` | the same, giving up after `ms` milliseconds (`0` waits forever) |
| `net.listen(addr)` | `Ok(Listener)` on `"127.0.0.1:8080"`, `"[::1]:8080"`, `"0.0.0.0:8080"`; port `0` picks a free one |
| `net.resolve(name)` | `Ok(addresses)`: every IPv4 and IPv6 address the name has, as text |
| `net.udp_bind(addr)` | `Ok(Udp)` bound to `addr`; `"0.0.0.0:0"` for any free port |
| `net.udp_bind_shared(addr)` | the same, sharing the port with other sockets (as mDNS's 5353 is) |
| `net.interfaces()` | `Ok(NetInterface[])`: this machine's interfaces |

### Narrowing

| Item | What it gives |
| --- | --- |
| `net.only("api.example.com:443")` | a `Net` that connects to that host and port only, and only at its public addresses; `resolve` likewise filtered |
| `only.allow_private()` | the same, private and local addresses allowed too (for `localhost` and a LAN) |
| `net.connect_only()` | a `Net` that connects and resolves, but never listens or uses UDP |
| `net.listen_only(":8080")` | a `Net` that listens on that port only, and never connects |

### Listener

| Item | What it does |
| --- | --- |
| `l.accept()` | waits for the next connection: `Ok(Conn)`, or `Err("closed")`, `Err("timed out")` |
| `l.addr()` | the address it listens on, as `host:port` (the real port, after port `0`) |
| `l.set_timeout(ms)` | how long `accept` waits; `0` forever |
| `l.close()` | stops listening |

### Conn

| Item | What it does |
| --- | --- |
| `c.read(max)` | `Ok(bytes)`: what has arrived, up to `max` (1 to 16 777 216); `[]` once the other end finished |
| `c.write(bytes)` | `Ok(())` once every byte is written |
| `c.set_timeout(ms)` | how long a read or write waits; `0` forever |
| `c.peer()`, `c.addr()` | the other end's and this end's `host:port` |
| `c.close()` | closes it |

### Udp

| Item | What it does |
| --- | --- |
| `u.send_to(bytes, addr)` | sends one datagram to an IP address and port (it may be lost) |
| `u.recv_from(max)` | `Ok((bytes, sender))`: the next datagram and who sent it (which can be forged) |
| `u.join(group, iface)`, `u.leave(group, iface)` | joins or leaves a multicast group (`"224.0.0.251"`, `"ff02::fb"`) |
| `u.set_multicast_ttl(n)` | how many routers multicast may cross (1 to 255) |
| `u.set_multicast_loop(on)` | whether multicast also reaches this machine |
| `u.set_multicast_interface(iface)` | which interface multicast leaves by |
| `u.set_timeout(ms)`, `u.addr()`, `u.close()` | as for `Conn` |

### NetInterface

| Field | Meaning |
| --- | --- |
| `name` | `"lo"`, `"eth0"`, … |
| `index` | the system's number for it |
| `up`, `multicast` | whether it is up, and can multicast |
| `addresses` | its IPv4 and IPv6 addresses as text |

## Examples

```lugha
// Answers `n` connections, then stops listening.
fun echo(l: Listener, n: i64) {
    for _ in 0..n {
        let c = l.accept().unwrap();
        let got = c.read(1024).unwrap_or([]);
        let w = c.write("echo: ".to_bytes() + got);
        c.close();
    }
    l.close();
}

fun ask(net: Net, addr: string, text: string): Result<string, string> {
    let c = net.connect(addr)?;
    defer c.close();
    c.write(text.to_bytes())?;
    string_from_utf8(c.read(1024)?)
}

fun main(console: Console, net: Net, root: Scope) {
    let server = net.listen("127.0.0.1:0").unwrap();
    root.spawn(echo(server, 2));
    console.println(ask(net, server.addr(), "hello").unwrap());
    console.println(ask(net, server.addr(), "again").unwrap());

    let a = net.udp_bind("127.0.0.1:0").unwrap();
    let b = net.udp_bind("127.0.0.1:0").unwrap();
    a.send_to("ping".to_bytes(), b.addr()).unwrap();
    let (bytes, from) = b.recv_from(100).unwrap();
    console.println("{string_from_utf8(bytes).unwrap()} {from == a.addr()}");
    console.println(net.resolve("127.0.0.1").unwrap().debug());

    let narrow = net.only("example.com:443");
    console.println(narrow.connect("127.0.0.1:22").is_err());
}
```
```output
echo: hello
echo: again
ping true
["127.0.0.1"]
true
```
