/*
 * chans.c — part of lugha_rt.c: channels (`select` is select.c).
 *
 * `Chan<T>.new(n)` holds up to `n` values in a ring; with 0 a send waits
 * for a receiver. `Chan<T>.unbounded()` grows its ring as it fills. A
 * `Sender` is the channel itself; every `Receiver` of it is its one
 * receiving end (`LughaRecv`), which holds the channel. When the receiving
 * end goes, nothing can receive again: the channel is *unread*, lets go of
 * what it buffers, and from then on drops what is sent (`try_send` gives it
 * back as closed). Waiting senders and receivers queue as `Waiter`s on
 * their own stacks; a `select` queues one per arm, and the first arm to
 * fire wins (`dequeue` drops the rest).
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
    /* Receivers: 1 for a value, 0 for closed. Senders: 0 when closed,
     * UNREAD_OK when the value stays theirs to drop. */
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
    /* 1 once its receiving end is gone. */
    int32_t unread;
    Waiter *recvq;
    Waiter *sendq;
    /* What a buffered value holds, for releasing and marking it. */
    RcVisit elem_visit;
    /* Its lock (rt_lock.c's kind). */
    uint32_t lock;
    /* A ring's writing side (rings.c), else NULL. */
    struct Ring *ring;
} LughaChan;

/* A channel's receiving end (a counted object), shared by every copy of
 * its `Receiver`. */
typedef struct {
    LughaChan *chan;
} LughaRecv;

/* A waiting sender's `ok` when the channel became unread: not sent. */
#define UNREAD_OK 2

static void ring_visit(LughaChan *c, int64_t op);
static int ring_recv_locked(LughaChan *c, void *out);
static bool ring_want(LughaChan *c);
static void ring_deliver_locked(LughaChan *c);
static void ring_unread(LughaChan *c);

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
    if (c->ring != NULL) {
        ring_visit(c, op);
        return;
    }
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

/* What an attempt that doesn't wait did. UNREAD: nothing will receive,
 * so the value is the sender's to drop (`drop_unsent`). */
enum { MOVED, BLOCKED, CLOSED, UNREAD };

/* Lets go of a value sent to an unread channel, outside its lock. */
static void drop_unsent(LughaChan *c, const void *value) {
    if (c->elem_visit != NULL) {
        c->elem_visit((void *)value, RC_RELEASE);
    }
}

/* Sends `value` without waiting, under `c`'s lock: to a claimed receiver,
 * else into the ring if it has room. */
static int try_send_locked(LughaChan *c, const void *value) {
    if (c->closed) {
        return CLOSED;
    }
    if (c->unread) {
        return UNREAD;
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
    if (c->ring != NULL) {
        return ring_recv_locked(c, out);
    }
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
        if (done == UNREAD) {
            drop_unsent(c, value);
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
    if (ok == UNREAD_OK) {
        drop_unsent(c, value);
    }
}

/* `c.recv()`: 1 and the value in `out`, or 0 once closed and empty. */
int32_t lugha_rt_chan_recv(LughaChan *c, void *out) {
    ensure_started();
    lugha_rt_cancel_point();
    lock_take(&c->lock);
    int done = try_recv_locked(c, out);
    // About to wait on a ring: its writer will wake this reader.
    if (done == BLOCKED && c->ring != NULL && ring_want(c)) {
        done = try_recv_locked(c, out);
    }
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

/* `tx.try_send(v)`: never waits. 0 sent, 1 full, 2 closed or unread;
 * unless sent, the value stays the caller's. */
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
    // A ring's waiting readers get what is left first.
    if (c->ring != NULL) {
        ring_deliver_locked(c);
    }
    __atomic_store_n(&c->closed, 1, __ATOMIC_RELEASE);
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

/* The last receiver is gone: under the lock, the channel takes out what it
 * buffers and wakes its waiting senders, which keep their values to drop;
 * after the lock, what it took out is let go of. */
static void chan_unread(LughaChan *c) {
    lock_take(&c->lock);
    __atomic_store_n(&c->unread, 1, __ATOMIC_SEQ_CST);
    if (c->ring != NULL) {
        lock_give(&c->lock);
        ring_unread(c);
        return;
    }
    unsigned char *buf = c->buf;
    int64_t head = c->head;
    int64_t len = c->len;
    int64_t cap = c->cap;
    c->buf = NULL;
    c->len = 0;
    c->head = 0;
    int64_t state;
    for (Waiter *w = claim_first(&c->sendq, &state); w != NULL;
         w = claim_first(&c->sendq, &state)) {
        *w->ok = UNREAD_OK;
        claimed_ready(w->fiber, state);
    }
    lock_give(&c->lock);
    // What it held may hold this channel's sender, or other channels.
    if (c->elem_visit != NULL) {
        for (int64_t i = 0; i < len; i++) {
            c->elem_visit(buf + ((head + i) % cap) * c->elem, RC_RELEASE);
        }
    }
    free(buf);
}

/* The visit of a receiving end: its channel, which goes unread when the
 * end goes. */
static void rc_recv_visit(void *object, int64_t op) {
    LughaRecv *end = object;
    if (op == RC_MARK) {
        lugha_rt_rc_mark(end->chan);
        return;
    }
    chan_unread(end->chan);
    lugha_rt_rc_release(end->chan);
}

/* `Chan<T>.new` and the rest: the receiving end of `c`, holding it once
 * more. */
LughaRecv *lugha_rt_chan_end(LughaChan *c) {
    LughaRecv *end = lugha_rt_alloc((int64_t)sizeof(LughaRecv));
    end->chan = c;
    lugha_rt_rc_retain(c);
    lugha_rt_rc_set_drop(end, rc_recv_visit);
    return end;
}
