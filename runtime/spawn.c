/*
 * spawn.c — part of lugha_rt.c: starting, joining and pausing green
 * threads: `spawn`, `join`, `task.yield_now()`, `task.sleep(ms)`
 * and the preemption check generated code calls.
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it right after
 * tasks.c, whose fibers and scheduler it uses.
 */

/* Where a new fiber starts: runs its call, stores the result, wakes its
 * joiners, and never comes back. */
static void fiber_exit(Fiber *self);
static void scope_task_done(Fiber *f);
static void woke(int64_t fired);
void lugha_rt_cancel_point(void);

/* The visit of a `Task`: what its result holds, once it has one; its
 * result's block goes with it. */
static void rc_task_visit(void *object, int64_t op) {
    LughaTask *task = object;
    if (task->done && task->result_visit != NULL) {
        task->result_visit(task->result, op);
    }
    // How it stopped and its name: a message or reason, an error's box.
    void *own[] = {task->text, task->error_box, task->name};
    for (size_t i = 0; i < sizeof own / sizeof own[0]; i++) {
        if (op == RC_MARK) {
            lugha_rt_rc_mark(own[i]);
        } else {
            lugha_rt_rc_release(own[i]);
        }
    }
    if (op == RC_RELEASE) {
        free(task->result);
    }
}

/* Generated code gives a new task its result's visit (none when the result holds nothing counted). */
void lugha_rt_rc_task_setup(LughaTask *task, RcVisit result_visit) {
    task->result_visit = result_visit;
}

/* When the entry returns, every unfinished thread stops, so what it holds as a thread (its call's environment, its
 * task, an actor's handle) is let go of. What only its stack holds stays:
 * stopping doesn't unwind. */
void lugha_rt_rc_stop_threads(void) {
    if (current == NULL) {
        return;
    }
    for (Fiber *f = live; f != NULL; f = f->next_live) {
        if (f == current || f == &main_fiber) {
            continue;
        }
        lugha_rt_rc_release(f->env);
        f->env = NULL;
        lugha_rt_rc_release(f->task);
        f->task = NULL;
    }
}


__attribute__((used, visibility("hidden"))) void lugha_rt_task_fiber_main(Fiber *self) {
    landed();
    self->entry(self->code, self->env, self->task->result);
    fiber_exit(self);
}

/* Finishes the running fiber: its task is done, its joiners wake, and the
 * next fiber runs; never returns. An actor's panic ends its fiber here too. */
static void fiber_exit(Fiber *self) {
    LughaTask *task = self->task;
    scope_task_done(self);
    task->done = 1;
    /* The call's environment was the fiber's to let go of. */
    lugha_rt_rc_release(self->env);
    self->env = NULL;
    for (Fiber *j = task->joiners; j != NULL;) {
        Fiber *after = j->wait_next;
        wake(j, 0);
        j = after;
    }
    task->joiners = NULL;
    if (self->prev_live != NULL) {
        self->prev_live->next_live = self->next_live;
    } else {
        live = self->next_live;
    }
    if (self->next_live != NULL) {
        self->next_live->prev_live = self->prev_live;
    }
    live_count--;
    /* Its joiners hold the task themselves now. */
    lugha_rt_rc_release(task);
    worker->zombie = self;
    schedule();
    die("panic: internal error: a finished green thread ran again\n");
}

/* `spawn`: a new green thread running `entry(code, env, result)`,
 * queued behind the ready ones; the caller keeps running. */
LughaTask *lugha_rt_task_spawn(void (*entry)(void *, void *, void *), void *code, void *env,
                               int64_t result_size) {
    ensure_started();
    LughaTask *task = lugha_rt_alloc((int64_t)sizeof(LughaTask));
    /* The result's block is the task's own, freed with it. The new
     * fiber holds the task until it ends, and two tasks hold it now. */
    task->result = lugha_rc_internal((size_t)(result_size < 1 ? 1 : result_size));
    lugha_rt_rc_set_drop(task, rc_task_visit);
    lugha_rt_rc_retain(task);
    lugha_rt_rc_mark(task);
    Fiber *f = lugha_rc_internal(sizeof(Fiber));
    f->map = new_stack();
    f->top = f->map + stack_size;
    f->entry = entry;
    f->code = code;
    f->env = env;
    f->task = task;
    f->number = ++fibers_started;
    f->wake_at = -1;
#if defined(__x86_64__)
    /* The first switch pops these: MXCSR and x87 defaults, then r15, r14,
     * r13, r12 (the fiber), rbx and rbp in slots 1 to 6, then returns into
     * lugha_rt_task_boot with %rsp 16-aligned for its call. */
    uint64_t *frame = (uint64_t *)(f->top - 16) - 8;
    frame[0] = 0x1f80 | ((uint64_t)0x037f << 32);
    frame[1] = 0;
    frame[2] = 0;
    frame[3] = 0;
    frame[4] = (uint64_t)(uintptr_t)f;
    frame[5] = 0;
    frame[6] = 0;
    frame[7] = (uint64_t)(uintptr_t)lugha_rt_task_boot;
#else
    /* AArch64: the switch's 176-byte frame, all zero (FPCR 0
     * is round-to-nearest, no traps) but x19 (slot 0) = the fiber and x30
     * (slot 11) = boot, the address its `ret` takes; `sp` ends at `top`, 16-byte aligned. */
    uint64_t *frame = (uint64_t *)(f->top - 176);
    memset(frame, 0, 176);
    frame[0] = (uint64_t)(uintptr_t)f;
    frame[11] = (uint64_t)(uintptr_t)lugha_rt_task_boot;
#endif
    f->sp = frame;
    f->next_live = live;
    live->prev_live = f;
    live = f;
    live_count++;
    make_ready(f);
    worker->last_spawned = f;
    return task;
}

/* `t.join()`: waits until the task's call has finished; its result. */
void *lugha_rt_task_join(LughaTask *task) {
    ensure_started();
    if (!task->done) {
        current->wait_next = task->joiners;
        task->joiners = current;
        block();
    }
    return task->result;
}

/* `task.yield_now()`: lets every ready thread run first. */
void lugha_rt_task_yield(void) {
    ensure_started();
    lugha_rt_cancel_point();
    wake_due_sleepers();
    poll_ready_sockets();
    if (worker->ready_head == NULL) {
        return;
    }
    make_ready(current);
    schedule();
    lugha_rt_cancel_point();
}

/* `task.sleep(ms)`: pauses this thread; others run. */
void lugha_rt_task_sleep(int64_t ms) {
    ensure_started();
    if (ms <= 0) {
        lugha_rt_task_yield();
        return;
    }
    int64_t ns = ms > INT64_MAX / 1000000 ? INT64_MAX / 2 : ms * 1000000;
    lugha_rt_cancel_point();
    add_sleeper(current, now_ns() + ns);
    current->cancellable = true;
    woke(block());
}

/* Called by generated code when its countdown reaches zero. */
void lugha_rt_task_preempt(void) {
    lugha_rt_task_budget = TASK_BUDGET;
    // A C callback runs on the C stack, which no other task may take.
    if (worker->on_c_stack) {
        return;
    }
    check_times();
    lugha_rt_cancel_point();
    if (live_count > 1) {
        lugha_rt_task_yield();
    }
}
