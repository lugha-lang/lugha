/*
 * os/layer.c — part of lugha_rt.c: the operating-system layer every system
 * implements in its own `os/<system>.c` (today `os/linux.c`): each event,
 * wake-up, wait on a word and system fact the shared parts need. They call
 * the operating system through nothing else.
 *
 * Events come back from one poll: on POSIX a socket is armed for one
 * wake-up per direction (`os_poll_arm`) and comes back as its descriptor
 * and the directions ready; on Windows an operation's completion comes
 * back with its waiting task's token; two signals other threads raise
 * (the helpers' and the workers' wake-up) come back with fixed tokens.
 * Waiting on a word is how workers park and the runtime's locks wait.
 *
 * A fragment, not compiled on its own: joined right after the core, before
 * the system's own file. The declarations only POSIX systems implement
 * (os/posix/'s helpers) are left out of a Windows build.
 */

/* The operating-system layer (os/<system>.c): every event, wake-up,
 * wait on a word and system fact the shared parts need. */

/* An event from `os_poll_wait`: a socket's (`fd` >= 0, ready for the
 * directions in `ready`), or else the token its source carries. */
typedef struct {
    uint64_t token;
    int fd;
    uint32_t ready;
} OsEvent;

/* A signal one thread raises to wake the poll (`os_signal_new`). */
typedef int64_t OsSignal;

/* What `os_poll_arm` waits for a socket to be ready for. */
enum { OS_READ = 1, OS_WRITE = 2 };

/* The tokens of the two signals in the poll: never a fiber's (aligned and
 * above the first page). */
enum { OS_TOKEN_HELPERS = 1, OS_TOKEN_WAKE = 2 };

static void os_poll_open(void);
#if !defined(_WIN32)
/* `fd` armed for exactly the directions in `want`, each for one event;
 * nothing for a descriptor closed meanwhile (whoever waits on it is woken
 * by the close). `os_poll_disarm`: armed for none. */
static void os_poll_arm(int fd, uint32_t want);
static void os_poll_disarm(int fd);
#endif
static int os_poll_wait(OsEvent *out, int max, int timeout_ms);
static OsSignal os_signal_new(uint64_t token);
static void os_signal_raise(OsSignal s);
static void os_signal_take(OsSignal s);
static void os_wait_on(uint32_t *word, uint32_t expected);
static void os_wake_on(uint32_t *word, int n);
static int os_cpus(void);
static bool os_random64(uint64_t *out);
static bool os_thread_stack(void **base, size_t *size);

/* Memory. `os_reserve`: `size` bytes of address space with no memory
 * behind them yet, or NULL; `os_commit` makes part of it usable (POSIX
 * systems back pages as they are touched, so it does nothing there).
 * A task's stack: `os_stack_ready` makes a carved or trimmed stack ready
 * to run on; `os_stack_trim` gives a waiting stack's pages back;
 * `os_stack_guard` makes its lowest `guard` bytes fault, for C, which has
 * no entry checks, and `os_stack_unguard` undoes it (false on failure). */
static void *os_reserve(size_t size);
static void os_commit(void *at, size_t size);
static void os_stack_ready(char *stack, size_t size);
static void os_stack_trim(char *stack, size_t size);
static bool os_stack_guard(char *stack, size_t guard);
static bool os_stack_unguard(char *stack, size_t guard);

/* Faults and Ctrl-C. `os_faults_install`: a C stack's overflow reported
 * as `panic: stack overflow`, as the program starts; `os_thread_faults`
 * readies a worker thread for it. `os_interrupts_install`: `on` called at
 * each Ctrl-C (in a signal handler or a thread of the system's: it may
 * only count and wake). `os_worker_signals`: a worker thread leaves Ctrl-C
 * to the main thread. `os_thread_start`: a helper thread started with
 * every signal blocked; pthread_create's result. */
