/*
 * lugha_rt.c — the Lugha runtime library.
 *
 * Linked into every program. Provides heap allocation with reference
 * counting (rc_alloc.c, rc.c), panics, and the printing and formatting behind the `print`,
 * `println` and `to_string` intrinsics. All output goes through C's `stdout`
 * so it interleaves with libc calls in program order.
 *
 * ABI notes: `bool` and `u8` arguments arrive zero-extended to 32 bits, so
 * no function depends on how a C compiler extends narrower parameters.
 *
 * Platform: Linux with glibc (pthread_getattr_np for the stack bounds).
 *
 * Source layout: this core comes first (it enables GNU extensions and holds
 * every include, type and declaration), then stack.c (the stack overflow
 * handler), then print.c (formatting and printing), then input.c (arguments
 * and parsing). lughac joins the four, in that order, into the single
 * lugha_rt.c it compiles with each program.
 */

#define _GNU_SOURCE

#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <arpa/inet.h>
#include <netinet/in.h>
#include <net/if.h>
#include <ifaddrs.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <spawn.h>
#include <linux/audit.h>
#include <linux/filter.h>
#include <linux/landlock.h>
#include <linux/openat2.h>
#include <linux/seccomp.h>
#include <sched.h>
#if defined(__SANITIZE_THREAD__)
#include <sanitizer/tsan_interface.h>
#endif
#include <stddef.h>
#include <sys/prctl.h>
#include <sys/epoll.h>
#include <sys/eventfd.h>
#include <sys/mman.h>
#include <sys/random.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <linux/futex.h>
#include <sys/wait.h>
#include <unistd.h>
#include <inttypes.h>
#include <math.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unwind.h>

/* A Lugha string: length, then the bytes and a terminating NUL. */
typedef struct {
    int64_t len;
    char bytes[];
} LughaString;

/* A Lugha array: length, capacity, then `cap` elements. */
typedef struct {
    int64_t len;
    int64_t cap;
    unsigned char elems[];
} LughaArray;

void lugha_rt_init(int32_t argc, char **argv);
LughaArray *lugha_rt_args(void);
int32_t lugha_rt_parse_f64(const LughaString *s, double *out);
int32_t lugha_rt_read_line(LughaString **out);
void lugha_rt_eprint(const LughaString *text, int32_t newline);
void lugha_rt_exit(int32_t code);
void *lugha_rt_alloc(int64_t size);
void lugha_rt_panic(const LughaString *msg, const char *file, int64_t line, int64_t col);
void lugha_rt_panic_bounds(int64_t len, int64_t index, const char *file, int64_t line, int64_t col);
LughaString *lugha_rt_str_concat(const LughaString *a, const LughaString *b);
LughaString *lugha_rt_str_slice(const LughaString *s, int64_t from, int64_t to, const char *file,
                                int64_t line, int64_t col);
LughaArray *lugha_rt_array_grow(const LughaArray *array, int64_t elem_size);
int32_t lugha_rt_str_eq(const LughaString *a, const LughaString *b);
int32_t lugha_rt_str_cmp(const LughaString *a, const LughaString *b);
void lugha_rt_print_i32(int32_t value);
void lugha_rt_print_i64(int64_t value);
void lugha_rt_print_u8(uint32_t value);
void lugha_rt_print_f64(double value);
void lugha_rt_print_bool(int32_t value);
void lugha_rt_print_str(const LughaString *value);
void lugha_rt_print_newline(void);
LughaString *lugha_rt_to_string_i32(int32_t value);
LughaString *lugha_rt_to_string_i64(int64_t value);
LughaString *lugha_rt_to_string_u8(uint32_t value);
LughaString *lugha_rt_to_string_f64(double value);
LughaString *lugha_rt_to_string_bool(int32_t value);

/* Room for any formatted primitive: the longest is an f64 in fixed notation
 * just below 1e16 with 17 significant digits, or "-9223372036854775808". */
enum { FORMAT_CAP = 64 };

/* Called by the C `main` before the program starts. */
static void install_stack_overflow_handler(void);

/* A scalar value's UTF-8 (`chars.c`), also used by `format.c`'s padding,
 * which comes earlier in the joined file. */
static int utf8_encode(uint32_t code, char *out);

/* Ends the program after a panic, or only the running actor (see
 * actors.c); `text` is what followed "panic: ". */
static void panic_end(const char *text, size_t len);
/* Unwinds the running task's stack after a panic (unwind.c), then
 * `panic_end`; never returns. */
static void panic_unwind(const char *text, size_t len);
static bool panic_caught(void);
static void install_interrupt_handler(void);
static void sched_init(void);
static void hash_key_init(void);
static void helpers_init(void);
struct Panic;
static void resume_unwind(struct Panic *panic);
/* What a counted object's visit does with what it holds: let go of it (the
 * object is being freed), or mark it shared (it crossed to another task). */
