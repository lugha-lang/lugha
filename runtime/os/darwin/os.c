/*
 * os/darwin/os.c — part of lugha_rt.c: the runtime's operating-system
 * layer for macOS (os/layer.c), with macOS's own headers.
 *
 * Events come from one kqueue: a socket armed for one wake-up
 * (EVFILT_READ or EVFILT_WRITE, one-shot) and the two signals other
 * threads raise (EVFILT_USER, cleared as reported), each carrying back its
 * token. Waiting on a word is `os_sync_wait_on_address` (macOS 14.4).
 *
 * The running thread's worker is found through a pthread key: macOS gives
 * each thread its thread-locals where it first touches them, so no fixed
 * offset from the thread pointer reaches them as on Linux.
 *
 * A fragment, not compiled on its own: joined right after the core and the
 * layer, before every other part.
 */

#include <sys/event.h>
#include <sys/resource.h>
#include <sys/random.h>
#include <sys/sysctl.h>

#if !defined(__aarch64__)
#error "lughac builds macOS programs for Apple Silicon only (os/darwin/ has aarch64.c)"
#endif

/* Apple's <os/os_sync_wait_on_address.h> (macOS 14.4), which zig's SDK
 * lacks; libSystem has the functions. A flags word of 0 is the default. */
int os_sync_wait_on_address(void *addr, uint64_t value, size_t size, uint32_t flags);
int os_sync_wake_by_address_any(void *addr, size_t size, uint32_t flags);
int os_sync_wake_by_address_all(void *addr, size_t size, uint32_t flags);

/* The clock deadlines and sleeps are measured on: macOS's
 * `CLOCK_MONOTONIC` keeps counting while the Mac sleeps, its
 * `CLOCK_UPTIME_RAW` stops, as Linux's `CLOCK_MONOTONIC` does. */
#define OS_MONOTONIC CLOCK_UPTIME_RAW

/* The key holding each thread's worker, made before `main`. */
static pthread_key_t os_worker_key;

__attribute__((constructor)) static void os_worker_key_init(void) {
    if (pthread_key_create(&os_worker_key, NULL) != 0) {
        die("panic: can't make the workers' thread key\n");
    }
}

/* This thread's worker, the first time it asks: its own `this_worker`,
 * its address taken here, on the thread, and kept in the key. */
__attribute__((noinline)) static Worker *os_worker_first(void) {
    Worker *w = &this_worker;
    if (pthread_setspecific(os_worker_key, w) != 0) {
        die("panic: can't keep a worker in its thread key\n");
    }
    return w;
}

/* The running thread's worker, read fresh: a task may move to another
 * thread at a switch. */
static inline Worker *os_worker(void) {
    Worker *w = pthread_getspecific(os_worker_key);
    return w != NULL ? w : os_worker_first();
}

/* The event queue, made at the first socket wait, job or wake-up. */
static int os_poll_fd = -1;

static void os_poll_open(void) {
    if (os_poll_fd < 0) {
        os_poll_fd = kqueue();
        if (os_poll_fd < 0 || fcntl(os_poll_fd, F_SETFD, FD_CLOEXEC) != 0) {
            die("panic: kqueue failed\n");
        }
    }
}

/* `fd` reported once per direction, as itself, when ready for `want`
 * (OS_READ, OS_WRITE: a filter each); armed again for each wait. */
static void os_poll_arm(int fd, uint32_t want) {
    struct kevent ev;
    // A filter not wanted is taken away; failing means it was never armed,
    // has fired, or `fd` is closed: nothing to stop then.
    if ((want & OS_READ) == 0) {
        EV_SET(&ev, (uintptr_t)fd, EVFILT_READ, EV_DELETE, 0, 0, NULL);
        (void)kevent(os_poll_fd, &ev, 1, NULL, 0, NULL);
    }
    if ((want & OS_WRITE) == 0) {
        EV_SET(&ev, (uintptr_t)fd, EVFILT_WRITE, EV_DELETE, 0, 0, NULL);
        (void)kevent(os_poll_fd, &ev, 1, NULL, 0, NULL);
    }
    struct kevent add[2];
    int n = 0;
    if ((want & OS_READ) != 0) {
        EV_SET(&add[n++], (uintptr_t)fd, EVFILT_READ, EV_ADD | EV_ONESHOT, 0, 0, NULL);
    }
    if ((want & OS_WRITE) != 0) {
        EV_SET(&add[n++], (uintptr_t)fd, EVFILT_WRITE, EV_ADD | EV_ONESHOT, 0, 0, NULL);
    }
    if (n > 0 && kevent(os_poll_fd, add, n, NULL, 0, NULL) != 0 && errno != EBADF) {
        die("panic: kevent failed\n");
    }
}