static void os_faults_install(void);
static void os_thread_faults(void);
static void os_interrupts_install(void (*on)(void));
static void os_worker_signals(void);
static int os_thread_start(pthread_t *thread, const pthread_attr_t *attr, void *(*run)(void *),
                           void *arg);

/* The standard streams. `os_write`: `len` bytes of text to `f` (stdout or
 * stderr), as a console shows UTF-8 where it is one. `os_read_line`: the
 * next line of `f` with its `\n`, as `getline`. `os_stream_lock` and
 * `os_stream_unlock`: a stream's own lock, which nests. `os_std_streams_open`:
 * a stream the program was started without opened onto nothing, as it
 * starts. `os_dup`: a copy of a descriptor, -1 and `errno` on failure. */
static void os_write(FILE *f, const char *text, size_t len);
static int64_t os_read_line(FILE *f, char **line, size_t *cap);
static void os_stream_lock(FILE *f);
static void os_stream_unlock(FILE *f);
static void os_std_streams_open(void);
static int os_dup(int fd);

/* The program's arguments and environment as UTF-8. `os_args`: C's
 * `argc` and `argv`, replaced where the system gives them another way.
 * `os_env`: a variable's value as a copy from `malloc` the caller frees,
 * or NULL when it is unset. */
static void os_args(int32_t *argc, char ***argv);
static char *os_env(const char *name);

/* The system zone's offset from UTC at `seconds` since 1970, in seconds
 * east, daylight saving included; false for a moment it can't give. */
static bool os_local_offset(int64_t seconds, int64_t *out);

/* `OS_TLS_TRUST`: 1 where the system's own verifier decides whether a TLS
 * client trusts a server, through `os_tls_trust` (os/darwin/trust.c); 0
 * where OpenSSL checks against the system's roots. Each system defines it:
 * an OpenSSL built for a system needn't know where that system keeps its
 * roots, or which it has stopped trusting. */

/* Sockets, at the level of operations: each call opens, connects, accepts,
 * receives or sends, waiting as a task does (parking the task, never the
 * thread) until it is done or `deadline` (monotonic ns, < 0 for none)
 * passes. How a system waits is its own: readiness and a retry on POSIX
 * (os/posix/sockets.c), a completion on Windows. */

/* A socket: a descriptor on POSIX; a Winsock SOCKET fits as well. */
typedef int64_t OsSocket;

/* What a socket operation gives instead of a count or a socket. A failure
 * sets `*message` to the system's words. `OS_CANCELLED` comes back only
 * once the system has let go of the operation's buffer, so the caller may
 * free it and unwind. `OS_CLOSED`: another task closed the socket. */
enum { OS_FAILED = -1, OS_TIMED_OUT = -2, OS_CANCELLED = -3, OS_CLOSED = -4 };

static OsSocket os_sock_open(int family, int type, int protocol, const char **message);
static int64_t os_sock_listen(OsSocket s, const struct sockaddr *at, socklen_t len, int backlog,
                              const char **message);
static int64_t os_sock_bind_udp(OsSocket s, const struct sockaddr *at, socklen_t len, bool shared,
                                const char **message);
static int64_t os_sock_connect(OsSocket s, const struct sockaddr *at, socklen_t len,
                               int64_t deadline, const char **message);
static OsSocket os_sock_accept(OsSocket s, int64_t deadline, const char **message);
static int64_t os_sock_recv(OsSocket s, char *buf, int64_t max, int64_t deadline,
                            const char **message);
static int64_t os_sock_send(OsSocket s, const char *buf, int64_t n, int64_t deadline,
                            const char **message);
static int64_t os_sock_recvfrom(OsSocket s, char *buf, int64_t max, struct sockaddr_storage *from,
                                socklen_t *len, int64_t deadline, const char **message);
static int64_t os_sock_sendto(OsSocket s, const char *buf, int64_t n, const struct sockaddr *to,
                              socklen_t len, int64_t deadline, const char **message);