#define RC_RELEASE 0
#define RC_MARK 1
static void lugha_rc_start(void);
void *lugha_rt_rc_alloc(int64_t size, void (*visit)(void *, int64_t));
void lugha_rt_rc_retain(void *object);
void lugha_rt_rc_free(void *object);
void lugha_rt_rc_mark(void *object);
void lugha_rt_rc_release(void *object);
void (*lugha_rt_rc_drop_of(const void *object))(void *, int64_t);
void lugha_rt_rc_set_drop(void *object, void (*visit)(void *, int64_t));
void lugha_rt_rc_drop_pointers(void *object, int64_t op);
void *lugha_rc_internal(size_t size);

/* Small blocks (rc_alloc.c): 16-byte classes up to `RC_SMALL` bytes, header
 * included, carved from `RC_CHUNK`-byte chunks. */
#define RC_CHUNK ((uintptr_t)1 << 16)
#define RC_SMALL 272
#define RC_CLASSES (RC_SMALL / 16 + 1)

/* Tasks a worker's own run queue holds (runq.c), a power of two; finished
 * tasks' stacks it keeps for its next tasks (stacks.c). */
#define RUNQ_SIZE 256
#define STACK_CACHE 16

/* What one worker thread schedules and allocates with: the task it runs
 * and the ones ready to, its blocks, and the per-thread state of counting
 * and `select`. Every fragment reaches the running thread's through
 * `worker`. */
typedef struct Worker {
    /* The running fiber (`current`, tasks.c), the fiber that finished
     * last, still to be reaped, and the one whose park the next switch
     * completes (waiting.c). */
    struct Fiber *running;
    struct Fiber *zombie;
    struct Fiber *parking;
    /* True while C runs, or a C callback's Lugha code (stacks.c): on the C
     * stack, which no other task may run on. */
    bool on_c_stack;
    /* Its C stack's top (NULL: C runs where it is called, as in a C
     * harness), and the lowest address of the guard below it, where a
     * fault is C overflowing it (stack.c). */
    char *c_stack_top;
    uintptr_t c_stack_guard;
    /* Objects marked shared whose contents are still to be marked (rc.c):
     * marking walks a list, not the stack, so a long chain can't overflow
     * it; between the freeze calls, marking makes objects immortal. */
    void **marking;
    int64_t marking_len;
    int64_t marking_cap;
    bool walking;
    bool freezing;
    /* `select`'s pick: pseudo-random, the same on every run (chans.c). */
    uint64_t pick_state;
    /* How deep the running fiber is in runtime sections (`RT_SECTION`):
     * the thread holds `rt_lock` while it is above 0. */
    int64_t rt_depth;
    /* Its idle loop (workers.c), where it looks for work and waits for it;
     * the word it sleeps on; whether it sleeps now, or waits as the
     * poller; whether it is looking for work to steal (spinning); its
     * place among the workers, and its own random state for stealing. */
    struct Fiber *idle;
    uint32_t wake;
    bool sleeping;
    bool polling;
    bool spinning;
    bool parked;
    int index;
    uint32_t steal_seed;
    /* Its run queue (runq.c): a ring it pushes and pops at its own ends
     * while others steal from its head; the task it runs next, and how
     * many picks in a row took that; picks in all. */
    struct Fiber *ring[RUNQ_SIZE];
    /* A fiber that yielded, and `main`'s going home, to queue once the
     * switch away from them has saved them (workers.c). */
    struct Fiber *requeue;
    struct Fiber *send_home;
    uint32_t ring_head;
    uint32_t ring_tail;
    struct Fiber *run_next;
    int next_streak;
    /* Picks until the shared queue's turn; scheduling points until the
     * next full look at timers and sockets (workers.c). */
    int fair_left;
    int tick_left;
    /* Set by a fiber switching to one that resumes inside its own
     * sections: the lock passes to it, untouched (waiting.c). */
    bool lock_passed;
    /* The object locks the fiber parking here holds, let go of once its
     * switch has saved it (waiting.c): a `select`'s channels at most. */
    uint32_t *park_locks[65];
    int park_lock_count;
    /* Finished tasks' stacks kept for its next tasks, and whether each
     * still holds the pages its task touched (stacks.c). */
    char *stack_cache[STACK_CACHE];
    bool stack_dirty[STACK_CACHE];
    int stack_cached;
    /* Its sleepers (timers.c): a heap by time under its own lock, and the
     * earliest time, -1 for none, read by others without the lock. */
    struct Fiber **heap;
    int64_t heap_len;
    int64_t heap_cap;
    uint32_t heap_lock;
    int64_t next_timer;
    /* Its freed blocks of each class, linked through their first word; the
     * unused end of its newest chunk of each class (rc_alloc.c). */
    void *lists[RC_CLASSES];
    uintptr_t carve[RC_CLASSES];
    uintptr_t carve_end[RC_CLASSES];
    /* Its blocks other threads freed, by class: pushed by them with a
     * compare-and-swap, taken whole by it with a swap. */
    void *remote[RC_CLASSES];
    /* Objects it made and freed, summed for `LUGHA_RC_REPORT` (rc.c). */
    int64_t made;
    int64_t freed;
    /* Every worker, for those sums. */
    struct Worker *next_worker;
} Worker;

