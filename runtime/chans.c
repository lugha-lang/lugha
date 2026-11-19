/*
 * chans.c — part of lugha_rt.c: channels and `select`.
 *
 * `Chan<T>.new(n)` holds up to `n` values in a ring; with 0 a send waits
 * for a receiver. `Chan<T>.unbounded()` grows its ring as it fills. Both
 * ends of a channel, `Sender` and `Receiver`, point at the one channel. Waiting senders and receivers queue as
 * `Waiter`s on their own stacks; a `select` queues one per arm, and the
 * first arm to fire wins (`dequeue` drops the rest).
 *
 * Each channel has its own lock over its ring, waiters and `closed`. A
 * fiber waiting on channels parks still holding their locks, let go of
 * once it is saved; a `select` takes its channels' locks in address order.
 * Whoever moves a value to or from a waiter claims it first (`claim`): a
 * waiter another case, a timer or a cancellation took already is dropped.
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it right after
 * tasks.c, whose types and scheduler (`block_holding`, `claim`,
 * `add_sleeper`) it uses.
 */

typedef struct Waiter Waiter;

/* One fiber waiting on one channel: in its `recvq` or `sendq`. Lives on the
 * waiting fiber's stack, which stays put while it waits. */
struct Waiter {
    Fiber *fiber;
    /* The wait it is in (a generation). */
    uint32_t gen;
    /* The value to send, or where a received one goes. */
    void *data;
    /* Receivers: 1 for a value, 0 for closed. Senders: 0 when closed. */
    int32_t *ok;
    /* The `select` case, or 0. */
    int64_t index;
    Waiter *next;
};

/* A channel (a counted object), and where it was made. */
typedef struct {
    int64_t cap;
    bool unbounded;
    const char *file;
    int64_t line;
    int64_t col;
    int64_t elem;
    int64_t len;
    int64_t head;
    unsigned char *buf;
    int32_t closed;
    Waiter *recvq;
    Waiter *sendq;
    /* What a buffered value holds, for releasing and marking it. */
    RcVisit elem_visit;
    /* Its lock (rt_lock.c's kind). */
    uint32_t lock;
} LughaChan;

/* One `select` arm handed over by generated code. */
typedef struct {
    LughaChan *chan;
    /* 0 receive, 1 send. */
    int64_t kind;
    void *data;
    int32_t ok;
} LughaCase;


static void chan_panic(const char *what, const char *file, int64_t line, int64_t col) {
    panic_format("%s at %s:%" PRId64 ":%" PRId64, what, file, line, col);
}

/* The visit of a `Chan`: the values still in its ring; the ring goes
 * with it. Waiting senders keep their values on their own stacks. */
static void rc_chan_visit(void *object, int64_t op) {
    // Nobody else holds it any more, nor waits on it: no lock.
    LughaChan *c = object;
    if (c->elem_visit != NULL) {
        for (int64_t i = 0; i < c->len; i++) {
            c->elem_visit(c->buf + ((c->head + i) % c->cap) * c->elem, op);
        }
    }
    if (op == RC_RELEASE) {
        free(c->buf);
    }
}

/* `Chan<T>.new(n)`. */
LughaChan *lugha_rt_chan_new(int64_t cap, int64_t elem, const char *file, int64_t line,
                             int64_t col) {
    if (cap < 0) {
        chan_panic("a channel's capacity can't be negative", file, line, col);
    }
    LughaChan *c = lugha_rt_alloc((int64_t)sizeof(LughaChan));
    c->cap = cap;
    c->elem = elem < 1 ? 1 : elem;
    c->file = file;
    c->line = line;
    c->col = col;
    if (cap > 0) {
        if (cap > INT64_MAX / c->elem) {
            die("panic: out of memory\n");
        }
        /* The ring is the channel's own, freed with it. */
        c->buf = lugha_rc_internal((size_t)(cap * c->elem));
    }
    lugha_rt_rc_set_drop(c, rc_chan_visit);
    return c;
}

/* `Chan<T>.unbounded()`: a ring that grows when full. */
LughaChan *lugha_rt_chan_unbounded(int64_t elem, const char *file, int64_t line, int64_t col) {
    LughaChan *c = lugha_rt_chan_new(0, elem, file, line, col);
    c->unbounded = true;
    return c;
}

