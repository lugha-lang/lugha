/*
 * chans.c — part of lugha_rt.c: channels and `select`.
 *
 * `Chan<T>.new(n)` holds up to `n` values in a ring; with 0 a send waits
 * for a receiver. `Chan<T>.unbounded()` grows its ring as it fills. Both
 * ends of a channel, `Sender` and `Receiver`, point at the one channel. Waiting senders and receivers queue as
 * `Waiter`s on their own stacks; a `select` queues one per arm, and the
 * first arm to fire wins (`dequeue` drops the rest).
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it right after
 * tasks.c, whose types and scheduler (`block`, `wake`, `add_sleeper`) it uses.
 */

typedef struct Waiter Waiter;

/* One fiber waiting on one channel: in its `recvq` or `sendq`. Lives on the
 * waiting fiber's stack, which stays put while it waits. */
struct Waiter {
    Fiber *fiber;
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
} LughaChan;

/* One `select` arm handed over by generated code. */
typedef struct {
    LughaChan *chan;
    /* 0 receive, 1 send. */
    int64_t kind;
    void *data;
    int32_t ok;
} LughaCase;

/* `select`'s pick: pseudo-random, the same on every run. */
static uint64_t pick_state = 0x9e3779b97f4a7c15u;

static void chan_panic(const char *what, const char *file, int64_t line, int64_t col) {
    panic_format("%s at %s:%" PRId64 ":%" PRId64, what, file, line, col);
}

/* The visit of a `Chan`: the values still in its ring; the ring goes
 * with it. Waiting senders keep their values on their own stacks. */
