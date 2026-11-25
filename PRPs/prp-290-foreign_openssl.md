## FEATURE: TLS and crypto programs built for a Mac, with an OpenSSL built for it — milestone 257

## OBJECTIVE

A program importing `std.tls`, `std.http` or `std.crypto` builds for `aarch64-apple-darwin` on Linux and runs on any Mac with nothing installed. Today that build is refused with E0334.

```bash
sh tests/macos/openssl.sh aarch64-apple-darwin ~/.cache/openssl-mac   # once: OpenSSL 3.5 LTS, built with zig
export LUGHA_OPENSSL_AARCH64_APPLE_DARWIN=~/.cache/openssl-mac
lughac build app.la --target aarch64-apple-darwin -o app            # links libssl.a and libcrypto.a into app
```

Without the variable, the build is E0334 as now, and its message names the variable.

## CONTEXT

- **Read first:**
  - `src/link/mod.rs`: `link_with` and `args`, which give `-lssl -lcrypto`;
  - `src/link/runtime.rs`: the runtime compile, and `missing_openssl` for E0332;
  - `src/link/errors.rs`: `foreign_refusals` for E0334;
  - `src/driver/pipeline.rs`: the call to `foreign_refusals`;
  - `runtime/openssl/tls.c`: `lugha_rt_tls_client_context`, where the roots are loaded;
  - `runtime/os/layer.c` with `os/linux/os.c` and `os/darwin/os.c`;
  - `tests/musl.rs`, which tests E0334;
  - `.github/workflows/ci.yml`: the build and `macos` jobs.
- **Creates:**
  - `tests/macos/openssl.sh`;
  - `docs/errors/E0336.md`;
  - a `tests/macos/` case for the public roots.
- **Changes:**
  - `src/link/` (security-sensitive: this is how `cc` links a program);
  - `src/driver/pipeline.rs`;
  - `runtime/openssl/tls.c` and the OS layer (security-sensitive: which certificate authorities a program trusts);
  - `ci.yml`;
  - `tests/macos_suite.rs` (E0334 is no longer expected on CI).
- **Spec:**
  - §8 link step ("Another platform");
  - the `std.tls` and `std.crypto` paragraphs ("the system's OpenSSL");
  - the E0334 row, and a new E0336 row;
  - §11 row 257.
- **Draft 9.1 rule 5 and the ledger.** PRT-05 says the host's OpenSSL is never linked into another platform's program. That stays true: this links an OpenSSL built *for* that platform. PRT-13 defers "OpenSSL for foreign targets", which this PRP resolves. Both need a line in the Draft and the ledger, and approving this PRP approves those lines.
- **Follows:** PRP-289 (milestone 256). **Next:** a PRP checking certificates through Apple's Security framework.

## RESEARCH (summary of what other languages do)

- **They put the crypto inside the program**, rather than relying on one installed on the user's Mac:
  - Node puts in its own OpenSSL;
  - Bun puts in BoringSSL;
  - Python's Mac installer comes with its own OpenSSL;
  - Swift on Linux carries a copy of BoringSSL;
  - Rust's `openssl` crate takes a per-target `<TARGET>_OPENSSL_DIR` or builds OpenSSL from source;
  - Go, Zig and Java write their own crypto.
- **Using the operating system's crypto** is what .NET does: on macOS it uses CryptoKit, and .NET 10 dropped OpenSSL there. It's closed to us: Apple has X25519, Ed25519, HPKE and ChaCha20-Poly1305 only in CryptoKit, which can only be called from Swift, and TLS with raw public keys needs OpenSSL 3.2.
- **Linking Homebrew's OpenSSL** ties a program to Macs that have Homebrew. It has broken on macOS 15, and in Nim it crashed when two copies were loaded.
- **Trusted roots:**
  - An OpenSSL you build yourself doesn't read the keychain. Go, rustls-platform-verifier, Python's `truststore` and Node's `--use-system-ca` all check the certificate chain through Apple's Security framework.
  - The file `/etc/ssl/cert.pem`, which curl, git and python3 use, lagged behind the keychain in 2026: it still trusted the compromised eMudhra certificate authorities until a macOS update.
  - So this PRP uses the file, and the next one moves to the Security framework.
