/*
 * scope_time.c — part of lugha_rt.c: deadlines, shields and Ctrl-C.
 *
 * - A scope with a deadline (`within ms`, `until t`; the earlier of its own
 *   and its enclosing scope's) is on `deadlines` while it runs. When the
 *   deadline passes, the scope fails with `TASK_DEADLINE` (unless something
 *   failed first) and its tasks and body are cancelled with
 *   `REASON_DEADLINE`.
 * - `shield within ms { … }` holds cancellation off for its fiber until the
 *   block ends or the deadline passes: a cancellation arriving meanwhile
 *   waits, and a wait inside the shield isn't cut short until the shield's
 *   deadline.
 * - Ctrl-C sets a flag (all a signal handler may safely do); the scheduler
 *   then cancels the root scope's tasks and `main` with `REASON_SHUTDOWN`,
 *   and the program ends with 130 once `main` has unwound. A second Ctrl-C,
 *   or one before any task started, ends the program at once.
 *
 * The scheduler checks all three where it wakes sleepers, and a busy loop
 * at its preemption points (`check_times`); the idle wait sleeps no later
 * than the next of them (`next_time`).
 *
 * A fragment, not compiled on its own: joined after scope_policy.c.
 */

/* Scopes with a deadline, earliest anywhere, through `deadline_next`. */
static LughaScope *deadlines;

/* Fibers inside a shield, through `shield_next`. */
static Fiber *shielded_fibers;

/* How many Ctrl-Cs arrived, and whether the root scope was cancelled for one. */
static volatile sig_atomic_t interrupts;
static bool shutting_down;

/* Monotonic nanoseconds from milliseconds, without overflowing. */
static int64_t ms_to_ns(int64_t ms) {
    return ms > INT64_MAX / 2000000 ? INT64_MAX / 2 : ms * 1000000;
}

/* True while cancellation is held off for `f`. */
static bool shielded(Fiber *f) {
    return f->shield_depth > 0 && now_ns() < f->shield_until;
}

/* True if the running fiber, already unwinding, may unwind again: a
 * shield entered while it unwound passed its deadline, and the shield's
 * landing pad will stop that unwind and give the first one back. */
static bool shield_cut_off(void) {
    return current != NULL && current->shield_depth > 0 && current->shield_in_unwind &&
           current->shield_outer == NULL && current->cancelled != REASON_NONE &&
           !shielded(current);
}

/* `s` starts with its own deadline (`within` ms from now, or `until`
 * monotonic ms; -1 for none) or its enclosing scope's, whichever is
 * earlier. */
static void deadline_start(LughaScope *s, int64_t within, int64_t until) {
    int64_t own = 0;
    if (within >= 0) {
        own = now_ns() + ms_to_ns(within);
    } else if (until >= 0) {
        own = ms_to_ns(until);
    }
    LughaScope *outer = s->outer != NULL ? s->outer : current->started_in;
    int64_t inherited = outer != NULL ? outer->deadline : 0;
    if (own == 0 || (inherited != 0 && inherited < own)) {
        own = inherited;
    }
    s->deadline = own;
    if (own != 0) {
        s->deadline_next = deadlines;
        deadlines = s;
    }
}

/* `s` ends: off the deadlines. */
static void deadline_stop(LughaScope *s) {
    for (LughaScope **at = &deadlines; *at != NULL; at = &(*at)->deadline_next) {
        if (*at == s) {
            *at = s->deadline_next;
            return;
        }
    }
}

/* Fires what is due: scopes past their deadline, waits in a shield past
 * its deadline with a cancellation held, and a Ctrl-C. */
static void check_times(void) {
    if (deadlines == NULL && shielded_fibers == NULL && interrupts == 0) {
        return;
    }
    int64_t now = now_ns();
    for (LughaScope **at = &deadlines; *at != NULL;) {
        LughaScope *s = *at;
        if (s->deadline > now) {
            at = &s->deadline_next;
            continue;
        }
        *at = s->deadline_next;
        if (s->failed == TASK_OK) {
            s->failed = TASK_DEADLINE;
            lugha_rt_rc_retain(s->name);
            s->failed_task = s->name;
        }
        cancel_tasks(s, REASON_DEADLINE);
        cancel_fiber(s->owner, REASON_DEADLINE, NULL, s);
    }
    for (Fiber *f = shielded_fibers; f != NULL; f = f->shield_next) {
        if (now >= f->shield_until && f->cancelled != REASON_NONE && f->fired == WAITING &&
            f->cancellable) {
            wake(f, CANCELLED);
        }
    }
    if (interrupts > 0 && !shutting_down) {
        shutting_down = true;
        cancel_tasks(&root_scope, REASON_SHUTDOWN);
        cancel_fiber(&main_fiber, REASON_SHUTDOWN, NULL, NULL);
    }
}

