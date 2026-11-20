## FEATURE: Close `net.only`'s gaps: IPv6 forms that embed or tunnel to a private IPv4 address and other non-public ranges are refused, `resolve` hides what `connect` would refuse, and connecting to one of the host's checked IP addresses works, so `tls.Options.address` works through it; milestone 227.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
let api = net.only("api.example.com:443");
api.connect("api.example.com:443")?;     // as before: resolve once, connect to a public address
// The host resolving to 64:ff9b::a00:1 (NAT64 for 10.0.0.1): refused, "has no public address".
// 2002:a00:1::1 (6to4 for 10.0.0.1), 2001::/32 (Teredo), 2001:db8::/32 and the
// IPv4 documentation ranges: refused too.

api.resolve("api.example.com")?;         // only the public addresses, unless allow_private()

// tls with a chosen address, through the narrowed network:
let opts = tls.Options { address: "93.184.215.14", ..tls.options() };
tls.connect_with(api, "api.example.com:443", opts)?;
// works when 93.184.215.14 is among the host's public addresses now
```

## CONTEXT
- **Found by:** the v18 spec review (2026-11-20), §5–6 finding 14.
- **Spec:** §6 "The network" (L1561: `net.only`'s list of non-public ranges, "the IPv4-mapped and -compatible forms of private IPv4 addresses"); milestone 121's `tls.Options.address`.
- **Code:**
  - `runtime/resolve.c` `lugha_rt_net_private` (`$os.net_private`);
  - `src/prelude/narrow.la` `OnlyNet` (`connect_timeout`, `resolve`, `interfaces`);
  - `src/std/tls.la` (`opts.address` makes the target `ip:port`).
- **The holes:**
  - `64:ff9b::/96` (NAT64), `2002::/16` (6to4), `2001::/32` (Teredo) and `fec0::/10` count as public, so `64:ff9b::a00:1` reaches 10.0.0.1 through a NAT64 gateway.
  - `OnlyNet.resolve` returns private addresses unfiltered.
  - `connect("ip:port")` is refused because the host is compared as written, so the documented resolve-once path (`tls.Options.address`) can't be used through `net.only`.
  - The spec says compatible forms of *private* IPv4 addresses are refused, but the code refuses every `::a.b.c.d`.
- **Security:** `net.only` is a capability boundary, so this PRP needs your explicit review. No `unsafe`, and no new dependency.
- **Decisions:** none open.

## DESIGN
1. **Judged as the IPv4 address inside:**
   - `::ffff:a.b.c.d` (as today);
   - `64:ff9b::a.b.c.d` (NAT64's well-known prefix, RFC 6052);
   - `2002:AABB:CCDD::/48` (6to4, RFC 3056: the IPv4 address in bits 16–47).
2. **Never public:**
   - IPv6, besides today's: `::/96` (every compatible form, as the code already does), `64:ff9b:1::/48` (local-use NAT64), `100::/64` (discard), `2001::/32` (Teredo), `2001:2::/48` (benchmarking), `2001:10::/28` and `2001:20::/28` (ORCHID), `2001:db8::/32` and `3fff::/20` (documentation), `fec0::/10` (old site-local);
   - IPv4, besides today's: `192.0.2/24`, `198.51.100/24`, `203.0.113/24` (documentation) and `192.88.99/24` (old 6to4 relays).
3. **`OnlyNet.resolve(host)`** gives only the addresses `connect` would use: the public ones, or all with `allow_private()`. A host with none is `Err("host has no public address")`.
4. **`OnlyNet.connect` to an IP literal** (`"93.184.215.14:443"`, `"[2606:4700::1111]:443"`) at its port:
   - it resolves its host once more and connects only if that address is among them and allowed, so the address checked is still the one connected to;
   - any other literal is `Err("this network reaches only host:port")`.
   - This is what `tls.Options.address` sends, so TLS with a chosen address works through `net.only`.
5. **`OnlyNet.interfaces()`** is `Ok([])`, as now: a narrowed network shows no local interfaces. The spec says so.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Refuse tunnels rather than decode them.**
   - Teredo embeds its client's address obfuscated, and the far end of a tunnel is a relay's choice.
   - 6to4 and NAT64 have fixed, documented embeddings, so they are judged by the IPv4 address inside, as `::ffff:` already is.
2. **Documentation and benchmarking ranges are refused:** they never route on the internet, so a host resolving to one is misconfigured or lying.
3. **An IP literal must be among a fresh lookup's answers,** not the answers of some earlier `resolve`: the check and the connection stay one step, so a lookup that changed in between can't be used.
4. **No new error texts:** the existing two cover every refusal.

## REQUIREMENTS
### Must Do
- Design 1–5.
- Spec §6 `net.only` (the full range lists, literal addresses, `resolve`, `interfaces`), §11 row 227, and a "Changes since v17" bullet.
- CHANGELOG.
### Must NOT Do
- Change what `allow_private()` allows, or `connect_only`/`listen_only`.
- Add a lookup to `Net` itself.

## TESTS
- [ ] `tests/programs/capabilities/narrowing/addresses.la`: every new range refused, NAT64 and 6to4 of a public IPv4 address allowed and of a private one refused, edge addresses just outside each range allowed.
- [ ] `tests/programs/capabilities/narrowing/narrowing.la` (the recording fake):
  - [ ] `resolve` filtered, or not with `allow_private`;
  - [ ] a literal among the host's addresses connects, after one lookup;
  - [ ] a literal not among them, or private, is refused;
  - [ ] another port is refused.
- [ ] `tests/net.rs`: `tls`-style `connect` to `127.0.0.1:port` through `net.only("localhost:port").allow_private()` connects.
- [ ] Targeted: `--lib`, `programs`, `net`, `tls`, `spec`, `comments`.

## ACCEPTANCE
- [ ] No address form in the lists reaches a private network through `net.only`.
- [ ] `resolve` and `connect` agree on what is allowed.
- [ ] A chosen address that the host's lookup confirms works, and nothing else does.
- [ ] `cargo fmt --check` and clippy clean; the targeted tests pass.
