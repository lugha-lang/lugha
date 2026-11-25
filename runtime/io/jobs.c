/*
 * jobs.c — part of lugha_rt.c: the scheduler's side of the helper threads
 * (helpers.c). A task giving a job to a helper parks until the job is
 * done; at most `JOBS_MOST` are given at once, and a task wanting to give
 * one more waits its turn. Finished jobs come back through the scheduler's
 * `epoll` set, which this part owns and net.c's sockets share: the poll
 * that wakes tasks for sockets wakes the jobs' tasks too.
 *
 * A task cancelled while waiting unwinds at once; its job runs on, and its
 * results are freed when it comes back. In a deterministic run, before
 * lockdown, and in a C callback (which can't wait), a job runs inline.
 *
 * A fragment, not compiled on its own: joined after tasks.c (fibers,
 * `block`, `wake`), stacks.c (`worker->on_c_stack`), scopes.c (`REASON_NONE`) and
 * unwind.c (`cancel_unwind`), before net.c.
 */

/* Jobs given to helpers and not yet taken back, at most. */
enum { JOBS_MOST = 256 };

static int64_t jobs_out;
/* Tasks waiting for room to give a job, first come first served. */
static Fiber *room_head;
static Fiber *room_tail;

/* Unused on Windows, whose parts that call it aren't built yet. */
__attribute__((unused)) static void io_open(void) {
    os_poll_open();
}

/* The helpers' signal, in the poll, made once. */
static void helper_event_open(void) {
    if (helper_event >= 0) {
        return;
    }
    helper_event = os_signal_new(OS_TOKEN_HELPERS);
    if (helper_event < 0) {
        die("panic: can't make the helper threads' event\n");
    }
}

/* The workers' wake-up signal, in the poll: raised to wake the worker
 * waiting in it (workers.c), and from the Ctrl-C handler. */
static OsSignal wake_fd = -1;

static void wake_open(void) {
    if (wake_fd >= 0) {
        return;
    }
    OsSignal s = os_signal_new(OS_TOKEN_WAKE);
    if (s < 0) {
        die("panic: can't make the workers' wake-up event\n");
    }
    wake_fd = s;
}

/* Wakes the worker waiting in the poll; safe in a signal handler. */
static void poller_kick(void) {
    if (wake_fd >= 0) {
        os_signal_raise(wake_fd);
    }
}

/* The wake-up signal was seen: lowered, so the next one is seen too. */
static void wake_taken(void) {
    os_signal_take(wake_fd);
}

/* Takes every finished job back: wakes its task, or frees it when its task
 * stopped waiting; each one taken makes room for a task waiting to give. */
static void jobs_collect(void) {
    os_signal_take(helper_event);
    Job *job = helpers_finished();
    while (job != NULL) {
        Job *next = job->next;
        __atomic_sub_fetch(&jobs_out, 1, __ATOMIC_ACQ_REL);
        if (job->hold != NULL) {
            lugha_rt_rc_release(job->hold);
            job->hold = NULL;
        }
        if (job->waiter != NULL) {
            Fiber *f = job->waiter;
            job->waiter = NULL;
            wake(f, 0);
        } else {
            job_free(job);
        }
        if (room_head != NULL) {
            Fiber *f = room_head;
            room_head = f->wait_next;
            if (room_head == NULL) {
                room_tail = NULL;
            }
            f->wait_next = NULL;
            wake(f, 0);
        }
        job = next;
    }
}

/* Takes `current` off the room queue (cancelled while waiting there). */
static void room_leave(void) {
    Fiber *before = NULL;
    for (Fiber *f = room_head; f != NULL; before = f, f = f->wait_next) {
        if (f == current) {
            if (before == NULL) {
                room_head = f->wait_next;
            } else {
                before->wait_next = f->wait_next;
            }
            if (room_tail == f) {
                room_tail = before;
            }
            f->wait_next = NULL;
            return;
        }
    }
}

/* Parks the current task until there is room to give a job; frees `job`
 * and unwinds when cancelled first, unless the job holds the task. */
static void room_wait(Job *job) {
    while (jobs_out >= JOBS_MOST) {
        current->wait_next = NULL;
        if (room_tail == NULL) {
            room_head = current;
        } else {
            room_tail->wait_next = current;
        }
        room_tail = current;
        current->cancellable = !job->holds_task;
        int64_t why = block();
        current->cancellable = false;
        if (why == CANCELLED) {
            room_leave();
            job_free(job);
            cancel_unwind();
        }
    }
}

static void job_run(Job *job) {
    if (!helpers_allowed || virtual_time() || worker->on_c_stack) {
        job->run(job);
        return;
    }
    RT_SECTION();
    ensure_started();
    if (!job->holds_task && current->cancelled != REASON_NONE && !current->unwinding) {
        job_free(job);
        cancel_unwind();
    }
    room_wait(job);
    if (job->hold != NULL) {
        // Let go of when the job comes back, by whichever task collects it.
        lugha_rt_rc_mark(job->hold);
        lugha_rt_rc_retain(job->hold);
    }
    job->waiter = current;
    __atomic_add_fetch(&jobs_out, 1, __ATOMIC_ACQ_REL);
    helper_event_open();
    helper_submit(job);
    current->cancellable = !job->holds_task;
    for (;;) {
        int64_t why = block();
        if (job->waiter == NULL) {
            break;
        }
        if (why == CANCELLED) {
            // The job is the helper's now; `jobs_collect` frees it.
            job->waiter = NULL;
            current->cancellable = false;
            cancel_unwind();
        }
    }
    current->cancellable = false;
}

/* On a helper: a `blocking` extern's thunk, on the helper's own stack. */
static void c_blocking_job(Job *job) {
    job->call(job->frame);
}

/* `lugha_rt_c_blocking(thunk, frame)`: generated code's call of a
 * `blocking` extern. `thunk(frame)` runs on a helper thread while the task
 * waits, uncancellably: the frame and what its pointers reach are the
 * task's. Where a helper can't be used it runs as any C call does. */
void lugha_rt_c_blocking(void (*thunk)(void *), void *frame) {
    if (!helpers_allowed || virtual_time() || worker->on_c_stack) {
        lugha_rt_c_call(thunk, frame);
        return;
    }
    Job *job = job_new(c_blocking_job);
    job->call = thunk;
    job->frame = frame;
    job->holds_task = true;
    job_run(job);
    job_free(job);
}
