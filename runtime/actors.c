/*
 * actors.c — part of lugha_rt.c: actors.
 *
 * An actor is a green thread running `actor_loop`: it takes messages from
 * its mailbox, an unbounded queue, one at a time, and runs each message's
 * handler (generated per method) on its state. A call queues a message and
 * waits for its outcome; a cast queues one and goes on; `stop` is a message
 * with no handler. A panic inside an actor (`panic_end`) ends only that
 * actor: its waiting callers panic in turn, `join` reports the message, and
 * its green thread is abandoned — Lugha has no destructors to run.
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it last,
 * after tasks.c and spawn.c, whose fibers and scheduler it uses.
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
    /* The panic's text once PANICKED. */
    LughaString *panic;
    Message *head;
    Message *tail;
    /* The actor's fiber while it waits for mail. */
    Fiber *idle;
    /* Fibers waiting in `join`, linked through `wait_next`. */
    Fiber *joiners;
    /* The message being handled, so a panic can still answer its caller
     * (a handler may wait, letting other actors run meanwhile). */
    Message *handling;
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

/* Settles every message still queued: their callers learn `outcome`. */
static void settle_mail(LughaActor *a, int32_t outcome) {
    for (Message *m = a->head; m != NULL;) {
        Message *after = m->next;
        if (m->waiter != NULL) {
            *m->outcome = outcome;
            wake(m->waiter, 0);
        }
        message_done(m);
        m = after;
    }
    a->head = a->tail = NULL;
}

static void wake_joiners(LughaActor *a) {
    for (Fiber *j = a->joiners; j != NULL;) {
        Fiber *after = j->wait_next;
        wake(j, 0);
        j = after;
    }
    a->joiners = NULL;
}

static void actor_loop(void *code, void *env, void *result) {
    (void)code;
    (void)result;
    LughaActor *a = env;
    for (;;) {
        while (a->head == NULL) {
            a->idle = current;
            block();
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
        a->handling = m;
        m->handler(a->state, m->args, m->result);
        a->handling = NULL;
        if (m->waiter != NULL) {
            *m->outcome = RUNNING;
            wake(m->waiter, 0);
        }
        message_done(m);
    }
    a->status = STOPPED;
    settle_mail(a, STOPPED);
    wake_joiners(a);
}

static void panic_end(const char *text, size_t len) {
    LughaActor *a = current == NULL ? NULL : current->actor;
    if (a == NULL) {
        exit(101);
    }
    LughaString *message = lugha_rt_alloc((int64_t)sizeof(LughaString) + (int64_t)len + 1);
    message->len = (int64_t)len;
    memcpy(message->bytes, text, len);
    message->bytes[len] = '\0';
    a->panic = message;
    a->status = PANICKED;
    Message *m = a->handling;
    a->handling = NULL;
    if (m != NULL && m->waiter != NULL) {
        *m->outcome = PANICKED;
        wake(m->waiter, 0);
    }
    if (m != NULL) {
        message_done(m);
    }
    settle_mail(a, PANICKED);
    wake_joiners(a);
    fiber_exit(current);
}

/* The visit of an actor's handle: its state, and its panic's text. */
static void rc_actor_visit(void *object, int64_t op) {
    LughaActor *a = object;
    if (op == RC_MARK) {
        lugha_rt_rc_mark(a->state);
        lugha_rt_rc_mark(a->panic);
    } else {
        lugha_rt_rc_release(a->state);
        lugha_rt_rc_release(a->panic);
    }
}

/* A call's result block, once the caller has its value. */
void lugha_rt_rc_block_done(void *block) {
    free(block);
}

/* `spawn Counter { … }`: an actor holding `state` (a counted block). */
LughaActor *lugha_rt_actor_spawn(void *state, const char *name) {
    LughaActor *a = lugha_rt_alloc((int64_t)sizeof(LughaActor));
    a->state = state;
    a->name = name;
    /* The actor's own fiber holds it (as its environment) until it ends,
     * and two tasks hold it now; the task's handle isn't kept. */
    lugha_rt_rc_set_drop(a, rc_actor_visit);
    lugha_rt_rc_retain(a);
    lugha_rt_rc_mark(a);
    lugha_rt_rc_release(lugha_rt_task_spawn(actor_loop, NULL, a, 1));
    last_spawned->actor = a;
    return a;
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

static void check_alive(const LughaActor *a, const char *file, int64_t line, int64_t col) {
    if (a->status == STOPPED) {
        actor_panic(a, "has stopped", file, line, col);
    }
    if (a->status == PANICKED) {
        actor_panic(a, "panicked", file, line, col);
    }
}

/* A call: queues `handler(state, args, result)` and waits for it; the
 * result's address (`size` bytes, freed by the caller once it has the
 * value: `lugha_rt_rc_block_done`). */
void *lugha_rt_actor_call(LughaActor *a, Handler handler, void *args, int64_t size,
                          const char *file, int64_t line, int64_t col) {
    ensure_started();
    check_alive(a, file, line, col);
    int32_t outcome = RUNNING;
    Message *m = new_message();
    m->handler = handler;
    m->args = args;
    /* The caller's to free once it has the value (`lugha_rt_rc_block_done`). */
    m->result = lugha_rc_internal((size_t)(size < 1 ? 1 : size));
    m->waiter = current;
    m->outcome = &outcome;
    post(a, m);
    block();
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
    check_alive(a, file, line, col);
    Message *m = new_message();
    m->handler = handler;
    m->args = args;
    post(a, m);
}

/* `a.stop()`: a message like any other, after what is queued. */
void lugha_rt_actor_stop(LughaActor *a, const char *file, int64_t line, int64_t col) {
    ensure_started();
    check_alive(a, file, line, col);
    Message *m = new_message();
    post(a, m);
}

/* `a.join()`: waits until the actor has ended; its panic's text, or NULL
 * when it stopped. */
LughaString *lugha_rt_actor_join(LughaActor *a) {
    ensure_started();
    if (a->status == RUNNING) {
        current->wait_next = a->joiners;
        a->joiners = current;
        block();
    }
    /* The caller holds the text it gets. */
    lugha_rt_rc_retain(a->panic);
    return a->panic;
}

/* After a deadlock: each task or actor waiting on a channel, and where the
 * channel was made. */
static void report_waits(void) {
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
        if (task != NULL) {
            fprintf(stderr, "  `%.*s`", (int)task->len, task->bytes);
        } else if (actor != NULL) {
            fprintf(stderr, "  actor `%s`", actor->name);
        } else {
            fputs("  `main`", stderr);
        }
        fprintf(stderr, " waits %s the channel made at %s:%" PRId64 ":%" PRId64 "\n", how, c->file,
                c->line, c->col);
    }
}
