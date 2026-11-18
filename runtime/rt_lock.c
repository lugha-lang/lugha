/*
 * rt_lock.c — part of lugha_rt.c: the lock over the scheduler's shared
 * state, and the runtime sections that take it.
 *
 * A fragment, not compiled on its own: joined right after the core
 * (lugha_rt.c: the `Worker` record, `worker`), before every fragment that
 * opens a section.
 */

/* Waits while `*word` is `value`, or until woken; wakes up to `n`
 * waiting on `word`. */
static void futex_wait(uint32_t *word, uint32_t value) {
    syscall(SYS_futex, word, FUTEX_WAIT_PRIVATE, value, NULL, NULL, 0);
}

static void futex_wake(uint32_t *word, int n) {
    syscall(SYS_futex, word, FUTEX_WAKE_PRIVATE, n, NULL, NULL, 0);
}

/* The scheduler's shared state (ready fibers, sleepers, scopes, channels,
 * actors, joins, the poller) is guarded by one lock. A runtime entry that
 * touches it opens a section, `RT_SECTION();`, closed when the function
 * returns; sections nest. The lock stays held across a task switch: the
 * fiber landing on the thread goes on inside its own section, and a new
 * fiber leaves the lock as it starts (`landed`, waiting.c). A lock of
 * atomics and a futex (0 free, 1 held, 2 held with waiters), not a pthread
 * mutex: it has no owner, so the fiber that lets it go needn't be the one
 * that took it. */
static uint32_t rt_lock;

/* False while the program runs on one thread: the lock is then taken and
 * let go of in name only, costing nothing. Set, under the lock, as the
 * other workers start (workers.c), never cleared. */
static bool rt_locking;

static inline __attribute__((always_inline)) void rt_lock_take(void) {
    if (!rt_locking) {
        return;
    }
    uint32_t was = 0;
    if (__atomic_compare_exchange_n(&rt_lock, &was, 1, false, __ATOMIC_ACQUIRE,
                                    __ATOMIC_RELAXED)) {
        return;
    }
    if (was != 2) {
        was = __atomic_exchange_n(&rt_lock, 2, __ATOMIC_ACQUIRE);
    }
    while (was != 0) {
        futex_wait(&rt_lock, 2);
        was = __atomic_exchange_n(&rt_lock, 2, __ATOMIC_ACQUIRE);
    }
}

static inline __attribute__((always_inline)) void rt_lock_give(void) {
    if (!rt_locking) {
        return;
    }
    if (__atomic_fetch_sub(&rt_lock, 1, __ATOMIC_RELEASE) != 1) {
        __atomic_store_n(&rt_lock, 0, __ATOMIC_RELEASE);
        futex_wake(&rt_lock, 1);
    }
}

static inline __attribute__((always_inline)) void rt_enter(void) {
    Worker *w = worker;
    if (w->rt_depth++ == 0) {
        rt_lock_take();
    }
}

static inline __attribute__((always_inline)) void rt_leave(void) {
    Worker *w = worker;
    if (--w->rt_depth == 0) {
        rt_lock_give();
    }
}

static inline __attribute__((always_inline)) void rt_section_end(int *unused) {
    (void)unused;
    rt_leave();
}

#define RT_SECTION()                                                                     \
    __attribute__((cleanup(rt_section_end), unused)) int rt_section_ = (rt_enter(), 0)

/* Before an unwind leaves the runtime: every section of the running fiber
 * is closed (the unwinder runs no C cleanups). */
static void rt_drop(void) {
    Worker *w = worker;
    if (w->rt_depth > 0) {
        w->rt_depth = 0;
        rt_lock_give();
    }
}
