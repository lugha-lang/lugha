/*
 * scopes.c — part of lugha_rt.c: scopes and cancellation.
 *
 * A scope's record lives while its body runs: the task running the body
 * (its owner), every task started in it, how many still run, and its first
 * failure. Tasks a scope starts know it (`started_in`); a fiber knows the
 * innermost scope whose body it is in (`scopes`, linked through `outer`).
 *
 * - A task's panic, or its `Err` (`lugha_rt_task_failed`), fails its
 *   scope: the first failure is kept, and every other task of the scope
 *   and its body are cancelled with `REASON_SIBLING`.
 * - Cancelling marks a fiber; it notices at its next safe point (a wait
 *   that may be cut short wakes at once) and unwinds, as a panic does but
 *   silently (unwind.c). Cancelling a fiber cancels the tasks of every
 *   scope whose body it runs, with `REASON_PARENT`.
 * - A scope's body is caught at its edge: generated code calls
 *   `lugha_rt_scope_caught` from the landing pad that stops the unwind,
 *   then the scope waits for its tasks and ends. A cancellation meant for
 *   an outer scope goes on from there (`lugha_rt_scope_exit`).
 * - The root scope is `main`'s: its tasks are waited for when `main`
 *   returns, and a failure in one ends the program once `main` has
 *   unwound.
 *
 * A fragment, not compiled on its own: joined after spawn.c, whose fibers,
 * tasks and scheduler it uses.
 */

enum { TASK_OK = 0, TASK_CANCELLED = 1, TASK_FAILED = 2, TASK_PANICKED = 3, TASK_DEADLINE = 4 };
enum {
    REASON_NONE = 0,
    REASON_SIBLING = 1,
    REASON_PARENT = 2,
    REASON_REQUESTED = 3,
    REASON_DEADLINE = 4,
    REASON_SHUTDOWN = 5,
};

typedef struct LughaScope {
    struct LughaScope *outer;
    Fiber *owner;
    /* Tasks started and not yet finished, how many of them are actors,
     * and how many its owner waits to see left. */
    int64_t running;
    int64_t actors;
    int64_t wait_left;
    bool owner_waiting;
    /* Every task it started, through `scope_next`; it holds each. */
    LughaTask *tasks;
    /* Its first failure: `TASK_FAILED` with an `Error` value's table and
     * box, or `TASK_PANICKED` with the message; `TASK_OK` while none. */
    int32_t failed;
    void *error_table;
    void *error_box;
    LughaString *panic;
    /* The failing task's name, with the failure. */
    LughaString *failed_task;
    /* Left early: no longer the fiber's, its tasks done and let go of. */
    bool left;
    /* Its name, for a failure of its body. */
    LughaString *name;
    /* What a task's failure does (`POLICY_*`, scope_policy.c), and the
     * handler `independent(f)` gives, a function value's code and env. */
    int32_t policy;
    void *handler_code;
    void *handler_env;
    /* `limit n`: how many tasks may run at once (0: any), and the fiber
     * waiting in `spawn` for one to finish. */
    int64_t limit;
    Fiber *limit_waiter;
    /* Its own lock (rt_lock.c's kind) over `tasks`, `running`, `actors`,
     * the owner's wait and `limit_waiter`: spawning and a task's end take
     * it, not the scopes lock; failing and cancelling, under the scopes
     * lock, take it to walk `tasks` (parent's before child's). */
    uint32_t lock;
    /* Its deadline (monotonic ns, 0: none), and the next scope with one. */
    int64_t deadline;
    struct LughaScope *deadline_next;
    /* The worker its body and tasks run on when pinned, its own or an
     * enclosing scope's, else NULL; the owner's pin before it began. */
    Worker *pinned;
    Worker *owner_pin;
    /* The callbacks registered in it, still running (callbacks.c), and
     * for `independent(f)`, the generated function that builds a panic's
     * `Failure<E>` and calls the handler with it. */
    struct CbRecord *callbacks;
    void (*panic_handler)(LughaString *text, LughaString *task, void *code, void *env);
} LughaScope;

