## FEATURE: Narrowing the network, the environment and the console: `net.only("host:port")` (public addresses only, unless `.allow_private()`), `net.connect_only()`, `net.listen_only(addr)`, `env.only(names)`, `console.stdout_only()` and `console.stderr_only()`; each gives a weaker capability, one-way; milestone 187, in v18's Phase 2. **Decides which addresses a narrowed `Net` may reach: needs the user's security review before any code.**

**Status:** proposed 2026-11-16.

## OBJECTIVE
```lugha
import std.http;

fun fetch(net: Net): string = match http.get(net, "https://api.example.com/v1/status") {
    .Ok(r) => "{r.status}",
    .Err(e) => e,
};

fun main(console: Console, net: Net, env: Env): i32 {
    let api = net.only("api.example.com:443");          // that host and port, public addresses only
    console.println(fetch(api));                         // 200 (or the network's error)
    console.println(api.connect("example.org:443").debug());       // Err("this network reaches only api.example.com:443")
    console.println(net.only("localhost:5432").connect("localhost:5432").debug());
                                                         // Err("localhost has no public address")
    let db = net.only("localhost:5432").allow_private(); // a local service, on purpose
    let server = net.listen_only("127.0.0.1:8080");      // listening on that address only
    console.println(server.connect("example.org:80").debug());     // Err("this network only listens")
    let lang = env.only(["LANG"]);
    console.println(lang.get("HOME").debug());           // None: not on the list
    let out = console.stdout_only();                     // printing only: no stderr, no input
    out.println("done");
    0
}
```

## CONTEXT
- Draft v18 §2.1 rule 6 [CAP-06]: "Narrowing makes a weaker capability from a stronger one, and is one-way": `net.only("host:port")` connections to one host; `net.connect_only()`, `net.listen_only(":8080")` connecting only, listening on one address only; `env.only(["HOME", "LANG"])` those variables only; `console.stdout_only()`, `console.stderr_only()` one output stream. §2.2 [SEC-02, SEC-03], approved: "`net.only` restricts by host name, and refuses private and local addresses unless `.allow_private()` is added"; "Listening and connecting are narrowed separately."
- Follows PRP-215 (`Net`, `Conn`, `Listener`, `Udp` interfaces; TLS and HTTP over a `Net`, PRP-216) and PRP-218 (`Dir` narrowing). Interfaces may have default methods, so narrowing can be written once for every `Net`, `Env` and `Console`, fakes included.
- Code: `src/prelude/net.la`, `net_real.la`, `capabilities.la`, `capabilities_real.la` (the last at 351 lines: the narrowed types go in a new `src/prelude/narrow.la`), `runtime/resolve.c`, `src/check/float_ops.rs`, `src/codegen`, `src/link/mod.rs`.

## DESIGN — for the security review

### Narrowing is a wrapper, written once
Each narrowing is a default method on the interface returning a prelude struct that wraps the capability it came from and implements the same interface, so it works on any `Net`, `Env` or `Console`, a fake one included, and narrows again by wrapping again. Nothing can unwrap one: the struct's field holding the wider capability isn't visible outside the prelude. A function given the narrowed value as a `Net` sees only `Net`'s methods.

### `net.only("host:port")` — `OnlyNet`
- **What it allows:** `connect` and `connect_timeout` to exactly that host (compared without case) and port; nothing else connects (`Err("this network reaches only api.example.com:443")`). `listen`, `udp_bind`, `udp_bind_shared` are `Err("this network only connects to api.example.com:443")`; `resolve` answers only for that host; `interfaces()` is empty.
- **Public addresses only:** a connection resolves the host through the wrapped `Net`, drops every address that isn't public, and connects to the first that is, by its IP literal: the address checked is the address connected to, so a name that resolves to a public address when checked and a private one when connecting (DNS rebinding) can't slip through. No public address is `Err("api.example.com has no public address")`. An IP literal given as the host is checked the same way.
- **Not public**, by a new runtime function `lugha_rt_net_private(ip)` using `inet_pton`: IPv4 `0.0.0.0/8`, `10/8`, `100.64/10` (shared, CGNAT), `127/8`, `169.254/16`, `172.16/12`, `192.0.0/24`, `192.168/16`, `198.18/15` (benchmarking), `224/4` and above (multicast, reserved, broadcast); IPv6 `::` and `::1`, `fc00::/7` (unique local), `fe80::/10` (link-local), `ff00::/8` (multicast), and the IPv4-mapped (`::ffff:a.b.c.d`) and IPv4-compatible forms of the IPv4 ranges, so a private IPv4 address can't come back dressed as IPv6. Text that isn't an IP address counts as not public.
- **`.allow_private()`** gives the same `OnlyNet` with every address allowed. It is a method of `OnlyNet` itself, not of `Net`: only the code that wrote `net.only(…)` can call it, before handing the value on as a `Net`.
- **TLS and HTTP** reach the host through `connect_timeout`, so `http.get(api, "https://api.example.com/…")` works, with the certificate still checked against the host name (PRP-216).