/* An unbounded channel's full ring doubles, its values first in order. */
static void grow(LughaChan *c) {
    int64_t cap = c->cap == 0 ? 8 : c->cap;
    if (cap > INT64_MAX / 2 / c->elem) {
        die("panic: out of memory\n");
    }
    cap = c->cap == 0 ? cap : cap * 2;
    unsigned char *buf = lugha_rc_internal((size_t)(cap * c->elem));
    for (int64_t i = 0; i < c->len; i++) {
        memcpy(buf + i * c->elem, c->buf + ((c->head + i) % c->cap) * c->elem, (size_t)c->elem);
    }
    free(c->buf);
    c->buf = buf;
    c->cap = cap;
    c->head = 0;
}

/* Generated code gives a new channel its element's visit (none when an element holds nothing counted). */
void lugha_rt_rc_chan_setup(LughaChan *c, RcVisit elem_visit) {
    c->elem_visit = elem_visit;
}

static void enqueue(Waiter **queue, Waiter *w) {
    w->next = NULL;
    while (*queue != NULL) {
        queue = &(*queue)->next;
    }
    *queue = w;
}

/* The first waiter on `queue` this caller claims for its `index`, taken
 * off; its state in `*state`. Waiters claimed already (another case of
 * their `select`, a timer, a cancellation) are dropped on the way. */
static Waiter *claim_first(Waiter **queue, int64_t *state) {
    while (*queue != NULL) {
        Waiter *w = *queue;
        *queue = w->next;
        int64_t was = claim(w->fiber, w->gen, w->index);
        if (was != CLAIM_LOST) {
            *state = was;
            return w;
        }
    }
    return NULL;
}

static void unqueue(Waiter **queue, Fiber *f) {
    while (*queue != NULL) {
        if ((*queue)->fiber == f) {
            *queue = (*queue)->next;
        } else {
            queue = &(*queue)->next;
        }
    }
}

/* What an attempt that doesn't wait did. */
enum { MOVED, BLOCKED, CLOSED };

/* Sends `value` without waiting, under `c`'s lock: to a claimed receiver,
 * else into the ring if it has room. */
static int try_send_locked(LughaChan *c, const void *value) {
    if (c->closed) {
        return CLOSED;
    }
    int64_t state;
    Waiter *r = claim_first(&c->recvq, &state);
    if (r != NULL) {
        memcpy(r->data, value, (size_t)c->elem);
        *r->ok = 1;
        claimed_ready(r->fiber, state);
        return MOVED;
    }
    if (c->unbounded || c->len < c->cap) {
        if (c->len == c->cap) {
            grow(c);
        }
        memcpy(c->buf + ((c->head + c->len) % c->cap) * c->elem, value, (size_t)c->elem);
        c->len++;
        return MOVED;
    }
    return BLOCKED;
}

/* Receives into `out` without waiting, under `c`'s lock: from the ring
 * (a claimed sender's value taking the freed place), else from a claimed
 * sender; CLOSED once closed and empty, `out` zeroed. */
static int try_recv_locked(LughaChan *c, void *out) {
    int64_t state;
    if (c->len > 0) {
        memcpy(out, c->buf + c->head * c->elem, (size_t)c->elem);
        c->head = (c->head + 1) % c->cap;
        c->len--;
        Waiter *s = claim_first(&c->sendq, &state);
        if (s != NULL) {
            memcpy(c->buf + ((c->head + c->len) % c->cap) * c->elem, s->data, (size_t)c->elem);
            c->len++;
            *s->ok = 1;
            claimed_ready(s->fiber, state);
        }
        return MOVED;
    }
    Waiter *s = claim_first(&c->sendq, &state);
    if (s != NULL) {
        memcpy(out, s->data, (size_t)c->elem);
        *s->ok = 1;
        claimed_ready(s->fiber, state);
        return MOVED;
    }
    if (c->closed) {
        memset(out, 0, (size_t)c->elem);
        return CLOSED;
    }
    return BLOCKED;
}

/* Parks on the `n` channel locks `locks`, which the caller holds and the
 * switch lets go of, noting what on for a deadlock report. */
static int64_t waiting(LughaChan *c, int32_t kind, uint32_t **locks, int n) {
    current->waits_on = c;
    current->wait_kind = kind;
    int64_t fired = block_holding(locks, n);
    current->waits_on = NULL;
    current->wait_kind = WAIT_NONE;
    return fired;
}