static int64_t os_sock_name(OsSocket s, bool peer, struct sockaddr_storage *at, socklen_t *len,
                            const char **message);
static void os_sock_close(OsSocket s);

/* Names. `os_lookup_message`: a name lookup's failure (`getaddrinfo`'s
 * code) in glibc's words, on every system. A link-local IPv6 address
 * carries its interface: `os_scope_name` turns a number after `%` in the
 * NUL-terminated `host` (`cap` bytes) into the interface's name, as
 * `net_interfaces` names it; `os_scope_numeric` writes `host` (`len`
 * bytes, no NUL) into `out` with a name after `%` turned into its number,
 * true if it did, false to use `host` as it is. Nothing to do where the
 * system's own calls take and give names. */
static const char *os_lookup_message(int code);
static void os_scope_name(char *host, size_t cap);
static bool os_scope_numeric(const char *host, size_t len, char *out, size_t cap);

/* A descriptor a helper writes to: a write to a pipe whose reader is gone
 * is `EPIPE` through it, never SIGPIPE ending the program. Linux sends
 * SIGPIPE to the writing thread, and a helper blocks it, so nothing is
 * needed there; macOS sends it to the process, which delivers it to a
 * thread that doesn't block it. */
static void os_no_sigpipe(int fd);

/* What a system sets up once, as the program starts. */
static void os_start(void);

#if !defined(_WIN32)
/* 64 random bits from `/dev/urandom` (os/posix/basics.c), for a system
 * whose own call failed. */
static bool posix_urandom64(uint64_t *out);

/* What a POSIX system's own file gives os/posix/: a non-blocking,
 * close-on-exec socket and accepted connection, and a socket hearing only
 * the multicast groups it joined; with the flag `OS_SEND_FLAGS` (a closed
 * peer an error, not SIGPIPE) as a macro. A system's file also defines
 * `OS_MONOTONIC`, the clock deadlines and sleeps are measured on, which
 * stops while the machine is suspended. */
static int os_sock_raw(int family, int type, int protocol);
static int os_accept_raw(int s);
static bool os_udp_own_groups(int s, int family);

/* Starting programs (os/posix/process.c): a pipe whose two ends are
 * closed in any program started after (0, or -1 and `errno`); and what a
 * system adds to a start, given whether the child shares this program's
 * stdin, stdout and stderr, as extra `posix_spawnattr` flags. */
static int os_pipe(int fds[2]);
static short os_spawn_prepare(posix_spawn_file_actions_t *actions, bool share);

/* Sets the length byte a system's socket addresses carry (macOS's
 * `sa_len`), `len` the address's size; nothing where they carry none. */
static void os_sockaddr_len(struct sockaddr *at, socklen_t len);

/* Folders (os/<system>/dir_resolve.c, before os/posix/dir.c): every path
 * a `Dir` is given is looked up beneath its folder's descriptor, never
 * leaving it, whatever runs at the same time. Each gives -1 with `errno`
 * on failure; `EXDEV` means the path leads outside the folder. A system's
 * file also defines `OS_DIR_FLAGS`, how a `Dir`'s own folder is opened:
 * search permission is all it needs.
 *
 * `os_dir_resolve`: `path` opened with `flags` (and `mode` when it
 * creates), close-on-exec. `os_dir_stat`: what `path` names, links inside
 * followed. `os_dir_chmod`: the same lookup, its mode set; never a link's
 * target outside. `os_dir_where`: where the folder `fd` is now, its length
 * (no NUL). `os_dir_message`: a system's own words for a lookup's `errno`,
 * or NULL for the usual ones. */
static int os_dir_resolve(int dirfd, const char *path, int flags, mode_t mode);
static int os_dir_stat(int dirfd, const char *path, struct stat *st);
static int os_dir_chmod(int dirfd, const char *path, mode_t mode);
static ssize_t os_dir_where(int fd, char *buf, size_t len);
static const char *os_dir_message(int code);
#endif
