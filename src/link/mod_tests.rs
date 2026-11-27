//! Tests of linking: the runtime's parts and how each system joins them,
//! the link's arguments, and that only OS parts call the OS or the CPU.
use super::{RUNTIME_PARTS, RUNTIME_SOURCE};

#[test]
fn runtime_parts_join_in_order_core_first() {
    // Each system's runtime: every part but the other systems' own
    // (and, for Windows, POSIX's).
    let joined = |others: &[&str]| -> String {
        let parts = RUNTIME_PARTS
            .iter()
            .filter(|(name, _)| !others.iter().any(|o| name.starts_with(o)));
        parts.map(|(_, src)| *src).collect()
    };
    assert_eq!(
        joined(&[
            "os/darwin/",
            "os/windows/",
            "os/android/",
            "os/posix/dir_walk"
        ]),
        RUNTIME_SOURCE
    );
    assert_eq!(super::runtime_source(super::System::Linux), RUNTIME_SOURCE);
    assert_eq!(
        super::runtime_source(super::System::MacOs),
        joined(&["os/linux/", "os/windows/", "os/android/"])
    );
    assert_eq!(
        super::runtime_source(super::System::Windows),
        joined(&["os/linux/", "os/darwin/", "os/android/", "os/posix/"])
    );
    for only in super::WINDOWS_ONLY {
        assert!(
            RUNTIME_PARTS.iter().any(|(n, _)| *n == only),
            "{only} is a runtime part"
        );
    }
    for only in super::DARWIN_ONLY {
        assert!(
            !RUNTIME_SOURCE.contains(
                super::RUNTIME_PARTS
                    .iter()
                    .find(|(n, _)| *n == only)
                    .expect("a part")
                    .1
            )
        );
    }
    assert_eq!(
        super::System::of("aarch64-apple-darwin"),
        super::System::MacOs
    );
    assert_eq!(
        super::System::of("x86_64-unknown-linux-musl"),
        super::System::Linux
    );
    assert_eq!(
        super::System::of("x86_64-pc-windows-gnu"),
        super::System::Windows
    );
    let (name, core) = RUNTIME_PARTS[0];
    assert_eq!(name, "core/lugha_rt.c");
    let feature = core
        .find("#define _GNU_SOURCE")
        .expect("the core enables GNU extensions");
    let first_include = core.find("#include").expect("the core has the includes");
    assert!(
        feature < first_include,
        "_GNU_SOURCE must precede every #include"
    );
}

