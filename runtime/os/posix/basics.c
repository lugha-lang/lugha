/*
 * os/posix/basics.c — part of lugha_rt.c: the layer's memory, signals,
 * standard streams and local time on a POSIX system (Linux and macOS),
 * where the shared parts once called them directly.
 *
 * A fragment, not compiled on its own: joined after the core and the
 * layer's declarations.
 */

/* Address space backed as it is touched: nothing to commit. */
static void *os_reserve(size_t size) {
    void *at = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE,
                    -1, 0);
    return at == MAP_FAILED ? NULL : at;
}

static void os_commit(void *at, size_t size) {
    (void)at;
    (void)size;
}

/* A reserved stack is ready as it is: its pages come as they are touched. */
static void os_stack_ready(char *stack, size_t size) {
    (void)stack;
    (void)size;
}

static void os_stack_trim(char *stack, size_t size) {
    // Advice the kernel may refuse: the pages then stay, still reusable.
    madvise(stack, size, MADV_DONTNEED);
}

static bool os_stack_guard(char *stack, size_t guard) {
    return mprotect(stack, guard, PROT_NONE) == 0;
}

static bool os_stack_unguard(char *stack, size_t guard) {
    return mprotect(stack, guard, PROT_READ | PROT_WRITE) == 0;
}

/* The runtime's Ctrl-C handler, called from SIGINT's. */
static void (*os_on_interrupt)(void);

static void os_sigint(int sig) {
    (void)sig;
    os_on_interrupt();
}

static void os_interrupts_install(void (*on)(void)) {
    os_on_interrupt = on;
    struct sigaction action;
    memset(&action, 0, sizeof action);
    action.sa_handler = os_sigint;
    sigemptyset(&action.sa_mask);
    action.sa_flags = SA_RESTART;
    sigaction(SIGINT, &action, NULL);
}

static void os_worker_signals(void) {
    sigset_t interrupt;
    sigemptyset(&interrupt);
    sigaddset(&interrupt, SIGINT);
    pthread_sigmask(SIG_BLOCK, &interrupt, NULL);
}

static int os_thread_start(pthread_t *thread, const pthread_attr_t *attr, void *(*run)(void *),
                           void *arg) {
    // A new thread takes its creator's signal mask: everything blocked.
    sigset_t all, before;
    sigfillset(&all);
    pthread_sigmask(SIG_SETMASK, &all, &before);
    int failed = pthread_create(thread, attr, run, arg);
    pthread_sigmask(SIG_SETMASK, &before, NULL);
    return failed;
}

/* A terminal takes UTF-8 bytes as they are. */
static void os_write(FILE *f, const char *text, size_t len) {
    fwrite(text, 1, len, f);
}

static int64_t os_read_line(FILE *f, char **line, size_t *cap) {
    return (int64_t)getline(line, cap, f);
}

static void os_stream_lock(FILE *f) {
    flockfile(f);
}

static void os_stream_unlock(FILE *f) {
    funlockfile(f);
}

/* Opens `/dev/null` onto any of stdin, stdout and stderr this program was
 * started without: otherwise the next descriptor opened takes its number,
 * so a file or pipe would receive what was meant for the closed stream, and
 * a program started with a pipe there would lose it. */
static void os_std_streams_open(void) {
    for (int fd = 0; fd < 3; fd++) {
        if (fcntl(fd, F_GETFD) < 0 && errno == EBADF) {
            int null = open("/dev/null", fd == 0 ? O_RDONLY : O_WRONLY);
            // Taking the lowest free number, it lands on `fd`; failing, the
            // program runs as it was started.
            if (null >= 0 && null != fd) {
                close(null);
            }
        }
    }
}

static int os_dup(int fd) {
    return fcntl(fd, F_DUPFD_CLOEXEC, 0);
}

/* C's arguments are the system's own bytes. */
static void os_args(int32_t *argc, char ***argv) {
    (void)argc;
    (void)argv;
}

static char *os_env(const char *name) {
    const char *value = getenv(name);
    return value == NULL ? NULL : strdup(value);
}

/* Through `localtime_r` (`TZ`, else `/etc/localtime`). */
static bool os_local_offset(int64_t seconds, int64_t *out) {
    time_t t = (time_t)seconds;
    struct tm parts;
    if ((int64_t)t != seconds || localtime_r(&t, &parts) == NULL) {
        return false;
    }
    *out = (int64_t)parts.tm_gmtoff;
    return true;
}

/* 64 random bits from `/dev/urandom`, for a system whose own call failed. */
static bool posix_urandom64(uint64_t *out) {
    int fd = open("/dev/urandom", O_RDONLY | O_CLOEXEC);
    if (fd < 0) {
        return false;
    }
    ssize_t got = read(fd, out, sizeof *out);
    close(fd);
    return got == (ssize_t)sizeof *out;
}

/* Name lookups: the C library's own words, and interfaces by name. */
static const char *os_lookup_message(int code) {
#if defined(__ANDROID__)
    // Android's resolver answers a name that doesn't exist with "no
    // address" (`EAI_NODATA`), where glibc says it isn't known: glibc's
    // words, as on Windows.
    if (code == EAI_NODATA) {
        return "Name or service not known";
    }
#endif
    return gai_strerror(code);
}

static void os_scope_name(char *host, size_t cap) {
    (void)host;
    (void)cap;
}

static bool os_scope_numeric(const char *host, size_t len, char *out, size_t cap) {
    (void)host;
    (void)len;
    (void)out;
    (void)cap;
    return false;
}