/* Generated code keeps a scope's record in its frame, in this many bytes. */
_Static_assert(sizeof(LughaScope) <= 208, "a scope's record fits its frame slot");

/* `main`'s scope: its owner is the main fiber. */
static LughaScope root_scope;

static void cancel_unwind(void);

static void cancel_tasks(LughaScope *s, int32_t reason);

static void scope_room(LughaScope *s);
static bool shielded(Fiber *f);
static bool shield_cut_off(void);
static void scope_stop_actors(LughaScope *s);
static void actor_task_ended(bool panicked);
static void deadline_start(LughaScope *s, int64_t within, int64_t until);
static void deadline_stop(LughaScope *s);

/* Cancels `f` with `reason` (and `text`, for `REASON_REQUESTED`), by
 * `by`: a wait that may be cut short ends at once. */
static void cancel_fiber(Fiber *f, int32_t reason, LughaString *text, void *by) {
    if (f == NULL || f->cancelled != REASON_NONE) {
        return;
    }
    __atomic_store_n(&f->cancelled, reason, __ATOMIC_RELEASE);
    if (text != NULL) {
        // Made by the canceller, read by `f`.
        lugha_rt_rc_mark(text);
        lugha_rt_rc_retain(text);
    }
    f->cancel_text = text;
    f->cancelled_by = by;
    for (LughaScope *s = f->scopes; s != NULL; s = s->outer) {
        cancel_tasks(s, REASON_PARENT);
    }
    // A wait in a shield goes on until the shield's deadline (scope_time.c).
    // Only a fiber parked in the scheduler is WAITING, the running one
    // too when the scheduler cancels it (a deadline, Ctrl-C). The fence
    // pairs with `block`'s: a fiber about to park sees the cancel there if
    // this load misses its PARKING.
    if (rt_locking) {
        __atomic_thread_fence(__ATOMIC_SEQ_CST);
    }
    if (is_waiting(f) && f->cancellable && !shielded(f)) {
        wake(f, CANCELLED);
    }
}

/* Cancels every unfinished task `s` started. */
static void cancel_tasks(LughaScope *s, int32_t reason) {
    lock_take(&s->lock);
    for (LughaTask *t = s->tasks; t != NULL; t = t->scope_next) {
        if (!task_done(t) && t->fiber != NULL) {
            cancel_fiber(t->fiber, reason, NULL, s);
        }
    }
    lock_give(&s->lock);
}

/* `s` failed: the first failure is kept, the rest are cancelled. Takes
 * the error's box or the message over. */
static void scope_fail(LughaScope *s, int32_t outcome, void *table, void *box,
                       LughaString *panic, LughaString *task) {
    if (s->failed != TASK_OK) {
        lugha_rt_rc_release(box);
        lugha_rt_rc_release(panic);
        return;
    }
    // Made by the failing task, read by the scope's owner.
    lugha_rt_rc_mark(box);
    lugha_rt_rc_mark(panic);
    lugha_rt_rc_mark(task);
    s->failed = outcome;
    s->error_table = table;
    s->error_box = box;
    s->panic = panic;
    lugha_rt_rc_retain(task);
    s->failed_task = task;
    cancel_tasks(s, REASON_SIBLING);
    if (s->owner != current) {
        cancel_fiber(s->owner, REASON_SIBLING, NULL, s);
    }
}

/* Waits until no more than `left` of `s`'s tasks still run. */
static void scope_wait_until(LughaScope *s, int64_t left) {
    lock_take(&s->lock);
    while (s->running > left) {
        s->owner_waiting = true;
        s->wait_left = left;
        current->cancellable = false;
        uint32_t *locks[1] = {&s->lock};
        block_holding(locks, 1);
        lock_take(&s->lock);
    }
    s->owner_waiting = false;
    lock_give(&s->lock);
}

/* Waits until every task `s` started has finished: first all but its
 * actors, which may still be called, then, once they are stopped, the
 * actors. Its owner waits even if cancelled: nothing outlives its scope. */