/* `fd` reported no more, until armed again. */
static void os_poll_disarm(int fd) {
    os_poll_arm(fd, 0);
}

/* Waits up to `timeout_ms` (< 0 for ever, 0 not at all) for events; up to
 * `max` of them into `out`. Their count, 0 when none. */
static int os_poll_wait(OsEvent *out, int max, int timeout_ms) {
    struct kevent events[64];
    struct timespec wait = {timeout_ms / 1000, (long)(timeout_ms % 1000) * 1000000};
    int n = kevent(os_poll_fd, NULL, 0, events, max < 64 ? max : 64,
                   timeout_ms < 0 ? NULL : &wait);
    for (int i = 0; i < n; i++) {
        int16_t filter = events[i].filter;
        bool user = filter == EVFILT_USER;
        // A signal's ident is its token; a socket's is its descriptor.
        out[i].token = user ? (uint64_t)events[i].ident : 0;
        out[i].fd = user ? -1 : (int)events[i].ident;
        out[i].ready = user ? 0 : filter == EVFILT_READ ? OS_READ : OS_WRITE;
    }
    return n < 0 ? 0 : n;
}

/* A signal another thread raises to wake the poll, reported with `token`
 * (its identity too: tokens are few and fixed); -1 if it can't be made. */
static OsSignal os_signal_new(uint64_t token) {
    os_poll_open();
    struct kevent ev;
    EV_SET(&ev, (uintptr_t)token, EVFILT_USER, EV_ADD | EV_CLEAR, 0, 0, (void *)(uintptr_t)token);
    return kevent(os_poll_fd, &ev, 1, NULL, 0, NULL) == 0 ? (OsSignal)token : -1;
}

/* Raises `s`; safe in a signal handler. Raised already is as good. */
static void os_signal_raise(OsSignal s) {
    struct kevent ev;
    // The token again: kevent replaces udata on every change to an event.
    EV_SET(&ev, (uintptr_t)s, EVFILT_USER, 0, NOTE_TRIGGER, 0, (void *)(uintptr_t)s);
    // Fails only if the queue is gone, when nothing waits on it.
    (void)kevent(os_poll_fd, &ev, 1, NULL, 0, NULL);
}

/* `s` was reported: EV_CLEAR lowered it as it was, so nothing to do. */
static void os_signal_take(OsSignal s) {
    (void)s;
}

/* Waits while `*word` is `expected`, or until woken (spuriously too). */
static void os_wait_on(uint32_t *word, uint32_t expected) {
    // Woken, interrupted or changed meanwhile: the caller looks again.
    (void)os_sync_wait_on_address(word, expected, sizeof *word, 0);
}

/* Wakes up to `n` threads waiting on `word`: one, or all of them. */
static void os_wake_on(uint32_t *word, int n) {
    // No thread waiting is an error here, and nothing to do.
    if (n <= 1) {
        (void)os_sync_wake_by_address_any(word, sizeof *word, 0);
    } else {
        (void)os_sync_wake_by_address_all(word, sizeof *word, 0);
    }
}

/* The number of CPUs this process may run on. */
static int os_cpus(void) {
    int n = 0;
    size_t size = sizeof n;
    if (sysctlbyname("hw.activecpu", &n, &size, NULL, 0) == 0 && n > 0) {
        return n;
    }
    long online = sysconf(_SC_NPROCESSORS_ONLN);
    return online > 0 ? (int)online : 1;
}

/* 64 random bits from the kernel into `out`: getentropy, else
 * /dev/urandom; false if it has none to give. */
static bool os_random64(uint64_t *out) {
    return getentropy(out, sizeof *out) == 0 || posix_urandom64(out);
}

/* The running thread's stack: its lowest address and size (macOS gives
 * its top). */
