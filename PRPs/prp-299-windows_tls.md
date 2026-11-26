## FEATURE: TLS and crypto on Windows, trusting what Windows trusts — milestone 272

## OBJECTIVE

`std.tls`, `std.http` and `std.crypto` build for Windows and work there as on Linux and macOS. A TLS client trusts exactly what Edge, `curl.exe` (Schannel) and Go programs trust on that machine: the chain and host name go to Windows' own verifier, which reads the certificate stores, so a root a company pushes by Group Policy is trusted and one Microsoft distrusts isn't.

```lugha
import std.tls;
// Built on Linux with --target x86_64-pc-windows-gnu, run on Windows:
let conn = tls.connect(net, "github.com:443", tls.options())?;     // Windows' verifier says yes
// Today: E0334, std.tls needs an OpenSSL built for the target.
```

Nothing changes on Linux or macOS, or in `std.tls`'s API.

🔒 **Security-sensitive** (which certificates a program accepts; cryptography's build): this needs your review before any code.

## CONTEXT

- **Read first:**
  - PRP-290 (an OpenSSL built for another target, `LUGHA_OPENSSL_<TARGET>`) and PRP-291 (the Mac's verifier), which this follows step for step;
  - `runtime/os/darwin/trust.c`: `os_tls_trust`, the hook `runtime/openssl/tls_trust.c` calls on a helper thread when `OS_TLS_TRUST` is 1;
  - `tests/macos/openssl.sh`, `tests/macos/certs.sh`, `tests/macos/openssl/{roots,trust}.la`;
  - CI's `build` job (the Mac's OpenSSL, cached) and `macos` job.
- **What exists:** TLS talks through memory BIOs over Lugha's own sockets, so it needs no Windows socket code. Only the build and the trust question are new.
- **Follows:** PRP-298 (milestone 271).

## RESEARCH (summary)

- **Go** (`crypto/x509/root_windows.go`): `CertGetCertificateChain` with the server's intermediates in an in-memory store, then `CertVerifyCertificateChainPolicy(CERT_CHAIN_POLICY_SSL)` with the server name. No revocation checks. Extra roots: Go verifies against them alone, without the system.
- **rustls-platform-verifier:** the same two calls, with a custom chain engine for extra roots. End-certificate revocation checked, but an unreachable or unknown answer passes ("soft fail").
- **Chrome on Windows:** its own verifier, no online revocation checks (CRLSets).
- **OpenSSL on MinGW:** `./Configure mingw64`, built with zig as the Mac's is. A static OpenSSL there needs `ws2_32`, `crypt32` and `advapi32` linked after it, all shipped with every Windows.
- **Wine:** its `crypt32` implements both calls and reads the Linux machine's roots, so the trust cases run under Wine as well as on CI.

## REQUIREMENTS

### Must Do

**OpenSSL for Windows**
- `tests/macos/openssl.sh` gains a recipe for `x86_64-pc-windows-gnu` (zig's `x86_64-windows-gnu`, OpenSSL's `mingw64`). The same pinned release and SHA-256 check.
  - The script moves to `tests/openssl.sh`, as it now serves three systems; the Mac's CI step follows.
- A Windows build linking OpenSSL adds `-lcrypt32` after it (`src/link/zig.rs`); `ws2_32` and `advapi32` are already linked.
- CI's `build` job builds and caches it as it does the Mac's, and names it with `LUGHA_OPENSSL_X86_64_PC_WINDOWS_GNU`. The program suite's TLS and crypto cases then build for Windows rather than being skipped with E0334.

**Windows' verifier** (`runtime/os/windows/trust.c`, `OS_TLS_TRUST` 1):
- `os_tls_trust(host, certs, …)`, the same contract as the Mac's: DER certificates in, 0 or a reason out, run on a helper thread.
- The server's chain goes into an in-memory store; `CertGetCertificateChain` builds a chain from its first certificate with that store as extra material.
- `CertVerifyCertificateChainPolicy(CERT_CHAIN_POLICY_SSL)` checks it, with `AUTHTYPE_SERVER` and the host name: a name, or an IP address as text.
- **Extra roots (`ca_pem`) join the system's, never replace them,** as on the Mac. When the system's roots don't reach the chain's top, it is built again with a chain engine trusting only the extra roots (`hExclusiveRoot`), and that answer counts.
- **Revocation:** not checked online, as Go and Chrome don't. Windows still refuses what it has marked distrusted in its own stores.
- **Reasons** map to the Mac's, so a refusal reads as on Linux: `CERT_E_CN_NO_MATCH` → host mismatch, `CERT_E_EXPIRED` → expired (or not yet valid, by the dates), `CERT_E_UNTRUSTEDROOT` and `CERT_E_CHAINING` → no issuer, `CRYPT_E_REVOKED` → revoked, anything else → refused.
- `crypt32` is linked into every Windows program that loads `std.tls`, never into others.

**Tests and CI**
- `tests/windows/certs.sh` (or the Mac's, shared): the same certificates the Mac's trust case uses.
- `tests/windows/openssl/{roots,trust}.la` with `.stdout`, the Mac's cases: a server signed by the program's own CA is trusted with `ca_pem`, refused without it; self-signed, expired and wrong-IP certificates are refused with Linux's words.
  - Run by `tests/windows.rs` under Wine when `LUGHA_OPENSSL_X86_64_PC_WINDOWS_GNU` is set, and by CI's Windows job.
- `roots.la` on Windows reaches a real public host as the Mac's does, on CI only (Wine's network in a sandbox may be off).
- The program suite's TLS and crypto cases run on CI's Windows runner; any that fail for a platform reason go on `tests/windows/known.txt` with why.

**Docs:** the spec's `std.tls` paragraph (Windows' verifier, extra roots, no online revocation) and §11 row 272; CHANGELOG, TODO, CLAUDE.md.

