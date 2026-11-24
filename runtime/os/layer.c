/*
 * os/layer.c — part of lugha_rt.c: the operating-system layer every system
 * implements in its own `os/<system>.c` (today `os/linux.c`): each event,
 * wake-up, wait on a word and system fact the shared parts need. They call
 * the operating system through nothing else.
 *
 * Events: sockets are armed for one wake-up each (`os_poll_arm`) and come
 * back with the token they were armed with; two signals other threads raise
 * (the helpers' and the workers' wake-up) come back with fixed tokens.
 * Waiting on a word is how workers park and the runtime's locks wait.
 *
 * A fragment, not compiled on its own: joined right after the core, before
 * the system's own file.
 */

/* The operating-system layer (os/<system>.c): every event, wake-up,
 * wait on a word and system fact the shared parts need. */

/* An event from `os_poll_wait`: the token its source was armed with. */
typedef struct {
    uint64_t token;
} OsEvent;

/* A signal one thread raises to wake the poll (`os_signal_new`). */
typedef int64_t OsSignal;

/* What `os_poll_arm` waits for a socket to be ready for. */
enum { OS_READ = 1, OS_WRITE = 2 };

/* The tokens of the two signals in the poll: never a fiber's (aligned and
 * above the first page). */
enum { OS_TOKEN_HELPERS = 1, OS_TOKEN_WAKE = 2 };

static void os_poll_open(void);
static void os_poll_arm(int fd, uint32_t want, uint64_t token);
static void os_poll_disarm(int fd);
static int os_poll_wait(OsEvent *out, int max, int timeout_ms);
static OsSignal os_signal_new(uint64_t token);
static void os_signal_raise(OsSignal s);
static void os_signal_take(OsSignal s);
static void os_wait_on(uint32_t *word, uint32_t expected);
static void os_wake_on(uint32_t *word, int n);
static int os_cpus(void);
static bool os_random64(uint64_t *out);
static bool os_thread_stack(void **base, size_t *size);

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

/* A descriptor a helper writes to: a write to a pipe whose reader is gone
 * is `EPIPE` through it, never SIGPIPE ending the program. Linux sends
 * SIGPIPE to the writing thread, and a helper blocks it, so nothing is
 * needed there; macOS sends it to the process, which delivers it to a
 * thread that doesn't block it. */
static void os_no_sigpipe(int fd);

/* What a system sets up once, as the program starts. */
static void os_start(void);

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
