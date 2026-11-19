/*
 * rings.c — part of lugha_rt.c: `Ring<T>`, a channel whose writing end
 * never locks, allocates or waits, on any thread, C's own included.
 *
 * A ring is a channel (chans.c) with fixed slots and a `Ring` record. Its
 * one writer at a time stores into the slot at `tail` and publishes it;
 * its readers, under the channel's lock as for any channel, take the slot
 * at `head`. A `try_send` holds `writing` for its duration (one
 * compare-and-swap): a second one starting meanwhile panics. Successive
 * writers on different threads are fine.
 *
 * A reader about to wait sets `wants` and looks again; a writer, having
 * published, reads `wants` (both sequentially consistent, so one of them
 * sees the other). Then the writer pushes the ring onto a lock-free list
 * and writes the poller's eventfd: the poller, back from `epoll_wait`,
 * hands values to the waiting readers under the ring's lock
 * (`rings_wake`). While a reader waits on a ring, the poll counts as
 * outstanding.
 *
 * A fragment, not compiled on its own: joined right after chans.c, whose
 * channels, waiters and claims it uses.
 */

typedef struct Ring {
    /* The next slot readers take (under the channel's lock) and the next
     * the writer fills; the writer reads `head` to see room. */
    int64_t head;
    int64_t tail;
    /* 1 while a `try_send` runs; 1 while readers wait; 1 while on the
     * list of rings to wake. */
    int32_t writing;
    int32_t wants;
    int32_t queued;
    LughaChan *next_wake;
} Ring;

static void wake_open(void);

/* True if a ring holds a value, read without its lock. */
static bool ring_has(const LughaChan *c) {
    const Ring *r = c->ring;
    return __atomic_load_n(&r->head, __ATOMIC_ACQUIRE) < __atomic_load_n(&r->tail, __ATOMIC_ACQUIRE);
}

/* Rings a writer found readers waiting on, for the poller. */
static LughaChan *ring_wakes;

/* `Ring<T>.new(n)`: a channel of `n` fixed slots, written lock-free. */
LughaChan *lugha_rt_ring_new(int64_t cap, int64_t elem, const char *file, int64_t line,
                             int64_t col) {
    if (cap < 1) {
        chan_panic("a ring holds at least one value", file, line, col);
    }
    LughaChan *c = lugha_rt_chan_new(cap, elem, file, line, col);
    c->ring = lugha_rc_internal(sizeof(Ring));
    // Its writer wakes readers through the poller, from any thread.
    wake_open();
    return c;
}

/* What a dropped ring still holds, and its record. */
static void ring_visit(LughaChan *c, int64_t op) {
    Ring *r = c->ring;
    if (c->elem_visit != NULL) {
        for (int64_t i = r->head; i < r->tail; i++) {
            c->elem_visit(c->buf + (i % c->cap) * c->elem, op);
        }
    }
    if (op == RC_RELEASE) {
        free(r);
        free(c->buf);
    }
}

/* Takes the oldest value into `out`, under the channel's lock; readers
 * stop waiting once none is queued. */
static int ring_recv_locked(LughaChan *c, void *out) {
    Ring *r = c->ring;
    int64_t tail = __atomic_load_n(&r->tail, __ATOMIC_ACQUIRE);
    if (r->head < tail) {
        memcpy(out, c->buf + (r->head % c->cap) * c->elem, (size_t)c->elem);
        __atomic_store_n(&r->head, r->head + 1, __ATOMIC_RELEASE);
        if (c->recvq == NULL && __atomic_exchange_n(&r->wants, 0, __ATOMIC_SEQ_CST)) {
            __atomic_sub_fetch(&rings_waiting, 1, __ATOMIC_ACQ_REL);
        }
        return MOVED;
    }
    if (__atomic_load_n(&c->closed, __ATOMIC_ACQUIRE)) {
        memset(out, 0, (size_t)c->elem);
        return CLOSED;
    }
    return BLOCKED;
}

/* A reader is about to wait on `c`, under its lock: the writer will wake
 * it. True if a value arrived meanwhile: look again before waiting. */
static bool ring_want(LughaChan *c) {
    Ring *r = c->ring;
    if (__atomic_exchange_n(&r->wants, 1, __ATOMIC_SEQ_CST) == 0) {
        __atomic_add_fetch(&rings_waiting, 1, __ATOMIC_ACQ_REL);
    }
    return r->head < __atomic_load_n(&r->tail, __ATOMIC_SEQ_CST);
}

