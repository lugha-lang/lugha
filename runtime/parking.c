/*
 * parking.c — part of lugha_rt.c: how an idle worker looks for work and
 * parks. It looks at its own queue, the shared queue and what sockets and
 * timers have ready; then, spinning, steals from the others; then parks,
 * on its own futex word or as the one poller in `epoll_wait`. A spinner
 * about to park looks at every queue once more after it stops counting as
 * spinning, so a task made ready meanwhile is never stranded; the last
 * worker to park with nothing outstanding declares a deadlock.
 *
 * A fragment, not compiled on its own: joined right after workers.c (the
 * parked list, waking, the poller's state).
 */

/* Work found: a spinning worker stops spinning, and if it was the last,
 * wakes another to go on looking; so does any worker leaving work behind. */
static Fiber *found(Worker *w, Fiber *f) {
    if (w->spinning) {
        w->spinning = false;
        if (__atomic_sub_fetch(&spinning_workers, 1, __ATOMIC_ACQ_REL) == 0) {
            wake_idle_worker();
            return f;
        }
    }
    if (runq_any(w) || __atomic_load_n(&shared_len, __ATOMIC_ACQUIRE) > 0) {
        wake_idle_worker();
    }
    return f;
}

/* Four rounds over the other workers, from a random one: their due timers,
 * then half of their queues; in the last round, a run-next fiber too. */
static Fiber *steal_work(Worker *w) {
    int n = __atomic_load_n(&workers_running, __ATOMIC_ACQUIRE);
    for (int round = 0; round < 4; round++) {
        w->steal_seed = w->steal_seed * 1103515245u + 12345u;
        int start = (int)((w->steal_seed >> 16) % (uint32_t)n);
        for (int i = 0; i < n; i++) {
            Worker *v = __atomic_load_n(&all_workers[(start + i) % n], __ATOMIC_ACQUIRE);
            if (v == NULL || v == w) {
                continue;
            }
            if (run_due_timers(v)) {
                Fiber *f = take_ready();
                if (f != NULL) {
                    return f;
                }
            }
            Fiber *f = runq_steal(v, round == 3);
            if (f != NULL) {
                return f;
            }
        }
        Fiber *f = shared_take();
        if (f != NULL) {
            return f;
        }
    }
    return NULL;
}

/* In a virtual-time run (one worker), with nothing ready: what sockets
 * have ready, else time jumps to the next timer; with neither, deadlock. */
static void virtual_wait(void) {
    int64_t at = next_due();
    if (!events_pending()) {
        RT_SECTION();
        deadlock();
    }
    if (events_out()) {
        poll_sockets(at < 0 ? -1 : 0);
    }
    if (!runq_any(worker) && at > virtual_ns) {
        virtual_ns = at;
    }
}

/* One more look at every queue: true (and spinning again) if there is
 * work, after this worker parked and stopped counting as spinning. */
static bool work_left(Worker *w) {
    int n = __atomic_load_n(&workers_running, __ATOMIC_ACQUIRE);
    for (int i = 0; i < n; i++) {
        Worker *v = __atomic_load_n(&all_workers[i], __ATOMIC_ACQUIRE);
        if (v != NULL && runq_any(v)) {
            lock_take(&sched_lock);
            if (unpark_locked(w)) {
                w->spinning = true;
                __atomic_add_fetch(&spinning_workers, 1, __ATOMIC_ACQ_REL);
            }
            lock_give(&sched_lock);
            return true;
        }
    }
    return false;
}

/* Parks this worker until roused, or until the poll it waits in returns.
 * Called with nothing found anywhere. */
static void park(Worker *w) {
    stack_cache_trim(w);
    lock_take(&sched_lock);
    // Checked under the lock `wake_main_worker` takes: `main` back for the
    // main thread just before it would park, it doesn't.
    if (shared_len > 0 || (w == main_worker && __atomic_load_n(&home_fiber, __ATOMIC_ACQUIRE) != NULL)) {
        lock_give(&sched_lock);
        return;
    }
    parked_workers[parked_count++] = w;
    w->parked = true;
    // Read and cleared under the lock: once it is let go of, a waker may
    // set it again, rousing this worker to spin.
    bool was_spinning = w->spinning;
    w->spinning = false;
    int idle = __atomic_add_fetch(&idle_workers, 1, __ATOMIC_ACQ_REL);
    uint32_t seen = __atomic_load_n(&w->wake, __ATOMIC_ACQUIRE);
    lock_give(&sched_lock);
    if (was_spinning) {
        // No longer spinning, then one more look everywhere: a fiber made
        // ready meanwhile saw this worker spinning and woke nobody.
        __atomic_sub_fetch(&spinning_workers, 1, __ATOMIC_SEQ_CST);
        __atomic_thread_fence(__ATOMIC_SEQ_CST);
        if (work_left(w)) {
            return;
        }
    }
    if (idle == __atomic_load_n(&workers_running, __ATOMIC_ACQUIRE) &&
        __atomic_load_n(&idle_workers, __ATOMIC_ACQUIRE) == idle &&
        __atomic_load_n(&home_fiber, __ATOMIC_ACQUIRE) == NULL &&
        __atomic_load_n(&shared_len, __ATOMIC_ACQUIRE) == 0 && !any_queued() &&
        !events_pending()) {
        RT_SECTION();
        deadlock();
    }
    bool idle_poller = false;
    if (__atomic_compare_exchange_n(&poller_busy, &idle_poller, true, false, __ATOMIC_SEQ_CST,
                                    __ATOMIC_RELAXED)) {
        wake_open();
        __atomic_store_n(&w->polling, true, __ATOMIC_RELEASE);
        __atomic_store_n(&poll_until, POLL_TIMING, __ATOMIC_SEQ_CST);
        int64_t at = next_due();
        __atomic_store_n(&poll_until, at, __ATOMIC_SEQ_CST);
        int64_t wait = at < 0 ? -1 : at - now_ns();
        // A wait of 0 polls without blocking: back from it at once.
        if (at >= 0 && wait <= 0) {
            poller_back();
        }
        poll_sockets(at >= 0 && wait <= 0 ? 0 : wait);
        __atomic_store_n(&poller_busy, false, __ATOMIC_RELEASE);
        return;
    }
    while (__atomic_load_n(&w->wake, __ATOMIC_ACQUIRE) == seen) {
        futex_wait(&w->wake, seen);
    }
}

/* Looks for work until it finds some, parking between looks. */
static Fiber *find_work(void) {
    Worker *w = worker;
    for (;;) {
        sched_tick();
        Fiber *f = take_ready();
        if (f != NULL) {
            return found(w, f);
        }
        if (virtual_time()) {
            virtual_wait();
            continue;
        }
        if (events_out() && !__atomic_load_n(&poller_busy, __ATOMIC_ACQUIRE)) {
            poll_sockets(0);
            f = take_ready();
            if (f != NULL) {
                return found(w, f);
            }
        }
        int running = __atomic_load_n(&workers_running, __ATOMIC_ACQUIRE);
        int idle = __atomic_load_n(&idle_workers, __ATOMIC_ACQUIRE);
        int spinning = __atomic_load_n(&spinning_workers, __ATOMIC_ACQUIRE);
        if (running > 1 && (w->spinning || 2 * spinning < running - idle)) {
            if (!w->spinning) {
                w->spinning = true;
                __atomic_add_fetch(&spinning_workers, 1, __ATOMIC_ACQ_REL);
            }
            f = steal_work(w);
            if (f != NULL) {
                return found(w, f);
            }
        }
        park(w);
    }
}
