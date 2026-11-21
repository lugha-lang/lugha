/*
 * spawn.c — part of lugha_rt.c: starting, joining and pausing green
 * threads: `spawn`, `join`, `task.yield_now()`, `task.sleep(ms)`
 * and the preemption check generated code calls.
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it right after
 * tasks.c, whose fibers and scheduler it uses.
 */

/* Starting the other workers, and waking an idle one (workers.c). */
static void workers_start(void);
static void wake_idle_worker(void);

/* Where a new fiber starts: runs its call, stores the result, wakes its
 * joiners, and never comes back. */
static void fiber_exit(Fiber *self);
static void cc_task_done(Fiber *f);
static void cc_maybe_collect(void);
static void scope_task_done(Fiber *f);
static void woke(int64_t fired);
void lugha_rt_cancel_point(void);

/* The visit of a `Task`: what its result holds, once it has one; its
 * result's block goes with it. */
static void rc_task_visit(void *object, int64_t op) {
    LughaTask *task = object;
    if (task_done(task) && task->result_visit != NULL) {
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
 * task, an actor's handle) is let go of, for `LUGHA_RC_REPORT`'s count.
 * What only its stack holds stays: stopping doesn't unwind. Without the
 * report nothing is let go of: the process ends at once, and a thread
 * still running on another worker would find its task freed under it. */
void lugha_rt_rc_stop_threads(void) {
    RT_SECTION();
    // The entry has let go of its values: its cycles are freed, so the
    // report counts only what the program lost.
    if (rc_reporting || cc_sites_on) {
        cc_task_done(current != NULL ? current : &main_fiber);
    }
    if (current == NULL || !rc_reporting) {
        return;
    }
    lock_take(&live_lock);
    for (Fiber *f = live; f != NULL; f = f->next_live) {
        if (f == current || f == &main_fiber) {
            continue;
        }
        lugha_rt_rc_release(f->env);
        f->env = NULL;
        lugha_rt_rc_release(f->task);
        f->task = NULL;
    }
    // Held while letting go: a fiber ending meanwhile on another worker
    // unlinks itself. What these release never ends a fiber.
    lock_give(&live_lock);
}


__attribute__((used, visibility("hidden"))) void lugha_rt_task_fiber_main(Fiber *self) {
    landed();
    // A worker's idle loop has no task (workers.c).
    self->entry(self->code, self->env, self->task == NULL ? NULL : self->task->result);
    fiber_exit(self);
}

/* Finishes the running fiber: its task is done, its joiners wake, and the
 * next fiber runs; never returns. An actor's panic ends its fiber here too. */
static void fiber_exit(Fiber *self) {
    LughaTask *task = self->task;
    /* The call's environment was the fiber's to let go of. */
    lugha_rt_rc_release(self->env);
    self->env = NULL;
    // Its values are all let go of: the cycles they left are freed now.
    cc_task_done(self);
    // Done, under the record's lock: its joiners park holding it, so each
    // is parked by now. Done before the scope counts it out: cancelling
    // walks the scope's tasks under the scope's lock and skips done ones,
    // so it never reaches this fiber once it is gone.
    lock_take(&task->lock);
    __atomic_store_n(&task->done, 1, __ATOMIC_RELEASE);
    for (Fiber *j = task->joiners; j != NULL;) {
        Fiber *after = j->wait_next;
        wake(j, 0);
        j = after;
    }
    task->joiners = NULL;
    lock_give(&task->lock);
    scope_task_done(self);
    lock_take(&live_lock);
    if (self->prev_live != NULL) {
        self->prev_live->next_live = self->next_live;
    } else {
        live = self->next_live;
    }
    if (self->next_live != NULL) {
        self->next_live->prev_live = self->prev_live;
    }
    lock_give(&live_lock);
    __atomic_sub_fetch(&live_count, 1, __ATOMIC_RELAXED);
    /* Its joiners hold the task themselves now. */
    lugha_rt_rc_release(task);
    worker->zombie = self;
    switch_away(false, false);
    die("panic: internal error: a finished green thread ran again\n");
}

/* `f`'s first frame on its stack, which the first switch to it pops,
 * landing in `lugha_rt_task_boot`. */
static void fiber_frame(Fiber *f) {
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
#if defined(__SANITIZE_THREAD__)
    f->tsan = __tsan_create_fiber(0);
#endif
}

/* A new green thread running `entry(code, env, result)`, not yet queued:
 * no worker runs it until `task_go`. */
static LughaTask *task_create(void (*entry)(void *, void *, void *), void *code, void *env,
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
    f->top = stack_top(f->map);
    budget_fill(f->map);
    f->entry = entry;
    f->code = code;
    f->env = env;
    f->task = task;
    f->number = __atomic_add_fetch(&fibers_started, 1, __ATOMIC_RELAXED);
    f->wake_at = -1;
    fiber_frame(f);
    lock_take(&live_lock);
    f->next_live = live;
    live->prev_live = f;
    live = f;
    lock_give(&live_lock);
    __atomic_add_fetch(&live_count, 1, __ATOMIC_RELAXED);
    task->fiber = f;
    return task;
}

/* Queues a task made by `task_create`: the spawner runs on, so another
 * worker may run the new task now. */
static void task_go(LughaTask *task) {
    make_ready(task->fiber);
    workers_start();
    wake_idle_worker();
}

/* Lets every ready thread run first, if any is (no section open). */
static void yield_now(void) {
    switch_away(false, true);
}

/* `lugha_rt_task_go(task)`: generated code's start of a task it finished
 * setting up (its result's visit, its name, an actor's record). */
void lugha_rt_task_go(LughaTask *task) {
    RT_SECTION();
    task_go(task);
}

/* `spawn`: a new green thread running `entry(code, env, result)`,
 * queued behind the ready ones; the caller keeps running. */
LughaTask *lugha_rt_task_spawn(void (*entry)(void *, void *, void *), void *code, void *env,
                               int64_t result_size) {
    RT_SECTION();
    LughaTask *task = task_create(entry, code, env, result_size);
    task_go(task);
    return task;
}

/* `t.join()`: waits until the task's call has finished; its result. */
void *lugha_rt_task_join(LughaTask *task) {
    ensure_started();
    lock_take(&task->lock);
    if (task->done) {
        lock_give(&task->lock);
        return task->result;
    }
    current->wait_next = task->joiners;
    task->joiners = current;
    uint32_t *locks[1] = {&task->lock};
    block_holding(locks, 1);
    return task->result;
}

/* `task.yield_now()`: lets every ready thread run first. */
void lugha_rt_task_yield(void) {
    ensure_started();
    lugha_rt_cancel_point();
    yield_now();
    lugha_rt_cancel_point();
}

/* `task.sleep(ms)`: pauses this thread; others run. */
void lugha_rt_task_sleep(int64_t ms) {
    RT_SECTION();
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
    // Only on a stack of the runtime's: a C harness's `main` runs on the
    // process's own, which has no budget word.
    if (current == NULL || current->map != NULL || worker->on_c_stack) {
        *budget_of((uintptr_t)__builtin_frame_address(0)) = TASK_BUDGET;
    }
    // A C callback runs on the C stack, which no other task may take.
    if (worker->on_c_stack) {
        return;
    }
    // A scheduling point: the task's cycles, if it has gathered enough.
    cc_maybe_collect();
    lugha_rt_cancel_point();
    yield_now();
    lugha_rt_cancel_point();
}
