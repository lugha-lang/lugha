/*
 * callbacks.c — part of lugha_rt.c: closures handed to C as callbacks with
 * user data. Each is pinned in a record C holds (`lugha_rt_cb_pin`), which
 * keeps the closure and its captures, and registers it in the innermost
 * scope of the task that made the extern call.
 *
 * - A `spawned` callback (`data`, no result) starts a task in that
 *   scope with C's arguments (`lugha_rt_cb_queue`) and returns at once.
 * - A `direct` one runs during C's call, in the calling task, which no
 *   cancellation point unwinds meanwhile (`in_callback`).
 * - A panic in a direct callback stops where it would leave for C
 *   (`lugha_rt_cb_caught`): C gets a zero value, and a task in the
 *   registering scope reports the panic, as any of its tasks would.
 * - Once the scope's body ends its callbacks stop: C's later calls are
 *   dropped, and the closures' captures are let go of then, or when C
 *   calls `release` (`lugha_rt_cb_release`), which frees the record.
 *
 * One lock guards every record's scope and every scope's list of them; it
 * comes before the scopes lock and a scope's own lock.
 *
 * A fragment, not compiled on its own: joined after scope_policy.c, whose
 * scopes, failures and policies it uses.
 */

typedef struct CbRecord {
    /* The closure, read by the trampolines at these two offsets. */
    void *code;
    void *env;
    /* Its scope while it runs, NULL once stopped; its neighbours there. */
    LughaScope *scope;
    struct CbRecord *prev;
    struct CbRecord *next;
} CbRecord;

static uint32_t cb_lock;

/* The scope a callback registered by the running task belongs to: its
 * innermost, else the one it started in, else the root scope. */
static LughaScope *innermost_scope(void) {
    Fiber *f = current;
    if (f != NULL && f->scopes != NULL) {
        return f->scopes;
    }
    if (f != NULL && f->started_in != NULL) {
        return f->started_in;
    }
    root_scope.owner = &main_fiber;
    return &root_scope;
}

/* A copy of the function value { code, env } C can hold, registered in
 * the running task's innermost scope. */
void *lugha_rt_cb_pin(void *code, void *env) {
    CbRecord *r = lugha_rc_internal(sizeof(CbRecord));
    r->code = code;
    r->env = env;
    // C may call it from any task.
    lugha_rt_rc_mark(env);
    lugha_rt_rc_retain(env);
    LughaScope *s = innermost_scope();
    lock_take(&cb_lock);
    r->scope = s;
    r->next = s->callbacks;
    if (s->callbacks != NULL) {
        s->callbacks->prev = r;
    }
    s->callbacks = r;
    lock_give(&cb_lock);
    return r;
}

/* C dropped the callback: its captures are let go of, if its scope
 * hasn't already, and the record freed. */
void lugha_rt_cb_release(void *pinned) {
    CbRecord *r = pinned;
    void *env = NULL;
    lock_take(&cb_lock);
    if (r->scope != NULL) {
        if (r->prev != NULL) {
            r->prev->next = r->next;
        } else {
            r->scope->callbacks = r->next;
        }
        if (r->next != NULL) {
            r->next->prev = r->prev;
        }
        r->scope = NULL;
        env = r->env;
        r->env = NULL;
    }
    lock_give(&cb_lock);
    lugha_rt_rc_release(env);
    free(r);
}

/* `s`'s body has ended: its callbacks stop, their captures let go of. */
static void callbacks_stop(LughaScope *s) {
    lock_take(&cb_lock);
    CbRecord *first = s->callbacks;
    s->callbacks = NULL;
    int64_t n = 0;
    for (CbRecord *r = first; r != NULL; r = r->next) {
        n++;
    }
    // Taken out under the lock: C may free a stopped record at once.
    void **envs = n > 0 ? malloc((size_t)n * sizeof(void *)) : NULL;
    if (n > 0 && envs == NULL) {
        die("panic: out of memory\n");
    }
    int64_t i = 0;
    for (CbRecord *r = first; r != NULL;) {
        CbRecord *next = r->next;
        envs[i++] = r->env;
        r->env = NULL;
        r->scope = NULL;
        r->prev = r->next = NULL;
        r = next;
    }
    lock_give(&cb_lock);
    for (i = 0; i < n; i++) {
        lugha_rt_rc_release(envs[i]);
    }
    free(envs);
}

/* The first call of every C entry into Lugha: on a helper thread the
 * program ends (`blocking` externs, helpers.c); in a task, it is inside
 * a direct callback until `lugha_rt_cb_leave`. */
