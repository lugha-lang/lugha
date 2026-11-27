/*
 * exports.c — part of lugha_rt.c: functions C calls by name, `extern fun
 * name(…) { … }`. Each C entry runs its body through `lugha_rt_export_run`:
 *
 * - From a thread C made, the body runs as a task, so it may wait (scopes,
 *   channels, sleep), while C's thread waits for it on a futex word. A
 *   panic ends the task, which has no scope, and with it the process.
 * - From the program's own threads (a task calling C that calls back by
 *   name) it runs in place, on the C stack, as a direct callback does.
 * - A caller may catch a call's panic (`lugha_rt_export_catch`, as the
 *   glue Java calls through does): the task unwinds as any, quietly, and
 *   ends there; the caller takes the message (`lugha_rt_export_caught`)
 *   instead of the process ending.
 *
 * In a shared library there is no `main`: the first call starts the
 * runtime, once, on a home thread of its own, which becomes the first
 * worker and runs a `main` task that only ever sleeps. A library leaves
 * the process alone: no signal handlers, no Ctrl-C handler, no lockdown,
 * stdio as the host set it. Its constants are made by its first call
 * (`lugha_rt_export_init`). In a program, `main` started the runtime
 * already, and its exports go straight to the workers.
 *
 * A fragment, not compiled on its own: joined after callbacks.c, whose
 * `foreign_attach` a thread C made goes through first.
 */

/* One call from C: the body's thunk and frame, whether it is done, and
 * whether its caller catches a panic, whose message it then holds. */
typedef struct {
    void (*thunk)(void *);
    void *frame;
    uint32_t done;
    bool catches;
    char *panic;
} ExportCall;

/* Whether this thread's next export call is caught, and the message of
 * its last caught panic, until taken. */
static __thread bool export_catch_next;
static __thread char *export_panic;

/* The task running one call: the body, then C's thread woken. */
static void export_task(void *code, void *env, void *result) {
    (void)env;
    (void)result;
    ExportCall *call = code;
    call->thunk(call->frame);
    current->export_call = NULL;
    // C's thread may return as soon as it sees `done`: the wake only
    // names the word's address, which a later wait there takes as spurious.
    __atomic_store_n(&call->done, 1, __ATOMIC_RELEASE);
    os_wake_on(&call->done, 1);
}

/* Whether the runtime is up: `lugha_rt_init` or a library's start made
 * this thread or another a worker. */
static bool runtime_up(void) {
    pthread_mutex_lock(&workers_lock);
    bool up = workers != NULL;
    pthread_mutex_unlock(&workers_lock);
    return up;
}

/* Set once the home thread's runtime is up, its `main` task running. */
static uint32_t library_up;

/* A library's `main` task: once it runs (and with it the task list C's
 * calls join), C may call in; then it sleeps, while the workers run C's
 * calls. */
static int32_t library_main(int32_t argc, char **argv) {
    (void)argc;
    (void)argv;
    ensure_started();
    __atomic_store_n(&library_up, 1, __ATOMIC_RELEASE);
    os_wake_on(&library_up, INT32_MAX);
    for (;;) {
        lugha_rt_task_sleep(1000LL * 60 * 60 * 24);
    }
    // Never reached: the loop never ends.
    return 0;
}

/* The home thread: the runtime's start without the process-wide parts a
 * program's `lugha_rt_init` makes (signals, Ctrl-C, stdio, arguments),
 * then a `main` that never returns. */
static void *library_home(void *arg) {
    (void)arg;
    __atomic_store_n(&library_mode, true, __ATOMIC_RELEASE);
    lugha_rt_worker_enter();
    sched_init();
    workers_init();
    hash_key_init();
    helpers_init();
    lugha_rc_start();
    // Lockdown would let helpers start; a library has none, so they may.
    helpers_allowed = true;
    lugha_rt_main(0, NULL, library_main);
    return NULL;
}

static pthread_once_t library_once = PTHREAD_ONCE_INIT;

static void library_spawn(void) {
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
    pthread_attr_setstacksize(&attr, 256 * 1024);
    pthread_t home;
    if (pthread_create(&home, &attr, library_home, NULL) != 0) {
        die("panic: can't start the Lugha runtime's thread\n");
    }
    pthread_attr_destroy(&attr);
}

/* Starts the runtime for a shared library, once, unless a program's
 * `main` started it; returns once it is up. */
