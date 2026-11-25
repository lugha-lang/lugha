## FEATURE: Mac programs check certificates the way the Mac does — milestone 258

## OBJECTIVE

On macOS, a `std.tls` or `std.http` client trusts exactly what Safari, curl and Go programs trust there: the chain and host name go to Apple's verifier (`SecTrustEvaluateWithError`), which reads the keychain. Today a Mac program trusts the file `/etc/ssl/cert.pem`. That file misses roots added only to the keychain, such as one a company installs, so those connections are refused. It also lags behind keychain distrust: the eMudhra authorities stayed trusted in it until a macOS update in 2026.

```lugha
import std.tls;
// Built on Linux with --target aarch64-apple-darwin, run on a Mac:
let conn = tls.connect(net, "github.com:443", tls.Options.default())?;   // Apple's verifier says yes
// A host whose root sits only in the login keychain: accepted (today: refused).
// A root the keychain distrusts: refused (today: accepted until cert.pem catches up).
```

Nothing changes on Linux. Nothing changes in the language or in `std.tls`'s API.

## CONTEXT

- **Read first:**
  - `runtime/openssl/tls.c`: `lugha_rt_tls_client_context` (roots), `lugha_rt_tls_step` (the handshake), `tls_failed` (messages);
  - `runtime/io/net.c`: `job_new` / `job_run`, which run a blocking call on a helper thread while the task waits;
  - `runtime/os/layer.c`, `os/linux/os.c` and `os/darwin/os.c` (`OS_TLS_ROOTS_FILE`, which this removes);
  - `src/link/runtime.rs`: which fragments are joined for which target;
  - `tests/macos/openssl/roots.la`; `.github/workflows/ci.yml`'s `macos` job.
- **Creates:**
  - `runtime/os/darwin/trust.c`, Apple's verifier behind a plain-bytes function;
  - `tests/macos/openssl/` cases for refusal and for `ca_pem`.
- **Changes:**
  - `runtime/openssl/tls.c` (security-sensitive: which certificates a program accepts);
  - the OS layer;
  - `src/link/runtime.rs` (security-sensitive: what is linked);
  - the spec's `std.tls` paragraph, and §11 row 258.
- **Follows:** PRP-290 (milestone 257), whose DESIGN named this step. **Next:** Intel Macs.

## RESEARCH (summary of what others do)

- **Go** (`crypto/x509/root_darwin.go`):
  - It creates a `SecCertificate` for the leaf and each intermediate, then calls `SecPolicyCreateSSL(true, host)`, `SecTrustCreateWithCertificates` and `SecTrustEvaluateWithError`.
  - It checks the host name again itself afterwards.
  - It maps `errSecCertificateExpired`, `errSecHostNameMismatch` and `errSecNotTrusted` to its own errors.
  - It leaves revocation at the system default.
  - It ignores `SSL_CERT_FILE`: "macOS has no default SSL_CERT_{FILE,DIR} paths".
- **rustls-platform-verifier** (`verification/apple.rs`):
  - It makes the same calls, plus `SecTrustSetVerifyDate`, and passes a stapled OCSP response when there is one.
  - Extra roots go in with `SecTrustSetAnchorCertificates` and `SecTrustSetAnchorCertificatesOnly(false)`: "we want to trust both the system-installed and the extra roots".
  - An IP address is passed as text, so the leaf is still matched against it.
- **Python's `truststore` and Node's `--use-system-ca`** use the same verifier.
- **Linking:**
  - zig ships no Security framework, so Lugha can't link it at build time.
  - It is always present on a Mac, and `dlopen` of a system framework works under the hardened runtime.
  - So the runtime loads Security and CoreFoundation with `dlopen` the first time it checks a certificate.
- **Blocking:** Apple's verifier may go to the network (fetching intermediates, revocation). Apple says not to call it where waiting matters, so it runs on a helper thread, as name lookup does.

## REQUIREMENTS

### Must Do

1. **Apple decides on a Mac.**
   - A Mac client context by certificate hands chain verification to the OS layer, through OpenSSL's `SSL_CTX_set_cert_verify_callback`.
   - The callback passes the server's chain as DER (the leaf first, then what the server sent) and the host to `os_tls_trust`.
   - `os_tls_trust` runs `SecPolicyCreateSSL(true, host)`, then `SecTrustCreateWithCertificates`, then `SecTrustEvaluateWithError`.
   - The host is a name or an IP address, as given to `tls_new`; both go to the policy as text.
2. **Off the worker.**
   - The callback doesn't wait. The first time it's called it saves the chain, calls `SSL_set_retry_verify`, and returns.
   - `tls_step` sees `SSL_ERROR_WANT_RETRY_VERIFY`, runs `os_tls_trust` on a helper thread (`job_run`), and calls the handshake again.
   - On the second call the callback returns the verdict.
   - Lugha's side (`std.tls`'s `drive`) doesn't change: this stays one `tls_step`.
3. **`ca_pem`.**
   - Its certificates are added as anchors with `SecTrustSetAnchorCertificates` and `SecTrustSetAnchorCertificatesOnly(false)`.
   - So, as on Linux, a client trusts the system's roots plus those.
   - Bad PEM is `Err("can't load trusted certificates")` as now.
4. **The same messages as Linux.**
   - The verdict is set on OpenSSL's store context, so `tls_failed` builds the message it builds today:
     - `errSecHostNameMismatch` is `X509_V_ERR_HOSTNAME_MISMATCH`;
     - `errSecCertificateExpired` is `X509_V_ERR_CERT_HAS_EXPIRED`;
     - `errSecCertificateRevoked` is `X509_V_ERR_CERT_REVOKED`;
     - `errSecNotTrusted` and `errSecCreateChainFailed` are `X509_V_ERR_UNABLE_TO_GET_ISSUER_CERT_LOCALLY`;
     - anything else is `X509_V_ERR_CERT_UNTRUSTED`.
   - A program sees, for example, `certificate verify failed: hostname mismatch` on both systems.
