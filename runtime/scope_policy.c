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
 *
 * A fragment, not compiled on its own: joined after scopes.c.
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

/* `s.spawn(…)` under `limit n`: waits while `n` of its tasks run. */
static void scope_room(LughaScope *s) {
    while (s->limit > 0 && s->running - s->actors >= s->limit) {
        lugha_rt_cancel_point();
        s->limit_waiter = current;
        current->cancellable = true;
        int64_t fired = block();
        if (s->limit_waiter == current) {
            s->limit_waiter = NULL;
        }
        woke(fired);
    }
}

/* `t.join()`: waits for `t`; how it ended (`TASK_*`), with what goes with
 * it: the reason and text, the error, or the message (each a new holder). */
int32_t lugha_rt_task_join_status(LughaTask *t, int32_t *reason, LughaString **text,
                                  void **table, void **box) {
    RT_SECTION();
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
    RT_SECTION();
    if (!t->done) {
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
