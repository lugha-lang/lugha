/*
 * actors.c — part of lugha_rt.c: actors.
 *
 * An actor is a task of the scope that started it, running
 * `lugha_rt_actor_run` as its closure (generated code starts it as any
 * task, so the scope's policy meets its failure as any task's): it takes
 * messages from its mailbox one at a time and runs each message's handler
 * (generated per method) on its state. A call queues a message and waits
 * for its outcome; a cast queues one and goes on; both wait while the
 * mailbox holds its limit (`mailbox n`). `stop` is a message with no
 * handler, queued past the limit. The scope stops its actors once its
 * other tasks are done (`scope_stop_actors`), and cancels them with them.
 * However an actor ends, its waiting callers and queued messages learn it:
 * a call then panics in the caller.
 *
 * Each actor has its own lock over its mailbox, status, waiting senders
 * and the message it handles. A fiber waiting on it (for mail, for room,
 * for an answer) parks still holding the lock, let go of once it is
 * saved. A handler runs without it; so does every panic and cancellation
 * point, which take the scopes lock, earlier in the order.
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it after
 * tasks.c, spawn.c and scopes.c, whose fibers, scheduler and scopes it uses.
 */

enum { RUNNING, STOPPED, PANICKED };

typedef void (*Handler)(void *state, void *args, void *result);

/* One queued call. */
typedef struct Message {
    /* NULL for `stop`. */
    Handler handler;
    void *args;
    void *result;
    /* The caller waiting for the outcome, NULL for a cast. */
    Fiber *waiter;
    int32_t *outcome;
    struct Message *next;
} Message;

/* An actor handle's object (a counted object). */
typedef struct {
    void *state;
    const char *name;
    int32_t status;
    Message *head;
    Message *tail;
    /* Messages queued with a handler, and how many may be. */
    int64_t queued;
    int64_t capacity;
    /* Senders waiting for room, linked through `wait_next`. */
    Fiber *room_waiters;
    /* The actor's fiber while it waits for mail. */
    Fiber *idle;
    /* The message being handled, so an end there still answers its
     * caller (a handler may wait, letting other actors run meanwhile). */
    Message *handling;
    /* Its lock (rt_lock.c's kind). */
    uint32_t lock;
} LughaActor;

static void actor_panic(const LughaActor *a, const char *what, const char *file, int64_t line,
                        int64_t col) {
    panic_format("actor `%s` %s at %s:%" PRId64 ":%" PRId64, a->name, what, file, line, col);
}

/* A message's record: the runtime's own memory, freed once it is
 * settled (`message_done`). */
static Message *new_message(void) {
    return lugha_rc_internal(sizeof(Message));
}

/* A message nobody will handle again lets go of its arguments; a cast's record (no caller frees it) goes too. */
static void message_done(Message *m) {
    lugha_rt_rc_release(m->args);
    m->args = NULL;
    if (m->waiter == NULL) {
        free(m);
    }
}

/* Answers `m`'s caller, if one waits, with `outcome`. */
static void answer(Message *m, int32_t outcome) {
    if (m->waiter != NULL) {
        *m->outcome = outcome;
        wake(m->waiter, 0);
    }
}

/* Wakes every sender waiting for room: each looks again. */
static void room(LughaActor *a) {
    for (Fiber *f = a->room_waiters; f != NULL;) {
        Fiber *after = f->wait_next;
        wake(f, 0);
        f = after;
    }
    a->room_waiters = NULL;
}

/* The actor ended (`outcome`), under its lock: the message it was
 * handling and every one still queued are answered, and senders waiting
 * for room look again. Its state, taken out, is the caller's to let go of
 * once the lock is given up (NULL if it had ended already). */
static void *actor_ended_locked(LughaActor *a, int32_t outcome) {
    if (a->status != RUNNING) {
        return NULL;
    }
    a->status = outcome;
    a->idle = NULL;
    // Nothing runs on the state again: it goes now, on the actor's own
    // task, so handles holding each other in a ring don't keep it.
    void *state = a->state;
    a->state = NULL;
    Message *m = a->handling;
    a->handling = NULL;
    if (m != NULL) {
        answer(m, outcome);
        message_done(m);
    }
    for (Message *q = a->head; q != NULL;) {
        Message *after = q->next;
        answer(q, outcome);
        message_done(q);
        q = after;
    }
    a->head = a->tail = NULL;
    a->queued = 0;
    room(a);
    return state;
}

static void actor_ended(LughaActor *a, int32_t outcome) {
    lock_take(&a->lock);
    void *state = actor_ended_locked(a, outcome);
    lock_give(&a->lock);
    // What it holds may hold handles of actors, whose locks it may take.
    lugha_rt_rc_release(state);
}

