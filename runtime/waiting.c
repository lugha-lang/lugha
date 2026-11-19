/*
 * waiting.c — part of lugha_rt.c: how a task waits and is woken, and how
 * a worker switches from one fiber to the next.
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
 * The word also holds the wait's generation (its high 32 bits), bumped by
 * each `block`: a timer or socket event armed for one wait claims only
 * that one, so arriving late, after its fiber was woken otherwise and
 * began another wait, it misses instead of waking the wrong one.
 *
 * A fiber waiting on a channel, an actor or a join parks still holding
 * that object's lock (`block_holding`); the fiber landing after it lets go
 * of it once the switch has saved it. A waker holding the lock therefore
 * only ever meets parked fibers, and claims one (`claim`) before moving a
 * value to it; a claim that fails means another waker won.
 *
 * A fragment, not compiled on its own: joined after tasks.c (fibers),
 * runq.c (the ready fibers) and timers.c (sleepers).
 */

/* `fired` changes atomically once other workers run (`rt_locking`); before
 * that, only this thread touches it, and plain loads and stores do. */
static inline int64_t fired_of(const Fiber *f) {
    return rt_locking ? __atomic_load_n(&f->fired, __ATOMIC_ACQUIRE) : f->fired;
}

static inline void fired_set(Fiber *f, int64_t to) {
    if (rt_locking) {
        __atomic_store_n(&f->fired, to, __ATOMIC_RELEASE);
    } else {
        f->fired = to;
    }
}

/* `f->fired` from `*was` to `to`, if it is still `*was`; else false, with
 * what it is in `*was`. */
static inline bool fired_swap(Fiber *f, int64_t *was, int64_t to) {
    if (rt_locking) {
        return __atomic_compare_exchange_n(&f->fired, was, to, false, __ATOMIC_ACQ_REL,
                                           __ATOMIC_ACQUIRE);
    }
    if (f->fired != *was) {
        *was = f->fired;
        return false;
    }
    f->fired = to;
    return true;
}

/* A wait word: generation `gen`, state `state`. */
static inline int64_t wait_word(uint32_t gen, int64_t state) {
    return (int64_t)(((uint64_t)gen << 32) | (uint32_t)state);
}

static inline int64_t word_state(int64_t word) {
    return (int64_t)(int32_t)(uint32_t)word;
}

static inline uint32_t word_gen(int64_t word) {
    return (uint32_t)((uint64_t)word >> 32);
}

/* `f`'s state: what woke it last, or WAITING or PARKING. */
static inline int64_t wait_state(const Fiber *f) {
    return word_state(fired_of(f));
}

/* The generation `f`'s next `block` will have: what its timers and socket
 * waits are armed for, before it blocks. */
static inline uint32_t next_wait_gen(const Fiber *f) {
    return word_gen(fired_of(f)) + 1;
}

/* True while `f` waits: parked, or parking. */
static bool is_waiting(const Fiber *f) {
    int64_t state = wait_state(f);
    return state == WAITING || state == PARKING;
}

/* A claim that lost: `f` isn't in wait `gen` any more. */
#define CLAIM_LOST INT64_MIN

/* Claims `f`'s wait `gen` for `why`: its state before (WAITING or
 * PARKING), or CLAIM_LOST. The claimer may then move a value to it; it
 * runs only once `claimed_ready` queues it. */
static int64_t claim(Fiber *f, uint32_t gen, int64_t why) {
    int64_t word = fired_of(f);
    int64_t state;
    do {
        state = word_state(word);
        if (word_gen(word) != gen || (state != WAITING && state != PARKING)) {
            return CLAIM_LOST;
        }
    } while (!fired_swap(f, &word, wait_word(gen, why)));
    return state;
}

/* Queues `f`, claimed from `state`. Woken by a running task, it runs next
 * on its waker's worker; by the scheduler (a timer, the poller), behind
 * what is ready. A fiber still parking is queued by its own worker once
 * its switch has saved it. */
static void claimed_ready(Fiber *f, int64_t state) {
    unsleep(f);
    if (state == WAITING) {
        Worker *w = worker;
        if (w->running != w->idle) {
            ready_next(f);
        } else {
            make_ready(f);
        }
    }
}

/* Wakes `f`'s wait `gen` for `why`; false if it isn't in it any more. */
static bool wake_gen(Fiber *f, uint32_t gen, int64_t why) {
    int64_t state = claim(f, gen, why);
    if (state == CLAIM_LOST) {
        return false;
    }
    claimed_ready(f, state);
    return true;
}

/* Wakes whatever wait `f` is in, for `why` (a case index, TIMED_OUT or
 * CANCELLED): by a waker whose waiters leave its list under its lock. */
static void wake(Fiber *f, int64_t why) {
    for (;;) {
        int64_t word = fired_of(f);
        int64_t state = word_state(word);
        // Woken already (cancelled while waiting): queued once is enough.
        if (state != WAITING && state != PARKING) {
            return;
        }
        if (wake_gen(f, word_gen(word), why)) {
            return;
        }
    }
}

static void park_unlock(Worker *w);

/* After a switch saved the fiber that was parking: it may be resumed
 * from now on. Woken meanwhile, it is queued here instead. Then the
 * object locks it parked holding are let go of. */