static void scope_wait(LughaScope *s) {
    lock_take(&s->lock);
    int64_t actors = s->actors;
    lock_give(&s->lock);
    scope_wait_until(s, actors);
    scope_stop_actors(s);
    scope_wait_until(s, 0);
}

/* A task `f` ran has finished: its scope counts it, and wakes the owner
 * once the last is done. */
static void scope_task_done(Fiber *f) {
    LughaScope *s = f->started_in;
    LughaTask *t = f->task;
    t->fiber = NULL;
    if (t->outcome == TASK_OK && f->cancelled != REASON_NONE && f->unwinding) {
        t->outcome = TASK_CANCELLED;
    }
    if (s == NULL) {
        return;
    }
    lock_take(&s->lock);
    s->running--;
    if (f->actor != NULL) {
        s->actors--;
    }
    if (s->running <= s->wait_left && s->owner_waiting) {
        wake(s->owner, 0);
    }
    // A `spawn` waiting for room under `limit n`.
    if (s->limit_waiter != NULL && s->running - s->actors < s->limit) {
        Fiber *waiter = s->limit_waiter;
        s->limit_waiter = NULL;
        wake(waiter, 0);
    }
    lock_give(&s->lock);
}

/* `scope s word… { … }` starts: its record, at `s` in the frame running
 * the body, is the current fiber's innermost. `name` is the scope's,
 * `policy` and the handler (`code`, `env`) what a task's failure does,
 * `limit` how many tasks may run at once (0: any; below 0 panics). */
static void scope_pin(LughaScope *s, bool pinned);
static void pin_to(Worker *w);
static void callbacks_stop(LughaScope *s);
static LughaTask *spawn_into(LughaScope *s, void (*entry)(void *, void *, void *), void *code,
                             void *env, int64_t result_size, bool room);

void lugha_rt_scope_enter(LughaScope *s, LughaString *name, int32_t policy, void *code,
                          void *env, int64_t limit, int64_t within, int64_t until,
                          int32_t pinned, void *panic_handler) {
    RT_SECTION();
    ensure_started();
    // `limit` is -1 when the scope says none.
    if (limit == 0 || limit < -1) {
        panic_with("a scope's limit must be at least 1", 34);
    }
    memset(s, 0, sizeof *s);
    // Both reach its tasks: the name in their failures, the handler's
    // environment in whichever task fails.
    lugha_rt_rc_mark(name);
    lugha_rt_rc_mark(env);
    lugha_rt_rc_retain(name);
    s->name = name;
    s->policy = policy;
    s->handler_code = code;
    lugha_rt_rc_retain(env);
    s->handler_env = env;
    s->panic_handler = panic_handler;
    s->limit = limit < 0 ? 0 : limit;
    s->owner = current;
    s->outer = current->scopes;
    current->scopes = s;
    deadline_start(s, within, until);
    scope_pin(s, pinned != 0);
}

/* `s` pinned, by `pinned` or by the scope it is in (its owner's, or the
 * one its owner's task started in): its owner moves there, if it isn't,
 * and stays while it runs. A task already pinned keeps its worker; with
 * one worker, nothing is pinned. */
static void scope_pin(LughaScope *s, bool pinned) {
    LughaScope *outer = s->outer != NULL ? s->outer : current->started_in;
    Worker *to = outer != NULL ? outer->pinned : NULL;
    if (to == NULL && pinned && workers_wanted > 1) {
        // `main`'s pins to the main thread, which toolkits expect.
        to = current == &main_fiber ? main_worker : worker;
    }
    s->owner_pin = current->pin;
    if (to == NULL) {
        return;
    }
    s->pinned = current->pin != NULL ? current->pin : to;
    pin_to(s->pinned);
}

/* `s.spawn(…)`: a new task, started in `s`. */
LughaTask *lugha_rt_task_spawn_in(LughaScope *s, void (*entry)(void *, void *, void *),
                                  void *code, void *env, int64_t result_size) {
    return spawn_into(s, entry, code, env, result_size, true);
}

