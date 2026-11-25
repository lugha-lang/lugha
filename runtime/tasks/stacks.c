/*
 * stacks.c — part of lugha_rt.c: task stacks and the stack limit.
 *
 * A task's stack is a large reservation (8 MiB, smaller on a small address
 * space) carved from regions of `REGION_STACKS` stacks, each region one
 * mapping, so a program's mappings stay few however many tasks it runs.
 * Only the pages a task touches use memory. A finished task's stack waits
 * in its worker's cache, up to `STACK_CACHE`, for the next task there,
 * pages and all; the worker gives those pages back (`MADV_DONTNEED`) when
 * it parks. Past the cache, a stack's pages go back at once and it waits
 * in a pool for any worker.
 * Stacks never move.
 *
 * Every stack Lugha code runs on (a task's, `main`'s, a worker's C stack)
 * is aligned to its size, a power of two, so the stack pointer alone says
 * where its stack begins and ends, on whatever thread it runs:
 *
 *   - There are no guard pages: every Lugha function's entry tests
 *     `(sp & lugha_rt_stack_mask) < STACK_RESERVE` and below it calls
 *     `lugha_rt_stack_low`, which panics with `stack overflow`. The panic
 *     unwinds, its frames' cleanups allowed down to `STACK_UNWIND_ROOM`; a
 *     second overflow there ends the program, as a panic while unwinding
 *     does.
 *   - The top 16 bytes hold the task's step budget, the countdown each
 *     function entry and loop iteration takes one from: at
 *     `(sp | lugha_rt_stack_mask) - 15`, so it moves with the task.
 *
 * A fragment, not compiled on its own: joined after tasks.c, whose fibers
 * it uses, and before spawn.c, which takes stacks from it.
 */

/* Room kept below the check, for the runtime's own functions and the
 * unwinder; and of it, what is left to an overflow's unwinding. */
#define STACK_RESERVE (256 * 1024)
#define STACK_UNWIND_ROOM (64 * 1024)
/* Stacks per mapping, and the smallest stack a small address space gets. */
#define REGION_STACKS 64
#define SMALLEST_STACK (1024 * 1024)

/* A stack's size less one, fixed by the first region: generated code finds
 * the running stack's bottom and top through it. */
uintptr_t lugha_rt_stack_mask;

/* A task stack's size, settled by the first region; the unused rest of
 * the newest region; and finished tasks' stacks, ready again. */
static size_t stack_size = 8 * 1024 * 1024;
static bool stacks_sized;
static char *region_next;
static int region_left;
static char **free_stacks;
static size_t free_count;
static size_t free_capacity;

/* The pool's own lock: stacks are taken under `rt_lock` (spawning) and
 * given back without it (by the fiber landing after a finished one). */
static uint32_t pool_lock;

static char *new_stack_locked(void);

/* Stacks whose pages went back to the kernel so far: what the stress tests
 * read to see the cache spare the system call. */
static int64_t stacks_given_back;

static void give_pages_back(char *stack) {
    os_stack_trim(stack, stack_size);
    __atomic_add_fetch(&stacks_given_back, 1, __ATOMIC_RELAXED);
}

/* A stack for a new task: from this worker's cache, else from the pool,
 * else carved from a region. */
static char *new_stack(void) {
    Worker *w = worker;
    if (w->stack_cached > 0) {
        return w->stack_cache[--w->stack_cached];
    }
    lock_take(&pool_lock);
    char *stack = new_stack_locked();
    lock_give(&pool_lock);
    return stack;
}

static char *new_stack_locked(void) {
    if (free_count > 0) {
        return free_stacks[--free_count];
    }
    while (region_left == 0) {
        // One stack more than it holds, so its stacks can start aligned.
        char *region = os_reserve(stack_size * (REGION_STACKS + 1));
        if (region != NULL) {
            region_next = (char *)(((uintptr_t)region + stack_size - 1) & ~(stack_size - 1));
            region_left = REGION_STACKS;
            // Set once, with the first region, before any other worker
            // runs: every thread reads it from then on, without a lock.
            if (!stacks_sized) {
                lugha_rt_stack_mask = stack_size - 1;
            }
            stacks_sized = true;
            break;
        }
        // A small address space: smaller stacks, settled before the first.
        if (stacks_sized || stack_size <= SMALLEST_STACK) {
            die("panic: out of memory\n");
        }
        stack_size /= 2;
    }
    char *stack = region_next;
    region_next += stack_size;
    region_left--;
    os_stack_ready(stack, stack_size);
    return stack;
}

/* A finished task's stack: kept by this worker for its next task, pages
 * and all, while its cache has room (no system call, and none of the TLB
 * flushes on every core that giving pages back costs); else its pages
 * given back and the stack pooled. */
static void give_stack(char *stack) {
    Worker *w = worker;
    if (w->stack_cached < STACK_CACHE) {
        w->stack_dirty[w->stack_cached] = true;
        w->stack_cache[w->stack_cached++] = stack;
        return;
    }
    give_pages_back(stack);
    lock_take(&pool_lock);
    if (free_count == free_capacity) {
        size_t capacity = free_capacity == 0 ? 64 : free_capacity * 2;
        char **grown = realloc(free_stacks, capacity * sizeof *grown);
        if (grown == NULL) {
            die("panic: out of memory\n");
        }
        free_stacks = grown;
        free_capacity = capacity;
    }
    free_stacks[free_count++] = stack;
    lock_give(&pool_lock);
}

