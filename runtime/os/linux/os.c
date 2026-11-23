/*
 * os/linux/os.c — part of lugha_rt.c: the runtime's operating-system layer
 * for Linux, with Linux's own headers. Shared fragments reach the OS only
 * through these `os_*` functions, declared in the core; another system
 * supplies the same functions in a file of its own.
 *
 * Events come from one epoll set: a socket armed for one wake-up, and two
 * eventfds, the helpers' and the workers' wake-up, each carrying back the
 * token it was registered with. Waiting on a word is a futex.
 *
 * A fragment, not compiled on its own: joined right after the core, before
 * every other part, so the headers below come before the code needing them
 * (lockdown.c's seccomp and Landlock, dir.c's openat2).
 */

#include <linux/audit.h>
#include <linux/filter.h>
#include <linux/futex.h>
#include <linux/landlock.h>
#include <linux/openat2.h>
#include <linux/seccomp.h>
#include <sys/epoll.h>
#include <sys/eventfd.h>
#include <sys/prctl.h>
#include <sys/random.h>
#include <sys/syscall.h>

#if !defined(__x86_64__) && !defined(__aarch64__)
#error "green threads need a stack switch for this CPU (os/linux/ has x86_64.c and aarch64.c)"
#endif

/* The event set, made at the first socket wait, job or wake-up. */
static int os_poll_fd = -1;

static void os_poll_open(void) {
    if (os_poll_fd < 0) {
        os_poll_fd = epoll_create1(EPOLL_CLOEXEC);
        if (os_poll_fd < 0) {
            die("panic: epoll_create1 failed\n");
        }
    }
}

/* `fd` reported once, with `token`, when ready for `want` (OS_READ,
 * OS_WRITE); armed again for each wait. */
static void os_poll_arm(int fd, uint32_t want, uint64_t token) {
    uint32_t events = EPOLLONESHOT | ((want & OS_READ) != 0 ? EPOLLIN : 0) |
                      ((want & OS_WRITE) != 0 ? EPOLLOUT : 0);
    struct epoll_event ev = {.events = events, .data = {.u64 = token}};
    if (epoll_ctl(os_poll_fd, EPOLL_CTL_MOD, fd, &ev) != 0 &&
        epoll_ctl(os_poll_fd, EPOLL_CTL_ADD, fd, &ev) != 0) {
        die("panic: epoll_ctl failed\n");
    }
}

/* `fd` reported no more, until armed again. */
static void os_poll_disarm(int fd) {
    struct epoll_event off = {.events = 0, .data = {.ptr = NULL}};
    // Failing means it was never armed, or is closed: nothing to stop.
    (void)epoll_ctl(os_poll_fd, EPOLL_CTL_MOD, fd, &off);
}

/* Waits up to `timeout_ms` (< 0 for ever, 0 not at all) for events; up to
 * `max` of them, each its token, into `out`. Their count, 0 when none. */
static int os_poll_wait(OsEvent *out, int max, int timeout_ms) {
    struct epoll_event events[64];
    int n = epoll_wait(os_poll_fd, events, max < 64 ? max : 64, timeout_ms);
    for (int i = 0; i < n; i++) {
        out[i].token = events[i].data.u64;
    }
    return n < 0 ? 0 : n;
}

/* A signal another thread raises to wake the poll, reported with `token`;
 * -1 if it can't be made. */
static OsSignal os_signal_new(uint64_t token) {
    os_poll_open();
    int fd = eventfd(0, EFD_CLOEXEC | EFD_NONBLOCK);
    struct epoll_event ev = {.events = EPOLLIN, .data = {.u64 = token}};
    if (fd < 0 || epoll_ctl(os_poll_fd, EPOLL_CTL_ADD, fd, &ev) != 0) {
        return -1;
    }
    return fd;
}

/* Raises `s`; safe in a signal handler. Raised already is as good. */
static void os_signal_raise(OsSignal s) {
    uint64_t one = 1;
    while (write((int)s, &one, sizeof one) < 0 && errno == EINTR) {
    }
}

/* `s` was reported: lowered, so the next raise is reported too. */
static void os_signal_take(OsSignal s) {
    uint64_t count;
    // Nothing to read is fine: one read takes every raise since the last.
    while (read((int)s, &count, sizeof count) < 0 && errno == EINTR) {
    }
}

/* Waits while `*word` is `expected`, or until woken (spuriously too). */
static void os_wait_on(uint32_t *word, uint32_t expected) {
    syscall(SYS_futex, word, FUTEX_WAIT_PRIVATE, expected, NULL, NULL, 0);
}

/* Wakes up to `n` threads waiting on `word`. */
static void os_wake_on(uint32_t *word, int n) {
    syscall(SYS_futex, word, FUTEX_WAKE_PRIVATE, n, NULL, NULL, 0);
}

/* The number of CPUs this process may run on. */
static int os_cpus(void) {
    cpu_set_t set;
    if (sched_getaffinity(0, sizeof set, &set) == 0) {
        int n = CPU_COUNT(&set);
        return n > 0 ? n : 1;
    }
    long n = sysconf(_SC_NPROCESSORS_ONLN);
    return n > 0 ? (int)n : 1;
}

/* 64 random bits from the kernel into `out`; false if it has none to give. */
static bool os_random64(uint64_t *out) {
    ssize_t got;
    do {
        got = getrandom(out, sizeof *out, 0);
    } while (got < 0 && errno == EINTR);
    return got == (ssize_t)sizeof *out;
}

/* The running thread's stack: its lowest address and size; false if the
 * system won't say. */
static bool os_thread_stack(void **base, size_t *size) {
    pthread_attr_t attr;
    if (pthread_getattr_np(pthread_self(), &attr) != 0) {
        return false;
    }
    int got = pthread_attr_getstack(&attr, base, size);
    pthread_attr_destroy(&attr);
    return got == 0;
}
