/*
 * lockdown.c — part of lugha_rt.c: process lockdown. Once the runtime has
 * made what `main` (or a test) asked for, the kernel limits the whole
 * process, C code and started programs included, to the kinds of
 * capability it took: no sockets without `Net`, no new processes without
 * `Proc`, no files without `Fs` beyond what the held capabilities read.
 *
 * seccomp refuses system calls (an error return, never a kill); Landlock
 * refuses file access. Both are inherited by every child through `fork`,
 * `clone` and `execve`, and nothing can undo them. Each part is applied
 * where the kernel supports it and skipped quietly where it doesn't;
 * `LUGHA_LOCKDOWN=report` prints what was applied.
 *
 * `lugha_rt_lockdown` is the porting point: another platform implements it
 * with its own mechanism, or not at all.
 *
 * A fragment, not compiled on its own: joined after the core (lugha_rt.c),
 * whose includes (`<linux/seccomp.h>`, `<linux/filter.h>`,
 * `<linux/audit.h>`, `<linux/landlock.h>`, `<sys/prctl.h>`) it relies on.
 */

/* The capability kinds, one bit each: what the compiler passes. */
enum {
    LOCK_FS = 1 << 0,
    LOCK_NET = 1 << 1,
    LOCK_PROC = 1 << 2,
    LOCK_CLOCK = 1 << 3,
    /* A test's folder (`LUGHA_TEST_DIR`): read and written beneath. */
    LOCK_TEST_DIR = 1 << 4,
};

#if defined(__x86_64__)
#define LOCK_ARCH AUDIT_ARCH_X86_64
#elif defined(__aarch64__)
#define LOCK_ARCH AUDIT_ARCH_AARCH64
#endif

/* Room for every instruction the filter below can need. */
enum { LOCK_FILTER_MAX = 64 };

typedef struct {
    struct sock_filter code[LOCK_FILTER_MAX];
    int len;
} LockFilter;

static void lock_emit(LockFilter *f, struct sock_filter instruction) {
    if (f->len >= LOCK_FILTER_MAX) {
        die("panic: the lockdown filter is too long\n");
    }
    f->code[f->len++] = instruction;
}

/* Refuses system call `nr` with `error`. */
static void lock_refuse(LockFilter *f, long nr, int error) {
    lock_emit(f, (struct sock_filter)BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, (uint32_t)nr, 0, 1));
    lock_emit(f, (struct sock_filter)BPF_STMT(BPF_RET | BPF_K,
                                              SECCOMP_RET_ERRNO | ((uint32_t)error & 0xFFFF)));
}

/* The seccomp filter for `kinds`: the always-refused calls, the socket
 * calls without `Net`, the process-making calls without `Proc`; everything
 * else allowed. 1 when installed. */
static int lock_seccomp(int64_t kinds) {
#ifndef LOCK_ARCH
    (void)kinds;
    return 0;
#else
    LockFilter f = {.len = 0};
    const uint32_t refuse = SECCOMP_RET_ERRNO | EPERM;
    // The architecture first: a call through another ABI is refused.
    lock_emit(&f, (struct sock_filter)BPF_STMT(BPF_LD | BPF_W | BPF_ABS,
                                               offsetof(struct seccomp_data, arch)));
    lock_emit(&f, (struct sock_filter)BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, LOCK_ARCH, 1, 0));
    lock_emit(&f, (struct sock_filter)BPF_STMT(BPF_RET | BPF_K, refuse));
    lock_emit(&f, (struct sock_filter)BPF_STMT(BPF_LD | BPF_W | BPF_ABS,
                                               offsetof(struct seccomp_data, nr)));
#if defined(__x86_64__)
    // x32 calls are numbered from bit 30 under the same architecture.
    lock_emit(&f, (struct sock_filter)BPF_JUMP(BPF_JMP | BPF_JGE | BPF_K, 0x40000000, 0, 1));
    lock_emit(&f, (struct sock_filter)BPF_STMT(BPF_RET | BPF_K, refuse));
