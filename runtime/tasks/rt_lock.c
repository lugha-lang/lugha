/*
 * rt_lock.c — part of lugha_rt.c: the lock over the scheduler's shared
 * state, and the runtime sections that take it.
 *
 * A fragment, not compiled on its own: joined right after the core
 * (lugha_rt.c: the `Worker` record, `worker`), before every fragment that
 * opens a section.
 */

/* A lock of atomics and a futex (0 free, 1 held, 2 held with waiters),
 * not a pthread mutex: it has no owner, so a fiber may take it on one
 * thread and let go of it after moving to another. */
static inline __attribute__((always_inline)) void lock_take(uint32_t *lock) {
    uint32_t was = 0;
    if (__atomic_compare_exchange_n(lock, &was, 1, false, __ATOMIC_ACQUIRE, __ATOMIC_RELAXED)) {
        return;
    }
    if (was != 2) {
        was = __atomic_exchange_n(lock, 2, __ATOMIC_ACQUIRE);
    }
    while (was != 0) {
        os_wait_on(lock, 2);
        was = __atomic_exchange_n(lock, 2, __ATOMIC_ACQUIRE);
    }
}

static inline __attribute__((always_inline)) void lock_give(uint32_t *lock) {
    if (__atomic_fetch_sub(lock, 1, __ATOMIC_RELEASE) != 1) {
        __atomic_store_n(lock, 0, __ATOMIC_RELEASE);
        os_wake_on(lock, 1);
    }
}

/* The shared objects tasks meet through (scopes, channels, actors, joins,
 * deadlines, shields, the job room) are guarded by one lock. A runtime
 * entry that touches them opens a section, `RT_SECTION();`, closed when the
 * function returns; sections nest. A fiber that waits closes its sections
 * before it switches and opens them again when it runs on, wherever
 * (`switch_away`, waiting.c): the lock is never held across a switch, and
 * scheduling itself never takes it. */
static uint32_t rt_lock;

/* False while the program runs on one thread: the lock is then taken and
 * let go of in name only, costing nothing. Set, under the lock, as the
 * other workers start (workers.c), never cleared. */
static bool rt_locking;

static inline __attribute__((always_inline)) void rt_lock_take(void) {
    if (rt_locking) {
        lock_take(&rt_lock);
    }
}

static inline __attribute__((always_inline)) void rt_lock_give(void) {
    if (rt_locking) {
        lock_give(&rt_lock);
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

/* Opens them again once the fiber runs on, on whichever thread: the lock
 * taken, unless the fiber that switched here passed it on. */
static void rt_reopen(int64_t depth) {
    Worker *w = worker;
    if (w->lock_passed) {
        w->lock_passed = false;
    } else if (depth > 0) {
        rt_lock_take();
    }
    w->rt_depth = depth;
}