fn strings(args: &[std::ffi::OsString]) -> Vec<String> {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn a_build_without_link_tables_links_the_runtime() {
    use std::path::Path;
    let plan = super::Plan::default();
    let args = super::args(
        &[Path::new("prog.o")],
        &[],
        &[Path::new("/t/rt.o")],
        &plan,
        Path::new("out"),
    );
    assert_eq!(strings(&args), ["prog.o", "/t/rt.o", "-lm", "-o", "out"]);
}

#[test]
fn a_plan_adds_c_objects_and_libraries() {
    use crate::modules::link_table::{LinkKey, LinkLine, LinkTable, PackageLink};
    use crate::span::Span;
    use std::path::{Path, PathBuf};
    let line = LinkLine {
        key: LinkKey::Libs,
        values: vec!["z".into()],
        span: Span::new(0, 1),
    };
    let link = PackageLink {
        package: None,
        folder: PathBuf::from("."),
        table: LinkTable { lines: vec![line] },
    };
    let plan = super::Plan::new(&[link], &mut |_, _| Ok(Vec::new())).unwrap();
    let objects = [Path::new("prog.o")];
    let shims = [PathBuf::from("/t/c0.o")];
    let args = super::args(
        &objects,
        &shims,
        &[Path::new("/t/rt.o")],
        &plan,
        Path::new("out"),
    );
    assert_eq!(
        strings(&args),
        ["prog.o", "/t/c0.o", "/t/rt.o", "-lm", "-lz", "-o", "out"]
    );
}

#[test]
fn link_errors_the_user_can_fix_are_diagnostics() {
    use super::LinkError;
    use crate::span::Span;
    let at = Span::new(3, 9);
    let c = LinkError::CFile {
        span: at,
        file: "shim.c".into(),
        stderr: "shim.c:1:1: error: expected ';'".into(),
    };
    let d = c.diagnostic().unwrap();
    assert_eq!((d.code, d.span), ("E0330", at));
    assert!(d.help.unwrap().contains("expected ';'"));
    let libs = LinkError::Libraries {
        span: at,
        stderr: "/usr/bin/ld: cannot find -lnosuch".into(),
    };
    let d = libs.diagnostic().unwrap();
    assert_eq!((d.code, d.span), ("E0331", at));
    let help = d.help.unwrap();
    assert!(help.contains("cannot find -lnosuch") && help.contains("install the missing library"));
    let twice = LinkError::Libraries {
        span: at,
        stderr: "/usr/bin/ld: a.o: multiple definition of `f'; b.o: first defined here".into(),
    };
    let help = twice.diagnostic().unwrap().help.unwrap();
    assert!(
        help.contains("defined twice") && !help.contains("install"),
        "{help}"
    );
    let ours = LinkError::Failed {
        status: Some(1),
        stderr: String::new(),
    };
    assert!(ours.diagnostic().is_none());
    assert!(LinkError::CcNotFound("`cc`".into()).diagnostic().is_none());
}

#[test]
fn fragments_rely_on_the_core_and_stay_small() {
    for (i, (name, src)) in RUNTIME_PARTS.iter().enumerate() {
        assert!(!src.trim().is_empty(), "{name} is empty");
        assert!(src.lines().count() <= 500, "{name} is over 500 lines");
        let includes = src.lines().any(|l| l.starts_with("#include"));
        let may = i == 0 || name.starts_with("os/");
        assert!(
            includes || i != 0,
            "the core holds the portable #include lines"
        );
        assert!(
            !includes || may,
            "{name}: only the core and os/ have #include lines"
        );
    }
}

#[test]
fn the_os_layer_comes_right_after_the_core_and_cpus_after_tasks() {
    let names: Vec<&str> = RUNTIME_PARTS.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names[..3],
        ["core/lugha_rt.c", "os/layer.c", "os/linux/os.c"]
    );
    let tasks = names
        .iter()
        .position(|n| *n == "tasks/tasks.c")
        .expect("tasks.c");
    assert_eq!(
        names[tasks + 1..tasks + 3],
        ["os/linux/x86_64.c", "os/linux/aarch64.c"]
    );
    for only in super::LINUX_ONLY.iter().chain(&super::POSIX_ONLY) {
        assert!(names.contains(only), "{only} is a runtime part");
    }
}

/// Every `.c` file under `dir`, as a path from `base`.
fn runtime_files(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir)
        .expect("runtime/ is readable")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            runtime_files(&path, base, out);
        } else if path.extension().is_some_and(|e| e == "c") {
            let rel = path.strip_prefix(base).expect("under runtime/");
            out.push(rel.to_string_lossy().into_owned());
        }
    }
}

#[test]
fn every_runtime_file_is_joined() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime");
    let mut files = Vec::new();
    runtime_files(&base, &base, &mut files);
    assert!(files.len() > 50, "{} files", files.len());
    for file in files {
        if file.starts_with("openssl/") {
            let text = std::fs::read_to_string(base.join(&file)).expect("readable");
            let joined = super::runtime::TLS_SOURCE.contains(&text)
                || super::runtime::CRYPTO_SOURCE.contains(&text);
            assert!(joined, "{file} is in neither OpenSSL part");
        } else {
            assert!(
                RUNTIME_PARTS.iter().any(|(n, _)| *n == file),
                "{file} isn't joined"
            );
        }
    }
}