#endif
    // `io_uring` opens sockets and files without the calls seen here.
    lock_refuse(&f, SYS_io_uring_setup, EPERM);
    lock_refuse(&f, SYS_io_uring_enter, EPERM);
    lock_refuse(&f, SYS_io_uring_register, EPERM);
    if (!(kinds & LOCK_NET)) {
        lock_refuse(&f, SYS_socket, EPERM);
        lock_refuse(&f, SYS_socketpair, EPERM);
    }
    bool thread_check = false;
    if (!(kinds & LOCK_PROC)) {
        lock_refuse(&f, SYS_execve, EPERM);
        lock_refuse(&f, SYS_execveat, EPERM);
#ifdef SYS_fork
        lock_refuse(&f, SYS_fork, EPERM);
#endif
#ifdef SYS_vfork
        lock_refuse(&f, SYS_vfork, EPERM);
#endif
        // Its flags are behind a pointer: refused as unknown, so the C
        // library falls back to `clone`, whose flags can be checked.
        lock_refuse(&f, SYS_clone3, ENOSYS);
        thread_check = true;
    }
    if (thread_check) {
        // `clone` making a thread is allowed; making a process isn't.
        lock_emit(&f, (struct sock_filter)BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, SYS_clone, 0, 3));
        lock_emit(&f, (struct sock_filter)BPF_STMT(BPF_LD | BPF_W | BPF_ABS,
                                                   offsetof(struct seccomp_data, args[0])));
        lock_emit(&f, (struct sock_filter)BPF_JUMP(BPF_JMP | BPF_JSET | BPF_K, CLONE_THREAD, 1, 0));
        lock_emit(&f, (struct sock_filter)BPF_STMT(BPF_RET | BPF_K, refuse));
    }
    lock_emit(&f, (struct sock_filter)BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW));
    struct sock_fprog program = {.len = (unsigned short)f.len, .filter = f.code};
    if (syscall(SYS_seccomp, SECCOMP_SET_MODE_FILTER, SECCOMP_FILTER_FLAG_TSYNC, &program) == 0) {
        return 1;
    }
    return syscall(SYS_seccomp, SECCOMP_SET_MODE_FILTER, 0, &program) == 0;
#endif
}

/* Every file right Landlock ABI `abi` knows. */
static uint64_t lock_fs_rights(int abi) {
    uint64_t rights = (LANDLOCK_ACCESS_FS_MAKE_SYM << 1) - 1; /* ABI 1: bits 0 to 12 */
    if (abi >= 2) {
        rights |= LANDLOCK_ACCESS_FS_REFER;
    }
    if (abi >= 3) {
        rights |= LANDLOCK_ACCESS_FS_TRUNCATE;
    }
#ifdef LANDLOCK_ACCESS_FS_IOCTL_DEV
    if (abi >= 5) {
        rights |= LANDLOCK_ACCESS_FS_IOCTL_DEV;
    }
#endif
    return rights;
}

/* Allows `access` (narrowed to `handled`, and to file rights for a file)
 * beneath `path` in `ruleset`; a path that doesn't exist is skipped. */
static void lock_allow(int ruleset, const char *path, uint64_t access, uint64_t handled) {
    int fd = open(path, O_PATH | O_CLOEXEC);
    if (fd < 0) {
        return;
    }
    struct stat info;
    if (fstat(fd, &info) == 0 && !S_ISDIR(info.st_mode)) {
        uint64_t file_rights = LANDLOCK_ACCESS_FS_EXECUTE | LANDLOCK_ACCESS_FS_WRITE_FILE |
                               LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_TRUNCATE;
#ifdef LANDLOCK_ACCESS_FS_IOCTL_DEV
        file_rights |= LANDLOCK_ACCESS_FS_IOCTL_DEV;
#endif
        access &= file_rights;
    }
    struct landlock_path_beneath_attr rule = {.allowed_access = access & handled,
                                              .parent_fd = fd};
    if (rule.allowed_access != 0) {
        syscall(SYS_landlock_add_rule, ruleset, LANDLOCK_RULE_PATH_BENEATH, &rule, 0);
    }
    close(fd);
}

/* What `Net` reads without `Fs`: name lookup's configuration, the trusted
 * certificates, and the libraries lookup may load. */
static const char *const LOCK_NET_READS[] = {
    "/etc/hosts",      "/etc/resolv.conf",  "/etc/nsswitch.conf", "/etc/host.conf",
    "/etc/gai.conf",   "/etc/services",     "/etc/ssl",           "/usr/lib/ssl",
    "/usr/share/ca-certificates",           "/lib",               "/usr/lib",
    "/lib64",          "/usr/lib64",
};

/* What `Clock` reads without `Fs`: the system's zone. */
static const char *const LOCK_CLOCK_READS[] = {"/etc/localtime", "/usr/share/zoneinfo"};