void lugha_rt_cb_enter(void) {
    callback_on_helper();
    // A panic in it needs a task to stop in.
    ensure_started();
    current->in_callback++;
}

/* The C entry returns to C. */
void lugha_rt_cb_leave(void) {
    if (worker->running != NULL) {
        worker->running->in_callback--;
    }
}

/* Before a callback with user data runs: its captures, a new holder, in
 * `env`, and true; false once its scope's body has ended. */
int32_t lugha_rt_cb_hold(void *pinned, void **env) {
    CbRecord *r = pinned;
    lock_take(&cb_lock);
    bool live = r->scope != NULL;
    if (live) {
        lugha_rt_rc_retain(r->env);
        *env = r->env;
    }
    lock_give(&cb_lock);
    return live;
}

/* A task in `s` running `entry` with `env` (handed over): C's queued
 * call, or a panic to report. */
static void callback_task(LughaScope *s, void (*entry)(void *, void *, void *), void *env,
                          bool pinned_here) {
    LughaTask *t = spawn_into(s, entry, NULL, env, 1, false);
    if (pinned_here && workers_wanted > 1) {
        __atomic_store_n(&t->fiber->pin, worker, __ATOMIC_RELEASE);
    }
    lock_give(&cb_lock);
    // Its name in failures; the task holds its own.
    LughaString *name = string_of("callback", 8);
    lugha_rt_task_name(t, name);
    lugha_rt_rc_release(name);
    lugha_rt_task_go(t);
    lugha_rt_rc_release(t);
}

/* a `spawned` callback's call: a task in its scope running `entry` with
 * `args` (handed over: C's arguments and the closure); dropped once the
 * scope's body has ended. `pins`: it takes a pinned handle, so it runs on
 * the thread C called from. */
void lugha_rt_cb_queue(void *pinned, void (*entry)(void *, void *, void *), void *args,
                       int32_t pins) {
    CbRecord *r = pinned;
    lock_take(&cb_lock);
    if (r->scope == NULL) {
        lock_give(&cb_lock);
        lugha_rt_rc_release(args);
        return;
    }
    callback_task(r->scope, entry, args, pins != 0);
}

/* Prints a panic as an `independent` scope's report does. */
static void report_panic(LughaString *task, LughaString *text) {
    lugha_rt_print_begin();
    fprintf(stderr, "task `%.*s` panicked: %.*s\n", (int)(task != NULL ? task->len : 0),
            task != NULL ? task->bytes : "", (int)text->len, text->bytes);
    fflush(stderr);
    lugha_rt_print_end();
}

/* The running task, started for a callback, panicked with `text` (handed
 * over): its scope's policy decides, as for any of its tasks. */
void lugha_rt_cb_task_panicked(LughaString *text) {
    LughaScope *s = current->started_in;
    int32_t policy = s == &root_scope ? POLICY_ROOT : s->policy;
    LughaString *name = current->task->name;
    if (policy == POLICY_PRINT || policy == POLICY_ROOT) {
        report_panic(name, text);
    }
    if (policy == POLICY_HANDLER && s->panic_handler != NULL) {
        // The handler's failure holds both, and lets them go.
        lugha_rt_rc_retain(text);
        lugha_rt_rc_retain(name);
        s->panic_handler(text, name, s->handler_code, s->handler_env);
    }
    lugha_rt_task_panicked(text, policy == POLICY_ALL || policy == POLICY_ROOT);
}

/* A task that reports a direct callback's panic, `env` its message. */
static void panic_entry(void *code, void *env, void *result) {
    (void)code;
    (void)result;
    // The message is the task's environment too, let go of at its end.
    lugha_rt_rc_retain(env);
    lugha_rt_cb_task_panicked(env);
}

/* A panic stopped where a direct callback returns to C: it is over here,
 * and a task in the registering scope (`pinned`'s, or the calling task's
 * innermost for one without user data or stopped) reports it. */
void lugha_rt_cb_caught(void *pinned) {
    LughaString *text;
    {
        RT_SECTION();
        overflow_over();
        Panic *caught = current->in_flight;
        current->in_flight = NULL;
        current->unwinding = false;
        text = caught != NULL ? string_of(caught->text, (int64_t)caught->len) : NULL;
        if (caught != NULL) {
            free(caught->text);
            free(caught);
        }
    }
    lugha_rt_cb_leave();
    if (text == NULL) {
        return;
    }
    lugha_rt_rc_mark(text);
    CbRecord *r = pinned;
    lock_take(&cb_lock);
    LughaScope *s = r != NULL && r->scope != NULL ? r->scope : innermost_scope();
    callback_task(s, panic_entry, text, false);
}