/* The next moment `check_times` has something to do (monotonic ns), or -1. */
static int64_t next_time(void) {
    int64_t next = -1;
    for (LughaScope *s = deadlines; s != NULL; s = s->deadline_next) {
        if (next < 0 || s->deadline < next) {
            next = s->deadline;
        }
    }
    for (Fiber *f = shielded_fibers; f != NULL; f = f->shield_next) {
        if (f->cancelled != REASON_NONE && (next < 0 || f->shield_until < next)) {
            next = f->shield_until;
        }
    }
    return next;
}

/* `s.remaining()`: the milliseconds left, at least 0, or -1 without a deadline. */
int64_t lugha_rt_scope_remaining(LughaScope *s) {
    if (s->deadline == 0) {
        return -1;
    }
    int64_t left = s->deadline - now_ns();
    return left <= 0 ? 0 : left / 1000000;
}

/* `shield within ms`: cancellation held off for at most `ms`, the earlier
 * deadline winning when shields nest. */
void lugha_rt_shield_enter(int64_t ms) {
    ensure_started();
    int64_t until = now_ns() + ms_to_ns(ms < 0 ? 0 : ms);
    if (current->shield_depth == 0) {
        current->shield_until = until;
        current->shield_next = shielded_fibers;
        current->shield_in_unwind = current->unwinding;
        shielded_fibers = current;
    } else if (until < current->shield_until) {
        current->shield_until = until;
    }
    current->shield_depth++;
}

/* The shield's block ends, however it ends. */
void lugha_rt_shield_exit(void) {
    if (current == NULL || current->shield_depth == 0) {
        return;
    }
    current->shield_depth--;
    if (current->shield_depth == 0) {
        for (Fiber **at = &shielded_fibers; *at != NULL; at = &(*at)->shield_next) {
            if (*at == current) {
                *at = current->shield_next;
                break;
            }
        }
        current->shield_until = 0;
        current->shield_in_unwind = false;
    }
}

/* An unwind reached the shield's landing pad, which stops it: a cut-off
 * of a shield entered while unwinding gives the first unwind back, which
 * goes on once the `defer` around the shield ends; any other goes on past
 * the shield (`lugha_rt_shield_end`). */
void lugha_rt_shield_caught(void) {
    Panic *caught = current->in_flight;
    if (current->shield_outer != NULL) {
        current->in_flight = current->shield_outer;
        current->shield_outer = NULL;
        free(caught->text);
        free(caught);
        return;
    }
    current->shield_caught = caught;
    current->in_flight = NULL;
    current->unwinding = false;
}

/* The shield was cut off while unwinding, and its value was lost with it:
 * there is none to go on with, so the program ends, as for a panic while
 * unwinding. */
void lugha_rt_shield_lost(void) {
    if (current->shield_caught == NULL) {
        die("panic: a shield with a value was cut off at its deadline while its task unwound\n");
    }
}

/* Past the shield: an unwind it stopped goes on, and a cancellation held
 * meanwhile applies now. */
void lugha_rt_shield_end(void) {
    Panic *caught = current->shield_caught;
    if (caught != NULL) {
        current->shield_caught = NULL;
        current->unwinding = true;
        resume_unwind(caught);
    }
    lugha_rt_cancel_point();
}

/* Ctrl-C: noted for the scheduler; the second, or one before any task
 * started (no scheduler to act on it), ends the program at once. */
static void on_interrupt(int sig) {
    (void)sig;
    interrupts++;
    if (interrupts > 1 || current == NULL) {
        _exit(130);
    }
}

/* Installs the Ctrl-C handler, as the runtime starts. */
static void install_interrupt_handler(void) {
    struct sigaction action;
    memset(&action, 0, sizeof action);
    action.sa_handler = on_interrupt;
    sigemptyset(&action.sa_mask);
    action.sa_flags = SA_RESTART;
    sigaction(SIGINT, &action, NULL);
}