static void library_start(void) {
    if (__atomic_load_n(&library_up, __ATOMIC_ACQUIRE) != 0) {
        return;
    }
    // Up without the library's start: a program's `main` started it. Read
    // after `runtime_up`'s lock, which the home thread took after setting
    // `library_mode`, so a library starting meanwhile is seen as one.
    if (runtime_up() && !__atomic_load_n(&library_mode, __ATOMIC_ACQUIRE)) {
        return;
    }
    pthread_once(&library_once, library_spawn);
    while (__atomic_load_n(&library_up, __ATOMIC_ACQUIRE) == 0) {
        os_wait_on(&library_up, 0);
    }
}

/* `lugha_rt_export_run(thunk, frame)`: every export's call of its body. */
void lugha_rt_export_run(void (*thunk)(void *), void *frame) {
    Worker *w = worker;
    if (w->runtime_thread || on_helper || w->on_c_stack) {
        thunk(frame);
        return;
    }
    library_start();
    // A thread C made gets a fiber of its own, as for a callback, so the
    // runtime's calls below know where they are.
    if (!w->foreign) {
        foreign_attach(w, false);
    }
    ExportCall call = {thunk, frame, 0, export_catch_next, NULL};
    export_catch_next = false;
    LughaTask *task;
    {
        RT_SECTION();
        task = task_create(export_task, &call, NULL, 1);
        // Set before it can run: its panic looks here.
        task->fiber->export_call = &call;
        task_go(task);
    }
    lugha_rt_rc_release(task);
    while (__atomic_load_n(&call.done, __ATOMIC_ACQUIRE) == 0) {
        os_wait_on(&call.done, 0);
    }
    export_panic = call.panic;
}

/* `lugha_rt_export_catch()`: this thread's next export call is caught. */
void lugha_rt_export_catch(void) {
    export_catch_next = true;
}

/* `lugha_rt_export_caught()`: the message of the panic that ended this
 * thread's last caught call, the caller's to `free`; NULL when it ended
 * without one. */
char *lugha_rt_export_caught(void) {
    char *text = export_panic;
    export_panic = NULL;
    return text;
}

/* Whether the running task's panic is its caller's to catch. */
static bool export_catches(void) {
    ExportCall *call = current->export_call;
    return call != NULL && call->catches;
}

static void export_unwound(const char *text, size_t len) {
    if (current == NULL || !export_catches()) {
        return;
    }
    ExportCall *call = current->export_call;
    char *copy = malloc(len + 1);
    if (copy == NULL) {
        die("panic: out of memory\n");
    }
    memcpy(copy, text, len);
    copy[len] = '\0';
    call->panic = copy;
    current->export_call = NULL;
    current->unwinding = false;
    // The caller may return as soon as it sees `done`: nothing of the call
    // is touched after.
    __atomic_store_n(&call->done, 1, __ATOMIC_RELEASE);
    os_wake_on(&call->done, 1);
    // Never returns: the task ends, the lock going on to the next fiber.
    rt_enter();
    fiber_exit(current);
}

/* `lugha_rt_export_init(init)`: a shared library's constants, made by
 * its first call while any other waits; nothing in a program, whose
 * `main` made them. */
void lugha_rt_export_init(void (*init)(void)) {
    if (!__atomic_load_n(&library_mode, __ATOMIC_ACQUIRE)) {
        return;
    }
    static uint32_t state; // 0 not made, 1 being made, 2 made
    uint32_t none = 0;
    if (__atomic_compare_exchange_n(&state, &none, 1, false, __ATOMIC_ACQ_REL,
                                    __ATOMIC_ACQUIRE)) {
        init();
        __atomic_store_n(&state, 2, __ATOMIC_RELEASE);
        os_wake_on(&state, INT32_MAX);
        return;
    }
    while (__atomic_load_n(&state, __ATOMIC_ACQUIRE) != 2) {
        os_wait_on(&state, 1);
    }
}

/* `lugha_rt_export_string(s)`: a string an export gives C, as C's own
 * NUL-terminated copy, which C frees with `free()`. */
char *lugha_rt_export_string(const LughaString *s) {
    if (memchr(s->bytes, 0, (size_t)s->len) != NULL) {
        static const char why[] = "a string given to C can't contain a NUL byte";
        panic_with(why, sizeof why - 1);
    }
    char *copy = malloc((size_t)s->len + 1);
    if (copy == NULL) {
        die("panic: out of memory\n");
    }
    memcpy(copy, s->bytes, (size_t)s->len);
    copy[s->len] = '\0';
    return copy;
}
