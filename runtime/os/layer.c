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
