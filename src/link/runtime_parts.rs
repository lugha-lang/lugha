//! The C runtime's source: its fragments under
//! `runtime/`, embedded and joined in order into the one `lugha_rt.c`
//! every program is linked with, and the runtime functions generated code
//! calls.

/// The parts of [`RUNTIME_PARTS`] only Linux has (`os/linux/`): its OS
/// layer, its CPU code, and what confines folders and locks the process
/// down with Linux's own calls. Every part outside `os/` is shared, and
/// calls the OS only through `os/layer.c`'s functions.
pub const LINUX_ONLY: [&str; 5] = [
    "os/linux/os.c",
    "os/linux/x86_64.c",
    "os/linux/aarch64.c",
    "os/linux/dir_resolve.c",
    "os/linux/lockdown.c",
];

/// The parts of [`RUNTIME_PARTS`] only macOS has (`os/darwin/`), each
/// joined where its Linux counterpart is.
pub const DARWIN_ONLY: [&str; 5] = [
    "os/darwin/os.c",
    "os/darwin/trust.c",
    "os/darwin/aarch64.c",
    "os/darwin/dir_resolve.c",
    "os/darwin/lockdown.c",
];

/// The parts of [`RUNTIME_PARTS`] only Windows has (`os/windows/`): its
/// OS layer, its CPU code, and what fails closed until it is built.
pub const WINDOWS_ONLY: [&str; 11] = [
    "os/windows/os.c",
    "os/windows/paths.c",
    "os/windows/dir_resolve.c",
    "os/windows/dir.c",
    "os/windows/dir_names.c",
    "os/windows/x86_64.c",
    "os/windows/net_options.c",
    "os/windows/sockets.c",
    "os/windows/command_line.c",
    "os/windows/process.c",
    "os/windows/missing.c",
];

/// The system a runtime is joined for: which `os/<system>/` parts it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    /// Linux, glibc or musl: `os/linux/`.
    Linux,
    /// macOS: `os/darwin/`.
    MacOs,
    /// Windows: `os/windows/`, and none of `os/posix/`.
    Windows,
}

impl System {
    /// The system of the LLVM triple `triple`.
    pub fn of(triple: &str) -> System {
        if triple.contains("-apple-") {
            System::MacOs
        } else if triple.contains("-windows") {
            System::Windows
        } else {
            System::Linux
        }
    }

    /// The folder of the parts only this system has.
    fn folder(self) -> &'static str {
        match self {
            System::Linux => "os/linux/",
            System::MacOs => "os/darwin/",
            System::Windows => "os/windows/",
        }
    }
}

/// The runtime joined for `system`: every shared part, `os/posix/` (but
/// for Windows), and that system's own; [`RUNTIME_SOURCE`] for Linux.
pub fn runtime_source(system: System) -> std::borrow::Cow<'static, str> {
    if system == System::Linux {
        return std::borrow::Cow::Borrowed(RUNTIME_SOURCE);
    }
    let mut others = vec![System::Linux, System::MacOs, System::Windows]
        .into_iter()
        .map(System::folder)
        .collect::<Vec<_>>();
    if system == System::Windows {
        others.push("os/posix/");
    }
    let parts = RUNTIME_PARTS.iter().filter(|(name, _)| {
        !others.iter().any(|f| name.starts_with(f)) || name.starts_with(system.folder())
    });
    std::borrow::Cow::Owned(parts.map(|(_, src)| *src).collect())
}

/// The parts Linux and macOS share but Windows doesn't (`os/posix/`).
pub const POSIX_ONLY: [&str; 7] = [
    "os/posix/faults.c",
    "os/posix/basics.c",
    "os/posix/dir.c",
    "os/posix/dir_names.c",
    "os/posix/process.c",
    "os/posix/sockets.c",
    "os/posix/udp_options.c",
];

