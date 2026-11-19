/*
 * workers.c — part of lugha_rt.c: the worker threads that run tasks.
 *
 * A program runs `LUGHA_WORKERS` workers (1 to 256), else one per CPU it
 * may use; a deterministic or shuffled run always one. The main thread is
 * the first. The others start together at the first spawn after process
 * lockdown, so a program that never spawns stays one thread and lockdown
 * covers every worker. A worker never exits: blocks it carved are freed
 * back to it.
 *
 * A worker with nothing ready runs its idle loop, a fiber of its own,
 * without any lock held, looking for work and parking (parking.c). Go's
 * protocol keeps a task from being stranded: making a task ready wakes a
 * parked worker only when none is spinning, a spinner finding work wakes
 * another, at most half the busy workers spin, and a spinner about to park
 * looks at every queue once more after it stops counting as spinning.
 *
 * `main` ends on the main thread, where `lugha_rt_main` returns: its fiber
 * hands itself back there when the program's entry returns.
 *
 * A fragment, not compiled on its own: joined after net.c (`poll_sockets`,
 * `net_waiting`) and jobs.c (the `epoll` set, the wake-up event).
 */

static bool workers_started;

/* Parked workers, under `sched_lock`; how many, and how many spin, read
 * without it. */
static Worker *parked_workers[WORKERS_MOST];
static int parked_count;
static int idle_workers;
static int spinning_workers;

/* Whether a worker waits in `epoll_wait`, and until when (-1: no timer). */
static bool poller_busy;
static int64_t poll_until = -1;

/* The number of CPUs this process may run on. */
static int cpus_usable(void) {
    cpu_set_t set;
    if (sched_getaffinity(0, sizeof set, &set) == 0) {
        int n = CPU_COUNT(&set);
        return n > 0 ? n : 1;
    }
    long n = sysconf(_SC_NPROCESSORS_ONLN);
    return n > 0 ? (int)n : 1;
}

/* Reads `LUGHA_WORKERS` as the runtime starts, after the scheduling mode. */
static void workers_init(void) {
    main_worker = worker;
    all_workers[0] = worker;
    const char *text = getenv("LUGHA_WORKERS");
    if (text != NULL) {
        char *end;
        errno = 0;
        long n = strtol(text, &end, 10);
        if (errno != 0 || end == text || *end != '\0' || n < 1 || n > WORKERS_MOST) {
            die("panic: LUGHA_WORKERS must be from 1 to 256\n");
        }
        workers_wanted = (int)n;
    } else {
        int n = cpus_usable();
        workers_wanted = n > WORKERS_MOST ? WORKERS_MOST : n;
    }
    // Virtual time and seeded order need everything on one thread.
    if (virtual_time()) {
        workers_wanted = 1;
    }
}

static void worker_loop(void);

static void idle_entry(void *code, void *env, void *result) {
    (void)code;
    (void)env;
    (void)result;
    worker_loop();
}

/* The calling worker's idle loop, a fiber of its own on a pooled stack. */
static void idle_make(void) {
    Fiber *f = lugha_rc_internal(sizeof(Fiber));
    f->map = new_stack();
    f->top = stack_top(f->map);
    budget_fill(f->map);
    f->entry = idle_entry;
    f->wake_at = -1;
    fiber_frame(f);
    __atomic_store_n(&worker->idle, f, __ATOMIC_RELEASE);
}

/* A worker thread: its signals, C stack and idle loop, then it looks for
 * work forever. Only the main thread takes Ctrl-C. */
static void *worker_thread(void *arg) {
    sigset_t interrupt;
    sigemptyset(&interrupt);
    sigaddset(&interrupt, SIGINT);
    pthread_sigmask(SIG_BLOCK, &interrupt, NULL);
    lugha_rt_worker_enter();
    stack_overflow_handler_thread();
    worker->index = (int)(intptr_t)arg;
    worker->steal_seed = (uint32_t)(intptr_t)arg * 2654435761u + 1;
    // Where this thread's native stack is saved as it goes to its loop.
    static __thread Fiber boot;
#if defined(__SANITIZE_THREAD__)
    boot.tsan = __tsan_get_current_fiber();
#endif
    c_stack_make();
    idle_make();
    __atomic_store_n(&worker->running, &boot, __ATOMIC_RELEASE);
    __atomic_store_n(&all_workers[worker->index], worker, __ATOMIC_RELEASE);
    switch_to(worker->idle, false);
    die("panic: internal error: a worker's boot came back\n");
    return NULL;
}

