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
#include <stddef.h>
#include <sys/prctl.h>
#include <sys/epoll.h>
#include <sys/mman.h>
#include <sys/random.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/syscall.h>
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

/* The C `main`'s arguments, for `args()`. */
static int32_t program_argc;
static char **program_argv;

void lugha_rt_init(int32_t argc, char **argv) {
    program_argc = argc;
    program_argv = argv;
    install_stack_overflow_handler();
    install_interrupt_handler();
    sched_init();
    hash_key_init();
    lugha_rc_start();
}

static void die(const char *message) {
    fflush(stdout);
    fputs(message, stderr);
    exit(101);
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