- **zig can build OpenSSL for a Mac:** its macOS headers include `CommonCrypto`, and its `libSystem` has `CCRandomGenerateBytes` and `getentropy`. Zig doesn't ship the Security framework.

## REQUIREMENTS

### Must Do

1. **The folder.** A foreign build of a program loading `std.tls`, `std.http` or `std.crypto` reads `LUGHA_OPENSSL_<TARGET>`. `<TARGET>` is the `--target` triple in capitals with `-` written `_`, so `LUGHA_OPENSSL_AARCH64_APPLE_DARWIN`.
   - The value is a folder holding `include/openssl/ssl.h`, `lib/libssl.a` and `lib/libcrypto.a`.
   - Unset or empty: E0334 as now. Its message and page add how to set the variable.
   - Set, but the folder isn't absolute, doesn't exist, or lacks any of those three files: **E0336** at the import, naming the variable, the folder and the first missing file.
   - A build for the machine lughac runs on never reads the variable, and links the system's OpenSSL as now.
2. **Compiling and linking.** The runtime is compiled with `-I<folder>/include`. The cached object's key includes the folder, through its flags. The link names `<folder>/lib/libssl.a` and `<folder>/lib/libcrypto.a` by path, instead of `-lssl -lcrypto`, so a stray `.dylib` or `.so` is never picked up. `std.crypto` alone links only `libcrypto.a`.
3. **Version.** An OpenSSL older than 3.2 in the folder stops the build at the header check, as now. It's reported as E0332, whose message for a foreign build names the folder.
4. **Roots on macOS.** A TLS client trusts what it trusts on Linux: `SSL_CTX_set_default_verify_paths`, which honours `SSL_CERT_FILE` and `SSL_CERT_DIR`. On macOS it also loads `/etc/ssl/cert.pem`, the roots macOS ships for OpenSSL-based tools.
   - The OS layer gives the file through `os_tls_roots_file()`: `/etc/ssl/cert.pem` on macOS, `NULL` on Linux.
   - A file that's named but doesn't load is `Err("can't load trusted certificates")`, as a bad `ca_pem` is now.
5. **The OpenSSL script.** `tests/macos/openssl.sh <target> <folder>` downloads OpenSSL 3.5 LTS (a pinned version) from its GitHub release and checks a pinned SHA-256. It then builds it with `zig cc -target <zig's name>` (`no-shared no-apps no-docs no-tests no-module`, `--openssldir=/etc/ssl`) and installs the headers and static libraries into `<folder>`. It's POSIX `sh`, refuses a non-empty folder, and stops on any failure.
6. **CI.** The build job:
   - runs the script for `aarch64-apple-darwin`, cached by OpenSSL and zig versions;
   - sets the variable;
   - builds the suite, which now includes the TLS, HTTP and crypto cases. `skipped.txt` becomes empty.

   The Mac runs them, and `capabilities/net/fake.la` passes.
7. **Public roots on a Mac.** A new `tests/macos/` case connects with `std.tls` to `github.com:443`, which CI already reaches, and prints that the certificate was verified. This shows the macOS roots are loaded. The suite's cases use their own `ca_pem`, so they don't show it.

### Must NOT Do

- **No OpenSSL source in lughac,** and lughac never downloads or builds it. The script is a helper for developers and CI.
- **No dynamic linking** of a foreign OpenSSL, and no Homebrew path.
- **No keychain or Security framework** in this PRP; that's the next PRP.
- **No change** to native builds, `-lssl`, or E0332 for this machine.

## DESIGN

