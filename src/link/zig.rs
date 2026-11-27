//! zig as the C compiler for foreign
//! targets: a build for any machine but the one lughac runs on compiles
//! the runtime and links with `zig cc -target <zig triple>`, since zig
//! carries a C library and a linker for every platform. zig is a program
//! found on `PATH`, run from an argument list, never through a shell.

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::process::Command;

use super::LinkError;

/// The oldest zig lughac builds with: 0.17.0 fixed parallel builds that
/// failed at random on a fresh cache.
pub const MINIMUM: (u32, u32, u32) = (0, 17, 0);

/// Where zig's releases are, for messages.
pub const DOWNLOAD: &str = "https://ziglang.org/download";

/// zig's spelling of the LLVM triple `triple`, from a fixed table, since
/// zig names platforms its own way and has renamed parts between
/// releases; `None` for a triple lughac doesn't build for. Foreign glibc
/// builds need glibc 2.28 or newer (Debian 10, Ubuntu 20.04), the oldest
/// the runtime links against.
pub fn zig_triple(triple: &str) -> Option<&'static str> {
    match triple {
        "x86_64-unknown-linux-gnu" => Some("x86_64-linux-gnu.2.28"),
        "aarch64-unknown-linux-gnu" => Some("aarch64-linux-gnu.2.28"),
        "x86_64-unknown-linux-musl" => Some("x86_64-linux-musl"),
        "aarch64-unknown-linux-musl" => Some("aarch64-linux-musl"),
        // macOS 14.4, Sonoma's: `os_sync_wait_on_address` came with it.
        "aarch64-apple-darwin" => Some("aarch64-macos.14.4-none"),
        // MinGW's headers and import libraries, Windows' own C runtime (UCRT).
        "x86_64-pc-windows-gnu" => Some("x86_64-windows-gnu"),
        "aarch64-pc-windows-gnu" => Some("aarch64-windows-gnu"),
        _ => None,
    }
}

/// The arguments zig's link takes beyond `cc`'s for `triple`: its
/// unwinder, since zig links none for C programs on Linux and panics
/// unwind, and `-static` for musl, whose executables need nothing
/// installed. macOS needs neither: libSystem has the unwinder, and macOS
/// has no static executables. Windows also needs the import libraries of
/// `WaitOnAddress` (an API set), `ProcessPrng`, Winsock (name lookups) and
/// ntdll (folders opened relative to a handle), which MinGW doesn't link
/// by default, and those a static OpenSSL needs (gdi32, crypt32). A
/// library a program calls nothing from adds no import.
pub fn link_args(triple: &str) -> Vec<OsString> {
    if triple.contains("-apple-") {
        return Vec::new();
    }
    let mut args = vec![OsString::from("-lunwind")];
    if triple.ends_with("-musl") {
        args.push(OsString::from("-static"));
    }
    if triple.contains("-windows") {
        args.push(OsString::from("-lapi-ms-win-core-synch-l1-2-0"));
        args.push(OsString::from("-lbcryptprimitives"));
        args.push(OsString::from("-lws2_32"));
        args.push(OsString::from("-liphlpapi"));
        args.push(OsString::from("-lntdll"));
        args.push(OsString::from("-lgdi32"));
        args.push(OsString::from("-lcrypt32"));
    }
    args
}

/// The version `zig version` printed: `MAJOR.MINOR.PATCH`, with anything
/// after a `-` or `+` (a development build's) ignored; `None` for any
/// other text.
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let core = text.trim().split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u32>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

/// Runs `zig version` from `PATH` for a build for `target`, and returns
/// what it printed.
///
/// # Errors
///
/// [`LinkError::ZigNotFound`] when there is no `zig`,
/// [`LinkError::ZigTooOld`] when it is older than [`MINIMUM`] or says
/// something that isn't a version, [`LinkError::Io`] when it can't start.
pub fn check(target: &str) -> Result<String, LinkError> {
    check_with(Path::new("zig"), target)
}

/// [`check`] with the program `zig`.
fn check_with(zig: &Path, target: &str) -> Result<String, LinkError> {
    let out = match Command::new(zig).arg("version").output() {
        Ok(out) => out,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(LinkError::ZigNotFound(target.to_string()));
        }
        Err(e) => return Err(LinkError::Io(e)),
    };
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && parse_version(&text).is_some_and(|v| v >= MINIMUM) {
        return Ok(text);
    }
    // What it said is shown, cut short: a broken `zig` could print anything.
    let found: String = text.chars().take(80).collect();
    Err(LinkError::ZigTooOld {
        found,
        target: target.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{MINIMUM, link_args, parse_version, zig_triple};

    #[test]
    fn every_target_has_its_zig_spelling() {
        let table = [
            ("x86_64-unknown-linux-gnu", "x86_64-linux-gnu.2.28"),
            ("aarch64-unknown-linux-gnu", "aarch64-linux-gnu.2.28"),
            ("x86_64-unknown-linux-musl", "x86_64-linux-musl"),
            ("aarch64-unknown-linux-musl", "aarch64-linux-musl"),
            ("aarch64-apple-darwin", "aarch64-macos.14.4-none"),
            ("x86_64-pc-windows-gnu", "x86_64-windows-gnu"),
            ("aarch64-pc-windows-gnu", "aarch64-windows-gnu"),
        ];
        for (llvm, zig) in table {
            assert_eq!(zig_triple(llvm), Some(zig), "{llvm}");
        }
        assert_eq!(zig_triple("aarch64-pc-windows-gnullvm"), None);
    }

    #[test]
    fn musl_links_statically_and_every_zig_link_has_an_unwinder() {
        let strings = |t| -> Vec<String> {
            link_args(t)
                .into_iter()
                .map(|a| a.into_string().unwrap())
                .collect()
        };
        assert_eq!(
            strings("x86_64-unknown-linux-musl"),
            ["-lunwind", "-static"]
        );
        assert_eq!(strings("aarch64-unknown-linux-gnu"), ["-lunwind"]);
        assert!(
            strings("aarch64-apple-darwin").is_empty(),
            "libSystem unwinds"
        );
        assert_eq!(
            strings("x86_64-pc-windows-gnu"),
            [
                "-lunwind",
                "-lapi-ms-win-core-synch-l1-2-0",
                "-lbcryptprimitives",
                "-lws2_32",
                "-liphlpapi",
                "-lntdll",
                "-lgdi32",
                "-lcrypt32"
            ]
        );
        assert_eq!(
            strings("aarch64-pc-windows-gnu"),
            strings("x86_64-pc-windows-gnu"),
            "the same import libraries on either CPU"
        );
    }

    #[test]
    fn versions_parse_with_development_suffixes() {
        assert_eq!(parse_version("0.17.0\n"), Some((0, 17, 0)));
        assert_eq!(parse_version("0.17.1"), Some((0, 17, 1)));
        assert_eq!(parse_version("0.18.0-dev.123+abc"), Some((0, 18, 0)));
        assert!(parse_version("0.16.0").unwrap() < MINIMUM);
        for bad in ["", "garbage", "0.17", "0.17.0.1", "v0.17.0", "0.x.0"] {
            assert_eq!(parse_version(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_missing_zig_is_named_with_the_target() {
        let missing = std::path::Path::new("/nonexistent/zig");
        let err = super::check_with(missing, "x86_64-unknown-linux-musl").unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("x86_64-unknown-linux-musl") && text.contains("zig 0.17.0"),
            "{text}"
        );
        assert!(text.contains("$LUGHA_CC"), "{text}");
    }
}
