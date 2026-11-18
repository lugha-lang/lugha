/*
 * stacks.c — part of lugha_rt.c: task stacks and the stack limit.
 *
 * A task's stack is a large reservation (8 MiB, smaller on a small address
 * space) carved from regions of `REGION_STACKS` stacks, each region one
 * mapping, so a program's mappings stay few however many tasks it runs.
 * Only the pages a task touches use memory; a finished task's stack gives
 * its pages back (`MADV_DONTNEED`) and waits in a pool for the next task.
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

/* A stack for a new task: from the pool, else carved from a region. */
static char *new_stack(void) {
    if (free_count > 0) {
        return free_stacks[--free_count];
    }
    while (region_left == 0) {
        // One stack more than it holds, so its stacks can start aligned.
        char *region = mmap(NULL, stack_size * (REGION_STACKS + 1), PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
        if (region != MAP_FAILED) {
            region_next = (char *)(((uintptr_t)region + stack_size - 1) & ~(stack_size - 1));
            region_left = REGION_STACKS;
            stacks_sized = true;
            lugha_rt_stack_mask = stack_size - 1;
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
    return stack;
}

/* A finished task's stack: its pages given back, the stack kept. */
static void give_stack(char *stack) {
    // Advice the kernel may refuse: the pages then stay, still reusable.
    madvise(stack, stack_size, MADV_DONTNEED);
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

#if defined(__x86_64__)
__asm__(".text\n"
        ".globl lugha_rt_on_stack\n"
        ".type lugha_rt_on_stack, @function\n"
        "lugha_rt_on_stack:\n"
        "    .cfi_startproc\n"
        "    pushq %rbp\n"
        "    .cfi_def_cfa_offset 16\n"
        "    .cfi_offset rbp, -16\n"
        "    movq %rsp, %rbp\n"
        "    .cfi_def_cfa_register rbp\n"
        "    movq %rdx, %rsp\n"
        "    movq %rdi, %rax\n"
        "    movq %rsi, %rdi\n"
        "    call *%rax\n"
        "    movq %rbp, %rsp\n"
        "    popq %rbp\n"
        "    .cfi_def_cfa rsp, 8\n"
        "    ret\n"
        "    .cfi_endproc\n"
        ".size lugha_rt_on_stack, .-lugha_rt_on_stack\n");
#elif defined(__aarch64__)
__asm__(".text\n"
        ".globl lugha_rt_on_stack\n"
        ".type lugha_rt_on_stack, %function\n"
        "lugha_rt_on_stack:\n"
        "    .cfi_startproc\n"
        "    stp x29, x30, [sp, #-16]!\n"
        "    .cfi_def_cfa_offset 16\n"
        "    .cfi_offset x29, -16\n"
        "    .cfi_offset x30, -8\n"
        "    mov x29, sp\n"
        "    .cfi_def_cfa_register x29\n"
        "    mov sp, x2\n"
        "    mov x9, x0\n"
        "    mov x0, x1\n"
        "    blr x9\n"
        "    mov sp, x29\n"
        "    ldp x29, x30, [sp], #16\n"
        "    .cfi_def_cfa sp, 0\n"
        "    ret\n"
        "    .cfi_endproc\n"
        ".size lugha_rt_on_stack, .-lugha_rt_on_stack\n");
#endif

/* Pages kept inaccessible at a C stack's bottom: C has no entry checks,
 * so an overflow there faults, and the handler (stack.c) reports it. */
#define C_STACK_GUARD C_GUARD_SIZE

/* The calling thread's C stack: a pooled stack, aligned like any other,
 * with its bottom pages made a guard. */
static void c_stack_make(void) {
    char *stack = new_stack();
    if (mprotect(stack, C_STACK_GUARD, PROT_NONE) != 0) {
        die("panic: can't make a C stack\n");
    }
    budget_fill(stack);
    worker->c_stack_guard = (uintptr_t)stack;
    worker->c_stack_top = stack_top(stack);
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

static void main_on_stack(void *p) {
    MainCall *call = p;
    call->code = call->entry(call->argc, call->argv);
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