/* Starts the other workers, at the first spawn after lockdown, inside the
 * spawn's section. */
static void workers_start(void) {
    if (workers_started || workers_wanted <= 1 || !helpers_allowed || virtual_time()) {
        return;
    }
    workers_started = true;
    // From here on the lock is real: this thread, inside its sections,
    // takes it before any other worker can run.
    rt_locking = true;
    rt_lock_take();
    wake_open();
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
    // The native stack only starts the thread; tasks run on pooled ones.
    pthread_attr_setstacksize(&attr, 256 * 1024);
    for (int i = 1; i < workers_wanted; i++) {
        pthread_t thread;
        if (pthread_create(&thread, &attr, worker_thread, (void *)(intptr_t)i) != 0) {
            break;
        }
        __atomic_fetch_add(&workers_running, 1, __ATOMIC_ACQ_REL);
    }
    pthread_attr_destroy(&attr);
}

/* Wakes `w`, parked: from its futex word, or out of `epoll_wait`. */
static void worker_rouse(Worker *w) {
    if (__atomic_load_n(&w->polling, __ATOMIC_ACQUIRE)) {
        poller_kick();
        return;
    }
    __atomic_fetch_add(&w->wake, 1, __ATOMIC_RELEASE);
    futex_wake(&w->wake, 1);
}

/* Takes `w` off the parked list; false if someone already did. Under
 * `sched_lock`. */
static bool unpark_locked(Worker *w) {
    if (!w->parked) {
        return false;
    }
    for (int i = 0; i < parked_count; i++) {
        if (parked_workers[i] == w) {
            parked_workers[i] = parked_workers[--parked_count];
            break;
        }
    }
    w->parked = false;
    __atomic_fetch_sub(&idle_workers, 1, __ATOMIC_ACQ_REL);
    return true;
}

/* Wakes a parked worker to spin, unless one spins already (Go's
 * `wakep`): it finds what was made ready, and wakes another if more is. */
static void wake_idle_worker(void) {
    if (__atomic_load_n(&idle_workers, __ATOMIC_ACQUIRE) == 0) {
        return;
    }
    int none = 0;
    if (__atomic_load_n(&spinning_workers, __ATOMIC_ACQUIRE) != 0 ||
        !__atomic_compare_exchange_n(&spinning_workers, &none, 1, false, __ATOMIC_ACQ_REL,
                                     __ATOMIC_RELAXED)) {
        return;
    }
    lock_take(&sched_lock);
    // A sleeper rather than the poller, which keeps polling.
    Worker *w = NULL;
    for (int i = 0; i < parked_count && w == NULL; i++) {
        if (!__atomic_load_n(&parked_workers[i]->polling, __ATOMIC_ACQUIRE)) {
            w = parked_workers[i];
        }
    }
    if (w == NULL && parked_count > 0) {
        w = parked_workers[0];
    }
    if (w != NULL) {
        unpark_locked(w);
        w->spinning = true;
    }
    lock_give(&sched_lock);
    if (w == NULL) {
        __atomic_fetch_sub(&spinning_workers, 1, __ATOMIC_ACQ_REL);
        return;
    }
    worker_rouse(w);
}

/* The poller, back from `epoll_wait`, is busy again before it wakes the
 * tasks it found: a worker parking meanwhile mustn't see everyone idle. */
static void poller_back(void) {
    Worker *w = worker;
    __atomic_store_n(&w->polling, false, __ATOMIC_RELEASE);
    lock_take(&sched_lock);
    unpark_locked(w);
    lock_give(&sched_lock);
}

/* True if any worker has a fiber in its queue. */
static bool any_queued(void) {
    int n = __atomic_load_n(&workers_running, __ATOMIC_ACQUIRE);
    for (int i = 0; i < n; i++) {
        Worker *v = __atomic_load_n(&all_workers[i], __ATOMIC_ACQUIRE);
        if (v != NULL && runq_any(v)) {
            return true;
        }
    }
    return false;
}

