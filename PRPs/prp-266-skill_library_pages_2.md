## FEATURE: The Lugha skill's `library/` pages, second batch: the network, HTTP, TLS and cryptography, JSON and TOML, and the environment and other programs; the library checks move to their own test file; milestone 234.

**Status:** approved 2026-11-20.

## OBJECTIVE
```
$ lughac skill --list
…
library/net                  Net, Conn, Listener, Udp: connecting, serving, resolving, UDP and narrowing.
library/http                 std.http: requests, responses, clients, servers and JSON bodies.
library/tls-and-crypto       std.tls for secure connections and std.crypto for hashes, keys and sealing.
library/json-and-toml        std.json and std.toml: parsing, writing, values, and encoding your own types.
library/env-and-processes    Env and Proc: environment variables and running other programs.
```

## CONTEXT
- **Follows** PRP-265: the same page shape and coverage checks.
- **Sources:**
  - the prelude (`net.la`, `capabilities.la`'s `Env`, `Proc` and `ProcessOutput`);
  - std (`http.la`, `tls.la`, `crypto.la`, and `json.la` with its `json_*.la` parts and `toml.la` with its `toml_*.la` parts, whose public items reach programs through `json` and `toml`).
- **Network examples can't reach the internet in the tests.** Examples run over loopback (a server task and a client in one program on `127.0.0.1`), or are checked without running.
- **Decisions:** none open.

## DESIGN
1. **Five pages**, in the library shape:
   1. **net:**
      - `Net`: `connect`, `connect_timeout`, `listen`, `resolve`, `udp_bind`, `udp_bind_shared`, `interfaces`;
      - `net.only`, `allow_private`, `connect_only`, `listen_only`;
      - `Listener`, `Conn` and `Udp`, every method;
      - `NetInterface`.
   2. **http:** `Request`, `Response`, the client functions, `serve` and the handler shape, the response helpers, JSON bodies, and errors.
   3. **tls-and-crypto:**
      - `std.tls`: connect, listen, `Options`, `SecureConn`, `SecureListener`;
      - `std.crypto`: hashes, HMAC, key pairs, signing, key agreement, sealing, `Entropy` for keys.
      - A warning box: never hand-roll crypto, and keep keys out of logs.
   4. **json-and-toml:**
      - `std.json`: `Value`, `parse`, `write`, `encode`/`decode` with `with Json`-style derives, `DecodeError`;
      - `std.toml`: `parse` and its values.
   5. **env-and-processes:** `Env` (`get`, `only`), `Proc` (`run`, `run_with`, `call`), `ProcessOutput`, and "there is never a shell".
2. **`SKILL.md`:** five more library rows.
3. **Tests:**
   - The library checks (shape, std coverage, prelude coverage, contents lists) move from `tests/skill.rs` (482 lines) to a new `tests/skill_library.rs`.
   - The prelude map gains `Net`, `Listener`, `Conn`, `Udp`, `Env` and `Proc`.
   - A page's `modules` may name a std module's internal parts, so coverage includes `json_value`'s and `toml_value`'s public items.

## DECISIONS MADE IN DISCOVERY (mine, for review)
1. **Loopback examples run; internet ones are only checked:** the tests never reach outside the machine. `std.http`'s and `std.tls`'s own test files already cover real exchanges, and the pages show the calls.
2. **`Env` and `Proc` get their own page** rather than joining console-and-files: they are separate capabilities, and a model looking for "run a program" should find one obvious page.
3. **The tls-and-crypto page leads with what to use, not how it works:** the safe defaults (`tls.connect`, `crypto.seal`), with the lower-level calls listed after them.

## REQUIREMENTS
### Must Do
- Design 1–3.
- Spec: §11 row 234, a "Changes since v17" bullet.
- CHANGELOG.
### Must NOT Do
- Change the compiler, the prelude or std.
- Reach the internet from a test.

## TESTS
- [x] `tests/skill_library.rs`: the moved checks, plus coverage of `http`, `tls`, `crypto`, `json` (and its public parts), `toml` (and its), and the new prelude blocks.
- [x] `tests/skill.rs`: every new example runs or checks; the file stays under 500 lines.
- [x] Targeted: `skill`, `skill_library`, `comments`.

## ACCEPTANCE
- [x] Every public item of the covered std modules and prelude blocks appears on its page.
- [x] Every example is checked, and none needs the internet.
- [x] `cargo fmt --check` and clippy clean; the targeted tests pass.
