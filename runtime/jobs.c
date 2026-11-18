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
 * `block`, `wake`), stacks.c (`on_c_stack`), scopes.c (`REASON_NONE`) and
 * unwind.c (`cancel_unwind`), before net.c.
 */

/* Jobs given to helpers and not yet taken back, at most. */
enum { JOBS_MOST = 256 };

/* The scheduler's `epoll` set, made at the first socket wait or job. */
static int net_epoll = -1;
/* What the helpers' event carries in the set, telling it from a fiber. */
static char helper_tag;
static int64_t jobs_out;
/* Tasks waiting for room to give a job, first come first served. */
static Fiber *room_head;
static Fiber *room_tail;

static void io_open(void) {
    if (net_epoll < 0) {
        net_epoll = epoll_create1(EPOLL_CLOEXEC);
        if (net_epoll < 0) {
            die("panic: epoll_create1 failed\n");
        }
    }
}

/* The helpers' `eventfd`, in the set, made once. */
static void helper_event_open(void) {
    if (helper_event >= 0) {
        return;
    }
    io_open();
    helper_event = eventfd(0, EFD_CLOEXEC | EFD_NONBLOCK);
    struct epoll_event ev = {.events = EPOLLIN, .data = {.ptr = &helper_tag}};
    if (helper_event < 0 || epoll_ctl(net_epoll, EPOLL_CTL_ADD, helper_event, &ev) != 0) {
        die("panic: can't make the helper threads' event\n");
    }
}

/* Takes every finished job back: wakes its task, or frees it when its task
 * stopped waiting; each one taken makes room for a task waiting to give. */
static void jobs_collect(void) {
    uint64_t count;
    // Nothing to read is fine: one read takes every write since the last.
    while (read(helper_event, &count, sizeof count) < 0 && errno == EINTR) {
    }
    Job *job = helpers_finished();
    while (job != NULL) {
        Job *next = job->next;
        jobs_out--;
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
 * and unwinds when cancelled first. */
static void room_wait(Job *job) {
    while (jobs_out >= JOBS_MOST) {
        current->wait_next = NULL;
        if (room_tail == NULL) {
            room_head = current;
        } else {
            room_tail->wait_next = current;
        }
        room_tail = current;
        current->cancellable = true;
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
    if (!helpers_allowed || virtual_time() || on_c_stack) {
        job->run(job);
        return;
    }
    ensure_started();
    if (current->cancelled != REASON_NONE && !current->unwinding) {
        job_free(job);
        cancel_unwind();
    }
    room_wait(job);
    if (job->hold != NULL) {
        lugha_rt_rc_retain(job->hold);
    }
    job->waiter = current;
    jobs_out++;
    helper_event_open();
    helper_submit(job);
    current->cancellable = true;
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