/// The runtime's source files, in the order they are joined: the core first,
/// since it holds every `#include`, then the fragments that rely on it.
pub const RUNTIME_PARTS: [(&str, &str); 72] = [
    (
        "core/lugha_rt.c",
        include_str!("../../runtime/core/lugha_rt.c"),
    ),
    ("os/layer.c", include_str!("../../runtime/os/layer.c")),
    ("os/linux/os.c", include_str!("../../runtime/os/linux/os.c")),
    (
        "os/darwin/os.c",
        include_str!("../../runtime/os/darwin/os.c"),
    ),
    (
        "os/darwin/trust.c",
        include_str!("../../runtime/os/darwin/trust.c"),
    ),
    (
        "os/windows/os.c",
        include_str!("../../runtime/os/windows/os.c"),
    ),
    (
        "tasks/rt_lock.c",
        include_str!("../../runtime/tasks/rt_lock.c"),
    ),
    (
        "os/posix/faults.c",
        include_str!("../../runtime/os/posix/faults.c"),
    ),
    (
        "os/posix/basics.c",
        include_str!("../../runtime/os/posix/basics.c"),
    ),
    ("io/helpers.c", include_str!("../../runtime/io/helpers.c")),
    ("text/print.c", include_str!("../../runtime/text/print.c")),
    ("text/input.c", include_str!("../../runtime/text/input.c")),
    ("text/format.c", include_str!("../../runtime/text/format.c")),
    ("text/bytes.c", include_str!("../../runtime/text/bytes.c")),
    ("text/chars.c", include_str!("../../runtime/text/chars.c")),
    ("io/random.c", include_str!("../../runtime/io/random.c")),
    ("io/env.c", include_str!("../../runtime/io/env.c")),
    ("io/fs.c", include_str!("../../runtime/io/fs.c")),
    ("io/file.c", include_str!("../../runtime/io/file.c")),
    (
        "os/linux/dir_resolve.c",
        include_str!("../../runtime/os/linux/dir_resolve.c"),
    ),
    (
        "os/darwin/dir_resolve.c",
        include_str!("../../runtime/os/darwin/dir_resolve.c"),
    ),
    (
        "os/windows/paths.c",
        include_str!("../../runtime/os/windows/paths.c"),
    ),
    (
        "os/windows/dir_resolve.c",
        include_str!("../../runtime/os/windows/dir_resolve.c"),
    ),
    (
        "os/windows/dir.c",
        include_str!("../../runtime/os/windows/dir.c"),
    ),
    (
        "os/windows/dir_names.c",
        include_str!("../../runtime/os/windows/dir_names.c"),
    ),
    (
        "os/posix/dir.c",
        include_str!("../../runtime/os/posix/dir.c"),
    ),
    (
        "os/posix/dir_names.c",
        include_str!("../../runtime/os/posix/dir_names.c"),
    ),
    (
        "os/linux/lockdown.c",
        include_str!("../../runtime/os/linux/lockdown.c"),
    ),
    (
        "os/darwin/lockdown.c",
        include_str!("../../runtime/os/darwin/lockdown.c"),
    ),
    (
        "memory/rc_alloc.c",
        include_str!("../../runtime/memory/rc_alloc.c"),
    ),
    (
        "memory/rc_check.c",
        include_str!("../../runtime/memory/rc_check.c"),
    ),
    ("memory/rc.c", include_str!("../../runtime/memory/rc.c")),
    (
        "os/posix/process.c",
        include_str!("../../runtime/os/posix/process.c"),
    ),
    ("core/test.c", include_str!("../../runtime/core/test.c")),
    ("text/cstr.c", include_str!("../../runtime/text/cstr.c")),
    ("tasks/tasks.c", include_str!("../../runtime/tasks/tasks.c")),
    (
        "os/linux/x86_64.c",
        include_str!("../../runtime/os/linux/x86_64.c"),
    ),
    (
        "os/linux/aarch64.c",
        include_str!("../../runtime/os/linux/aarch64.c"),
    ),
    (
        "os/darwin/aarch64.c",
        include_str!("../../runtime/os/darwin/aarch64.c"),
    ),
    (
        "os/windows/x86_64.c",
        include_str!("../../runtime/os/windows/x86_64.c"),
    ),
    ("tasks/runq.c", include_str!("../../runtime/tasks/runq.c")),
    (
        "tasks/timers.c",
        include_str!("../../runtime/tasks/timers.c"),
    ),
    (
        "tasks/waiting.c",
        include_str!("../../runtime/tasks/waiting.c"),
    ),
    (
        "tasks/stacks.c",
        include_str!("../../runtime/tasks/stacks.c"),
    ),
    ("tasks/spawn.c", include_str!("../../runtime/tasks/spawn.c")),
    (
        "memory/cycles.c",
        include_str!("../../runtime/memory/cycles.c"),
    ),
    (
        "memory/cycle_sites.c",
        include_str!("../../runtime/memory/cycle_sites.c"),
    ),
    ("memory/weak.c", include_str!("../../runtime/memory/weak.c")),
    (
        "tasks/scopes.c",
        include_str!("../../runtime/tasks/scopes.c"),
    ),
    (
        "tasks/scope_unwind.c",
        include_str!("../../runtime/tasks/scope_unwind.c"),
    ),
    (
        "tasks/scope_policy.c",
        include_str!("../../runtime/tasks/scope_policy.c"),
    ),
    (
        "tasks/callbacks.c",
        include_str!("../../runtime/tasks/callbacks.c"),
    ),
    (
        "tasks/scope_time.c",
        include_str!("../../runtime/tasks/scope_time.c"),
    ),
    (
        "memory/cells.c",
        include_str!("../../runtime/memory/cells.c"),
    ),
    (
        "channels/chans.c",
        include_str!("../../runtime/channels/chans.c"),
    ),
    (
        "channels/rings.c",
        include_str!("../../runtime/channels/rings.c"),
    ),
    (
        "channels/select.c",
        include_str!("../../runtime/channels/select.c"),
    ),
    (
        "channels/actors.c",
        include_str!("../../runtime/channels/actors.c"),
    ),
    (
        "tasks/unwind.c",
        include_str!("../../runtime/tasks/unwind.c"),
    ),
    ("io/jobs.c", include_str!("../../runtime/io/jobs.c")),
    ("io/net.c", include_str!("../../runtime/io/net.c")),
    (
        "tasks/workers.c",
        include_str!("../../runtime/tasks/workers.c"),
    ),
    (
        "tasks/parking.c",
        include_str!("../../runtime/tasks/parking.c"),
    ),
    ("io/resolve.c", include_str!("../../runtime/io/resolve.c")),
    ("io/udp.c", include_str!("../../runtime/io/udp.c")),
    (
        "os/posix/sockets.c",
        include_str!("../../runtime/os/posix/sockets.c"),
    ),
    (
        "os/posix/udp_options.c",
        include_str!("../../runtime/os/posix/udp_options.c"),
    ),
    (
        "os/windows/net_options.c",
        include_str!("../../runtime/os/windows/net_options.c"),
    ),
    (
        "os/windows/sockets.c",
        include_str!("../../runtime/os/windows/sockets.c"),
    ),
    (
        "os/windows/command_line.c",
        include_str!("../../runtime/os/windows/command_line.c"),
    ),
    (
        "os/windows/process.c",
        include_str!("../../runtime/os/windows/process.c"),
    ),
    (
        "os/windows/missing.c",
        include_str!("../../runtime/os/windows/missing.c"),
    ),
];

