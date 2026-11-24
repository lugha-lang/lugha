/*
 * helpers.c — part of lugha_rt.c: the helper threads, which run the
 * runtime's blocking work (files, reading input, name lookups, starting
 * and waiting for programs) while its task waits and the others run.
 *
 * A job is a C function and the memory it works on, all from `malloc`:
 * copies of its inputs, made by the task asking, and plain C results. A
 * helper never touches a counted object, the runtime's allocator or the
 * scheduler: only the worker turns results into Lugha values. Finished
 * jobs go on a locked list, and an `eventfd` in the scheduler's `epoll`
 * set says so (jobs.c takes them).
 *
 * Helpers start one at a time, when a job is queued and none is idle, up
 * to `LUGHA_HELPERS` (16 unless set), and never before lockdown: Landlock
 * limits a thread and those it starts later, not threads already running.
 * Each blocks every signal, so Ctrl-C reaches only the workers; a helper's
 * write to a pipe whose reader is gone is `EPIPE`, never SIGPIPE
 * (`os_no_sigpipe`).
 *
 * A fragment, not compiled on its own: joined after the core (lugha_rt.c),
 * whose includes (`<pthread.h>`, `<sys/eventfd.h>`, `<signal.h>`) it relies
 * on, and before every part that gives it work.
 */

/* The default and largest number of helpers, and each one's stack. */
enum { HELPERS_DEFAULT = 16, HELPERS_MOST = 256 };
#define HELPER_STACK (8 * 1024 * 1024)

typedef struct Job Job;
struct Job {
    /* On a helper: the blocking calls, from the inputs to the results. */
    void (*run)(Job *);
    /* Lets go of a result the free calls below don't cover and the caller
     * didn't take (a descriptor, a `getaddrinfo` list); NULL for none. */
    void (*drop)(Job *);
    Job *next;
    /* The task waiting (a `Fiber`), NULL once it stopped waiting. */
    void *waiter;
    /* A counted object the job's descriptor belongs to (a `Dir`), kept
     * alive while the job runs; released by the worker. */
    void *hold;
    /* A `blocking` extern's call: its thunk and frame. The task's wait for
     * it can't be cut short, since C may use what the task holds. */
    void (*call)(void *);
    void *frame;
    bool holds_task;
    /* Inputs: descriptors, numbers, copied text. */
    int fd;
    int fd2;
    int flags;
    int64_t n;
    char *path;
    char *path2;
    char *in;
    size_t in_len;
    char **argv;
    /* Results: an error number (0 for none), and whether it came from
     * looking a path up; a number, bytes, a descriptor (`fd2`). */
    int err;
    bool lookup;
    int64_t value;
    char *out;
    size_t out_len;
    char *out2;
    size_t out2_len;
    void *found;
};

/* Set once lockdown is applied: helpers may start. Before it, and in a
 * deterministic run, jobs run inline. */
static bool helpers_allowed;
static int helpers_limit = HELPERS_DEFAULT;
static OsSignal helper_event = -1;

/* Shared with the helpers, under `helper_lock`. */
static pthread_mutex_t helper_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t helper_wake = PTHREAD_COND_INITIALIZER;
static Job *queue_head;
static Job *queue_tail;
static int64_t queued;
static Job *finished;
static int helpers_started;
static int helpers_idle;

/* `LUGHA_HELPERS`, read at start: 1 to 256. */
static void helpers_init(void) {
    const char *wanted = getenv("LUGHA_HELPERS");
    if (wanted == NULL) {
        return;
    }
    char *end;
    errno = 0;
    long n = strtol(wanted, &end, 10);
    if (wanted[0] < '0' || wanted[0] > '9' || *end != '\0' || errno != 0 || n < 1 ||
        n > HELPERS_MOST) {
        die("panic: LUGHA_HELPERS must be from 1 to 256\n");
    }
    helpers_limit = (int)n;
}

/* A job running `run`, its fields empty. */
static Job *job_new(void (*run)(Job *)) {
    Job *job = calloc(1, sizeof *job);
    if (job == NULL) {
        die("panic: out of memory\n");
    }
    job->run = run;
    job->fd = -1;
    job->fd2 = -1;
    return job;
}

/* `len` bytes from `bytes` as a NUL-ended copy from `malloc`. */
static char *job_copy(const char *bytes, size_t len) {
    char *copy = malloc(len + 1);
    if (copy == NULL) {
        die("panic: out of memory\n");
    }
    memcpy(copy, bytes, len);
    copy[len] = '\0';
    return copy;
}

