/*
 * tasks.c — part of lugha_rt.c: green threads, channels and `select`.
 *
 * Every green thread runs on the program's one OS thread (M:1), so nothing
 * here locks: a switch happens only inside these functions, when a thread
 * waits (`join`, a channel, `select`, `sleep`), yields, or is preempted by
 * the countdown generated code keeps at its stack's top (stacks.c).
 *
 * Each green thread is a `Fiber`: a stack from the pool (stacks.c) and the
 * stack pointer saved while it is switched out. `main` is a fiber too. A
 * fiber is the runtime's own memory, freed once another fiber runs after
 * it ends.
 *
 * A `Task<T>` value is a `LughaTask`, a counted object holding the result; a
 * `Sender<T>` is a `LughaChan` and a `Receiver<T>` its `LughaRecv`. Values
 * cross channels as `elem` bytes in element layout, already copied by
 * generated code.
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it after the
 * core (lugha_rt.c, which holds the `Worker` record), stack.c and random.c;
 * waiting.c (how a task waits and is woken) and stacks.c follow it.
 */

/* Steps between preemption checks in generated code. */
#define TASK_BUDGET 4096

typedef struct Fiber Fiber;

/* A panic or cancellation being unwound (unwind.c): what a scope catching
 * it reads. */
typedef struct Panic {
    struct _Unwind_Exception exception;
    bool cancel;
    char *text;
    size_t len;
} Panic;

/* A `Task<T>` (a counted object): what `join` reads. */
typedef struct {
    void *result;
    int32_t done;
    /* Fibers waiting in `join`, linked through `wait_next`. */
    Fiber *joiners;
    /* What the result holds, for releasing and marking it. */
    RcVisit result_visit;
    /* The fiber running it, until it ends; and how it ended (scopes.c):
     * `TASK_OK`, `TASK_CANCELLED`, `TASK_FAILED` or `TASK_PANICKED`, with
     * the cancellation's reason and text, the error (an `Error` value's
     * table and box) or the panic's message. */
    Fiber *fiber;
    int32_t outcome;
    int32_t reason;
    LughaString *text;
    void *error_table;
    void *error_box;
    /* Its name, for reports: `t.name(…)`, or the called function's. */
    LughaString *name;
    /* The next task of its scope. */
    void *scope_next;
    /* Its lock over `done` and `joiners` (rt_lock.c's kind). */
    uint32_t lock;
} LughaTask;

/* True once `t`'s call has finished: set under its lock, read without. */
static inline bool task_done(const LughaTask *t) {
    return __atomic_load_n(&t->done, __ATOMIC_ACQUIRE) != 0;
}