/* Wakes the main thread's worker, if parked: `main` is back for it. */
static void wake_main_worker(void) {
    lock_take(&sched_lock);
    bool was_parked = unpark_locked(main_worker);
    lock_give(&sched_lock);
    if (was_parked) {
        worker_rouse(main_worker);
    }
}

/* A fiber that yielded, or `main`'s going home, queued now that the
 * switch away from it has saved it. */
static void requeue_finish(void) {
    Worker *w = worker;
    Fiber *f = w->requeue;
    if (f != NULL) {
        w->requeue = NULL;
        make_ready(f);
        // It ran a slice: another worker may take it, while this one runs
        // what it yielded to.
        wake_idle_worker();
    }
    Fiber *home = w->send_home;
    if (home != NULL) {
        w->send_home = NULL;
        __atomic_store_n(&home_fiber, home, __ATOMIC_RELEASE);
        wake_main_worker();
    }
}

/* `poll_until` while the poller works out its wait: any new timer kicks. */
#define POLL_TIMING INT64_MIN

/* A timer at `at` was just added (its heap's `next_timer` stored first):
 * the poller, if its wait would outlast it, times its wait again. Both
 * sides use sequentially consistent order, so either the poller's reading
 * of the timers sees this one, or this sees the poller's wait and kicks. */
static void timers_changed(int64_t at) {
    if (!__atomic_load_n(&poller_busy, __ATOMIC_SEQ_CST)) {
        return;
    }
    int64_t until = __atomic_load_n(&poll_until, __ATOMIC_SEQ_CST);
    if (until == POLL_TIMING || until < 0 || at < until) {
        poller_kick();
    }
}

/* Sockets waited on or helper jobs out. */
static bool events_out(void) {
    return __atomic_load_n(&net_waiting, __ATOMIC_ACQUIRE) > 0 ||
           __atomic_load_n(&jobs_out, __ATOMIC_ACQUIRE) > 0;
}

/* The earliest time something is due: a sleeper on any worker, a scope's
 * deadline or a shield's end; -1 for none. */
static int64_t next_due(void) {
    int64_t at = earliest_timer();
    if (__atomic_load_n(&timed_any, __ATOMIC_ACQUIRE)) {
        RT_SECTION();
        int64_t timed = next_time();
        if (timed >= 0 && (at < 0 || timed < at)) {
            at = timed;
        }
    }
    return at;
}

/* Something may happen without a task acting: a timer, a socket, a helper
 * job or a Ctrl-C to see to. */
static bool events_pending(void) {
    return next_due() >= 0 || events_out() ||
           __atomic_load_n(&interrupts, __ATOMIC_ACQUIRE) != 0;
}

/* At a scheduling point with something pending, and every 61st anyway:
 * this worker's due timers; deadlines, shields and Ctrl-C, when there are
 * any; and every 61st, what sockets and helpers have ready, when no worker
 * polls. */
static void sched_tick_slow(Worker *w) {
    bool look = w->tick_left <= 0;
    if (look) {
        w->tick_left = PICKS_FAIR;
    }
    run_due_timers(w);
    if (__atomic_load_n(&timed_any, __ATOMIC_ACQUIRE) ||
        __atomic_load_n(&interrupts, __ATOMIC_ACQUIRE) != 0) {
        RT_SECTION();
        check_times();
    }
    if (look && events_out() &&
        !__atomic_load_n(&poller_busy, __ATOMIC_ACQUIRE)) {
        poll_sockets(0);
    }
}

/* Looks for work until it finds some, parking between looks (parking.c). */
static Fiber *find_work(void);

/* A worker's idle loop: runs whatever it finds. Each fiber it switches to
 * comes back here when nothing else is ready. */
static void worker_loop(void) {
    for (;;) {
        switch_to(find_work(), false);
    }
}

/* The program's entry returned on `main`'s fiber: back to the main thread,
 * where `lugha_rt_main` returns, if it isn't there. */
static void main_go_home(void) {
    if (worker == main_worker) {
        return;
    }
    worker->send_home = current;
    switch_away(false, false);
}