/* Frees a job and everything it still owns; what a caller took it set to
 * NULL first. */
static void job_free(Job *job) {
    if (job->drop != NULL) {
        job->drop(job);
    }
    if (job->argv != NULL) {
        for (char **a = job->argv; *a != NULL; a++) {
            free(*a);
        }
        free(job->argv);
    }
    free(job->path);
    free(job->path2);
    free(job->in);
    free(job->out);
    free(job->out2);
    free(job);
}

/* Grows `*buffer` (`*cap` bytes, `*len` used) to hold `more` bytes more. */
static void job_grow(char **buffer, size_t *cap, size_t len, size_t more) {
    if (len + more <= *cap) {
        return;
    }
    size_t bigger = *cap == 0 ? 4096 : *cap;
    while (bigger < len + more) {
        bigger *= 2;
    }
    char *grown = realloc(*buffer, bigger);
    if (grown == NULL) {
        die("panic: out of memory\n");
    }
    *buffer = grown;
    *cap = bigger;
}

/* True on a helper thread, where Lugha code must never run. */
static __thread bool on_helper;

/* At every C entry into Lugha (a callback, callbacks.c): on a helper
 * thread, running a `blocking` extern, Lugha code can't run, so the
 * program ends. No cleanups run, being on the wrong thread. */
static void callback_on_helper(void) {
    if (on_helper) {
        fflush(stdout);
        fputs("panic: a C callback ran on a helper thread, during a `blocking` extern call\n",
              stderr);
        _exit(101);
    }
}

/* A helper: takes queued jobs one at a time, forever. */
static void *helper_main(void *unused) {
    (void)unused;
    on_helper = true;
    for (;;) {
        pthread_mutex_lock(&helper_lock);
        helpers_idle++;
        while (queue_head == NULL) {
            pthread_cond_wait(&helper_wake, &helper_lock);
        }
        helpers_idle--;
        Job *job = queue_head;
        queue_head = job->next;
        if (queue_head == NULL) {
            queue_tail = NULL;
        }
        queued--;
        pthread_mutex_unlock(&helper_lock);

        job->run(job);

        pthread_mutex_lock(&helper_lock);
        job->next = finished;
        finished = job;
        pthread_mutex_unlock(&helper_lock);
        // Raised once per finished job; the worker lowers it on each wake.
        os_signal_raise(helper_event);
    }
    return NULL;
}

/* Starts one more helper, its signals blocked; under `helper_lock`. False
 * when the system refuses a thread. */
static bool helper_start(void) {
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
    pthread_attr_setstacksize(&attr, HELPER_STACK);
    // A new thread takes its creator's signal mask: everything blocked.
    sigset_t all, before;
    sigfillset(&all);
    pthread_sigmask(SIG_SETMASK, &all, &before);
    pthread_t thread;
    int failed = pthread_create(&thread, &attr, helper_main, NULL);
    pthread_sigmask(SIG_SETMASK, &before, NULL);
    pthread_attr_destroy(&attr);
    if (failed != 0) {
        return false;
    }
    helpers_started++;
    return true;
}

/* Queues `job` for a helper, starting one if every helper is busy. */
static void helper_submit(Job *job) {
    pthread_mutex_lock(&helper_lock);
    job->next = NULL;
    if (queue_tail == NULL) {
        queue_head = job;
    } else {
        queue_tail->next = job;
    }
    queue_tail = job;
    queued++;
    if (queued > helpers_idle && helpers_started < helpers_limit && !helper_start() &&
        helpers_started == 0) {
        pthread_mutex_unlock(&helper_lock);
        die("panic: can't start a helper thread\n");
    }
    pthread_cond_signal(&helper_wake);
    pthread_mutex_unlock(&helper_lock);
}

/* Every finished job, oldest last; the list is emptied. */
static Job *helpers_finished(void) {
    pthread_mutex_lock(&helper_lock);
    Job *done = finished;
    finished = NULL;
    pthread_mutex_unlock(&helper_lock);
    return done;
}

/* Runs `job` off the task's thread while the task waits (jobs.c): on
 * return it is done and its results are the caller's, who frees it with
 * `job_free`. A cancelled task unwinds instead, the job running on. */
static void job_run(Job *job);
