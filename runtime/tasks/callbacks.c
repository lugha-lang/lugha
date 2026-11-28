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
 * comes before the scopes lock and a scope's own lock. A direct call never
 * takes it: it checks and holds the captures through the record's `uses`
 * word (`lugha_rt_cb_hold`), so a real-time call waits on no other thread.
 *
 * A thread C made (no runtime thread, no helper) gets, on its first
 * callback, a fiber of its own and a C stack like a worker's, aligned and
 * guarded, which its callbacks run on (`lugha_rt_cb_run`); a pthread key's
 * destructor gives them back when the thread exits. Registering a
 * real-time callback sets one such pair aside (`lugha_rt_cb_reserve`), which
 * a first real-time call (`lugha_rt_cb_run_realtime`) takes without
 * allocating or locking. It is never a worker:
 * nothing waits there, its allocations come from the system allocator,
 * and a task it makes ready goes to the shared queue.
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
    /* Calls between their check and their hold on `env`, counted in the
     * low bits, with `CB_STOPPED` once its scope's body ended or C let go,
     * and `CB_DROPPED` once the record's own hold on `env` is let go of. */
    uint64_t uses;
} CbRecord;

#define CB_STOPPED (1ULL << 62)
#define CB_DROPPED (1ULL << 63)

/* The record's own hold on `env`, let go of by whoever finds it stopped
 * with no call between check and hold: exactly one compare-and-swap wins. */
static void cb_drop_if_last(CbRecord *r, uint64_t seen) {
    if (seen != CB_STOPPED) {
        return;
    }
    if (__atomic_compare_exchange_n(&r->uses, &seen, CB_STOPPED | CB_DROPPED, false,
                                    __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        void *env = r->env;
        r->env = NULL;
        lugha_rt_rc_release(env);
    }
}

/* Stops `r` (under `cb_lock`): later calls are dropped, and the captures
 * are let go of now, or by the last call still taking hold of them. */
static void cb_stop(CbRecord *r) {
    uint64_t seen = __atomic_or_fetch(&r->uses, CB_STOPPED, __ATOMIC_ACQ_REL);
    cb_drop_if_last(r, seen);
}

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
    r->uses = 0;
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
    __atomic_add_fetch(&callbacks_live, 1, __ATOMIC_ACQ_REL);
    lock_give(&cb_lock);
    return r;
}

/* C dropped the callback: its captures are let go of, if its scope
 * hasn't already, and the record freed. */
void lugha_rt_cb_release(void *pinned) {
    CbRecord *r = pinned;
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
        __atomic_sub_fetch(&callbacks_live, 1, __ATOMIC_ACQ_REL);
        // C calls no more: no call is between check and hold, so this
        // lets go of the captures.
        cb_stop(r);
    }
    lock_give(&cb_lock);
    free(r);
}

/* `s`'s body has ended: its callbacks stop, their captures let go of. */
static void callbacks_stop(LughaScope *s) {
    lock_take(&cb_lock);
    CbRecord *first = s->callbacks;
    s->callbacks = NULL;
    int64_t n = 0;
    for (CbRecord *r = first; r != NULL;) {
        CbRecord *next = r->next;
        r->scope = NULL;
        r->prev = r->next = NULL;
        // Under the lock: C may free a stopped record at once.
        cb_stop(r);
        n++;
        r = next;
    }
    __atomic_sub_fetch(&callbacks_live, n, __ATOMIC_ACQ_REL);
    lock_give(&cb_lock);
}

static pthread_key_t foreign_key;
static pthread_once_t foreign_key_once = PTHREAD_ONCE_INIT;

/* A thread record set aside for a thread C made, so its first call of a
 * real-time callback allocates nothing and takes no lock: a fiber and a C
 * stack, made when a real-time callback is registered. `spare_state` is
 * SPARE_EMPTY, SPARE_FILLING (under `spare_lock`), SPARE_READY, or
 * SPARE_TAKING (by the one thread whose compare-and-swap won it). */
enum { SPARE_EMPTY, SPARE_FILLING, SPARE_READY, SPARE_TAKING };
static int spare_state;
static Fiber *spare_fiber;
static char *spare_stack;
static pthread_mutex_t spare_lock = PTHREAD_MUTEX_INITIALIZER;

/* A thread C made exits: its fiber and C stack go back. */
static void foreign_exit(void *unused) {
    (void)unused;
    Worker *w = worker;
    // Off the list of records, its counts kept for `LUGHA_RC_REPORT`. A
    // thread that ran only real-time callbacks was never listed, but may
    // still have freed captures it held last, so its counts are kept too.
    pthread_mutex_lock(&workers_lock);
    if (w->foreign_listed) {
        for (Worker **at = &workers; *at != NULL; at = &(*at)->next_worker) {
            if (*at == w) {
                *at = w->next_worker;
                break;
            }
        }
        w->foreign_listed = false;
    }
    rc_retired_made += w->made;
    rc_retired_freed += w->freed;
    w->made = 0;
    w->freed = 0;
    pthread_mutex_unlock(&workers_lock);
    char *stack = (char *)w->c_stack_guard;
    if (os_stack_unguard(stack, C_STACK_GUARD)) {
        // To the shared pool: a cache of this thread's ends with it.
        w->stack_cached = STACK_CACHE;
        give_stack(stack);
    }
    free(w->running);
    w->running = NULL;
    w->c_stack_top = NULL;
    w->foreign = false;
    __atomic_sub_fetch(&foreign_live, 1, __ATOMIC_ACQ_REL);
}

static void foreign_key_make(void) {
    if (pthread_key_create(&foreign_key, foreign_exit) != 0) {
        die("panic: can't track threads C made\n");
    }
}