/* The running task's actor, if it is one, ended with its task: cancelled
 * (it stopped) or by a panic (task_caught, scope_policy.c). */
static void actor_task_ended(bool panicked) {
    if (current != NULL && current->actor != NULL) {
        actor_ended(current->actor, panicked ? PANICKED : STOPPED);
    }
}

/* The actor's loop, its task's closure (`env` the actor): message after
 * message until `stop`; a cancellation while it waits for mail ends it. */
void lugha_rt_actor_run(void *env) {
    LughaActor *a = env;
    // Its mailbox under its lock; each handler, Lugha code, without it.
    lock_take(&a->lock);
    for (;;) {
        while (a->head == NULL) {
            a->idle = current;
            current->cancellable = true;
            uint32_t *locks[1] = {&a->lock};
            int64_t fired = block_holding(locks, 1);
            lock_take(&a->lock);
            a->idle = NULL;
            if (fired == CANCELLED) {
                lock_give(&a->lock);
                woke(fired);
            }
            current->cancellable = false;
        }
        Message *m = a->head;
        a->head = m->next;
        if (a->head == NULL) {
            a->tail = NULL;
        }
        if (m->handler == NULL) {
            message_done(m);
            break;
        }
        a->queued--;
        room(a);
        a->handling = m;
        lock_give(&a->lock);
        m->handler(a->state, m->args, m->result);
        lock_take(&a->lock);
        a->handling = NULL;
        answer(m, RUNNING);
        message_done(m);
    }
    void *state = actor_ended_locked(a, STOPPED);
    lock_give(&a->lock);
    lugha_rt_rc_release(state);
}

/* A panic nothing caught ends the program: an actor's is caught by its
 * task's entry, as any task's. */
static void panic_end(const char *text, size_t len) {
    (void)text;
    (void)len;
    program_exit(101);
}

/* The visit of an actor's handle: its state, while it has one (an actor
 * lets go of it when it ends; one still running at the report's exit
 * keeps it). */
static void rc_actor_visit(void *object, int64_t op) {
    LughaActor *a = object;
    if (a->state == NULL) {
        return;
    }
    if (op == RC_MARK) {
        lugha_rt_rc_mark(a->state);
    } else {
        lugha_rt_rc_release(a->state);
    }
}

/* A call's result block, once the caller has its value. */
void lugha_rt_rc_block_done(void *block) {
    free(block);
}

/* `s.spawn(Counter { … })`: an actor holding `state` (a counted block),
 * whose mailbox holds `capacity` messages; generated code then starts its
 * loop as a task of the scope. */
LughaActor *lugha_rt_actor_new(void *state, const char *name, int64_t capacity) {
    LughaActor *a = lugha_rt_alloc((int64_t)sizeof(LughaActor));
    a->state = state;
    a->name = name;
    a->capacity = capacity < 1 ? 1 : capacity;
    lugha_rt_rc_set_drop(a, rc_actor_visit);
    /* Its handle and its task reach it from two tasks. */
    lugha_rt_rc_mark(a);
    return a;
}

/* The actor's task has started in its scope: the scope stops it once its
 * other tasks are done. */
void lugha_rt_actor_started(LughaTask *task, LughaActor *a) {
    Fiber *f = task->fiber;
    LughaScope *s = f->started_in;
    lock_take(&s->lock);
    f->actor = a;
    s->actors++;
    lock_give(&s->lock);
}

static void post(LughaActor *a, Message *m) {
    m->next = NULL;
    if (a->tail == NULL) {
        a->head = m;
    } else {
        a->tail->next = m;
    }
    a->tail = m;
    if (a->idle != NULL) {
        Fiber *f = a->idle;
        a->idle = NULL;
        wake(f, 0);
    }
}

/* Under `a`'s lock: panics (letting go of it first) unless `a` runs. */
static void check_alive(LughaActor *a, const char *file, int64_t line, int64_t col) {
    int32_t status = a->status;
    if (status != RUNNING) {
        lock_give(&a->lock);
        actor_panic(a, status == STOPPED ? "has stopped" : "panicked", file, line, col);
    }
}

/* Under `a`'s lock: waits while the mailbox is full (a pause point), then
 * queues `m`. Cancelled meanwhile, `m` goes and the caller unwinds, the
 * lock let go of. */
