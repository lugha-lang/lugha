/*
 * runq.c — part of lugha_rt.c: where ready fibers wait for a worker.
 *
 * Each worker has a ring of `RUNQ_SIZE` fibers. Only its owner pushes (at
 * the tail) and pops (at the head, by compare-and-swap); another worker
 * with nothing to run steals half of it, taking the head with one
 * compare-and-swap. A full ring moves half its fibers to the shared queue,
 * a list under `sched_lock`, which every worker checks when its own runs
 * dry, and every 61st pick anyway, so nothing there waits forever.
 *
 * With several workers, a fiber a running task wakes (a send, a join, a
 * reply) goes to its waker's run-next slot instead, taken before the ring,
 * at most three picks in a row: a ping-pong stays on one thread. A stealer
 * takes it only from a worker that isn't running a task. With one worker
 * there is no slot, so the order stays first in, first out, as tests and
 * deterministic runs expect.
 *
 * A pinned fiber (`pin` set) never enters a ring or the shared queue: made
 * ready, by anyone, it goes to its worker's inbox, which only that worker
 * takes from, first, taking turns with its ring when both have fibers.
 *
 * No fiber is put where another worker may take it before its own switch
 * has saved it: a fiber yielding or going home is queued by the fiber that
 * lands after it (waiting.c), as a parking one is.
 *
 * A fragment, not compiled on its own: joined after tasks.c (fibers, the
 * scheduling mode), before waiting.c.
 */

enum { WORKERS_MOST = 256, PICKS_FAIR = 61, NEXT_MOST = 3 };

/* How many workers the program runs (workers.c), how many have started,
 * and each, by index. */
static int workers_wanted = 1;
static int workers_running = 1;
static Worker *all_workers[WORKERS_MOST];

/* The shared queue, and (workers.c) the parked workers, under one lock;
 * the queue's length, read without it. */
static uint32_t sched_lock;
static Fiber *shared_head;
static Fiber *shared_tail;
static int64_t shared_len;

/* Whether woken fibers go to their waker's run-next slot. */
static bool run_next_on(void) {
    return workers_wanted > 1;
}

/* `n` fibers linked through `next`, `head` to `tail`, onto the shared
 * queue. */
static void shared_put(Fiber *head, Fiber *tail, int64_t n) {
    tail->next = NULL;
    lock_take(&sched_lock);
    if (shared_tail == NULL) {
        shared_head = head;
    } else {
        shared_tail->next = head;
    }
    shared_tail = tail;
    __atomic_store_n(&shared_len, shared_len + n, __ATOMIC_RELEASE);
    lock_give(&sched_lock);
}

/* The shared queue's first fiber, or NULL. */
static Fiber *shared_take(void) {
    if (__atomic_load_n(&shared_len, __ATOMIC_ACQUIRE) == 0) {
        return NULL;
    }
    lock_take(&sched_lock);
    Fiber *f = shared_head;
    if (f != NULL) {
        shared_head = f->next;
        if (shared_head == NULL) {
            shared_tail = NULL;
        }
        __atomic_store_n(&shared_len, shared_len - 1, __ATOMIC_RELEASE);
    }
    lock_give(&sched_lock);
    return f;
}

/* Half of `w`'s full ring, and `f`, onto the shared queue; false if a
 * stealer took some meanwhile (the caller tries again). */
static bool runq_spill(Worker *w, Fiber *f, uint32_t head, uint32_t tail) {
    uint32_t n = (tail - head) / 2;
    Fiber *batch[RUNQ_SIZE / 2 + 1];
    for (uint32_t i = 0; i < n; i++) {
        batch[i] = w->ring[(head + i) % RUNQ_SIZE];
    }
    if (!__atomic_compare_exchange_n(&w->ring_head, &head, head + n, false, __ATOMIC_RELEASE,
                                     __ATOMIC_RELAXED)) {
        return false;
    }
    batch[n] = f;
    for (uint32_t i = 0; i < n; i++) {
        batch[i]->next = batch[i + 1];
    }
    shared_put(batch[0], f, n + 1);
    return true;
}