static bool os_thread_stack(void **base, size_t *size) {
    pthread_t self = pthread_self();
    char *top = pthread_get_stackaddr_np(self);
    *size = pthread_get_stacksize_np(self);
    *base = top - *size;
    return top != NULL && *size > 0;
}

/* Sockets' macOS details for os/posix/sockets.c: macOS takes no flags at
 * `socket` or `accept`, so non-blocking and close-on-exec are set just
 * after (a program another thread starts in between could inherit the
 * socket, as with Go's and libuv's macOS sockets); no SIGPIPE, through
 * `SO_NOSIGPIPE` on the socket itself. */
#define OS_SEND_FLAGS 0

static bool os_sock_setup(int s) {
    int one = 1;
    int flags = fcntl(s, F_GETFL);
    return flags >= 0 && fcntl(s, F_SETFL, flags | O_NONBLOCK) == 0 &&
           fcntl(s, F_SETFD, FD_CLOEXEC) == 0 &&
           setsockopt(s, SOL_SOCKET, SO_NOSIGPIPE, &one, sizeof one) == 0;
}

/* `s`, just made or accepted, set up; or -1, closed, `errno` saying why. */
static int os_sock_ready(int s) {
    if (s >= 0 && !os_sock_setup(s)) {
        int saved = errno;
        close(s);
        errno = saved;
        return -1;
    }
    return s;
}

static int os_sock_raw(int family, int type, int protocol) {
    return os_sock_ready(socket(family, type, protocol));
}

static int os_accept_raw(int s) {
    return os_sock_ready(accept(s, NULL, NULL));
}

/* macOS hands a socket only the multicast groups it joined itself. */
static bool os_udp_own_groups(int s, int family) {
    (void)s;
    (void)family;
    return true;
}

/* A pipe, both ends close-on-exec: macOS before 27 has no `pipe2`, so set
 * just after. A program started in between by our own code can't inherit
 * them (`os_spawn_prepare`); one started by foreign C code could, as with
 * Go's, Rust's and libuv's pipes on macOS. */
static int os_pipe(int fds[2]) {
    if (pipe(fds) != 0) {
        return -1;
    }
    if (fcntl(fds[0], F_SETFD, FD_CLOEXEC) != 0 || fcntl(fds[1], F_SETFD, FD_CLOEXEC) != 0) {
        int saved = errno;
        close(fds[0]);
        close(fds[1]);
        errno = saved;
        return -1;
    }
    return 0;
}

/* A write through `fd` to a pipe whose reader is gone is `EPIPE`, never
 * SIGPIPE: the flag is the open file's, shared by its copies. */
static void os_no_sigpipe(int fd) {
    // Fails only for a bad descriptor, whose write fails too.
    (void)fcntl(fd, F_SETNOSIGPIPE, 1);
}

/* The open-file limit: macOS's soft one is 256 by default, which a server
 * outgrows; raised to Linux's usual 1024 (never past the hard limit), a
 * number a started program using `select` can still live with, as it
 * inherits it. */
static void os_start(void) {
    struct rlimit files;
    if (getrlimit(RLIMIT_NOFILE, &files) == 0 && files.rlim_cur < 1024) {
        files.rlim_cur = files.rlim_max < 1024 ? files.rlim_max : 1024;
        // Refused, the program keeps the limit it was given.
        (void)setrlimit(RLIMIT_NOFILE, &files);
    }
}

/* Every descriptor the file actions don't name is closed in the child,
 * atomically (`POSIX_SPAWN_CLOEXEC_DEFAULT`); a child sharing this
 * program's stdin, stdout and stderr gets them named. */
static short os_spawn_prepare(posix_spawn_file_actions_t *actions, bool share) {
    if (share) {
        for (int fd = 0; fd < 3; fd++) {
            posix_spawn_file_actions_addinherit_np(actions, fd);
        }
    }
    return POSIX_SPAWN_CLOEXEC_DEFAULT;
}

/* `posix_spawnp` itself says why a program can't start. */
static int os_spawn_check(const char *cmd) {
    (void)cmd;
    return 0;
}

/* macOS's socket addresses carry their length, which its kernel checks. */
static void os_sockaddr_len(struct sockaddr *at, socklen_t len) {
    at->sa_len = (uint8_t)len;
}