/* A new task in `s`; with `room`, waiting while `limit n` of its tasks
 * run (a `spawned` callback doesn't: C never waits on Lugha). */
static LughaTask *spawn_into(LughaScope *s, void (*entry)(void *, void *, void *), void *code,
                             void *env, int64_t result_size, bool room) {
    ensure_started();
    lugha_rt_cancel_point();
    // Only the scope's own lock: spawners in other scopes don't wait.
    lock_take(&s->lock);
    if (room) {
        scope_room(s);
    }
    // Queued once generated code has set it up (`lugha_rt_task_go`).
    LughaTask *task = task_create(entry, code, env, result_size);
    Fiber *f = task->fiber;
    f->started_in = s;
    f->pin = s->pinned;
    lugha_rt_rc_retain(task);
    task->scope_next = s->tasks;
    s->tasks = task;
    s->running++;
    bool failed = __atomic_load_n(&s->failed, __ATOMIC_ACQUIRE) != TASK_OK;
    bool leaving = __atomic_load_n(&s->owner->cancelled, __ATOMIC_ACQUIRE) != REASON_NONE;
    lock_give(&s->lock);
    // A scope already failed, or leaving, stops what it starts at once:
    // under the scopes lock, which comes before the scope's. One failing
    // after this saw it fine finds the task in its list.
    if (failed || leaving) {
        RT_SECTION();
        if (s->failed != TASK_OK) {
            cancel_fiber(f, REASON_SIBLING, NULL, s);
        } else if (s->owner->cancelled != REASON_NONE) {
            cancel_fiber(f, REASON_PARENT, NULL, s);
        }
    }
    return task;
}

/* The body has finished: its callbacks stop, and it waits for every
 * task. */
void lugha_rt_scope_end(LughaScope *s) {
    // Before the section: the callback lock comes before the scopes lock.
    callbacks_stop(s);
    RT_SECTION();
    scope_wait(s);
}

/* Ends `s` as the current fiber's scope: its tasks let go of, and a
 * cancellation it made over. */
static void scope_pop(LughaScope *s) {
    current->scopes = s->outer;
    // Free again once its pinned scope ends, unless a handle pinned it.
    if (s->pinned != NULL && !current->pin_kept) {
        __atomic_store_n(&current->pin, s->owner_pin, __ATOMIC_RELEASE);
    }
    deadline_stop(s);
    lugha_rt_rc_release(s->name);
    lugha_rt_rc_release(s->handler_env);
    s->name = NULL;
    s->handler_env = NULL;
    lock_take(&s->lock);
    LughaTask *done = s->tasks;
    s->tasks = NULL;
    lock_give(&s->lock);
    for (LughaTask *t = done; t != NULL;) {
        LughaTask *next = t->scope_next;
        lugha_rt_rc_release(t);
        t = next;
    }
    if (current->cancelled_by == s) {
        __atomic_store_n(&current->cancelled, REASON_NONE, __ATOMIC_RELEASE);
        current->cancelled_by = NULL;
        lugha_rt_rc_release(current->cancel_text);
        current->cancel_text = NULL;
    }
}

/* The body is left early: its tasks are cancelled and waited for, and the
 * scope ends; a failure meanwhile is dropped, as the exit goes on. */
void lugha_rt_scope_leave(LughaScope *s) {
    callbacks_stop(s);
    RT_SECTION();
    cancel_tasks(s, REASON_PARENT);
    scope_wait(s);
    scope_pop(s);
    lugha_rt_rc_release(s->error_box);
    lugha_rt_rc_release(s->panic);
    s->left = true;
}

/* A panic or cancellation unwinding the body stopped at the scope: a
 * panic fails the scope; a cancellation by this scope (a task failed) is
 * over; one from outside stays, to go on once the scope ends. Then the
 * tasks are cancelled and waited for. */