struct Fiber {
    /* Saved stack pointer while switched out. */
    void *sp;
    /* The mapping (guard pages first), or NULL for `main`. */
    char *map;
    /* The stack's high end. */
    char *top;
    /* What to run: `entry(code, env, result)`, generated per spawn site. */
    void (*entry)(void *, void *, void *);
    void *code;
    void *env;
    LughaTask *task;
    /* True while a panic or a cancellation unwinds this fiber's stack;
     * and while what unwinds is a stack overflow (stacks.c). */
    bool unwinding;
    bool overflowed;
    /* The scope that started it (scopes.c), and the innermost scope whose
     * body it runs, or NULL. */
    void *started_in;
    void *scopes;
    /* Cancelled with this reason (`REASON_*`, 0 for none) and text, by this
     * scope (NULL: by its own scope, or by `cancel`); noticed at its next
     * safe point. A wait that may be cut short says so first. */
    int32_t cancelled;
    LughaString *cancel_text;
    void *cancelled_by;
    bool cancellable;
    /* The panic or cancellation unwinding it, until caught or done. */
    Panic *in_flight;
    /* `shield within ms`: how deep, until when (monotonic ns) cancellation
     * is held off, and the next fiber in a shield. A shield entered while
     * the fiber unwinds (in a `defer`) is cut off at its deadline by an
     * unwind of its own, `shield_outer` keeping the first until the shield
     * stops it; `shield_caught` is an unwind a shield stopped that goes on
     * past it. */
    int32_t shield_depth;
    int64_t shield_until;
    struct Fiber *shield_next;
    bool shield_in_unwind;
    Panic *shield_outer;
    Panic *shield_caught;
    /* Run queue, and the list of every live fiber. */
    Fiber *next;
    /* The worker it runs on alone, or NULL: set by the fiber itself
     * (touching a pinned handle, a pinned scope), read by its wakers. */
    struct Worker *pin;
    /* Pinned by a handle: stays pinned until it ends. */
    bool pin_kept;
    /* How many direct C callbacks it is inside: no cancellation point
     * unwinds there, since C waits for the callback (callbacks.c). */
    int32_t in_callback;
    Fiber *prev_live;
    Fiber *next_live;
    /* In a `join` wait list. */
    Fiber *wait_next;
    /* When it sleeps until, < 0 when it doesn't; the worker whose timer
     * heap holds it, and where (timers.c). */
    int64_t wake_at;
    struct Worker *timer_owner;
    int64_t heap_at;
    /* The wait its timer is for (a generation, waiting.c). */
    uint32_t timer_gen;
    /* How deep in runtime sections it switched away (waiting.c). */
    int64_t rt_saved;
    /* Which wait woke it: a `select` case, or TIMED_OUT; WAITING before. */
    int64_t fired;
    /* A wake that came before its wait was published (`fired`'s layout):
     * a timer or socket armed for the wait it was about to begin, found
     * by `block` once it publishes it (waiting.c). */
    int64_t early;
    /* The actor this fiber runs (actors.c), or NULL. */
    void *actor;
    /* For a deadlock report: the channel it waits on (chans.c) and how
     * (`WAIT_*`), or NULL and WAIT_NONE. */
    void *waits_on;
    int32_t wait_kind;
    /* Last, away from what every switch touches: its number (1 for
     * `main`, then in the order started), the owner check's name for it;
     * and the `Ref` boxes it is printing (print.c). */
    int64_t number;
    ShowStack show;
    /* Its cells whose count dropped but stayed above zero, and how many
     * make it look at them (0: the first threshold) (cycles.c). */
    void **cc_suspects;
    int64_t cc_len;
    int64_t cc_cap;
    int64_t cc_threshold;
#if defined(__SANITIZE_THREAD__)
    /* ThreadSanitizer's record of it, told of each switch. */
    void *tsan;
#endif
};

enum { WAIT_NONE, WAIT_RECV, WAIT_SEND, WAIT_SELECT };

enum {
    WAITING = -1,
    TIMED_OUT = -2,
    DEFAULT_ARM = -1,
    CANCELLED = -3,
    PARKING = -4,
    /* A socket wait ended by another task closing the socket (net.c). */
    SOCKET_CLOSED = -5
};


/* The running fiber, of the running worker. */
#define current (worker->running)
static Fiber main_fiber;
static Fiber *live;
/* A leaf lock over `live`'s links: nothing is taken while holding it. */
static uint32_t live_lock;
/* Atomic: tasks start and end on every worker. */
static int64_t live_count;
/* Fibers started so far, `main` included. */
static int64_t fibers_started = 1;

static int64_t task_number(void) {
    return current == NULL ? 1 : current->number;
}

/* Printing before any task starts. */
static ShowStack show_before_tasks;

static ShowStack *show_stack_of(void) {
    return current == NULL ? &show_before_tasks : &current->show;
}

void lugha_rt_task_swap(void **save, void *load);
void lugha_rt_task_boot(void);


/* How the scheduler runs, as `LUGHA_SCHED` says (`lughac test` sets it):
 * normal (real time, ready tasks first in, first out), deterministic
 * (virtual time, which jumps to the next timer once every task waits and
 * no I/O is ready), or shuffle (deterministic, each next ready task picked
 * at random from a seed, as `select`'s ready arm is). */
