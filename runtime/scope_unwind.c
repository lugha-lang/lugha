/*
 * scope_unwind.c — part of lugha_rt.c: how a scoped task ends by failing
 * or unwinding, and the safe points where a cancelled task starts to
 * unwind.
 *
 * - A task returning `Err` fails with that error (`lugha_rt_task_failed`),
 *   failing its scope when the error is the scope's.
 * - A task unwound to its start ends cancelled, or, after a panic, fails
 *   its scope (`scope_task_unwound`).
 * - A cancelled task notices at a safe point (`lugha_rt_cancel_point`) or
 *   when a wait is cut short (`woke`), and unwinds unless shielded.
 *
 * A fragment, not compiled on its own: joined right after scopes.c, whose
 * records, failures and cancelling it uses.
 */

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
