/*
 * scope_policy.c — part of lugha_rt.c: what a scope does with its tasks'
 * failures, `limit n`, and what a task's handle does (`join`, `cancel`,
 * `name`).
 *
 * A scoped task's entry (generated code) catches its own unwind: a panic
 * or cancellation reaching it is taken over here (`lugha_rt_task_caught`),
 * so the failure is built as a Lugha `Failure` and handed on as its
 * scope's policy says (`lugha_rt_task_policy`): failing an all-or-nothing
 * scope; printed, or given to the handler, by an `independent` one (the
 * scope runs on); printed, then failing the root scope, for a root task.
 * The root scope, `main`'s, waits for its tasks when `main` returns or
 * unwinds, and a failure there ends the program.
 *
 * A fragment, not compiled on its own: joined after scopes.c and scope_unwind.c.
 */

enum {
    /* The first failure cancels the rest and is the scope's value. */
    POLICY_ALL = 0,
    /* `independent`: each failure printed; the scope runs on. */
    POLICY_PRINT = 1,
    /* `independent(f)`: each failure given to `f`. */
    POLICY_HANDLER = 2,
    /* The root scope: each failure printed, then failing it. */
    POLICY_ROOT = 3,
};

/* What the current task's failure does: a `POLICY_*`. */
int32_t lugha_rt_task_policy(void) {
    RT_SECTION();
    LughaScope *s = current->started_in;
    if (s == &root_scope) {
        return POLICY_ROOT;
    }
    return s->policy;
}

/* The handler of the current task's scope: its code and environment. */
void lugha_rt_task_handler(void **code, void **env) {
    RT_SECTION();
    LughaScope *s = current->started_in;
    *code = s->handler_code;
    *env = s->handler_env;
}

/* The current task's name, a new holder. */
LughaString *lugha_rt_task_own_name(void) {
    RT_SECTION();
    LughaString *name = current->task->name;
    lugha_rt_rc_retain(name);
    return name;
}

/* A panic or cancellation reached the current task's entry, which stops
 * it: 1 for a cancellation (the task ends cancelled), 2 for a panic, its
 * message in `text` (a new holder). */
int32_t lugha_rt_task_caught(LughaString **text) {
    RT_SECTION();
    Panic *caught = current->in_flight;
    current->in_flight = NULL;
    current->unwinding = false;
    LughaTask *t = current->task;
    int32_t how = 2;
    *text = NULL;
    if (caught == NULL || caught->cancel) {
        t->outcome = TASK_CANCELLED;
        t->reason = current->cancelled;
        t->text = current->cancel_text;
        current->cancel_text = NULL;
        how = 1;
    } else {
        *text = string_of(caught->text, (int64_t)caught->len);
    }
    if (caught != NULL) {
        free(caught->text);
        free(caught);
    }
    // An actor's callers and queued messages learn how it ended.
    actor_task_ended(how == 2);
    return how;
}

/* The current task panicked with `text` (handed over): its outcome, and,
 * when `fails`, its scope's failure. */
void lugha_rt_task_panicked(LughaString *text, int32_t fails) {
    RT_SECTION();
    LughaTask *t = current->task;
    t->outcome = TASK_PANICKED;
    t->text = text;
    // Its joiners read the message.
    lugha_rt_rc_mark(text);
    if (fails) {
        lugha_rt_rc_retain(text);
        scope_fail(current->started_in, TASK_PANICKED, NULL, NULL, text, t->name);
    }
}

/* `s.spawn(…)` under `limit n`: waits while `n` of its tasks run. Called
 * and returns holding the scope's lock, parked holding it. */
static void scope_room(LughaScope *s) {
    while (s->limit > 0 && s->running - s->actors >= s->limit) {
        uint32_t *locks[1] = {&s->lock};
        may_wait(locks, 1);
        s->limit_waiter = current;
        current->cancellable = true;
        int64_t fired = block_holding(locks, 1);
        lock_take(&s->lock);
        if (s->limit_waiter == current) {
            s->limit_waiter = NULL;
        }
        current->cancellable = false;
        if (fired == CANCELLED) {
            // Unwinding takes the scopes lock, which comes before this one.
            lock_give(&s->lock);
            woke(fired);
        }
    }
}

/* `t.join()`: waits for `t`; how it ended (`TASK_*`), with what goes with
 * it: the reason and text, the error, or the message (each a new holder). */
int32_t lugha_rt_task_join_status(LughaTask *t, int32_t *reason, LughaString **text,
                                  void **table, void **box) {
    ensure_started();
    lugha_rt_cancel_point();
    lock_take(&t->lock);
    if (t->done) {
        lock_give(&t->lock);
    } else {
        uint32_t *locks[1] = {&t->lock};
        may_wait(locks, 1);
        current->wait_next = t->joiners;
        t->joiners = current;
        current->cancellable = true;
        int64_t fired = block_holding(locks, 1);
        if (fired == CANCELLED) {
            lock_take(&t->lock);
            for (Fiber **at = &t->joiners; *at != NULL; at = &(*at)->wait_next) {
                if (*at == current) {
                    *at = current->wait_next;
                    break;
                }
            }
            lock_give(&t->lock);
        }
        woke(fired);
    }
    // How it ended was set before `done`, and never changes after.
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
    RT_SECTION();
    if (!task_done(t)) {
        cancel_fiber(t->fiber, REASON_REQUESTED, why, NULL);
    }
}

/* `t.name(text)`. */
void lugha_rt_task_name(LughaTask *t, LughaString *name) {
    RT_SECTION();
    // Read by its scope and joiners, in its failures.
    lugha_rt_rc_mark(name);
    lugha_rt_rc_retain(name);
    lugha_rt_rc_release(t->name);
    t->name = name;
}

/* The root scope, which `main` and tests take. */
void *lugha_rt_scope_root(void) {
    RT_SECTION();
    ensure_started();
    root_scope.owner = &main_fiber;
    return &root_scope;
}

/* True if a root-scope task failed. */
static int32_t root_scope_failed(void) {
    return root_scope.failed != TASK_OK;
}

/* Waits for the root scope's tasks, and lets go of them and of its
 * failure, once reported. */
static void root_scope_wait(void) {
    scope_wait(&root_scope);
    lock_take(&root_scope.lock);
    LughaTask *done = root_scope.tasks;
    root_scope.tasks = NULL;
    lock_give(&root_scope.lock);
    for (LughaTask *t = done; t != NULL;) {
        LughaTask *next = t->scope_next;
        lugha_rt_rc_release(t);
        t = next;
    }
    lugha_rt_rc_release(root_scope.error_box);
    lugha_rt_rc_release(root_scope.panic);
    lugha_rt_rc_release(root_scope.failed_task);
    root_scope.error_box = NULL;
    root_scope.panic = NULL;
    root_scope.failed_task = NULL;
}

/* `main` unwound: when cancelled because a root-scope task failed, the
 * other tasks, cancelled too, stop before the program ends. */
void lugha_rt_scope_root_unwound(void) {
    callbacks_stop(&root_scope);
    RT_SECTION();
    if (current != NULL && current->cancelled != REASON_NONE) {
        current->unwinding = false;
        root_scope_wait();
        current->unwinding = true;
    }
}

/* When `main` returns: its scope's tasks finish first. True if one failed:
 * the program then ends with 101, as after an unhandled panic. */
int32_t lugha_rt_scope_root_finish(void) {
    callbacks_stop(&root_scope);
    RT_SECTION();
    if (current == NULL) {
        return 0;
    }
    root_scope_wait();
    return root_scope_failed();
}
