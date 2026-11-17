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

enum { TASK_OK = 0, TASK_CANCELLED = 1, TASK_FAILED = 2, TASK_PANICKED = 3 };
enum { REASON_NONE = 0, REASON_SIBLING = 1, REASON_PARENT = 2, REASON_REQUESTED = 3 };

typedef struct LughaScope {
    struct LughaScope *outer;
    Fiber *owner;
    /* Tasks started and not yet finished. */
    int64_t running;
    bool owner_waiting;
    /* Every task it started, through `scope_next`; it holds each. */
    LughaTask *tasks;
    /* Its first failure: `TASK_FAILED` with an `Error` value's table and
     * box, or `TASK_PANICKED` with the message; `TASK_OK` while none. */
    int32_t failed;
    void *error_table;
    void *error_box;
    LughaString *panic;
    /* Left early: no longer the fiber's, its tasks done and let go of. */
    bool left;
} LughaScope;

/* Generated code keeps a scope's record in its frame, in this many bytes. */
_Static_assert(sizeof(LughaScope) <= 96, "a scope's record fits its frame slot");

/* `main`'s scope: its owner is the main fiber. */
static LughaScope root_scope;

static void cancel_unwind(void);

static void cancel_tasks(LughaScope *s, int32_t reason);

/* Cancels `f` with `reason` (and `text`, for `REASON_REQUESTED`), by
 * `by`: a wait that may be cut short ends at once. */
static void cancel_fiber(Fiber *f, int32_t reason, LughaString *text, void *by) {
    if (f == NULL || f->cancelled != REASON_NONE) {
        return;
    }
    f->cancelled = reason;
    if (text != NULL) {
        lugha_rt_rc_retain(text);
    }
    f->cancel_text = text;
    f->cancelled_by = by;
    for (LughaScope *s = f->scopes; s != NULL; s = s->outer) {
        cancel_tasks(s, REASON_PARENT);
    }
    if (f != current && f->fired == WAITING && f->cancellable) {
        wake(f, CANCELLED);
    }
}

/* Cancels every unfinished task `s` started. */
static void cancel_tasks(LughaScope *s, int32_t reason) {
    for (LughaTask *t = s->tasks; t != NULL; t = t->scope_next) {
        if (!t->done) {
            cancel_fiber(t->fiber, reason, NULL, s);
        }
    }
}

/* `s` failed: the first failure is kept, the rest are cancelled. Takes
 * the error's box or the message over. */
static void scope_fail(LughaScope *s, int32_t outcome, void *table, void *box,
                       LughaString *panic) {
    if (s->failed != TASK_OK) {
        lugha_rt_rc_release(box);
        lugha_rt_rc_release(panic);
        return;
    }
    s->failed = outcome;
    s->error_table = table;
    s->error_box = box;
    s->panic = panic;
    cancel_tasks(s, REASON_SIBLING);
    if (s->owner != current) {
        cancel_fiber(s->owner, REASON_SIBLING, NULL, s);
    }
}

/* Waits until every task `s` started has finished. Its owner waits even
 * if cancelled: nothing outlives its scope. */
static void scope_wait(LughaScope *s) {
    while (s->running > 0) {
        s->owner_waiting = true;
        current->cancellable = false;
        block();
    }
    s->owner_waiting = false;
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
    s->running--;
    if (s->running == 0 && s->owner_waiting) {
        wake(s->owner, 0);
    }
}

/* `scope s { … }` starts: its record, at `s` in the frame running the
 * body, is the current fiber's innermost. */
void lugha_rt_scope_enter(LughaScope *s) {
    ensure_started();
    memset(s, 0, sizeof *s);
    s->owner = current;
    s->outer = current->scopes;
    current->scopes = s;
}

/* The root scope, which `main` and tests take. */
void *lugha_rt_scope_root(void) {
    ensure_started();
    root_scope.owner = &main_fiber;
    return &root_scope;
}

/* `s.spawn(…)`: a new task, started in `s`. */
LughaTask *lugha_rt_task_spawn_in(LughaScope *s, void (*entry)(void *, void *, void *),
                                  void *code, void *env, int64_t result_size) {
    LughaTask *task = lugha_rt_task_spawn(entry, code, env, result_size);
    Fiber *f = last_spawned;
    f->started_in = s;
    task->fiber = f;
    lugha_rt_rc_retain(task);
    task->scope_next = s->tasks;
    s->tasks = task;
    s->running++;
    // A scope already failed, or leaving, stops what it starts at once.
    if (s->failed != TASK_OK) {
        cancel_fiber(f, REASON_SIBLING, NULL, s);
    } else if (s->owner->cancelled != REASON_NONE) {
        cancel_fiber(f, REASON_PARENT, NULL, s);
    }
    return task;
}

/* The body has finished: waits for every task. */
void lugha_rt_scope_end(LughaScope *s) {
    scope_wait(s);
}

/* Ends `s` as the current fiber's scope: its tasks let go of, and a
 * cancellation it made over. */
static void scope_pop(LughaScope *s) {
    current->scopes = s->outer;
    for (LughaTask *t = s->tasks; t != NULL;) {
        LughaTask *next = t->scope_next;
        lugha_rt_rc_release(t);
        t = next;
    }
    s->tasks = NULL;
    if (current->cancelled_by == s) {
        current->cancelled = REASON_NONE;
        current->cancelled_by = NULL;
        lugha_rt_rc_release(current->cancel_text);
        current->cancel_text = NULL;
    }
}