static void post_counted(LughaActor *a, Message *m, const char *file, int64_t line, int64_t col) {
    check_alive(a, file, line, col);
    while (a->queued >= a->capacity) {
        current->wait_next = a->room_waiters;
        a->room_waiters = current;
        current->cancellable = true;
        uint32_t *locks[1] = {&a->lock};
        int64_t fired = block_holding(locks, 1);
        lock_take(&a->lock);
        current->cancellable = false;
        if (fired == CANCELLED) {
            for (Fiber **at = &a->room_waiters; *at != NULL; at = &(*at)->wait_next) {
                if (*at == current) {
                    *at = current->wait_next;
                    break;
                }
            }
            lock_give(&a->lock);
            // Read first: a cast's record goes with `message_done`.
            bool called = m->waiter != NULL;
            message_done(m);
            if (called) {
                free(m);
            }
            woke(fired);
        }
        check_alive(a, file, line, col);
    }
    a->queued++;
    post(a, m);
}

/* A call: queues `handler(state, args, result)` and waits for it; the
 * result's address (`size` bytes, freed by the caller once it has the
 * value: `lugha_rt_rc_block_done`). */
void *lugha_rt_actor_call(LughaActor *a, Handler handler, void *args, int64_t size,
                          const char *file, int64_t line, int64_t col) {
    ensure_started();
    lugha_rt_cancel_point();
    int32_t outcome = RUNNING;
    Message *m = new_message();
    m->handler = handler;
    m->args = args;
    m->waiter = current;
    m->outcome = &outcome;
    /* The caller's to free once it has the value (`lugha_rt_rc_block_done`). */
    m->result = lugha_rc_internal((size_t)(size < 1 ? 1 : size));
    lock_take(&a->lock);
    post_counted(a, m, file, line, col);
    // The answer comes under the lock, so only once this fiber is parked.
    uint32_t *locks[1] = {&a->lock};
    block_holding(locks, 1);
    if (outcome == STOPPED) {
        actor_panic(a, "has stopped", file, line, col);
    }
    if (outcome == PANICKED) {
        actor_panic(a, "panicked", file, line, col);
    }
    void *result = m->result;
    free(m);
    return result;
}

/* A cast: queues `handler(state, args, …)` and goes on. */
void lugha_rt_actor_cast(LughaActor *a, Handler handler, void *args, const char *file,
                         int64_t line, int64_t col) {
    ensure_started();
    lugha_rt_cancel_point();
    Message *m = new_message();
    m->handler = handler;
    m->args = args;
    lock_take(&a->lock);
    post_counted(a, m, file, line, col);
    lock_give(&a->lock);
}

/* Stops `a` after what is queued, past its mailbox's limit. */
static void stop_after_mail(LughaActor *a) {
    post(a, new_message());
}

/* `a.stop()`: a message like any other, after what is queued. */
void lugha_rt_actor_stop(LughaActor *a, const char *file, int64_t line, int64_t col) {
    ensure_started();
    lock_take(&a->lock);
    check_alive(a, file, line, col);
    stop_after_mail(a);
    lock_give(&a->lock);
}

/* The scope's other tasks are done: each actor it still runs stops after
 * its queued mail. */
static void scope_stop_actors(LughaScope *s) {
    // Under the scope's lock, a task not yet done still has its fiber: it
    // counts itself out under this lock after it is done.
    lock_take(&s->lock);
    for (LughaTask *t = s->tasks; t != NULL; t = t->scope_next) {
        LughaActor *a = !task_done(t) && t->fiber != NULL ? t->fiber->actor : NULL;
        if (a != NULL) {
            lock_take(&a->lock);
            if (a->status == RUNNING) {
                stop_after_mail(a);
            }
            lock_give(&a->lock);
        }
    }
    lock_give(&s->lock);
}

/* After a deadlock: each task or actor waiting on a channel, and where the
 * channel was made. */
static void report_waits(void) {
    // Every worker is parked: nothing ends meanwhile, but the list is read
    // under its lock all the same.
    lock_take(&live_lock);
    for (Fiber *f = live; f != NULL; f = f->next_live) {
        LughaChan *c = f->waits_on;
        if (c == NULL) {
            continue;
        }
        const char *how = f->wait_kind == WAIT_SEND   ? "to send on"
                          : f->wait_kind == WAIT_RECV ? "to receive on"
                                                      : "in a select, on";
        LughaString *task = f->task != NULL ? f->task->name : NULL;
        LughaActor *actor = f->actor;
        if (actor != NULL) {
            fprintf(stderr, "  actor `%s`", actor->name);
        } else if (task != NULL) {
            fprintf(stderr, "  `%.*s`", (int)task->len, task->bytes);
        } else {
            fputs("  `main`", stderr);
        }
        fprintf(stderr, " waits %s the channel made at %s:%" PRId64 ":%" PRId64 "\n", how, c->file,
                c->line, c->col);
    }
    lock_give(&live_lock);
}