/// Whether `code` uses `word`: for a call (`send(`), only where it
/// isn't the end of a longer name (`os_sock_send(`).
fn has_call(code: &str, word: &str) -> bool {
    if !word.ends_with('(') {
        return code.contains(word);
    }
    code.match_indices(word).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// `src` without its `/* */` and `//` comments.
fn code_of(src: &str) -> String {
    let mut out = String::new();
    let mut rest = src;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, r)| r);
        } else if let Some(after) = rest.strip_prefix("//") {
            rest = after.split_once('\n').map_or("", |(_, r)| r);
            out.push('\n');
        } else if rest.starts_with('"') {
            // A string literal is code: kept whole, so `"//"` isn't a comment.
            let end = rest[1..].find('"').map_or(rest.len(), |i| i + 2);
            out.push_str(&rest[..end]);
            rest = &rest[end..];
        } else {
            let c = rest.chars().next().expect("not empty");
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

#[test]
fn only_os_parts_call_the_os_or_the_cpu() {
    const LINUX: [&str; 23] = [
        "epoll_",
        "eventfd",
        "futex",
        "SYS_",
        "syscall(",
        "getrandom",
        "sched_getaffinity",
        "pthread_getattr_np",
        "openat2",
        "RESOLVE_",
        "landlock",
        "seccomp",
        "prctl",
        "<linux/",
        "accept4",
        "SOCK_NONBLOCK",
        "SOCK_CLOEXEC",
        "MSG_NOSIGNAL",
        "IP_MULTICAST_ALL",
        "O_PATH",
        "/proc/",
        "fchmodat2",
        "memrchr",
    ];
    // macOS's own: Seatbelt, kqueue, a descriptor's path.
    const DARWIN: [&str; 4] = ["sandbox_", "kqueue", "kevent", "F_GETPATH"];
    // Socket calls: the system's, behind `os_sock_*`.
    const SOCKETS: [&str; 13] = [
        "socket(",
        "accept(",
        "connect(",
        "bind(",
        "listen(",
        "recv(",
        "send(",
        "recvfrom(",
        "sendto(",
        "setsockopt(",
        "getsockopt(",
        "getsockname(",
        "getpeername(",
    ];
    // A CPU's own code: assembly, or code chosen by the CPU.
    const CPU: [&str; 3] = ["__asm__", "__x86_64__", "__aarch64__"];
    let mut found = Vec::new();
    for (name, src) in RUNTIME_PARTS {
        let code = code_of(src);
        if !super::LINUX_ONLY.contains(&name) && !super::ANDROID_ONLY.contains(&name) {
            let words = LINUX.iter().filter(|w| code.contains(**w));
            found.extend(words.map(|w| format!("{name}: {w}")));
        }
        if !super::DARWIN_ONLY.contains(&name) {
            let words = DARWIN.iter().filter(|w| code.contains(**w));
            found.extend(words.map(|w| format!("{name}: {w}")));
        }
        if !name.starts_with("os/") {
            let words = CPU.iter().chain(&SOCKETS).filter(|w| has_call(&code, w));
            found.extend(words.map(|w| format!("{name}: {w}")));
        }
    }
    assert!(
        found.is_empty(),
        "OS or CPU code outside os/:\n{}",
        found.join("\n")
    );
}

#[test]
fn runtime_defines_every_symbol_codegen_calls() {
    for name in super::RUNTIME_SYMBOLS {
        assert!(
            RUNTIME_SOURCE.contains(&format!("{name}(")),
            "lugha_rt.c lacks {name}"
        );
    }
}

#[test]
fn the_runtime_never_parses_or_checks_utf8() {
    // The prelude parses numbers and checks UTF-8 in Lugha.
    for name in [
        "lugha_rt_parse_i64",
        "lugha_rt_parse_u64",
        "lugha_rt_utf8_check",
    ] {
        assert!(
            !RUNTIME_SOURCE.contains(name),
            "{name} is still in the runtime"
        );
        assert!(
            !super::RUNTIME_SYMBOLS.contains(&name),
            "{name} is still listed"
        );
    }
}