enum { MODE_NORMAL, MODE_DETERMINISTIC, MODE_SHUFFLE };
static int sched_mode;
static int64_t virtual_ns;
static uint64_t shuffle_seed;
static uint64_t shuffle_state;

/* The process's secret key for hashing (the prelude's `Hasher`,
 * SipHash-1-3), read inline by generated code. It can't change what a
 * program computes: maps keep insertion order. */
uint64_t lugha_rt_hash_key[2];

/* splitmix64: a well-spread word from `x`. */
static uint64_t splitmix(uint64_t x) {
    uint64_t z = x + 0x9e3779b97f4a7c15u;
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9u;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebu;
    return z ^ (z >> 31);
}

/* Reads the mode once, as the runtime starts; anything else is normal. */
static void sched_init(void) {
    const char *mode = getenv("LUGHA_SCHED");
    if (mode == NULL) {
        return;
    }
    if (strcmp(mode, "deterministic") == 0) {
        sched_mode = MODE_DETERMINISTIC;
        return;
    }
    if (strncmp(mode, "shuffle:", 8) == 0 && mode[8] >= '0' && mode[8] <= '9') {
        char *end;
        errno = 0;
        unsigned long long seed = strtoull(mode + 8, &end, 10);
        if (errno == 0 && *end == '\0') {
            sched_mode = MODE_SHUFFLE;
            shuffle_seed = (uint64_t)seed;
            /* Spread, so nearby seeds give unrelated orders; never zero,
             * or the generator would stay there. */
            shuffle_state = splitmix(shuffle_seed) | 1;
        }
    }
}

/* The hash key: the operating system's randomness, or, in a deterministic
 * run, fixed (from the shuffle seed when shuffled), so the run repeats.
 * After `sched_init`, before anything can hash or lock the process down. */
static void hash_key_init(void) {
    if (sched_mode == MODE_NORMAL) {
        lugha_rt_hash_key[0] = lugha_rt_os_seed();
        lugha_rt_hash_key[1] = lugha_rt_os_seed();
        return;
    }
    lugha_rt_hash_key[0] = splitmix(shuffle_seed * 2);
    lugha_rt_hash_key[1] = splitmix(shuffle_seed * 2 + 1);
}

static bool virtual_time(void) {
    return sched_mode != MODE_NORMAL;
}

/* The shuffle's next number (xorshift64). */
static uint64_t shuffle_next(void) {
    shuffle_state ^= shuffle_state << 13;
    shuffle_state ^= shuffle_state >> 7;
    shuffle_state ^= shuffle_state << 17;
    return shuffle_state;
}

/* Monotonic nanoseconds: real, or the virtual clock. */
static int64_t now_ns(void) {
    if (virtual_time()) {
        return virtual_ns;
    }
    struct timespec t;
    clock_gettime(OS_MONOTONIC, &t);
    return (int64_t)t.tv_sec * 1000000000 + t.tv_nsec;
}

/* The calling worker's idle loop (workers.c). */
static void idle_make(void);

/* Set once `main` became a task: Ctrl-C then cancels instead of ending. */
static volatile sig_atomic_t tasks_started;

/* `main` becomes a fiber the first time anything here runs, and the main
 * thread's worker gets its idle loop. */
static void ensure_started(void) {
    if (current != NULL) {
        return;
    }
    main_fiber.wake_at = -1;
    main_fiber.fired = 0;
    main_fiber.number = 1;
#if defined(__SANITIZE_THREAD__)
    main_fiber.tsan = __tsan_get_current_fiber();
#endif
    __atomic_store_n(&worker->running, &main_fiber, __ATOMIC_RELEASE);
    live = &main_fiber;
    live_count = 1;
    idle_make();
    tasks_started = 1;
}