static void rc_chan_visit(void *object, int64_t op) {
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

/* The first waiter whose fiber still waits; waiters of a `select` that
 * another case already woke are dropped on the way. */
static Waiter *dequeue(Waiter **queue) {
    while (*queue != NULL) {
        Waiter *w = *queue;
        *queue = w->next;
        if (w->fiber->fired == WAITING) {
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

static bool can_send(LughaChan *c) {
    if (c->closed || c->unbounded || c->len < c->cap) {
        return true;
    }
    for (Waiter *w = c->recvq; w != NULL; w = w->next) {
        if (w->fiber->fired == WAITING) {
            return true;
        }
    }
    return false;
}

static bool can_recv(LughaChan *c) {
    if (c->closed || c->len > 0) {
        return true;
    }
    for (Waiter *w = c->sendq; w != NULL; w = w->next) {
        if (w->fiber->fired == WAITING) {
            return true;
        }
    }
    return false;
}

/* Blocks, noting what on for a deadlock report. */
static int64_t waiting(LughaChan *c, int32_t kind) {
    current->waits_on = c;
    current->wait_kind = kind;
    int64_t fired = block();
    current->waits_on = NULL;
    current->wait_kind = WAIT_NONE;
    return fired;
}

/* Sends without waiting; the caller made sure it can. */
static void send_now(LughaChan *c, const void *value, const char *file, int64_t line,
                     int64_t col) {
    if (c->closed) {
        chan_panic("send on a closed channel", file, line, col);
    }
    Waiter *r = dequeue(&c->recvq);
    if (r != NULL) {
        memcpy(r->data, value, (size_t)c->elem);
        *r->ok = 1;
        wake(r->fiber, r->index);
        return;
    }
    if (c->len == c->cap) {
        grow(c);
    }
    memcpy(c->buf + ((c->head + c->len) % c->cap) * c->elem, value, (size_t)c->elem);
    c->len++;
}

/* Receives without waiting; the caller made sure it can. 1 for a value,
 * 0 when closed and empty. */
static int32_t recv_now(LughaChan *c, void *out) {
    if (c->len > 0) {
        memcpy(out, c->buf + c->head * c->elem, (size_t)c->elem);
        c->head = (c->head + 1) % c->cap;
        c->len--;
        /* A waiting sender's value takes the freed place. */
        Waiter *s = dequeue(&c->sendq);
        if (s != NULL) {
            memcpy(c->buf + ((c->head + c->len) % c->cap) * c->elem, s->data, (size_t)c->elem);
            c->len++;
            *s->ok = 1;
            wake(s->fiber, s->index);
        }
        return 1;
    }
    Waiter *s = dequeue(&c->sendq);
    if (s != NULL) {
        memcpy(out, s->data, (size_t)c->elem);
        *s->ok = 1;
        wake(s->fiber, s->index);
        return 1;
    }
    memset(out, 0, (size_t)c->elem);
    return 0;
}

/* `c.send(v)`: `value` points at the copy in element layout. */
void lugha_rt_chan_send(LughaChan *c, const void *value, const char *file, int64_t line,
                        int64_t col) {
    ensure_started();
    lugha_rt_cancel_point();
    if (can_send(c)) {
        send_now(c, value, file, line, col);
        return;
    }
    int32_t ok = 1;
    Waiter w = {.fiber = current, .data = (void *)value, .ok = &ok, .index = 0};
    enqueue(&c->sendq, &w);
    current->cancellable = true;
    int64_t fired = waiting(c, WAIT_SEND);
    if (fired == CANCELLED) {
        unqueue(&c->sendq, current);
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
    if (can_recv(c)) {
        return recv_now(c, out);
    }
    int32_t ok = 0;
    Waiter w = {.fiber = current, .data = out, .ok = &ok, .index = 0};
    enqueue(&c->recvq, &w);
    current->cancellable = true;
    int64_t fired = waiting(c, WAIT_RECV);
    if (fired == CANCELLED) {
        unqueue(&c->recvq, current);
    }
    woke(fired);
    return ok;
}

/* `rx.try_recv()`: never waits. 1 with a value in `out`, 0 when empty,
 * 2 once closed and empty. */
int32_t lugha_rt_chan_try_recv(LughaChan *c, void *out) {
    ensure_started();
    if (can_recv(c)) {
        return recv_now(c, out) ? 1 : 2;
    }
    memset(out, 0, (size_t)c->elem);
    return 0;
}

/* `tx.try_send(v)`: never waits. 0 sent, 1 full, 2 closed; unless sent,
 * the value stays the caller's. */
int32_t lugha_rt_chan_try_send(LughaChan *c, const void *value) {
    ensure_started();
    if (c->closed) {
        return 2;
    }
    if (!can_send(c)) {
        return 1;
    }
    send_now(c, value, "", 0, 0);
    return 0;
}

/* `c.close()`: receivers get `None`, waiting senders panic. */
void lugha_rt_chan_close(LughaChan *c, const char *file, int64_t line, int64_t col) {
    ensure_started();
    if (c->closed) {
        chan_panic("close of a closed channel", file, line, col);
    }
    c->closed = 1;
    for (Waiter *r = dequeue(&c->recvq); r != NULL; r = dequeue(&c->recvq)) {
        memset(r->data, 0, (size_t)c->elem);
        *r->ok = 0;
        wake(r->fiber, r->index);
    }
    for (Waiter *s = dequeue(&c->sendq); s != NULL; s = dequeue(&c->sendq)) {
        *s->ok = 0;
        wake(s->fiber, s->index);
    }
}

static uint64_t pick(uint64_t n) {
    if (sched_mode == MODE_SHUFFLE) {
        return shuffle_next() % n;
    }
    pick_state ^= pick_state << 13;
    pick_state ^= pick_state >> 7;
    pick_state ^= pick_state << 17;
    return pick_state % n;
}

/* `select`: the case that ran, DEFAULT_ARM (-1) for `default`, or
 * TIMED_OUT (-2) for `after`. `timeout_ms` < 0 means no `after`. A ready
 * case is picked at random; otherwise the thread waits on all of them. */
int64_t lugha_rt_task_select(LughaCase *cases, int64_t n, int64_t timeout_ms, int32_t has_default,
                             const char *file, int64_t line, int64_t col) {
    ensure_started();
    lugha_rt_cancel_point();
    int64_t ready = 0;
    for (int64_t i = 0; i < n; i++) {
        LughaChan *c = cases[i].chan;
        if (cases[i].kind == 0 ? can_recv(c) : can_send(c)) {
            ready++;
        }
    }
    if (ready > 0) {
        int64_t chosen = (int64_t)pick((uint64_t)ready);
        for (int64_t i = 0; i < n; i++) {
            LughaChan *c = cases[i].chan;
            if (!(cases[i].kind == 0 ? can_recv(c) : can_send(c))) {
                continue;
            }
            if (chosen-- > 0) {
                continue;
            }
            if (cases[i].kind == 0) {
                cases[i].ok = recv_now(c, cases[i].data);
            } else {
                send_now(c, cases[i].data, file, line, col);
            }
            return i;
        }
    }
    if (has_default) {
        return DEFAULT_ARM;
    }
    if (n > 64) {
        die("panic: a select with more than 64 arms\n");
    }
    Waiter waiters[64];
    for (int64_t i = 0; i < n; i++) {
        cases[i].ok = cases[i].kind == 0 ? 0 : 1;
        waiters[i] = (Waiter){.fiber = current, .data = cases[i].data, .ok = &cases[i].ok,
                              .index = i};
        enqueue(cases[i].kind == 0 ? &cases[i].chan->recvq : &cases[i].chan->sendq, &waiters[i]);
    }
    if (timeout_ms >= 0) {
        int64_t ns = timeout_ms > INT64_MAX / 4000000 ? INT64_MAX / 4 : timeout_ms * 1000000;
        add_sleeper(current, now_ns() + ns);
    }
    current->cancellable = true;
    int64_t fired = waiting(n > 0 ? cases[0].chan : NULL, WAIT_SELECT);
    for (int64_t i = 0; i < n; i++) {
        unqueue(&cases[i].chan->recvq, current);
        unqueue(&cases[i].chan->sendq, current);
    }
    woke(fired);
    if (fired >= 0 && cases[fired].kind == 1 && !cases[fired].ok) {
        chan_panic("send on a closed channel", file, line, col);
    }
    return fired;
}
