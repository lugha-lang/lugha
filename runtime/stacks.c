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
 * There are no guard pages: every Lugha function's entry compares the
 * stack pointer with `lugha_rt_stack_limit`, the running task's stack
 * bottom plus `STACK_RESERVE`, set at each switch, and below it calls
 * `lugha_rt_stack_low`, which panics with `stack overflow`. The panic
 * unwinds, with the limit lowered so the frames' cleanups have room; a
 * second overflow there ends the program, as a panic while unwinding does.
 * A fiber without a pooled stack (a C harness's `main`) has the limit 0:
 * nothing is checked.
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

/* The running code's lowest usable stack address: below it, generated
 * code calls `lugha_rt_stack_low`. 0 checks nothing. */
uintptr_t lugha_rt_stack_limit;

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
        char *region = mmap(NULL, stack_size * REGION_STACKS, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
        if (region != MAP_FAILED) {
            region_next = region;
            region_left = REGION_STACKS;
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

/* The limit for code running on `f`'s stack. */
static uintptr_t stack_limit_of(const Fiber *f) {
    return f->map == NULL ? 0 : (uintptr_t)f->map + STACK_RESERVE;
}

/* After an unwind stopped (a scope or shield caught it): the running
 * fiber's own limit again, which an overflow had lowered. */
static void stack_limit_restore(void) {
    if (current != NULL) {
        lugha_rt_stack_limit = stack_limit_of(current);
    }
}

/* Called by a function's entry below the limit: a panic that unwinds,
 * with room kept for its cleanups. */
void lugha_rt_stack_low(void) {
    if (current != NULL && current->map != NULL) {
        lugha_rt_stack_limit = (uintptr_t)current->map + STACK_UNWIND_ROOM;
    } else {
        lugha_rt_stack_limit = 0;
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

/* Where C's calls start on the C stack (NULL: they stay where they are,
 * as in a C harness's `main`). */
static char *c_stack_top;

/* `lugha_rt_c_call(thunk, frame)`: generated code's call of a C function,
 * `thunk(frame)`, run on the C stack; a callback's Lugha code there checks
 * against the C stack's end, and can't wait. Already on the C stack (C
 * calling back into Lugha, which calls C again), it runs in place. */
void lugha_rt_c_call(void (*thunk)(void *), void *frame) {
    if (worker->on_c_stack || c_stack_top == NULL) {
        thunk(frame);
        return;
    }
    uintptr_t limit = lugha_rt_stack_limit;
    worker->on_c_stack = true;
    // `stack_low` (stack.c) is the process stack's end, once known.
    lugha_rt_stack_limit = stack_low == 0 ? 0 : stack_low + STACK_RESERVE;
    lugha_rt_on_stack(thunk, frame, c_stack_top);
    worker->on_c_stack = false;
    lugha_rt_stack_limit = limit;
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
    // C's calls run below this frame, on the process's own stack.
    char here;
    c_stack_top = (char *)(((uintptr_t)&here - 4096) & ~(uintptr_t)15);
    main_fiber.map = new_stack();
    main_fiber.top = main_fiber.map + stack_size;
    lugha_rt_stack_limit = stack_limit_of(&main_fiber);
    MainCall call = {entry, argc, argv, 0};
    lugha_rt_on_stack(main_on_stack, &call, main_fiber.top);
    lugha_rt_stack_limit = 0;
    return call.code;
}