/* `select` about to wait, under its channels' locks: each ring it reads
 * wakes it; the first whose value arrived meanwhile, or -1. */
static int64_t ring_select_ready(const LughaCase *cases, int64_t n) {
    int64_t ready = -1;
    for (int64_t i = 0; i < n; i++) {
        LughaChan *c = cases[i].chan;
        if (cases[i].kind == 0 && c->ring != NULL && ring_want(c) && ready < 0) {
            ready = i;
        }
    }
    return ready;
}

/* Hands buffered values to waiting readers, under the channel's lock. */
static void ring_deliver_locked(LughaChan *c) {
    Ring *r = c->ring;
    int64_t state;
    while (r->head < __atomic_load_n(&r->tail, __ATOMIC_ACQUIRE)) {
        Waiter *w = claim_first(&c->recvq, &state);
        if (w == NULL) {
            break;
        }
        memcpy(w->data, c->buf + (r->head % c->cap) * c->elem, (size_t)c->elem);
        __atomic_store_n(&r->head, r->head + 1, __ATOMIC_RELEASE);
        *w->ok = 1;
        claimed_ready(w->fiber, state);
    }
    if (c->recvq == NULL && __atomic_exchange_n(&r->wants, 0, __ATOMIC_SEQ_CST)) {
        __atomic_sub_fetch(&rings_waiting, 1, __ATOMIC_ACQ_REL);
    }
}

/* The poller: every ring a writer found readers waiting on. */
static void rings_wake(void) {
    LughaChan *c = __atomic_exchange_n(&ring_wakes, NULL, __ATOMIC_ACQ_REL);
    while (c != NULL) {
        LughaChan *next = c->ring->next_wake;
        // Off the list first: a write from now on queues it again.
        __atomic_store_n(&c->ring->queued, 0, __ATOMIC_SEQ_CST);
        lock_take(&c->lock);
        ring_deliver_locked(c);
        lock_give(&c->lock);
        lugha_rt_rc_release(c);
        c = next;
    }
}

/* Readers wait on `c`: onto the list for the poller, which is kicked. One
 * system call, a non-blocking write, and no lock. */
static void ring_kick(LughaChan *c) {
    Ring *r = c->ring;
    int32_t idle = 0;
    if (!__atomic_compare_exchange_n(&r->queued, &idle, 1, false, __ATOMIC_ACQ_REL,
                                     __ATOMIC_RELAXED)) {
        return;
    }
    // Held while listed: its readers may let go of it meanwhile.
    lugha_rt_rc_retain(c);
    LughaChan *head = __atomic_load_n(&ring_wakes, __ATOMIC_RELAXED);
    do {
        r->next_wake = head;
    } while (!__atomic_compare_exchange_n(&ring_wakes, &head, c, true, __ATOMIC_RELEASE,
                                          __ATOMIC_RELAXED));
    poller_kick();
}

/* `ring.try_send(v)`: 0 sent, 1 full, 2 closed; unless sent, the value
 * stays the caller's. Never locks, allocates or waits. */
int32_t lugha_rt_ring_try_send(LughaChan *c, const void *value) {
    Ring *r = c->ring;
    int32_t idle = 0;
    if (!__atomic_compare_exchange_n(&r->writing, &idle, 1, false, __ATOMIC_ACQUIRE,
                                     __ATOMIC_RELAXED)) {
        chan_panic("a ring has one writer at a time", c->file, c->line, c->col);
    }
    int32_t how = 0;
    int64_t tail = r->tail;
    if (__atomic_load_n(&c->closed, __ATOMIC_ACQUIRE)) {
        how = 2;
    } else if (tail - __atomic_load_n(&r->head, __ATOMIC_ACQUIRE) >= c->cap) {
        how = 1;
    } else {
        memcpy(c->buf + (tail % c->cap) * c->elem, value, (size_t)c->elem);
        __atomic_store_n(&r->tail, tail + 1, __ATOMIC_SEQ_CST);
    }
    __atomic_store_n(&r->writing, 0, __ATOMIC_RELEASE);
    if (how == 0 && __atomic_load_n(&r->wants, __ATOMIC_SEQ_CST)) {
        ring_kick(c);
    }
    return how;
}
