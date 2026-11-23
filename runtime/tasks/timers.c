/*
 * timers.c — part of lugha_rt.c: sleepers, kept per worker.
 *
 * A fiber that sleeps or waits with a timeout goes into the heap of the
 * worker it waits on, ordered by time, under that heap's own lock. Each
 * worker keeps its earliest time in `next_timer` (-1 for none), which
 * others read without the lock: the worker waiting for events times its
 * wait by the earliest of all, and a worker looking for work fires
 * another's due timers too.
 *
 * A timer is armed for one wait, its generation (waiting.c): firing late,
 * after its fiber was woken otherwise and began another wait, it misses.
 * So it fires with no lock but its heap's.
 *
 * A fragment, not compiled on its own: joined after runq.c (the workers),
 * before waiting.c (`wake`).
 */

static bool wake_gen(Fiber *f, uint32_t gen, int64_t why);
static inline uint32_t next_wait_gen(const Fiber *f);

/* A new timer at `at`: the worker waiting for events times its wait again
 * (workers.c). */
static void timers_changed(int64_t at);

static void heap_place(Worker *w, int64_t i, Fiber *f) {
    w->heap[i] = f;
    f->heap_at = i;
}

static void heap_up(Worker *w, int64_t i) {
    Fiber *f = w->heap[i];
    while (i > 0) {
        int64_t parent = (i - 1) / 2;
        if (w->heap[parent]->wake_at <= f->wake_at) {
            break;
        }
        heap_place(w, i, w->heap[parent]);
        i = parent;
    }
    heap_place(w, i, f);
}

static void heap_down(Worker *w, int64_t i) {
    Fiber *f = w->heap[i];
    for (;;) {
        int64_t child = 2 * i + 1;
        if (child >= w->heap_len) {
            break;
        }
        if (child + 1 < w->heap_len && w->heap[child + 1]->wake_at < w->heap[child]->wake_at) {
            child++;
        }
        if (w->heap[child]->wake_at >= f->wake_at) {
            break;
        }
        heap_place(w, i, w->heap[child]);
        i = child;
    }
    heap_place(w, i, f);
}

/* Removes the fiber at `i` from `w`'s heap, under its lock. */
static void heap_remove(Worker *w, int64_t i) {
    Fiber *last = w->heap[--w->heap_len];
    if (i < w->heap_len) {
        heap_place(w, i, last);
        heap_down(w, i);
        heap_up(w, last->heap_at);
    }
}

static void next_timer_note(Worker *w) {
    int64_t at = w->heap_len > 0 ? w->heap[0]->wake_at : -1;
    // In one order with the poller's reading of it (`timers_changed`).
    __atomic_store_n(&w->next_timer, at, __ATOMIC_SEQ_CST);
}

/* `f` sleeps until `deadline` (monotonic ns), on the running worker. */
static void add_sleeper(Fiber *f, int64_t deadline) {
    Worker *w = worker;
    lock_take(&w->heap_lock);
    if (w->heap_len == w->heap_cap) {
        int64_t cap = w->heap_cap == 0 ? 64 : w->heap_cap * 2;
        Fiber **grown = realloc(w->heap, (size_t)cap * sizeof *grown);
        if (grown == NULL) {
            die("panic: out of memory\n");
        }
        w->heap = grown;
        w->heap_cap = cap;
    }
    f->wake_at = deadline;
    f->timer_gen = next_wait_gen(f);
    f->timer_owner = w;
    w->heap[w->heap_len] = f;
    heap_up(w, w->heap_len++);
    next_timer_note(w);
    lock_give(&w->heap_lock);
    timers_changed(deadline);
}

/* `f` sleeps no more, whichever heap held it. */
static void unsleep(Fiber *f) {
    Worker *w = __atomic_load_n(&f->timer_owner, __ATOMIC_ACQUIRE);
    if (w == NULL) {
        return;
    }
    lock_take(&w->heap_lock);
    if (f->timer_owner == w) {
        heap_remove(w, f->heap_at);
        __atomic_store_n(&f->timer_owner, NULL, __ATOMIC_RELEASE);
        f->wake_at = -1;
        next_timer_note(w);
    }
    lock_give(&w->heap_lock);
}

/* The earliest timer of any worker, or -1. */
static int64_t earliest_timer(void) {
    int64_t at = -1;
    int n = __atomic_load_n(&workers_running, __ATOMIC_ACQUIRE);
    for (int i = 0; i < n; i++) {
        Worker *w = __atomic_load_n(&all_workers[i], __ATOMIC_ACQUIRE);
        int64_t t = w == NULL ? -1 : __atomic_load_n(&w->next_timer, __ATOMIC_SEQ_CST);
        if (t >= 0 && (at < 0 || t < at)) {
            at = t;
        }
    }
    return at;
}

/* Fires `w`'s due timers: each sleeper woken with `TIMED_OUT`. True if
 * any fired. */
static bool run_due_timers(Worker *w) {
    int64_t next = __atomic_load_n(&w->next_timer, __ATOMIC_ACQUIRE);
    if (next < 0) {
        return false;
    }
    int64_t now = now_ns();
    if (next > now) {
        return false;
    }
    bool fired = false;
    for (;;) {
        lock_take(&w->heap_lock);
        Fiber *f = NULL;
        uint32_t gen = 0;
        if (w->heap_len > 0 && w->heap[0]->wake_at <= now) {
            f = w->heap[0];
            gen = f->timer_gen;
            heap_remove(w, 0);
            __atomic_store_n(&f->timer_owner, NULL, __ATOMIC_RELEASE);
            f->wake_at = -1;
        }
        next_timer_note(w);
        lock_give(&w->heap_lock);
        if (f == NULL) {
            return fired;
        }
        fired |= wake_gen(f, gen, TIMED_OUT);
    }
}