### `net.connect_only()` and `net.listen_only(addr)`
- `connect_only()`: `connect`, `connect_timeout` and `resolve` pass through; `listen` and UDP are `Err("this network only connects")`. No address check: it narrows what may be done, not where.
- `listen_only(addr)`: `listen` only on `addr`, compared as text after normalising (`":8080"` means any host on port 8080, otherwise host and port must match); `connect`, `resolve` and UDP are `Err("this network only listens")`; `interfaces()` passes through.

### `env.only(names)` — `OnlyEnv`
`get(name)` passes through when `name` is on the list, else `None`, exactly as an unset variable.

### `console.stdout_only()` and `console.stderr_only()` — `OneStream`
`stdout_only()`: `print` and `println` pass through, `flush` too; `eprint` and `eprintln` do nothing; `read_line` is `None` and `read_bytes` `[]`, as at the end of input. `stderr_only()` the mirror: `eprint`/`eprintln` pass through, `print`/`println` do nothing. Doing nothing rather than panicking: a library given one stream shouldn't crash the program by logging to the other.

### For the security review, in one list
1. The address classes above, and that a mapped or compatible IPv6 form of a private IPv4 address is private.
2. Resolve, check, then connect to the checked IP literal (no second lookup).
3. Host compared without case, port exactly; one host per `only`; `allow_private` only on the concrete `OnlyNet`.
4. Narrowings bind Lugha code only; C code with `ffi` isn't bound (process lockdown is a later PRP).
5. The one new C function, `lugha_rt_net_private`, in `runtime/resolve.c`: `inet_pton` on the text, then the range checks; no lookup, no socket.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Default methods, not core methods:** fakes get narrowing for free and can't get it wrong.
2. **`only` takes one `"host:port"`**; a program needing two hosts narrows twice and passes two values.
3. **The narrowed types are prelude structs** `OnlyNet`, `ConnectNet`, `ListenNet`, `OnlyEnv`, `OneStream`, in `src/prelude/narrow.la`, visible by name only as far as their methods: their fields stay the prelude's.
4. **Error messages** as written above.

## REQUIREMENTS

### Must Do
- C: `lugha_rt_net_private(ip) -> i64` (1 not public, 0 public) in `runtime/resolve.c`; the checker's and codegen's built-in tables; the link list.
- Prelude: the five default methods and their structs (`narrow.la`).
- Spec (capabilities paragraph; "The network"; §11 row 187; Changes), CHANGELOG.

### Must NOT Do
- The `ffi` permission and process lockdown.
- Narrowing `Fs` beyond PRP-218, `Clock`, `Proc` or `Entropy` (the Draft names none).
- More than one host per `only`, or wildcards.

## TESTS
- [ ] Unit (`runtime` through a program): every address class above, public and private, IPv4, IPv6, mapped and compatible forms, and non-addresses.
- [ ] `tests/programs/capabilities/narrowing/` with a fake `Net` that records what it is asked: `only` connecting to its host by the public IP the fake's `resolve` gave, refusing other hosts and ports, refusing a host resolving only to private addresses, refusing a rebinding fake whose second lookup would differ (only one lookup happens), `allow_private`, `connect_only`, `listen_only` (`":8080"` and a full address), narrowing twice; `env.only`; `stdout_only`/`stderr_only` (output checked).
- [ ] `tests/net.rs`: `listen_only` and `connect_only` over the real loopback; `only("localhost:…")` refused, then allowed with `allow_private`.
- [ ] HTTP over an `only` fake (`http.get`).
- [ ] Targeted runs: `--lib`, `programs`, `net`, `http`; CI the rest.

## ACCEPTANCE
- [ ] The objective's narrowed values refuse what their comments say.
- [ ] No narrowed `Net` connects anywhere but its host's public addresses (unless `allow_private`), and none listens or connects beyond what it was narrowed to.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean; the targeted tests pass
- [ ] Every `pub` item documented; spec, CHANGELOG and CLAUDE.md updated where they change