/* The body is left early: its tasks are cancelled and waited for, and the
 * scope ends; a failure meanwhile is dropped, as the exit goes on. */
void lugha_rt_scope_leave(LughaScope *s) {
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
    Panic *caught = current->in_flight;
    // Unwinding from the exit that left it (a `defer` there): not its own.
    if (s->left && caught != NULL) {
        resume_unwind(caught);
    }
    current->in_flight = NULL;
    current->unwinding = false;
    if (caught != NULL && !caught->cancel) {
        scope_fail(s, TASK_PANICKED, NULL, NULL, string_of(caught->text, (int64_t)caught->len));
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
int32_t lugha_rt_scope_exit(LughaScope *s, void **table, void **box, LughaString **panic) {
    int32_t failed = s->failed;
    *table = s->error_table;
    *box = s->error_box;
    *panic = s->panic;
    scope_pop(s);
    if (current->cancelled != REASON_NONE) {
        lugha_rt_rc_release(*box);
        lugha_rt_rc_release(*panic);
        cancel_unwind();
    }
    return failed;
}

/* The current task returned `Err`: it failed with that error, an `Error`
 * value's table and box (handed over). */
void lugha_rt_task_failed(void *table, void *box) {
    LughaTask *t = current->task;
    t->outcome = TASK_FAILED;
    t->error_table = table;
    t->error_box = box;
    if (current->started_in != NULL) {
        lugha_rt_rc_retain(box);
        scope_fail(current->started_in, TASK_FAILED, table, box, NULL);
    }
}

/* A scoped task unwound to its start: a cancellation ends it cancelled,
 * a panic fails its scope. */
static void scope_task_unwound(bool cancel, const char *text, size_t len) {
    LughaTask *t = current->task;
    if (cancel) {
        t->outcome = TASK_CANCELLED;
        t->reason = current->cancelled;
        t->text = current->cancel_text;
        current->cancel_text = NULL;
    } else {
        t->outcome = TASK_PANICKED;
        t->text = string_of(text, (int64_t)len);
        lugha_rt_rc_retain(t->text);
        scope_fail(current->started_in, TASK_PANICKED, NULL, NULL, t->text);
    }
    current->unwinding = false;
    fiber_exit(current);
}

/* True if a scope other than the root catches a panic here: the current
 * fiber runs a scope's body, or a task a scope started. */
static bool panic_caught(void) {
    if (current == NULL) {
        return false;
    }
    bool started = current->started_in != NULL && current->started_in != &root_scope;
    return current->scopes != NULL || started;
}

/* At a safe point: a cancelled task unwinds from here. */
void lugha_rt_cancel_point(void) {
    if (current != NULL && current->cancelled != REASON_NONE && !current->unwinding) {
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

/* `t.join()`: waits for `t`; how it ended (`TASK_*`), with what goes with
 * it: the reason and text, the error, or the message (each a new holder). */
int32_t lugha_rt_task_join_status(LughaTask *t, int32_t *reason, LughaString **text,
                                  void **table, void **box) {
    ensure_started();
    lugha_rt_cancel_point();
    if (!t->done) {
        current->wait_next = t->joiners;
        t->joiners = current;
        current->cancellable = true;
        int64_t fired = block();
        if (fired == CANCELLED) {
            for (Fiber **at = &t->joiners; *at != NULL; at = &(*at)->wait_next) {
                if (*at == current) {
                    *at = current->wait_next;
                    break;
                }
            }
        }
        woke(fired);
    }
    *reason = t->reason;
    *text = t->text;
    *table = t->error_table;
    *box = t->error_box;
    lugha_rt_rc_retain(*text);
    lugha_rt_rc_retain(*box);
    return t->outcome;
}

/* `t.cancel(why)`. */
void lugha_rt_task_cancel(LughaTask *t, LughaString *why) {
    if (!t->done) {
        cancel_fiber(t->fiber, REASON_REQUESTED, why, NULL);
    }
}

/* `t.name(text)`. */
void lugha_rt_task_name(LughaTask *t, LughaString *name) {
    lugha_rt_rc_retain(name);
    lugha_rt_rc_release(t->name);
    t->name = name;
}

/* When `main` returns: its scope's tasks finish first; one that failed
 * ends the program as an unhandled panic does. */
/* Waits for the root scope's tasks, and lets go of them and of its
 * failure, once reported. */
static void root_scope_wait(void) {
    scope_wait(&root_scope);
    for (LughaTask *t = root_scope.tasks; t != NULL;) {
        LughaTask *next = t->scope_next;
        lugha_rt_rc_release(t);
        t = next;
    }
    root_scope.tasks = NULL;
    lugha_rt_rc_release(root_scope.error_box);
    lugha_rt_rc_release(root_scope.panic);
    root_scope.error_box = NULL;
    root_scope.panic = NULL;
}

/* `main` unwound: when cancelled because a root-scope task failed, the
 * other tasks, cancelled too, stop before the program ends. */
void lugha_rt_scope_root_unwound(void) {
    if (current != NULL && current->cancelled != REASON_NONE) {
        current->unwinding = false;
        root_scope_wait();
        current->unwinding = true;
    }
}

void lugha_rt_scope_root_finish(void) {
    if (current == NULL) {
        return;
    }
    root_scope_wait();
    if (root_scope.failed == TASK_FAILED) {
        fflush(stdout);
        fputs("error: a task in the root scope failed\n", stderr);
    }
    if (root_scope.failed != TASK_OK) {
        fflush(stdout);
        exit(101);
    }
}