/* `c.send(v)`: `value` points at the copy in element layout. */
void lugha_rt_chan_send(LughaChan *c, const void *value, const char *file, int64_t line,
                        int64_t col) {
    ensure_started();
    lugha_rt_cancel_point();
    lock_take(&c->lock);
    int done = try_send_locked(c, value);
    if (done != BLOCKED) {
        lock_give(&c->lock);
        if (done == CLOSED) {
            chan_panic("send on a closed channel", file, line, col);
        }
        return;
    }
    int32_t ok = 1;
    Waiter w = {.fiber = current, .gen = next_wait_gen(current), .data = (void *)value, .ok = &ok,
                .index = 0};
    enqueue(&c->sendq, &w);
    current->cancellable = true;
    uint32_t *locks[1] = {&c->lock};
    int64_t fired = waiting(c, WAIT_SEND, locks, 1);
    if (fired == CANCELLED) {
        lock_take(&c->lock);
        unqueue(&c->sendq, current);
        lock_give(&c->lock);
    }
    woke(fired);
    if (!ok) {
        chan_panic("send on a closed channel", file, line, col);
    }
}

/* `c.recv()`: 1 and the value in `out`, or 0 once closed and empty. */
int32_t lugha_rt_chan_recv(LughaChan *c, void *out) {
    ensure_started();
    lugha_rt_cancel_point();
    lock_take(&c->lock);
    int done = try_recv_locked(c, out);
    if (done != BLOCKED) {
        lock_give(&c->lock);
        return done == MOVED;
    }
    int32_t ok = 0;
    Waiter w = {.fiber = current, .gen = next_wait_gen(current), .data = out, .ok = &ok,
                .index = 0};
    enqueue(&c->recvq, &w);
    current->cancellable = true;
    uint32_t *locks[1] = {&c->lock};
    int64_t fired = waiting(c, WAIT_RECV, locks, 1);
    if (fired == CANCELLED) {
        lock_take(&c->lock);
        unqueue(&c->recvq, current);
        lock_give(&c->lock);
    }
    woke(fired);
    return ok;
}

/* `rx.try_recv()`: never waits. 1 with a value in `out`, 0 when empty,
 * 2 once closed and empty. */
int32_t lugha_rt_chan_try_recv(LughaChan *c, void *out) {
    ensure_started();
    lock_take(&c->lock);
    int done = try_recv_locked(c, out);
    lock_give(&c->lock);
    if (done == BLOCKED) {
        memset(out, 0, (size_t)c->elem);
        return 0;
    }
    return done == MOVED ? 1 : 2;
}

/* `tx.try_send(v)`: never waits. 0 sent, 1 full, 2 closed; unless sent,
 * the value stays the caller's. */
int32_t lugha_rt_chan_try_send(LughaChan *c, const void *value) {
    ensure_started();
    lock_take(&c->lock);
    int done = try_send_locked(c, value);
    lock_give(&c->lock);
    return done == MOVED ? 0 : done == BLOCKED ? 1 : 2;
}

/* `c.close()`: receivers get `None`, waiting senders panic. */
void lugha_rt_chan_close(LughaChan *c, const char *file, int64_t line, int64_t col) {
    ensure_started();
    lock_take(&c->lock);
    if (c->closed) {
        lock_give(&c->lock);
        chan_panic("close of a closed channel", file, line, col);
    }
    c->closed = 1;
    int64_t state;
    for (Waiter *r = claim_first(&c->recvq, &state); r != NULL;
         r = claim_first(&c->recvq, &state)) {
        memset(r->data, 0, (size_t)c->elem);
        *r->ok = 0;
        claimed_ready(r->fiber, state);
    }
    for (Waiter *s = claim_first(&c->sendq, &state); s != NULL;
         s = claim_first(&c->sendq, &state)) {
        *s->ok = 0;
        claimed_ready(s->fiber, state);
    }
    lock_give(&c->lock);
}

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
    if (c->closed) {
        return true;
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
        cases[i].ok = done == MOVED;
        return i;
    }
    if (has_default) {
        unlock_all(locks, nlocks);
        return DEFAULT_ARM;
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
    return fired;
}