/* The Landlock ruleset for `kinds`: the Landlock ABI applied, or 0. */
static int lock_landlock(int64_t kinds) {
    int abi = (int)syscall(SYS_landlock_create_ruleset, NULL, 0, LANDLOCK_CREATE_RULESET_VERSION);
    if (abi < 1) {
        return 0;
    }
    struct landlock_ruleset_attr attr;
    memset(&attr, 0, sizeof attr);
    if (!(kinds & LOCK_FS)) {
        attr.handled_access_fs = lock_fs_rights(abi);
    }
    // Only the fields this ABI (and these headers) know: a newer header's
    // struct is longer, and an older kernel refuses fields it doesn't know.
    size_t size = offsetof(struct landlock_ruleset_attr, handled_access_fs) +
                  sizeof attr.handled_access_fs;
    bool anything = attr.handled_access_fs != 0;
#ifdef LANDLOCK_ACCESS_NET_BIND_TCP
    if (abi >= 4) {
        size = offsetof(struct landlock_ruleset_attr, handled_access_net) +
               sizeof attr.handled_access_net;
        if (!(kinds & LOCK_NET)) {
            attr.handled_access_net =
                LANDLOCK_ACCESS_NET_BIND_TCP | LANDLOCK_ACCESS_NET_CONNECT_TCP;
            anything = true;
        }
    }
#endif
#ifdef LANDLOCK_SCOPE_SIGNAL
    if (abi >= 6) {
        size = offsetof(struct landlock_ruleset_attr, scoped) + sizeof attr.scoped;
        attr.scoped = LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET | LANDLOCK_SCOPE_SIGNAL;
        anything = true;
    }
#endif
    if (!anything) {
        return 0;
    }
    int ruleset = (int)syscall(SYS_landlock_create_ruleset, &attr, size, 0);
    if (ruleset < 0) {
        return 0;
    }
    uint64_t handled = attr.handled_access_fs;
    uint64_t read = LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR;
    if (handled != 0) {
        if (kinds & LOCK_NET) {
            for (size_t i = 0; i < sizeof LOCK_NET_READS / sizeof *LOCK_NET_READS; i++) {
                lock_allow(ruleset, LOCK_NET_READS[i], read | LANDLOCK_ACCESS_FS_EXECUTE,
                           handled);
            }
        }
        if (kinds & LOCK_CLOCK) {
            for (size_t i = 0; i < sizeof LOCK_CLOCK_READS / sizeof *LOCK_CLOCK_READS; i++) {
                lock_allow(ruleset, LOCK_CLOCK_READS[i], read, handled);
            }
        }
        if (kinds & LOCK_PROC) {
            // Started programs need their binaries and libraries, and could
            // read anything anyway; writing stays refused.
            lock_allow(ruleset, "/", read | LANDLOCK_ACCESS_FS_EXECUTE, handled);
        }
        const char *folder = getenv("LUGHA_TEST_DIR");
        if ((kinds & LOCK_TEST_DIR) && folder != NULL && folder[0] == '/') {
            lock_allow(ruleset, folder, handled, handled);
        }
    }
    int done = syscall(SYS_landlock_restrict_self, ruleset, 0) == 0;
    close(ruleset);
    return done ? abi : 0;
}

/* `lugha_rt_lockdown(kinds)`: limits the process to the capability kinds
 * in `kinds` (the `LOCK_*` bits), as far as the kernel can; nothing
 * printed unless `LUGHA_LOCKDOWN=report`. */
void lugha_rt_lockdown(int64_t kinds) {
    // Both seccomp (without privileges) and Landlock need it; inherited too.
    bool private = prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == 0;
    int landlock = private ? lock_landlock(kinds) : 0;
    int seccomp = private ? lock_seccomp(kinds) : 0;
    const char *report = getenv("LUGHA_LOCKDOWN");
    if (report != NULL && strcmp(report, "report") == 0) {
        fprintf(stderr, "lockdown: seccomp %s; landlock %s", seccomp ? "applied" : "not applied",
                landlock ? "applied" : "not applied");
        if (landlock) {
            fprintf(stderr, " (ABI %d)", landlock);
        }
        fprintf(stderr, "; net %s, proc %s, fs %s\n", (kinds & LOCK_NET) ? "held" : "refused",
                (kinds & LOCK_PROC) ? "held" : "refused", (kinds & LOCK_FS) ? "held" : "refused");
    }
}
