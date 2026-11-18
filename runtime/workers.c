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
 * Every worker takes ready fibers from one queue under `rt_lock`. With none
 * ready it runs its idle loop, a fiber of its own: one idle worker at a
 * time waits in `epoll_wait` for sockets, helper jobs and the wake-up
 * event, until the next timer (the poller); the others sleep on their own
 * condition variable. An idle worker is woken only for work the waking
 * worker won't take itself: a spawned task, what the poller found, or what
 * is left after a worker took one. A task woken by a message waits for its
 * waker's worker, so a channel ping-pong stays on one thread.
 *
 * `main` ends on the main thread, where `lugha_rt_main` returns: its fiber
 * hands itself back there when the program's entry returns.
 *
 * A fragment, not compiled on its own: joined after net.c (`poll_sockets`,
 * `net_waiting`) and jobs.c (the `epoll` set, the wake-up event).
 */

enum { WORKERS_MOST = 256 };

/* How many workers the program runs, how many have started, and each. */
static int workers_wanted = 1;
static int workers_running = 1;
static bool workers_started;
static Worker *all_workers[WORKERS_MOST];
static Worker *main_worker;

/* Workers in their idle loop, and whether one of them is the poller. */
static int workers_idle;
static bool poller_busy;

/* `main`'s fiber, waiting for the main thread to take it back. */
static Fiber *home_fiber;

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

/* The calling worker's idle loop, a fiber of its own on a pooled stack,
 * started inside a section (it runs holding the lock). */
static void idle_make(void) {
    Fiber *f = lugha_rc_internal(sizeof(Fiber));
    f->map = new_stack();
    f->top = stack_top(f->map);
    budget_fill(f->map);
    f->entry = idle_entry;
    f->wake_at = -1;
    f->rt_depth = 1;
    fiber_frame(f);
    worker->idle = f;
}

/* A worker thread: its signals, C stack and idle loop, then it waits for
 * work forever. Only the main thread takes Ctrl-C. */
static void *worker_thread(void *arg) {
    (void)arg;
    sigset_t interrupt;
    sigemptyset(&interrupt);
    sigaddset(&interrupt, SIGINT);
    pthread_sigmask(SIG_BLOCK, &interrupt, NULL);
    lugha_rt_worker_enter();
    stack_overflow_handler_thread();
    // Where this thread's native stack is saved as it goes to its loop.
    static __thread Fiber boot;
#if defined(__SANITIZE_THREAD__)
    boot.tsan = __tsan_get_current_fiber();
#endif
    rt_enter();
    c_stack_make();
    idle_make();
    current = &boot;
    switch_to(worker->idle, false);
    die("panic: internal error: a worker's boot came back\n");
    return NULL;
}

/* Starts the other workers, at the first spawn after lockdown. */
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
        if (pthread_create(&thread, &attr, worker_thread, NULL) != 0) {
            break;
        }
        workers_running++;
    }
    pthread_attr_destroy(&attr);
}

/* Sleeps the calling worker, the lock let go of meanwhile, until another
 * rouses it (`worker_rouse`, under the lock). */
static void worker_sleep(void) {
    Worker *w = worker;
    uint32_t seen = __atomic_load_n(&w->wake, __ATOMIC_ACQUIRE);
    w->sleeping = true;
    rt_lock_give();
    while (__atomic_load_n(&w->wake, __ATOMIC_ACQUIRE) == seen) {
        futex_wait(&w->wake, seen);
    }
    rt_lock_take();
    w->sleeping = false;
}

static void worker_rouse(Worker *w) {
    w->sleeping = false;
    __atomic_fetch_add(&w->wake, 1, __ATOMIC_RELEASE);
    futex_wake(&w->wake, 1);
}

/* Wakes a sleeping or polling worker, if any is idle. */
static void wake_idle_worker(void) {
    if (workers_idle == 0) {
        return;
    }
    pthread_mutex_lock(&workers_lock);
    for (Worker *w = workers; w != NULL; w = w->next_worker) {
        if (w->sleeping) {
            worker_rouse(w);
            pthread_mutex_unlock(&workers_lock);
            return;
        }
    }
    pthread_mutex_unlock(&workers_lock);
    if (poller_busy) {
        poller_kick();
    }
}

/* Wakes the main thread's worker, wherever it waits. */
static void wake_main_worker(void) {
    if (main_worker->sleeping) {
        worker_rouse(main_worker);
    } else if (main_worker->polling) {
        poller_kick();
    }
}

static void timers_changed(void) {
    if (poller_busy) {
        poller_kick();
    }
}

/* The next fiber this worker may run: `main`'s, back home, first. */
static Fiber *take_ready(void) {
    if (home_fiber != NULL && worker == main_worker) {
        Fiber *f = home_fiber;
        home_fiber = NULL;
        return f;
    }
    return pop_ready();
}

/* True if something ready waits for this worker: a fiber, or `main` back
 * home on the main thread. */
static bool ready_here(void) {
    return ready_head != NULL || (home_fiber != NULL && worker == main_worker);
}

/* Something may happen without a task acting: a timer, a socket, a helper
 * job or a Ctrl-C to see to. */
static bool events_pending(int64_t at) {
    return at >= 0 || net_waiting > 0 || jobs_out > 0 || interrupts != 0;
}

/* The next sleeper's, deadline's or shield's time; -1 for none. */
static int64_t next_timer(void) {
    int64_t at = sleepers == NULL ? -1 : sleepers->wake_at;
    int64_t timer = next_time();
    if (timer >= 0 && (at < 0 || timer < at)) {
        at = timer;
    }
    return at;
}

/* Nothing is ready for this worker: in a virtual-time run, time jumps to
 * the next timer; else it waits as the poller, or sleeps. A deadlock is
 * every worker idle with nothing that could wake one. */
static void idle_wait(void) {
    int64_t at = next_timer();
    if (virtual_time()) {
        if (!events_pending(at)) {
            deadlock();
        }
        if (net_waiting > 0) {
            poll_sockets(at < 0 ? -1 : 0);
        }
        if (ready_head == NULL && at > virtual_ns) {
            virtual_ns = at;
        }
        return;
    }
    workers_idle++;
    if (workers_idle == workers_running && home_fiber == NULL && !events_pending(at)) {
        deadlock();
    }
    if (!poller_busy) {
        wake_open();
        poller_busy = true;
        worker->polling = true;
        int64_t wait = at < 0 ? -1 : at - now_ns();
        poll_sockets(at >= 0 && wait < 0 ? 0 : wait);
        worker->polling = false;
        poller_busy = false;
        // Off to run what it found: another worker polls on, while timers,
        // sockets or jobs are out.
        if (ready_here() && events_pending(next_timer())) {
            wake_idle_worker();
        }
    } else {
        worker_sleep();
    }
    workers_idle--;
}

/* A worker's idle loop: runs whatever is ready, else waits for it. Each
 * fiber it switches to comes back here when nothing else is ready. */
static void worker_loop(void) {
    for (;;) {
        wake_due_sleepers();
        Fiber *f = take_ready();
        if (f != NULL) {
            // More than this worker takes: another may run it.
            if (ready_head != NULL) {
                wake_idle_worker();
            }
            switch_to(f, false);
            continue;
        }
        idle_wait();
    }
}

/* The program's entry returned on `main`'s fiber: back to the main thread,
 * where `lugha_rt_main` returns, if it isn't there. */
static void main_go_home(void) {
    RT_SECTION();
    if (worker == main_worker) {
        return;
    }
    home_fiber = current;
    wake_main_worker();
    Fiber *next = pop_ready();
    switch_to(next != NULL ? next : worker->idle, false);
}