/* `f` at the tail of the running worker's ring. */
static void runq_put(Fiber *f) {
    Worker *w = worker;
    for (;;) {
        uint32_t head = __atomic_load_n(&w->ring_head, __ATOMIC_ACQUIRE);
        uint32_t tail = w->ring_tail;
        if (tail - head < RUNQ_SIZE) {
            w->ring[tail % RUNQ_SIZE] = f;
            __atomic_store_n(&w->ring_tail, tail + 1, __ATOMIC_RELEASE);
            return;
        }
        if (runq_spill(w, f, head, tail)) {
            return;
        }
    }
}

/* The head of the running worker's ring, or NULL; in a shuffled run, a
 * fiber anywhere in it, from the seed. */
static Fiber *runq_get(void) {
    Worker *w = worker;
    for (;;) {
        uint32_t head = __atomic_load_n(&w->ring_head, __ATOMIC_ACQUIRE);
        uint32_t tail = w->ring_tail;
        if (tail == head) {
            return NULL;
        }
        // Shuffled runs have one worker: nobody steals meanwhile.
        if (sched_mode == MODE_SHUFFLE) {
            uint32_t at = head + (uint32_t)(shuffle_next() % (tail - head));
            Fiber *picked = w->ring[at % RUNQ_SIZE];
            w->ring[at % RUNQ_SIZE] = w->ring[head % RUNQ_SIZE];
            w->ring[head % RUNQ_SIZE] = picked;
        }
        Fiber *f = w->ring[head % RUNQ_SIZE];
        if (__atomic_compare_exchange_n(&w->ring_head, &head, head + 1, false, __ATOMIC_RELEASE,
                                        __ATOMIC_RELAXED)) {
            return f;
        }
    }
}

/* True if `v` runs a task now, not its idle loop. */
static bool runs_task(Worker *v) {
    Fiber *running = __atomic_load_n(&v->running, __ATOMIC_ACQUIRE);
    return running != NULL && running != __atomic_load_n(&v->idle, __ATOMIC_ACQUIRE);
}

/* Half of `v`'s ring into `batch`; with `next`, its run-next fiber when
 * its ring is empty and it runs no task. How many were taken. */
static uint32_t runq_grab(Worker *v, Fiber **batch, bool next) {
    for (;;) {
        uint32_t head = __atomic_load_n(&v->ring_head, __ATOMIC_ACQUIRE);
        uint32_t tail = __atomic_load_n(&v->ring_tail, __ATOMIC_ACQUIRE);
        uint32_t n = tail - head;
        n -= n / 2;
        if (n == 0) {
            if (next && !runs_task(v)) {
                Fiber *f = __atomic_load_n(&v->run_next, __ATOMIC_ACQUIRE);
                if (f != NULL && __atomic_compare_exchange_n(&v->run_next, &f, NULL, false,
                                                             __ATOMIC_ACQ_REL, __ATOMIC_RELAXED)) {
                    batch[0] = f;
                    return 1;
                }
            }
            return 0;
        }
        // Read while the owner moved on: the two ends are inconsistent.
        if (n > RUNQ_SIZE / 2) {
            continue;
        }
        for (uint32_t i = 0; i < n; i++) {
            batch[i] = v->ring[(head + i) % RUNQ_SIZE];
        }
        if (__atomic_compare_exchange_n(&v->ring_head, &head, head + n, false, __ATOMIC_ACQ_REL,
                                        __ATOMIC_RELAXED)) {
            return n;
        }
    }
}

/* Steals half of `v`'s fibers for the running worker, whose ring is
 * empty: one to run, the rest into its ring. */
static Fiber *runq_steal(Worker *v, bool next) {
    Fiber *batch[RUNQ_SIZE / 2];
    uint32_t n = runq_grab(v, batch, next);
    if (n == 0) {
        return NULL;
    }
    for (uint32_t i = 0; i + 1 < n; i++) {
        runq_put(batch[i]);
    }
    return batch[n - 1];
}

