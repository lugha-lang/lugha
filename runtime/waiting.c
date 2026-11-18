/*
 * waiting.c — part of lugha_rt.c: how a task waits and is woken, and the
 * scheduler's loop that runs the next ready task.
 *
 * A fiber's `fired` word says where its wait stands:
 *
 *   PARKING  it called `block` and its worker hasn't saved its registers
 *            yet: it can't be resumed;
 *   WAITING  its context is saved: whoever wakes it queues it;
 *   anything else: running or ready, `fired` telling what woke it last.
 *
 * `wake` changes `fired` by compare-and-swap, so of two wakers only one
 * queues the fiber. A fiber woken while still PARKING is queued by its own
 * worker once its context is saved (`park_finish`), or, if its worker
 * hasn't switched away from it at all, simply runs on: never resumed from
 * a half-saved context.
 *
 * While tasks can't move between threads (one worker), nothing else touches
 * `fired`, so the same steps compile to plain loads and stores: an
 * uncontended compare-and-swap on every wait and wake costs a ping-pong of
 * two tasks a fifth of its time, and even a flag read at run time to skip
 * them costs a tenth. Built with `LUGHA_TASKS_CROSS_THREADS` 1, they are
 * atomic.
 *
 * A fragment, not compiled on its own: joined right after tasks.c (fibers,
 * the ready queue, sleepers).
 */

/* 1 once fibers may be woken from other threads than their worker's. */
#ifndef LUGHA_TASKS_CROSS_THREADS
#define LUGHA_TASKS_CROSS_THREADS 0
#endif

static int64_t fired_of(const Fiber *f) {
#if LUGHA_TASKS_CROSS_THREADS
    return __atomic_load_n(&f->fired, __ATOMIC_ACQUIRE);
#else
    return f->fired;
#endif
}

static void fired_set(Fiber *f, int64_t to) {
#if LUGHA_TASKS_CROSS_THREADS
    __atomic_store_n(&f->fired, to, __ATOMIC_RELEASE);
#else
    f->fired = to;
#endif
}

/* `f->fired` from `*was` to `to`, if it is still `*was`; else false, with
 * what it is in `*was`. */
static bool fired_swap(Fiber *f, int64_t *was, int64_t to) {
#if LUGHA_TASKS_CROSS_THREADS
    return __atomic_compare_exchange_n(&f->fired, was, to, false, __ATOMIC_ACQ_REL,
                                       __ATOMIC_ACQUIRE);
#else
    if (f->fired != *was) {
        *was = f->fired;
        return false;
    }
    f->fired = to;
    return true;
#endif
}

/* True while `f` waits: parked, or parking. */
static bool is_waiting(const Fiber *f) {
    int64_t fired = fired_of(f);
    return fired == WAITING || fired == PARKING;
}

/* Wakes a waiting fiber for `why` (a case index, TIMED_OUT or CANCELLED). */
static void wake(Fiber *f, int64_t why) {
    int64_t was = fired_of(f);
    do {
        // Woken already (cancelled while waiting): queued once is enough.
        if (was != WAITING && was != PARKING) {
            return;
        }
    } while (!fired_swap(f, &was, why));
    unsleep(f);
    // A fiber still parking is queued by its worker once it is saved.
    if (was == WAITING) {
        make_ready(f);
    }
}

/* After a switch saved the fiber that was parking: it may be resumed
 * from now on. Woken meanwhile, it is queued here instead. */
static void park_finish(void) {
    Fiber *f = worker->parking;
    if (f == NULL) {
        return;
    }
    worker->parking = NULL;
    int64_t expected = PARKING;
    if (!fired_swap(f, &expected, WAITING)) {
        make_ready(f);
    }
}

static void check_times(void);

static void wake_due_sleepers(void) {
    // Deadlines, shields and Ctrl-C (scope_time.c).
    check_times();
    if (sleepers == NULL) {
        return;
    }
    int64_t now = now_ns();
    while (sleepers != NULL && sleepers->wake_at <= now) {
        Fiber *f = sleepers;
        sleepers = f->sleep_next;
        f->wake_at = -1;
        wake(f, TIMED_OUT);
    }
}

/* Task stacks and the limit (stacks.c). */
static void reap(void);
static uintptr_t stack_limit_of(const Fiber *f);
extern uintptr_t lugha_rt_stack_limit;

/* What the fiber landing on a worker does first, before its own code: the
 * fiber it replaced is saved now, so its park completes and, had it
 * finished, its stack goes back. */
static void landed(void) {
    park_finish();
    reap();
}

/* Switches to `next`; `parking`: the current fiber is in `block`. */
static void switch_to(Fiber *next, bool parking) {
    Fiber *prev = current;
    if (next == prev) {
        return;
    }
    if (parking) {
        worker->parking = prev;
    }
    current = next;
    lugha_rt_stack_limit = stack_limit_of(next);
    lugha_rt_task_swap(&prev->sp, next->sp);
    /* Back on `prev`'s stack: another fiber ran and switched back. */
    landed();
}

/* Nothing is ready: waits for a socket or the next sleeper; wakes threads
 * whose sockets are ready without waiting (net.c). */
static void idle_wait(void);
static void poll_ready_sockets(void);

/* Each waiting task and what it waits on, after a deadlock (actors.c). */
static void report_waits(void);

static void deadlock(void) {
    fflush(stdout);
    fputs("panic: deadlock: every green thread is waiting\n", stderr);
    report_waits();
    exit(101);
}

/* Runs the next ready fiber; the current one has already queued itself, or
 * is parking (`parking`). Sleeps the OS thread when only sleepers remain. */
static void schedule_from(bool parking) {
    if (worker->on_c_stack) {
        panic_with("a C callback can't wait: start a task instead", 45);
    }
    for (;;) {
        wake_due_sleepers();
        // Woken while still on this worker (a deadline, a socket the idle
        // wait polled): nothing to switch to, it runs on.
        if (parking && !is_waiting(current)) {
            return;
        }
        Fiber *next = pop_ready();
        if (next != NULL) {
            switch_to(next, parking);
            return;
        }
        idle_wait();
    }
}

static void schedule(void) {
    schedule_from(false);
}

/* Blocks the current fiber until something wakes it; what did. */
static int64_t block(void) {
    fired_set(current, PARKING);
    schedule_from(true);
    return fired_of(current);
}
