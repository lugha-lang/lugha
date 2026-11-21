/*
 * select.c — part of lugha_rt.c: `select`. It locks its channels in
 * address order, takes a ready case at random (one draw among those that
 * look ready, as a shuffled run's seed expects), or else queues a waiter
 * on every case's channel and waits for the first to fire; a ring it reads
 * is told to wake it (rings.c). A send arm on an unread channel is ready,
 * and its value dropped.
 *
 * A fragment, not compiled on its own: joined right after rings.c, after
 * chans.c, whose channels, waiters and claims it uses.
 */

static uint64_t pick(uint64_t n) {
    if (sched_mode == MODE_SHUFFLE) {
        return shuffle_next() % n;
    }
    Worker *w = worker;
    w->pick_state ^= w->pick_state << 13;
    w->pick_state ^= w->pick_state >> 7;
    w->pick_state ^= w->pick_state << 17;
    return w->pick_state % n;
}

/* The locks of `cases`' channels, each once, in address order (the order
 * every `select` takes them in): how many. */
static int select_locks(const LughaCase *cases, int64_t n, uint32_t **locks) {
    int count = 0;
    for (int64_t i = 0; i < n; i++) {
        uint32_t *lock = &cases[i].chan->lock;
        int at = count;
        while (at > 0 && locks[at - 1] > lock) {
            at--;
        }
        if (at > 0 && locks[at - 1] == lock) {
            continue;
        }
        memmove(locks + at + 1, locks + at, (size_t)(count - at) * sizeof *locks);
        locks[at] = lock;
        count++;
    }
    return count;
}

/* True if `c`'s case looks ready under its lock: a waiter on the other
 * side, room or a value, or closed. A waiter may still be lost to another
 * claim. */
static bool looks_ready(const LughaCase *k) {
    const LughaChan *c = k->chan;
    if (c->closed || (k->kind == 1 && c->unread)) {
        return true;
    }
    if (c->ring != NULL) {
        return k->kind == 0 && ring_has(c);
    }
    if (k->kind == 0 ? c->len > 0 : (c->unbounded || c->len < c->cap)) {
        return true;
    }
    for (const Waiter *w = k->kind == 0 ? c->sendq : c->recvq; w != NULL; w = w->next) {
        if (is_waiting(w->fiber)) {
            return true;
        }
    }
    return false;
}

static void unlock_all(uint32_t **locks, int n) {
    for (int i = 0; i < n; i++) {
        lock_give(locks[i]);
    }
}

/* `select`: the case that ran, DEFAULT_ARM (-1) for `default`, or
 * TIMED_OUT (-2) for `after`. `timeout_ms` < 0 means no `after`. A ready
 * case is picked at random; otherwise the thread waits on all of them. */
int64_t lugha_rt_task_select(LughaCase *cases, int64_t n, int64_t timeout_ms, int32_t has_default,
                             const char *file, int64_t line, int64_t col) {
    ensure_started();
    lugha_rt_cancel_point();
    if (n > 64) {
        die("panic: a select with more than 64 arms\n");
    }
    uint32_t *locks[64];
    int nlocks = select_locks(cases, n, locks);
    for (int i = 0; i < nlocks; i++) {
        lock_take(locks[i]);
    }
    // A ready case at random, by one draw among those that look ready, as
    // a shuffled run's seed expects; one whose waiter another thread took
    // meanwhile is passed over, and the rest tried in turn.
    int64_t ready = 0;
    for (int64_t i = 0; i < n; i++) {
        ready += looks_ready(&cases[i]);
    }
    int64_t start = 0;
    if (ready > 0) {
        int64_t chosen = (int64_t)pick((uint64_t)ready);
        for (int64_t i = 0; i < n; i++) {
            if (looks_ready(&cases[i]) && chosen-- == 0) {
                start = i;
                break;
            }
        }
    }
    for (int64_t k = 0; ready > 0 && k < n; k++) {
        int64_t i = (start + k) % n;
        LughaChan *c = cases[i].chan;
        int done = cases[i].kind == 0 ? try_recv_locked(c, cases[i].data)
                                      : try_send_locked(c, cases[i].data);
        if (done == BLOCKED) {
            continue;
        }
        unlock_all(locks, nlocks);
        if (cases[i].kind == 1 && done == CLOSED) {
            chan_panic("send on a closed channel", file, line, col);
        }
        if (done == UNREAD) {
            drop_unsent(c, cases[i].data);
            done = MOVED;
        }
        cases[i].ok = done == MOVED;
        return i;
    }
    if (has_default) {
        unlock_all(locks, nlocks);
        return DEFAULT_ARM;
    }
    // A ring read here wakes this select; a value that came meanwhile wins.
    int64_t late = ring_select_ready(cases, n);
    if (late >= 0 && try_recv_locked(cases[late].chan, cases[late].data) == MOVED) {
        unlock_all(locks, nlocks);
        cases[late].ok = 1;
        return late;
    }
    Waiter waiters[64];
    uint32_t gen = next_wait_gen(current);
    for (int64_t i = 0; i < n; i++) {
        cases[i].ok = cases[i].kind == 0 ? 0 : 1;
        waiters[i] = (Waiter){.fiber = current, .gen = gen, .data = cases[i].data,
                              .ok = &cases[i].ok, .index = i};
        enqueue(cases[i].kind == 0 ? &cases[i].chan->recvq : &cases[i].chan->sendq, &waiters[i]);
    }
    if (timeout_ms >= 0) {
        int64_t ns = timeout_ms > INT64_MAX / 4000000 ? INT64_MAX / 4 : timeout_ms * 1000000;
        add_sleeper(current, now_ns() + ns);
    }
    current->cancellable = true;
    int64_t fired = waiting(n > 0 ? cases[0].chan : NULL, WAIT_SELECT, locks, nlocks);
    for (int i = 0; i < nlocks; i++) {
        lock_take(locks[i]);
    }
    for (int64_t i = 0; i < n; i++) {
        unqueue(&cases[i].chan->recvq, current);
        unqueue(&cases[i].chan->sendq, current);
    }
    unlock_all(locks, nlocks);
    woke(fired);
    if (fired >= 0 && cases[fired].kind == 1 && !cases[fired].ok) {
        chan_panic("send on a closed channel", file, line, col);
    }
    if (fired >= 0 && cases[fired].ok == UNREAD_OK) {
        drop_unsent(cases[fired].chan, cases[fired].data);
        cases[fired].ok = 1;
    }
    return fired;
}
