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

/* The actor ended (`outcome`): the message it was handling and every one
 * still queued are answered, and senders waiting for room look again. */
static void actor_ended(LughaActor *a, int32_t outcome) {
    if (a->status != RUNNING) {
        return;
    }
    a->status = outcome;
    a->idle = NULL;
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
    for (;;) {
        while (a->head == NULL) {
            a->idle = current;
            current->cancellable = true;
            int64_t fired = block();
            a->idle = NULL;
            woke(fired);
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
        m->handler(a->state, m->args, m->result);
        a->handling = NULL;
        answer(m, RUNNING);
        message_done(m);
    }
    actor_ended(a, STOPPED);
}

/* A panic nothing caught ends the program: an actor's is caught by its
 * task's entry, as any task's. */
static void panic_end(const char *text, size_t len) {
    (void)text;
    (void)len;
    exit(101);
}

/* The visit of an actor's handle: its state. */
static void rc_actor_visit(void *object, int64_t op) {
    LughaActor *a = object;
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
    f->actor = a;
    LughaScope *s = f->started_in;
    s->actors++;
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

/* Waits while the mailbox is full (a pause point), then queues `m`. */
static void post_counted(LughaActor *a, Message *m, const char *file, int64_t line, int64_t col) {
    lugha_rt_cancel_point();
    check_alive(a, file, line, col);
    while (a->queued >= a->capacity) {
        current->wait_next = a->room_waiters;
        a->room_waiters = current;
        current->cancellable = true;
        int64_t fired = block();
        if (fired == CANCELLED) {
            for (Fiber **at = &a->room_waiters; *at != NULL; at = &(*at)->wait_next) {
                if (*at == current) {
                    *at = current->wait_next;
                    break;
                }
            }
            message_done(m);
            if (m->waiter != NULL) {
                free(m);
            }
        }
        woke(fired);
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
    int32_t outcome = RUNNING;
    Message *m = new_message();
    m->handler = handler;
    m->args = args;
    m->waiter = current;
    m->outcome = &outcome;
    post_counted(a, m, file, line, col);
    /* The caller's to free once it has the value (`lugha_rt_rc_block_done`). */
    m->result = lugha_rc_internal((size_t)(size < 1 ? 1 : size));
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
    Message *m = new_message();
    m->handler = handler;
    m->args = args;
    post_counted(a, m, file, line, col);
}

/* Stops `a` after what is queued, past its mailbox's limit. */
static void stop_after_mail(LughaActor *a) {
    post(a, new_message());
}

/* `a.stop()`: a message like any other, after what is queued. */
void lugha_rt_actor_stop(LughaActor *a, const char *file, int64_t line, int64_t col) {
    ensure_started();
    check_alive(a, file, line, col);
    stop_after_mail(a);
}

/* The scope's other tasks are done: each actor it still runs stops after
 * its queued mail. */
static void scope_stop_actors(LughaScope *s) {
    for (LughaTask *t = s->tasks; t != NULL; t = t->scope_next) {
        LughaActor *a = t->fiber != NULL ? t->fiber->actor : NULL;
        if (!t->done && a != NULL && a->status == RUNNING) {
            stop_after_mail(a);
        }
    }
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
}