/// The C runtime for Linux: [`RUNTIME_PARTS`] but macOS's joined in order,
/// compiled as one `lugha_rt.c`.
pub const RUNTIME_SOURCE: &str = concat!(
    include_str!("../../runtime/core/lugha_rt.c"),
    include_str!("../../runtime/os/layer.c"),
    include_str!("../../runtime/os/linux/os.c"),
    include_str!("../../runtime/tasks/rt_lock.c"),
    include_str!("../../runtime/os/posix/faults.c"),
    include_str!("../../runtime/os/posix/basics.c"),
    include_str!("../../runtime/io/helpers.c"),
    include_str!("../../runtime/text/print.c"),
    include_str!("../../runtime/text/input.c"),
    include_str!("../../runtime/text/format.c"),
    include_str!("../../runtime/text/bytes.c"),
    include_str!("../../runtime/text/chars.c"),
    include_str!("../../runtime/io/random.c"),
    include_str!("../../runtime/io/env.c"),
    include_str!("../../runtime/io/fs.c"),
    include_str!("../../runtime/io/file.c"),
    include_str!("../../runtime/os/linux/dir_resolve.c"),
    include_str!("../../runtime/os/posix/dir.c"),
    include_str!("../../runtime/os/posix/dir_names.c"),
    include_str!("../../runtime/os/linux/lockdown.c"),
    include_str!("../../runtime/memory/rc_alloc.c"),
    include_str!("../../runtime/memory/rc_check.c"),
    include_str!("../../runtime/memory/rc.c"),
    include_str!("../../runtime/os/posix/process.c"),
    include_str!("../../runtime/core/test.c"),
    include_str!("../../runtime/text/cstr.c"),
    include_str!("../../runtime/tasks/tasks.c"),
    include_str!("../../runtime/os/linux/x86_64.c"),
    include_str!("../../runtime/os/linux/aarch64.c"),
    include_str!("../../runtime/tasks/runq.c"),
    include_str!("../../runtime/tasks/timers.c"),
    include_str!("../../runtime/tasks/waiting.c"),
    include_str!("../../runtime/tasks/stacks.c"),
    include_str!("../../runtime/tasks/spawn.c"),
    include_str!("../../runtime/memory/cycles.c"),
    include_str!("../../runtime/memory/cycle_sites.c"),
    include_str!("../../runtime/memory/weak.c"),
    include_str!("../../runtime/tasks/scopes.c"),
    include_str!("../../runtime/tasks/scope_unwind.c"),
    include_str!("../../runtime/tasks/scope_policy.c"),
    include_str!("../../runtime/tasks/callbacks.c"),
    include_str!("../../runtime/tasks/scope_time.c"),
    include_str!("../../runtime/memory/cells.c"),
    include_str!("../../runtime/channels/chans.c"),
    include_str!("../../runtime/channels/rings.c"),
    include_str!("../../runtime/channels/select.c"),
    include_str!("../../runtime/channels/actors.c"),
    include_str!("../../runtime/tasks/unwind.c"),
    include_str!("../../runtime/io/jobs.c"),
    include_str!("../../runtime/io/net.c"),
    include_str!("../../runtime/tasks/workers.c"),
    include_str!("../../runtime/tasks/parking.c"),
    include_str!("../../runtime/io/resolve.c"),
    include_str!("../../runtime/io/udp.c"),
    include_str!("../../runtime/os/posix/sockets.c"),
    include_str!("../../runtime/os/posix/udp_options.c"),
);