/* This thread, made by C, joins the list of `workers` while it lives, so
 * `LUGHA_RC_REPORT` counts what it allocates. */
static void foreign_list(Worker *w) {
    pthread_mutex_lock(&workers_lock);
    w->next_worker = workers;
    workers = w;
    pthread_mutex_unlock(&workers_lock);
    w->foreign_listed = true;
}

/* `lugha_rt_cb_reserve()`, before an extern call registering a real-time
 * callback: the pthread key made, and the spare ready if it isn't. */
void lugha_rt_cb_reserve(void) {
    pthread_once(&foreign_key_once, foreign_key_make);
    if (__atomic_load_n(&spare_state, __ATOMIC_ACQUIRE) != SPARE_EMPTY) {
        return;
    }
    pthread_mutex_lock(&spare_lock);
    int empty = SPARE_EMPTY;
    if (__atomic_compare_exchange_n(&spare_state, &empty, SPARE_FILLING, false,
                                    __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        spare_fiber = lugha_rc_internal(sizeof(Fiber));
        spare_stack = c_stack_new();
        __atomic_store_n(&spare_state, SPARE_READY, __ATOMIC_RELEASE);
    }
    pthread_mutex_unlock(&spare_lock);
}

/* This thread, made by C, calls back for the first time: its fiber and C
 * stack, kept until it exits. With `realtime`, it takes the spare when one
 * is ready, allocating nothing and taking no lock, and is listed at its
 * first other callback; otherwise, or with no spare, they are made now. */
static void foreign_attach(Worker *w, bool realtime) {
    w->foreign = true;
    __atomic_add_fetch(&foreign_live, 1, __ATOMIC_ACQ_REL);
    Fiber *f = NULL;
    char *stack = NULL;
    int ready = SPARE_READY;
    if (realtime && __atomic_compare_exchange_n(&spare_state, &ready, SPARE_TAKING, false,
                                                __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        f = spare_fiber;
        stack = spare_stack;
        __atomic_store_n(&spare_state, SPARE_EMPTY, __ATOMIC_RELEASE);
    } else {
        pthread_once(&foreign_key_once, foreign_key_make);
        foreign_list(w);
        f = lugha_rc_internal(sizeof(Fiber));
        stack = c_stack_new();
    }
    f->wake_at = -1;
    f->number = __atomic_add_fetch(&fibers_started, 1, __ATOMIC_RELAXED);
    w->running = f;
    c_stack_use(stack);
#if defined(OS_THREAD_AT_EXIT)
    // Before the thread-locals go, `w` among them.
    OS_THREAD_AT_EXIT(foreign_exit, w);
#else
    // Any value: the destructor runs only for a non-NULL one.
    pthread_setspecific(foreign_key, w);
#endif
}

/* `lugha_rt_cb_run(body, frame)`: every C entry into Lugha runs its body
 * through this. On the program's own threads (and nested, already on the
 * C stack) in place; on a thread C made, on that thread's C stack. */
void lugha_rt_cb_run(void (*body)(void *), void *frame) {
    Worker *w = worker;
    if (w->runtime_thread || on_helper || w->on_c_stack) {
        body(frame);
        return;
    }
    if (!w->foreign) {
        foreign_attach(w, false);
    } else if (!w->foreign_listed) {
        // It took the spare for a real-time callback: this one may allocate.
        foreign_list(w);
    }
    w->on_c_stack = true;
    lugha_rt_on_stack(body, frame, w->c_stack_top);
    w->on_c_stack = false;
}

/* `lugha_rt_cb_run_realtime(body, frame)`: a real-time callback's entry,
 * as `lugha_rt_cb_run`, but a thread C made attaches from the spare. */
void lugha_rt_cb_run_realtime(void (*body)(void *), void *frame) {
    Worker *w = worker;
    if (w->runtime_thread || on_helper || w->on_c_stack) {
        body(frame);
        return;
    }
    if (!w->foreign) {
        foreign_attach(w, true);
    }
    w->on_c_stack = true;
    lugha_rt_on_stack(body, frame, w->c_stack_top);
    w->on_c_stack = false;
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
 * `env`, and true; false once its scope's body has ended. Takes no lock:
 * while this call is counted in `uses`, the record's own hold keeps `env`
 * alive, so retaining it is safe. */
int32_t lugha_rt_cb_hold(void *pinned, void **env) {
    CbRecord *r = pinned;
    uint64_t seen = __atomic_add_fetch(&r->uses, 1, __ATOMIC_ACQ_REL);
    bool live = (seen & CB_STOPPED) == 0;
    if (live) {
        lugha_rt_rc_retain(r->env);
        *env = r->env;
    }
    seen = __atomic_sub_fetch(&r->uses, 1, __ATOMIC_ACQ_REL);
    cb_drop_if_last(r, seen);
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
static void panic_entry(void *code, void *env, void *result);

void lugha_rt_cb_queue(void *pinned, void (*entry)(void *, void *, void *), void *args,
                       int32_t pins) {
    CbRecord *r = pinned;
    lock_take(&cb_lock);
    if (r->scope == NULL) {
        lock_give(&cb_lock);
        lugha_rt_rc_release(args);
        return;
    }
    // A pinned handle belongs to C's thread, where no task runs.
    if (pins != 0 && worker->foreign) {
        lugha_rt_rc_release(args);
        const char *why = "a spawned callback taking a pinned handle can't run as a task from a thread C made: make it `direct`";
        LughaString *text = string_of(why, (int64_t)strlen(why));
        lugha_rt_rc_mark(text);
        callback_task(r->scope, panic_entry, text, false);
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