/* The running thread's worker, in the thread's own storage, so each field
 * is one `fs`-relative access (x86-64; `tpidr_el0` on ARM64). Every thread
 * has one; a helper thread's is never used, since helpers never make or
 * keep Lugha values. */
static __thread Worker this_worker = {.pick_state = 0x9e3779b97f4a7c15u, .next_timer = -1};

/* The thread pointer, read fresh: a task may move to another thread at a
 * switch, and a compiler may keep a thread-local address in a register
 * across a call, so `worker` reads it each time instead. */
static inline uintptr_t thread_pointer(void) {
    uintptr_t tp;
#if defined(__x86_64__)
    __asm__ volatile("movq %%fs:0, %0" : "=r"(tp));
#else
    __asm__ volatile("mrs %0, tpidr_el0" : "=r"(tp));
#endif
    return tp;
}

/* Where `this_worker` lies from the thread pointer: the same in every
 * thread (the executable's own thread-local block), set before `main`. */
static intptr_t worker_offset;

__attribute__((constructor)) static void worker_offset_init(void) {
    worker_offset = (intptr_t)((uintptr_t)&this_worker - thread_pointer());
}

static inline Worker *worker_now(void) {
    return (Worker *)(thread_pointer() + (uintptr_t)worker_offset);
}

#define worker (worker_now())
/* Every thread that entered as a worker, for the report's sums. */
static Worker *workers;
static pthread_mutex_t workers_lock = PTHREAD_MUTEX_INITIALIZER;

/* Makes the calling thread a worker: `main`'s at start, and each thread
 * that runs tasks. A worker thread never exits: other threads keep
 * freeing blocks back to it. */
void lugha_rt_worker_enter(void) {
    pthread_mutex_lock(&workers_lock);
    this_worker.next_worker = workers;
    workers = &this_worker;
    pthread_mutex_unlock(&workers_lock);
}

/* The C `main`'s arguments, for `args()`. */
static int32_t program_argc;
static char **program_argv;

static void workers_init(void);

void lugha_rt_init(int32_t argc, char **argv) {
    lugha_rt_worker_enter();
    program_argc = argc;
    program_argv = argv;
    install_stack_overflow_handler();
    install_interrupt_handler();
    sched_init();
    workers_init();
    hash_key_init();
    helpers_init();
    lugha_rc_start();
}

/* Ends the process with `code`: the first thread to end it does, and any
 * other trying at the same time waits, since two `exit`s at once would
 * run the exit handlers twice. */
static pthread_mutex_t exit_lock = PTHREAD_MUTEX_INITIALIZER;

__attribute__((noreturn)) static void program_exit(int code) {
    pthread_mutex_lock(&exit_lock);
    exit(code);
}

static void die(const char *message) {
    fflush(stdout);
    fputs(message, stderr);
    program_exit(101);
}

/* Prints `panic: <text>` (program data, so never a format string), then
 * unwinds the running task and ends the program or the running actor. */
static void panic_with(const char *text, size_t len) {
    // A scope that catches it makes it a value instead.
    if (!panic_caught()) {
        fflush(stdout);
        fputs("panic: ", stderr);
        fwrite(text, 1, len, stderr);
        fputc('\n', stderr);
    }
    panic_unwind(text, len);
}

/* `panic_with` for a message built from a format. */
__attribute__((format(printf, 1, 2))) static void panic_format(const char *format, ...) {
    char *text = NULL;
    va_list args;
    va_start(args, format);
    int len = vasprintf(&text, format, args);
    va_end(args);
    if (len < 0) {
        die("panic: out of memory\n");
    }
    panic_with(text, (size_t)len);
}


/* Scratch memory the runtime lets go of itself (`rt_scratch_done`),
 * zeroed. */
static void *rt_scratch(int64_t size) {
    return lugha_rc_internal((size_t)size);
}

static void rt_scratch_done(void *memory) {
    free(memory);
}

/* Lets go of `object`, a counted object the runtime made and won't hand
 * to the program (an error message no one asked for). */