**Any foreign target, proved on the Mac.** The variable works for every foreign target: the same code serves the musl and ARM Linux builds. CI proves it end to end only for `aarch64-apple-darwin`. The musl and ARM paths are covered by tests that check the arguments built for a folder, not by running a TLS program.

**Static, so a fix means a rebuild.** A program carries the OpenSSL it was built with. The `std.tls` paragraph and E0334's page say so: rebuilding with a patched folder is how a program gets an OpenSSL security fix. Node, Bun and Python's installer carry the same cost.

**The trust file is a step, not the end.**
- What `/etc/ssl/cert.pem` misses fails closed: a root added only to the keychain, such as one a company installs, isn't trusted, and the connection is refused.
- What it lags on fails open: a certificate authority distrusted in the keychain but still in the file is trusted until macOS updates the file, as with eMudhra in 2026.
- So the next PRP verifies through `SecTrustEvaluateWithError`, with the Security framework loaded by `dlopen` while the program runs, because zig has no Security framework to link at build time. That PRP comes for your review.

**Errors.** E0336, "the OpenSSL folder for a target is incomplete", is a diagnostic at the import, like E0334. It gets its page in `docs/errors/E0336.md`. The variable is checked before anything is compiled.

**Security.**
- The folder is trusted build input, like a `[link]` table's `libs` and `paths`. lughac checks its shape and never runs anything from it.
- Arguments are paths in an argument list, never through a shell.
- The script checks the tarball's SHA-256 before unpacking it. CI's cache key includes that checksum.
- No new crate.

## TESTS

- [ ] `src/link/` unit tests:
  - the variable's name for each target;
  - a folder giving `-I…/include` for the runtime and `…/lib/libssl.a …/lib/libcrypto.a` for the link, with `libcrypto.a` alone for `std.crypto`;
  - E0336 for a relative path, a missing folder, and each missing file;
  - a native build ignores the variable.
- [ ] `tests/musl.rs`:
  - E0334's message names `LUGHA_OPENSSL_X86_64_UNKNOWN_LINUX_MUSL`;
  - E0336 for a folder missing `lib/libssl.a`.
- [ ] `tests/macos_suite.rs`: E0334 still lists a case as skipped when the variable is unset, and the CI build has none skipped.
- [ ] `tests/macos/` public-roots case on CI's Mac. The suite's TLS, HTTP and crypto cases pass on CI's Mac.
- [ ] `tests/macos/openssl.sh` builds on Linux. I'll run it here if you allow the build; otherwise CI is its first run.

## ACCEPTANCE

- [ ] On CI's Mac, the whole suite passes with nothing skipped, plus the public-roots case.
- [ ] Spec: §8 link step, the `std.tls` and `std.crypto` paragraphs, the E0334 and E0336 rows, and §11 row 257.
- [ ] Draft 9.1 rule 5 adds "unless an OpenSSL built for the target is named". The ledger marks PRT-05 amended and the OpenSSL part of PRT-13 resolved.
- [ ] CHANGELOG, TODO and CLAUDE.md (the targets line and the OpenSSL note) are updated.
- [ ] `cargo test --lib`, `--test musl`, `--test macos`, `--test macos_suite`, `--test explain` pass. `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.

## CALLS I MADE (for review)

1. **The variable name** follows Rust's `openssl-sys` (`<TARGET>_OPENSSL_DIR`), with Lugha's `LUGHA_` prefix. It names one folder, not separate `include` and `lib` folders.
2. **Static libraries only, linked by path.**
3. **Any foreign target accepted,** proved end to end only on the Mac.
4. **E0336** is a new code for a bad folder, instead of reusing E0334.
5. **`/etc/ssl/cert.pem` now, the Security framework next.** Its known gap, the lag behind keychain distrust, is written down.
6. **OpenSSL 3.5 LTS** in the script, pinned by version and SHA-256, supported to 2030.
7. **A live `github.com` connection** in the Mac job to prove public roots. It depends on the network, which CI needs anyway.
