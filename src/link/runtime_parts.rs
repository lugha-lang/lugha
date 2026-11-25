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

/// The system a runtime is joined for: which `os/<system>/` parts it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    /// Linux, glibc or musl: `os/linux/`.
    Linux,
    /// macOS: `os/darwin/`.
    MacOs,
}

impl System {
    /// The system of the LLVM triple `triple`.
    pub fn of(triple: &str) -> System {
        if triple.contains("-apple-") {
            System::MacOs
        } else {
            System::Linux
        }
    }

    /// The folder of the parts only this system has.
    fn folder(self) -> &'static str {
        match self {
            System::Linux => "os/linux/",
            System::MacOs => "os/darwin/",
        }
    }
}

/// The runtime joined for `system`: every shared part, `os/posix/`, and
/// that system's own; [`RUNTIME_SOURCE`] for Linux.
pub fn runtime_source(system: System) -> std::borrow::Cow<'static, str> {
    if system == System::Linux {
        return std::borrow::Cow::Borrowed(RUNTIME_SOURCE);
    }
    let others = [System::Linux, System::MacOs].map(System::folder);
    let parts = RUNTIME_PARTS.iter().filter(|(name, _)| {
        !others.iter().any(|f| name.starts_with(f)) || name.starts_with(system.folder())
    });
    std::borrow::Cow::Owned(parts.map(|(_, src)| *src).collect())
}

/// The parts Linux and macOS share but Windows doesn't (`os/posix/`).
pub const POSIX_ONLY: [&str; 5] = [
    "os/posix/dir.c",
    "os/posix/dir_names.c",
    "os/posix/process.c",
    "os/posix/sockets.c",
    "os/posix/udp_options.c",
];

/// The runtime's source files, in the order they are joined: the core first,
/// since it holds every `#include`, then the fragments that rely on it.
pub const RUNTIME_PARTS: [(&str, &str); 60] = [
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
        "tasks/rt_lock.c",
        include_str!("../../runtime/tasks/rt_lock.c"),
    ),
    ("tasks/stack.c", include_str!("../../runtime/tasks/stack.c")),
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
];