void lugha_rt_scope_caught(LughaScope *s) {
    callbacks_stop(s);
    RT_SECTION();
    overflow_over();
    Panic *caught = current->in_flight;
    // Unwinding from the exit that left it (a `defer` there): not its own.
    if (s->left && caught != NULL) {
        resume_unwind(caught);
    }
    current->in_flight = NULL;
    current->unwinding = false;
    if (caught != NULL && !caught->cancel) {
        LughaString *text = string_of(caught->text, (int64_t)caught->len);
        scope_fail(s, TASK_PANICKED, NULL, NULL, text, s->name);
    }
    if (caught != NULL) {
        free(caught->text);
        free(caught);
    }
    cancel_tasks(s, s->failed != TASK_OK ? REASON_SIBLING : REASON_PARENT);
    scope_wait(s);
}

/* The scope ends: its failure, if any (`TASK_FAILED` with the error's
 * table and box, `TASK_PANICKED` with the message, handed over), or
 * `TASK_OK`. A cancellation from outside goes on unwinding from here. */
int32_t lugha_rt_scope_exit(LughaScope *s, void **table, void **box, LughaString **panic,
                            LughaString **task) {
    RT_SECTION();
    int32_t failed = s->failed;
    *table = s->error_table;
    *box = s->error_box;
    *panic = s->panic;
    *task = s->failed_task;
    scope_pop(s);
    if (current->cancelled != REASON_NONE) {
        lugha_rt_rc_release(*box);
        lugha_rt_rc_release(*panic);
        lugha_rt_rc_release(*task);
        cancel_unwind();
    }
    return failed;
}

/* The current task returned `Err`: it failed with that error, its table
 * (NULL unless the scope's error type is an interface) and box, handed
 * over; `fails` when the failure fails its scope. */
void lugha_rt_task_failed(void *table, void *box, int32_t fails) {
    RT_SECTION();
    LughaTask *t = current->task;
    // Its joiners read the error.
    lugha_rt_rc_mark(box);
    t->outcome = TASK_FAILED;
    t->error_table = table;
    t->error_box = box;
    if (current->started_in != NULL && fails) {
        lugha_rt_rc_retain(box);
        scope_fail(current->started_in, TASK_FAILED, table, box, NULL, t->name);
    }
}

/* A scoped task unwound to its start: a cancellation ends it cancelled,
 * a panic fails its scope. */
static void scope_task_unwound(bool cancel, const char *text, size_t len) {
    // Never returns: the task ends, the lock going on to the next fiber.
    rt_enter();
    LughaTask *t = current->task;
    if (cancel) {
        t->outcome = TASK_CANCELLED;
        t->reason = current->cancelled;
        t->text = current->cancel_text;
        current->cancel_text = NULL;
    } else {
        t->outcome = TASK_PANICKED;
        t->text = string_of(text, (int64_t)len);
        lugha_rt_rc_mark(t->text);
        lugha_rt_rc_retain(t->text);
        scope_fail(current->started_in, TASK_PANICKED, NULL, NULL, t->text, t->name);
    }
    current->unwinding = false;
    fiber_exit(current);
}

/* True if a panic here is reported elsewhere, so not printed as it
 * happens: the current fiber runs a scope's body (its scope's value), or
 * is a task a scope started (its entry reports it, naming the task). */
static bool panic_caught(void) {
    if (current == NULL) {
        return false;
    }
    // Inside a direct callback, the registering scope reports it.
    return current->scopes != NULL || current->started_in != NULL || current->in_callback > 0;
}

/* At a safe point: a cancelled task unwinds from here. */
void lugha_rt_cancel_point(void) {
    // Not cancelled, the common case: no lock taken.
    Fiber *self = current;
    if (self == NULL || __atomic_load_n(&self->cancelled, __ATOMIC_ACQUIRE) == REASON_NONE) {
        return;
    }
    // Inside a direct C callback: C waits for it, so it runs on.
    if (self->in_callback > 0) {
        return;
    }
    RT_SECTION();
    if (current->unwinding ? shield_cut_off() : !shielded(current)) {
        cancel_unwind();
    }
}

/* After a wait that may be cut short: unwinds if it was. */
static void woke(int64_t fired) {
    current->cancellable = false;
    if (fired == CANCELLED) {
        cancel_unwind();
    }
}