/* A worker about to park gives its cached stacks' pages back, off the hot
 * path: an idle program holds no more than one without the cache. */
static void stack_cache_trim(Worker *w) {
    for (int i = 0; i < w->stack_cached; i++) {
        if (w->stack_dirty[i]) {
            give_pages_back(w->stack_cache[i]);
            w->stack_dirty[i] = false;
        }
    }
}

/* Gives a finished fiber's stack back once another one is running on its own. */
static void reap(void) {
    Fiber *dead = worker->zombie;
    if (dead == NULL) {
        return;
    }
    worker->zombie = NULL;
    if (dead->map != NULL) {
        give_stack(dead->map);
    }
    rt_scratch_done(dead->show.boxes);
#if defined(__SANITIZE_THREAD__)
    __tsan_destroy_fiber(dead->tsan);
#endif
    free(dead);
}

/* Where code on `stack` starts: its top, less the budget's 16 bytes. */
static char *stack_top(char *stack) {
    return stack + stack_size - 16;
}

/* The step budget of the stack `sp` is on. */
static int64_t *budget_of(uintptr_t sp) {
    return (int64_t *)((sp | lugha_rt_stack_mask) - 15);
}

/* A full slice for the code on `stack`. */
static void budget_fill(char *stack) {
    *budget_of((uintptr_t)stack) = TASK_BUDGET;
}

/* After an unwind stopped (a scope or shield caught it): an overflow the
 * running fiber was unwinding from is over. */
static void overflow_over(void) {
    if (current != NULL) {
        current->overflowed = false;
    }
}

/* Called by a function's entry when its stack has less than
 * `STACK_RESERVE` left: a `stack overflow` panic, which unwinds; while it
 * unwinds, cleanups may go on down to `STACK_UNWIND_ROOM`. */
void lugha_rt_stack_low(void) {
    uintptr_t left = (uintptr_t)__builtin_frame_address(0) & lugha_rt_stack_mask;
    if (current != NULL && current->overflowed && left >= STACK_UNWIND_ROOM) {
        return;
    }
    if (current != NULL) {
        current->overflowed = true;
    }
    panic_with("stack overflow", 14);
}

/* Runs `fn(arg)` with the stack pointer at `top` (16-byte aligned), then
 * comes back to the caller's stack. Its frame keeps the old stack pointer
 * in the frame pointer, which the unwind tables say, so an unwinder walks
 * from the new stack back to the old. */
void lugha_rt_on_stack(void (*fn)(void *), void *arg, char *top);


/* A C stack: a pooled stack, aligned like any other, with its bottom
 * pages made a guard. */
static char *c_stack_new(void) {
    char *stack = new_stack();
    if (!os_stack_guard(stack, C_STACK_GUARD)) {
        die("panic: can't make a C stack\n");
    }
    budget_fill(stack);
    return stack;
}

/* This thread's C stack is `stack`, made by `c_stack_new`. */
static void c_stack_use(char *stack) {
    worker->c_stack_guard = (uintptr_t)stack;
    worker->c_stack_top = stack_top(stack);
}

/* The calling thread's C stack, made now. */
static void c_stack_make(void) {
    c_stack_use(c_stack_new());
}

/* `lugha_rt_c_call(thunk, frame)`: generated code's call of a C function,
 * `thunk(frame)`, run on the running worker's C stack; a callback's Lugha
 * code there checks against that stack's end, and can't wait. Already on
 * the C stack (C calling back into Lugha, which calls C again), it runs in
 * place. */
void lugha_rt_c_call(void (*thunk)(void *), void *frame) {
    if (worker->on_c_stack || worker->c_stack_top == NULL) {
        thunk(frame);
        return;
    }
    worker->on_c_stack = true;
    lugha_rt_on_stack(thunk, frame, worker->c_stack_top);
    worker->on_c_stack = false;
}

/* `main`'s call, run on its pooled stack. */
typedef struct {
    int32_t (*entry)(int32_t, char **);
    int32_t argc;
    char **argv;
    int32_t code;
} MainCall;

/* Back to the main thread, where `lugha_rt_main` returns (workers.c). */
static void main_go_home(void);

static void main_on_stack(void *p) {
    MainCall *call = p;
    call->code = call->entry(call->argc, call->argv);
    main_go_home();
}

/* The C `main` lughac emits: runs `entry` (the program, the runtime's
 * start included) as the `main` task, on a pooled stack, and gives its
 * exit code. The process's own stack stays for C. The scheduler isn't
 * started: a program without tasks still has none. */
int32_t lugha_rt_main(int32_t argc, char **argv, int32_t (*entry)(int32_t, char **)) {
    main_fiber.map = new_stack();
    main_fiber.top = stack_top(main_fiber.map);
    budget_fill(main_fiber.map);
    c_stack_make();
    MainCall call = {entry, argc, argv, 0};
    lugha_rt_on_stack(main_on_stack, &call, main_fiber.top);
    return call.code;
}