/* True if `v` has a fiber waiting in its ring or run-next slot. */
static bool runq_any(Worker *v) {
    uint32_t head = __atomic_load_n(&v->ring_head, __ATOMIC_ACQUIRE);
    uint32_t tail = __atomic_load_n(&v->ring_tail, __ATOMIC_ACQUIRE);
    return tail != head || __atomic_load_n(&v->run_next, __ATOMIC_ACQUIRE) != NULL;
}

static void wake_worker(Worker *w);

/* `f`, pinned, onto its worker's inbox; that worker roused if parked. */
static void inbox_put(Fiber *f, Worker *w) {
    f->next = NULL;
    lock_take(&w->inbox_lock);
    if (w->inbox_tail == NULL) {
        w->inbox_head = f;
    } else {
        w->inbox_tail->next = f;
    }
    w->inbox_tail = f;
    __atomic_store_n(&w->inbox_len, w->inbox_len + 1, __ATOMIC_RELEASE);
    lock_give(&w->inbox_lock);
    // Stored before the parking check's lock is taken: either `w`, about
    // to park, sees it, or it is parked by then and is roused.
    if (w != worker) {
        wake_worker(w);
    }
}

/* The first of the running worker's pinned fibers, or NULL. */
static Fiber *inbox_take(Worker *w) {
    if (__atomic_load_n(&w->inbox_len, __ATOMIC_ACQUIRE) == 0) {
        return NULL;
    }
    lock_take(&w->inbox_lock);
    Fiber *f = w->inbox_head;
    if (f != NULL) {
        w->inbox_head = f->next;
        if (w->inbox_head == NULL) {
            w->inbox_tail = NULL;
        }
        __atomic_store_n(&w->inbox_len, w->inbox_len - 1, __ATOMIC_RELEASE);
    }
    lock_give(&w->inbox_lock);
    return f;
}

/* True if `f` is pinned: it goes to its worker's inbox. */
static bool pinned_ready(Fiber *f) {
    Worker *to = __atomic_load_n(&f->pin, __ATOMIC_ACQUIRE);
    if (to == NULL) {
        return false;
    }
    inbox_put(f, to);
    return true;
}

/* `f`, woken by the running task, runs next on this worker: the fiber
 * that was to run next goes to the ring. */
static void ready_next(Fiber *f) {
    if (pinned_ready(f)) {
        return;
    }
    if (!run_next_on()) {
        runq_put(f);
        return;
    }
    Fiber *before = __atomic_exchange_n(&worker->run_next, f, __ATOMIC_ACQ_REL);
    if (before != NULL) {
        runq_put(before);
    }
}

/* `f` is ready, behind what this worker already has. */
static void make_ready(Fiber *f) {
    if (!pinned_ready(f)) {
        runq_put(f);
    }
}

/* The main thread's worker: `main` goes back to it to end the program,
 * and a pinned scope `main` opens pins to it (workers.c). */
static Worker *main_worker;

/* The next fiber this worker runs, or NULL: its pinned fibers, taking
 * turns with its ring when both have some; every 61st pick, the shared
 * queue; the run-next slot, three picks in a row at most; the ring; the
 * shared queue. */
static Fiber *take_ready(void) {
    Worker *w = worker;
    if (!(w->inbox_turn && runq_any(w))) {
        Fiber *f = inbox_take(w);
        if (f != NULL) {
            w->inbox_turn = true;
            return f;
        }
    }
    w->inbox_turn = false;
    if (--w->fair_left <= 0) {
        w->fair_left = PICKS_FAIR;
        Fiber *f = shared_take();
        if (f != NULL) {
            return f;
        }
    }
    if (w->next_streak < NEXT_MOST) {
        Fiber *f = __atomic_exchange_n(&w->run_next, NULL, __ATOMIC_ACQ_REL);
        if (f != NULL) {
            w->next_streak++;
            return f;
        }
    }
    w->next_streak = 0;
    Fiber *f = runq_get();
    if (f == NULL) {
        f = __atomic_exchange_n(&w->run_next, NULL, __ATOMIC_ACQ_REL);
    }
    if (f == NULL) {
        f = shared_take();
    }
    return f;
}