static void rt_let_go(void *object) {
    lugha_rt_rc_release(object);
}

/* Marks `array`, made here, as holding strings or arrays: freeing it
 * releases them. */
static void rt_holds_pointers(LughaArray *array) {
    lugha_rt_rc_set_drop(array, lugha_rt_rc_drop_pointers);
}

/* A copy of `array` with room for at least one more element: capacity
 * doubles, starting at 4. The old object is freed at once; its single
 * holder switches to the copy. */
LughaArray *lugha_rt_array_grow(const LughaArray *array, int64_t elem_size) {
    int64_t old_cap = array->cap;
    int64_t cap = old_cap < 2 ? 4 : old_cap * 2;
    /* `()` elements take no bytes: only the length grows. */
    if (elem_size > 0 && cap > (INT64_MAX - (int64_t)sizeof(LughaArray)) / elem_size) {
        die("panic: out of memory\n");
    }
    LughaArray *grown = lugha_rt_alloc((int64_t)sizeof(LughaArray) + cap * elem_size);
    lugha_rt_rc_set_drop(grown, lugha_rt_rc_drop_of(array));
    grown->len = array->len;
    grown->cap = cap;
    memcpy(grown->elems, array->elems, (size_t)(array->len * elem_size));
    /* The elements moved to `grown`; the old block, unique since codegen
     * made it so before growing, goes without dropping them. */
    lugha_rt_rc_set_drop((void *)array, NULL);
    lugha_rt_rc_release((void *)array);
    return grown;
}

void lugha_rt_panic(const LughaString *msg, const char *file, int64_t line, int64_t col) {
    char *where = NULL;
    int tail = asprintf(&where, " at %s:%" PRId64 ":%" PRId64, file, line, col);
    char *text = tail < 0 ? NULL : malloc((size_t)msg->len + (size_t)tail);
    if (text == NULL) {
        die("panic: out of memory\n");
    }
    // memcpy, never printf: the message is program data, not a format string.
    memcpy(text, msg->bytes, (size_t)msg->len);
    memcpy(text + msg->len, where, (size_t)tail);
    panic_with(text, (size_t)msg->len + (size_t)tail);
}

/* Out-of-range indexing; `index` may be negative. */
void lugha_rt_panic_bounds(int64_t len, int64_t index, const char *file, int64_t line, int64_t col) {
    panic_format("index out of bounds: the length is %" PRId64 " but the index is %" PRId64
                 " at %s:%" PRId64 ":%" PRId64,
                 len, index, file, line, col);
}

/* `a + b` for strings: a new string holding both. */
LughaString *lugha_rt_str_concat(const LughaString *a, const LughaString *b) {
    int64_t len = a->len + b->len;
    LughaString *s = lugha_rt_alloc((int64_t)sizeof(LughaString) + len + 1);
    s->len = len;
    memcpy(s->bytes, a->bytes, (size_t)a->len);
    memcpy(s->bytes + a->len, b->bytes, (size_t)b->len);
    s->bytes[len] = '\0';
    return s;
}

/* `s.slice(from, to)`: a new string of the bytes `from..to`. Panics at
 * the caller's location unless 0 <= from <= to <= len, so no byte outside
 * `s` is ever read. */
LughaString *lugha_rt_str_slice(const LughaString *s, int64_t from, int64_t to, const char *file,
                                int64_t line, int64_t col) {
    if (from < 0 || from > to || to > s->len) {
        panic_format("slice out of bounds: from %" PRId64 " to %" PRId64
                     " of a string of length %" PRId64 " at %s:%" PRId64 ":%" PRId64,
                     from, to, s->len, file, line, col);
    }
    int64_t len = to - from;
    LughaString *out = lugha_rt_alloc((int64_t)sizeof(LughaString) + len + 1);
    out->len = len;
    memcpy(out->bytes, s->bytes + from, (size_t)len);
    out->bytes[len] = '\0';
    return out;
}

/* `a == b` for strings compares contents; 1 or 0. */
int32_t lugha_rt_str_eq(const LughaString *a, const LughaString *b) {
    return a->len == b->len && memcmp(a->bytes, b->bytes, (size_t)a->len) == 0;
}

/* `a < b` and friends for strings: bytewise, as unsigned
 * values, and a proper prefix is smaller. -1, 0 or 1. Reads only `len` bytes
 * of each, never relying on the terminating NUL. */
int32_t lugha_rt_str_cmp(const LughaString *a, const LughaString *b) {
    size_t shorter = (size_t)(a->len < b->len ? a->len : b->len);
    int order = memcmp(a->bytes, b->bytes, shorter);
    if (order != 0) {
        return order < 0 ? -1 : 1;
    }
    return a->len < b->len ? -1 : a->len > b->len ? 1 : 0;
}