/// Every runtime function codegen may call; `lugha_rt.c` defines them all.
pub const RUNTIME_SYMBOLS: [&str; 161] = [
    "lugha_rt_init",
    "lugha_rt_task_go",
    "lugha_rt_print_begin",
    "lugha_rt_print_end",
    "lugha_rt_rc_freeze_start",
    "lugha_rt_rc_freeze_end",
    "lugha_rt_main",
    "lugha_rt_c_call",
    "lugha_rt_c_blocking",
    "lugha_rt_cb_enter",
    "lugha_rt_stack_low",
    "lugha_rt_read_line",
    "lugha_rt_eprint",
    "lugha_rt_exit",
    "lugha_rt_str_slice",
    "lugha_rt_args",
    "lugha_rt_parse_f64",
    "lugha_rt_alloc",
    "lugha_rt_panic",
    "lugha_rt_panic_bounds",
    "lugha_rt_str_concat",
    "lugha_rt_str_eq",
    "lugha_rt_str_cmp",
    "lugha_rt_array_grow",
    "lugha_rt_print_i32",
    "lugha_rt_print_i64",
    "lugha_rt_print_u8",
    "lugha_rt_print_f64",
    "lugha_rt_print_u64",
    "lugha_rt_print_f32",
    "lugha_rt_print_bool",
    "lugha_rt_print_str",
    "lugha_rt_str_quote",
    "lugha_rt_fmt_int",
    "lugha_rt_fmt_float",
    "lugha_rt_pad",
    "lugha_rt_read_stdin_bytes",
    "lugha_rt_str_to_bytes",
    "lugha_rt_str_from_bytes",
    "lugha_rt_env",
    "lugha_rt_spawn",
    "lugha_rt_show_enter",
    "lugha_rt_show_leave",
    "lugha_rt_print_char",
    "lugha_rt_to_string_char",
    "lugha_rt_char_quote",
    "lugha_rt_print_newline",
    "lugha_rt_to_string_i32",
    "lugha_rt_to_string_i64",
    "lugha_rt_to_string_u8",
    "lugha_rt_to_string_f64",
    "lugha_rt_to_string_u64",
    "lugha_rt_to_string_f32",
    "lugha_rt_to_string_bool",
    "lugha_rt_os_seed",
    "lugha_rt_now_ms",
    "lugha_rt_wall_ms",
    "lugha_rt_local_offset",
    "lugha_rt_flush",
    "lugha_rt_test_is",
    "lugha_rt_test_unknown",
    "lugha_rt_test_skip",
    "lugha_rt_cb_pin",
    "lugha_rt_cb_release",
    "lugha_rt_str_from_c",
    "lugha_rt_task_spawn",
    "lugha_rt_task_join",
    "lugha_rt_task_yield",
    "lugha_rt_task_sleep",
    "lugha_rt_task_preempt",
    "lugha_rt_task_select",
    "lugha_rt_chan_new",
    "lugha_rt_ring_new",
    "lugha_rt_ring_try_send",
    "lugha_rt_chan_send",
    "lugha_rt_chan_recv",
    "lugha_rt_chan_try_recv",
    "lugha_rt_chan_try_send",
    "lugha_rt_chan_unbounded",
    "lugha_rt_chan_close",
    "lugha_rt_chan_end",
    "lugha_rt_actor_new",
    "lugha_rt_actor_run",
    "lugha_rt_actor_started",
    "lugha_rt_actor_call",
    "lugha_rt_actor_cast",
    "lugha_rt_actor_stop",
    "lugha_rt_net_listen",
    "lugha_rt_net_connect",
    "lugha_rt_net_resolve",
    "lugha_rt_net_private",
    "lugha_rt_udp_bind",
    "lugha_rt_udp_send",
    "lugha_rt_udp_recv",
    "lugha_rt_udp_group",
    "lugha_rt_udp_option",
    "lugha_rt_udp_multicast_if",
    "lugha_rt_net_interfaces",
    "lugha_rt_net_accept",
    "lugha_rt_net_read",
    "lugha_rt_net_write",
    "lugha_rt_net_close",
    "lugha_rt_net_name",
    "lugha_rt_file_read",
    "lugha_rt_file_write",
    "lugha_rt_file_close",
    "lugha_rt_dir_cwd",
    "lugha_rt_dir_open",
    "lugha_rt_dir_sub",
    "lugha_rt_dir_read_bytes",
    "lugha_rt_dir_read_text",
    "lugha_rt_dir_write",
    "lugha_rt_dir_list",
    "lugha_rt_dir_make",
    "lugha_rt_dir_remove",
    "lugha_rt_dir_rename",
    "lugha_rt_dir_kind",
    "lugha_rt_dir_size",
    "lugha_rt_dir_chmod",
    "lugha_rt_dir_sync",
    "lugha_rt_dir_file",
    "lugha_rt_dir_path",
    "lugha_rt_lockdown",
    "lugha_rt_unwound_to_c",
    "lugha_rt_scope_enter",
    "lugha_rt_pin_here",
    "lugha_rt_cb_leave",
    "lugha_rt_cb_hold",
    "lugha_rt_cb_queue",
    "lugha_rt_cb_task_panicked",
    "lugha_rt_cb_caught",
    "lugha_rt_cb_run",
    "lugha_rt_scope_root",
    "lugha_rt_task_spawn_in",
    "lugha_rt_scope_end",
    "lugha_rt_scope_leave",
    "lugha_rt_scope_caught",
    "lugha_rt_scope_exit",
    "lugha_rt_task_failed",
    "lugha_rt_cancel_point",
    "lugha_rt_task_join_status",
    "lugha_rt_task_cancel",
    "lugha_rt_task_name",
    "lugha_rt_scope_root_finish",
    "lugha_rt_scope_root_unwound",
    "lugha_rt_cell_new",
    "lugha_rt_cell_get",
    "lugha_rt_cell_set",
    "lugha_rt_cell_bytes",
    "lugha_rt_cell_set_bytes",
    "lugha_rt_task_policy",
    "lugha_rt_task_handler",
    "lugha_rt_task_own_name",
    "lugha_rt_task_caught",
    "lugha_rt_task_panicked",
    "lugha_rt_scope_remaining",
    "lugha_rt_shield_enter",
    "lugha_rt_shield_exit",
    "lugha_rt_shield_caught",
    "lugha_rt_shield_lost",
    "lugha_rt_shield_end",
];

/// The C runtime for Linux: [`RUNTIME_PARTS`] but macOS's joined in order,
/// compiled as one `lugha_rt.c`.
pub const RUNTIME_SOURCE: &str = concat!(
    include_str!("../../runtime/core/lugha_rt.c"),
    include_str!("../../runtime/os/layer.c"),
    include_str!("../../runtime/os/linux/os.c"),
    include_str!("../../runtime/tasks/rt_lock.c"),
    include_str!("../../runtime/tasks/stack.c"),
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