### Must NOT Do

- **No Schannel:** OpenSSL stays the one TLS implementation, so ciphers, versions and messages are the same everywhere. Only the trust decision is Windows'.
- **No roots file on Windows,** and no fallback to one when the verifier can't be asked: then nothing is trusted, as on the Mac.
- **No change on Linux or macOS** beyond the script's move.

## DESIGN

- **Files:**
  - `runtime/os/windows/trust.c` (≈200 lines), joined on Windows when `std.tls` is loaded, as the Mac's is;
  - `runtime/os/windows/os.c`: `OS_TLS_TRUST` 1;
  - `tests/openssl.sh` (moved, one recipe more);
  - `src/link/zig.rs`, `src/link/runtime_parts.rs`, CI.
- **No new crate, no `unsafe` Rust.** The C side uses `CertOpenStore(CERT_STORE_PROV_MEMORY)`, `CertAddEncodedCertificateToStore`, `CertGetCertificateChain`, `CertVerifyCertificateChainPolicy`, `CertCreateCertificateChainEngine`, and their frees.
- **What a reviewer should check hardest:**
  - the policy is `CERT_CHAIN_POLICY_SSL` with the host given, never without;
  - no `dwFlags` in the policy that ignores a check (`*_IGNORE_*`);
  - extra roots are tried only after the system's answer is "untrusted root", and their engine trusts only them.

## TESTS

- [ ] `tests/windows/openssl/trust.la`: own CA trusted with `ca_pem` (`x`), refused without (`unable to get local issuer certificate`), self-signed refused, expired refused, IP mismatch refused, under Wine and on CI.
- [ ] `tests/windows/openssl/roots.la`: a public host trusted through the system's roots, on CI.
- [ ] Unit: the link arguments add `-lcrypt32` for a Windows TLS build only.
- [ ] CI: the Windows suite with the TLS and crypto cases built.
- [ ] Linux and macOS: `--test tls` (or the TLS files the change touches) and `--test macos` unchanged.

## ACCEPTANCE

- [ ] The trust cases print the same as the Mac's, under Wine and on CI's Windows runner.
- [ ] `cargo fmt --check`, clippy and the touched tests are clean; each C file at or under 500 lines.

## DECIDED HERE (for review)

1. **Windows' verifier, not a roots file:** the same choice the Mac made, for the same reasons (company roots, Microsoft's distrust).
2. **No online revocation checks,** as Go and Chrome, and as Windows' own SSL policy unless asked. An online check would make every connection wait on a CRL or OCSP server, and fail offline.
3. **Extra roots join the system's,** tried as a second chain only when the system's roots don't reach the top, so `ca_pem` never weakens what the system refuses for another reason (an expired leaf stays refused).
4. **The OpenSSL script moves to `tests/openssl.sh`,** serving every foreign target.
5. **Wine runs the trust cases:** its `crypt32` implements both calls. Only the public-host case is CI's alone.

## MADE WHILE BUILDING (for review)

🔒 Items 6 and 7 change the reviewed design; both make the check stricter or more predictable, never looser.

6. **The chain's own status is read before the SSL policy, and any error refuses** (untrusted root, partial chain, dates, usage, revocation), as Go does. Under Wine the policy alone accepted a certificate from an unknown CA, so the policy isn't trusted to look at everything.
7. **An IP address goes to the policy as no name** (`pwszServerName` NULL). Windows' and Wine's matches of an IP differ: Wine called every `127.0.0.1` a mismatch. OpenSSL's `X509_check_ip_asc` then matches it after Windows says yes, exactly as on Linux; that check was already there. A DNS name still goes to the policy, and OpenSSL checks it again too.
8. **crypt32 is loaded with `LoadLibraryExW(…, LOAD_LIBRARY_SEARCH_SYSTEM32)`** when first needed, as the Mac loads Security, so `trust.c` joins every Windows runtime while a program without TLS imports nothing new. `-lgdi32 -lcrypt32` join zig's Windows link for OpenSSL itself; a library nothing calls adds no import.
9. **`tests/macos/certs.sh` moves to `tests/certs.sh`** with the OpenSSL script.
10. **Wine ignores an exclusive root,** so a program's own CA is never trusted there. Under Wine the test checks the three cases that need no CA (no CA given, self-signed, expired); CI's Windows runner checks all seven lines. The public-host case passes under Wine here too (`verified`, `true`).