5. **Fails closed.** If Security or CoreFoundation can't be loaded, or a symbol is missing, the client context is `Err("can't load trusted certificates")`. Nothing falls back to `/etc/ssl/cert.pem`.
6. **No more trust file.** `OS_TLS_ROOTS_FILE` and the load of `/etc/ssl/cert.pem` go away.
   - On a Mac, `SSL_CERT_FILE` and `SSL_CERT_DIR` aren't read, as in Go; the keychain is the one store.
   - On Linux, both are honoured as now.
7. **Linux is unchanged.** The Linux layer has no `os_tls_trust` (`OS_TLS_TRUST` is 0), so its context is built exactly as today.
8. **Servers and connections by key are unchanged.** A server doesn't verify clients by certificate, and `tls_keys.c` checks keys itself.

### Must NOT Do

- No link-time dependency on any Apple framework, and no new `-framework` flag.
- No OCSP stapling request, no `SecTrustSetNetworkFetchAllowed`, and no revocation setting: the system default applies, as in Go.
- No keychain writes, and no reading of any certificate other than the server's.
- No change to Linux, to `std.tls`'s API, or to servers.

## DESIGN

**Where the code lives.**
- `runtime/os/darwin/trust.c` is part of the Mac OS layer. It sees only bytes: `int os_tls_trust(const uint8_t *const *ders, const size_t *lens, size_t n, const char *host, const uint8_t *const *anchors, const size_t *anchor_lens, size_t nanchors)`.
- It returns 0 for trusted, a small reason code otherwise, or −1 when the framework can't be loaded.
- It knows nothing of OpenSSL; `tls.c` knows nothing of Apple.
- `trust.c` keeps the CoreFoundation and Security entry points it loads in one static table, filled once under `pthread_once`.
- The CF and Sec types are declared as opaque pointers in the file itself, since zig has no Security headers. It is about 180 lines.

**The anchors** are the context's `ca_pem` certificates, kept as DER on the context (`TlsContext`) when it's made, on a Mac only.

**Why the retry and not a wait inside the callback.** The callback runs inside OpenSSL. Parking the task there would let it resume on another worker in the middle of an OpenSSL call, with OpenSSL's error queue, which is per thread, split in two. `SSL_set_retry_verify` (OpenSSL 3.0+) exists for this: the handshake pauses between OpenSSL calls instead.

**A risk the Mac CI will show.**
- Apple's policy is stricter than OpenSSL's. Among other rules, a TLS server certificate must name the host in subjectAltName, must say `serverAuth` if it lists key usages, and may be valid for at most 825 days.
- The suite's test certificates may break those rules. If they do, I regenerate them to meet the rules (on every system), and the PRP needs no change.
- If a case needs a certificate Apple refuses by design, I stop and bring it to you.

**Security.**
- A verdict of "trusted" comes only from `SecTrustEvaluateWithError` returning true. Every other path is a refusal.
- The DER copies are freed on every path, and the framework's objects are released with `CFRelease`.
- The `dlopen` paths are absolute system paths (`/System/Library/Frameworks/…`), never searched.
- No new crate.

## TESTS

- [ ] **Linux:**
  - `tests/tls.rs` and `tests/tls_over_conn.rs` pass unchanged;
  - a `src/link/runtime.rs` unit test checks that `trust.c` is joined for `aarch64-apple-darwin` and not for Linux;
  - the Mac runtime compiles with `zig cc` and the OpenSSL folder when `LUGHA_OPENSSL_AARCH64_APPLE_DARWIN` is set (skipped otherwise; required on CI).
- [ ] **CI's Mac** (`tests/macos/openssl/`):
  - `roots.la` (github.com is verified) still passes, now through Apple's verifier;
  - new `refused.la`: a local server with a self-signed certificate and no `ca_pem` is refused with Linux's message;
  - new `anchors.la`: the same server with its certificate as `ca_pem` is accepted, and connecting under a name not on the certificate is `hostname mismatch`.
- [ ] The whole suite still passes on CI's Mac, including the TLS and HTTP cases with their own `ca_pem`.

## ACCEPTANCE

- [ ] CI's Mac passes the suite and the three `tests/macos/openssl/` cases.
- [ ] Spec: the `std.tls` paragraph ("on macOS, the system's verifier and keychain"), and §11 row 258.
- [ ] CHANGELOG and TODO are updated, and CLAUDE.md's OpenSSL note.
- [ ] `cargo test --lib`, `--test tls`, `--test tls_over_conn`, `--test macos`, `--test macos_suite` pass. `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.

## CALLS I MADE (for review)

1. **Apple's verifier for the whole decision** (chain, host and the system's revocation policy), as Go, rustls-platform-verifier, Python's truststore and Node do. OpenSSL still does the TLS itself.
2. **`dlopen`, not link-time,** because zig has no Security framework. The paths are absolute.
3. **Fail closed** when the framework can't be loaded; no fallback to `cert.pem`.
4. **`ca_pem` adds to the system's roots** (rustls's choice), matching Linux.
5. **`SSL_CERT_FILE` and `SSL_CERT_DIR` aren't read on a Mac** (Go's choice). They are honoured on Linux.
6. **Linux's messages,** by mapping Apple's codes to OpenSSL's, so a program behaves the same on both.
7. **A helper thread** for the check, through OpenSSL's retry-verify, so a slow revocation fetch never holds a worker.
8. **No OCSP stapling in this PRP.** Go doesn't use it either; it can come later if wanted.