static void park_finish(void) {
    Worker *w = worker;
    Fiber *f = w->parking;
    if (f != NULL) {
        w->parking = NULL;
        int64_t word = fired_of(f);
        int64_t expected = wait_word(word_gen(word), PARKING);
        if (!fired_swap(f, &expected, wait_word(word_gen(word), WAITING))) {
            make_ready(f);
        }
    }
    park_unlock(w);
}

/* Gives a finished fiber's stack back (stacks.c). */
static void reap(void);

/* A fiber that yielded, or `main`'s going home (workers.c), queued once
 * its switch has saved it. */
static void requeue_finish(void);

/* What the fiber landing on a worker does first, before its own code: the
 * fiber it replaced is saved now, so its park completes, a yielding one
 * is queued, and a finished one's stack goes back. */
static void landed(void) {
    park_finish();
    requeue_finish();
    reap();
}

/* Switches to `next`, on this thread; `parking`: the current fiber is in
 * `block`. No section is open: the fiber may come back on another thread,
 * and everything after the swap reads `worker` afresh. */
static void switch_to(Fiber *next, bool parking) {
    Worker *w = worker;
    Fiber *prev = w->running;
    if (next == prev) {
        return;
    }
    if (parking) {
        w->parking = prev;
    }
    __atomic_store_n(&w->running, next, __ATOMIC_RELEASE);
#if defined(__SANITIZE_THREAD__)
    __tsan_switch_to_fiber(next->tsan, 0);
#endif
    lugha_rt_task_swap(&prev->sp, next->sp);
    /* Back on `prev`'s stack: another fiber ran and switched back. */
    landed();
}

/* Each waiting task and what it waits on, after a deadlock (actors.c). */
static void report_waits(void);

static void deadlock(void) {
    fflush(stdout);
    fputs("panic: deadlock: every green thread is waiting\n", stderr);
    report_waits();
    program_exit(101);
}

/* Lets go of the object locks the fiber parking on `w` held. */
static void park_unlock(Worker *w) {
    for (int i = 0; i < w->park_lock_count; i++) {
        lock_give(w->park_locks[i]);
    }
    w->park_lock_count = 0;
}

/* Due timers, deadlines, shields and Ctrl-C, and now and then what
 * sockets and helpers have ready (workers.c): at each scheduling point,
 * nearly free when none of them is pending. */
static void sched_tick_slow(Worker *w);

/* Whether any scope has a deadline or any fiber a shield, and how many
 * Ctrl-Cs arrived (scope_time.c): read without the lock. */
static bool timed_any;
static int interrupts;

static inline void sched_tick(void) {
    Worker *w = worker;
    if (--w->tick_left > 0 && __atomic_load_n(&w->next_timer, __ATOMIC_ACQUIRE) < 0 &&
        !__atomic_load_n(&timed_any, __ATOMIC_ACQUIRE) &&
        __atomic_load_n(&interrupts, __ATOMIC_ACQUIRE) == 0) {
        return;
    }
    sched_tick_slow(w);
}

/* The current fiber stops running here: parking (`parking`, in `block`),
 * yielding (`yielding`, queued again once saved), or finished. With
 * nothing else ready the worker's idle loop runs; a yielding fiber with
 * nothing to yield to runs on. Its runtime sections close as it switches:
 * the lock is let go of, or, when the next fiber resumes inside sections
 * of its own, passed to it untouched (`lock_passed`). Idle loops and new
 * fibers never hold it. When this fiber runs again, wherever, its
 * sections open again. */
static void switch_away(bool parking, bool yielding) {
    Worker *w = worker;
    if (w->on_c_stack) {
        park_unlock(w);
        panic_with("an inline C callback can't wait: C is waiting for it; declare it with `data` and no result, not `inline`, so it runs as a task", 126);
    }
    // Holding object locks, only timers fire here: deadlines, shields and
    // the poll take the scopes lock, which comes before them.
    if (w->park_lock_count == 0) {
        sched_tick();
    } else {
        run_due_timers(w);
    }
    int64_t depth = w->rt_depth;
    Fiber *self = w->running;
    // Woken already (a timer): nothing to switch to, it runs on.
    if (parking && !is_waiting(self)) {
        park_unlock(w);
        return;
    }
    Fiber *next = take_ready();
    if (next == NULL && yielding) {
        return;
    }
    if (next == NULL) {
        next = w->idle;
    }
    w->rt_depth = 0;
    if (depth > 0) {
        if (next->rt_saved > 0) {
            w->lock_passed = true;
        } else {
            rt_lock_give();
        }
    }
    self->rt_saved = depth;
    if (yielding) {
        w->requeue = self;
    }
    switch_to(next, parking);
    // Perhaps on another thread now: `w` is stale, `self` isn't.
    rt_reopen(depth);
    self->rt_saved = 0;
}

/* Blocks the current fiber until something wakes it; what did. */
static int64_t block(void) {
    Fiber *self = current;
    fired_set(self, wait_word(next_wait_gen(self), PARKING));
    switch_away(true, false);
    return wait_state(self);
}

/* `block`, parking still holding the `n` object locks `locks`, let go of
 * once the switch has saved this fiber. */
static int64_t block_holding(uint32_t **locks, int n) {
    Worker *w = worker;
    for (int i = 0; i < n; i++) {
        w->park_locks[i] = locks[i];
    }
    w->park_lock_count = n;
    return block();
}
