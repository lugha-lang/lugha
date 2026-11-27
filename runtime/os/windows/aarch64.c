/*
 * os/windows/aarch64.c — part of lugha_rt.c: what Windows on ARM64 needs
 * written for its CPU: the task switch and a new task's first entry,
 * running on another stack, and the frame a new task starts from.
 *
 * Windows' ARM64 rules are AAPCS64's for these registers: x19–x28, the
 * frame pointer x29, the link register x30 and d8–d15 are the callee's to
 * keep. x18 holds the thread's TEB and is the system's: read here, never
 * written. The TEB records each thread's stack as on x86-64, at the same
 * offsets: where it ends (`StackBase`, +0x08), how far it is committed
 * (`StackLimit`, +0x10) and where its reservation begins
 * (`DeallocationStack`, +0x1478). The system grows a stack only inside
 * those bounds and unwinding checks each frame lies within them, so a
 * switch carries the three with the registers, as x86_64.c's does.
 *
 * A fragment, not compiled on its own: joined right after tasks/tasks.c
 * (`Fiber`), beside the other CPUs' files; wrapped in `#if`, so only the
 * build's own CPU's is compiled.
 */

#if defined(__aarch64__)

/* Saves the callee-saved registers, FPCR (the rounding mode) and the TEB's
 * stack bounds on the old stack, switches stacks, and restores them from
 * the new one: `lugha_rt_task_swap(save, load)`, in x0 and x1. The frame is
 * 192 bytes, so `sp` stays 16-byte aligned:
 *   0–95 x19–x30, 96–159 d8–d15, 160 FPCR, 168 StackBase, 176 StackLimit,
 *   184 DeallocationStack. */
__asm__(".text\n"
        ".p2align 2\n"
        ".globl win_task_swap_raw\n"
        "win_task_swap_raw:\n"
        "    sub sp, sp, #192\n"
        "    stp x19, x20, [sp, #0]\n"
        "    stp x21, x22, [sp, #16]\n"
        "    stp x23, x24, [sp, #32]\n"
        "    stp x25, x26, [sp, #48]\n"
        "    stp x27, x28, [sp, #64]\n"
        "    stp x29, x30, [sp, #80]\n"
        "    stp d8, d9, [sp, #96]\n"
        "    stp d10, d11, [sp, #112]\n"
        "    stp d12, d13, [sp, #128]\n"
        "    stp d14, d15, [sp, #144]\n"
        "    mrs x9, fpcr\n"
        "    str x9, [sp, #160]\n"
        "    ldr x9, [x18, #0x08]\n"
        "    ldr x10, [x18, #0x10]\n"
        "    stp x9, x10, [sp, #168]\n"
        "    ldr x9, [x18, #0x1478]\n"
        "    str x9, [sp, #184]\n"
        "    mov x9, sp\n"
        "    str x9, [x0]\n"
        "    mov sp, x1\n"
        "    ldr x9, [sp, #184]\n"
        "    str x9, [x18, #0x1478]\n"
        "    ldp x9, x10, [sp, #168]\n"
        "    str x10, [x18, #0x10]\n"
        "    str x9, [x18, #0x08]\n"
        "    ldr x9, [sp, #160]\n"
        "    msr fpcr, x9\n"
        "    ldp x19, x20, [sp, #0]\n"
        "    ldp x21, x22, [sp, #16]\n"
        "    ldp x23, x24, [sp, #32]\n"
        "    ldp x25, x26, [sp, #48]\n"
        "    ldp x27, x28, [sp, #64]\n"
        "    ldp x29, x30, [sp, #80]\n"
        "    ldp d8, d9, [sp, #96]\n"
        "    ldp d10, d11, [sp, #112]\n"
        "    ldp d12, d13, [sp, #128]\n"
        "    ldp d14, d15, [sp, #144]\n"
        "    add sp, sp, #192\n"
        "    ret\n"
        /* A new fiber's first `ret` lands here, its fiber in x19 and a zero
         * frame pointer. Its prologue saves a zero frame pointer and link
         * register, which its unwind data says, so unwinding finds a zero
         * return address here and ends. */
        ".p2align 2\n"
        ".globl lugha_rt_task_boot\n"
        ".seh_proc lugha_rt_task_boot\n"
        "lugha_rt_task_boot:\n"
        "    mov x30, xzr\n"
        "    .seh_nop\n"
        "    stp x29, x30, [sp, #-16]!\n"
        "    .seh_save_fplr_x 16\n"
        "    mov x29, sp\n"
        "    .seh_set_fp\n"
        "    .seh_endprologue\n"
        "    mov x0, x19\n"
        "    bl lugha_rt_task_fiber_main\n"
        "    brk #0xf000\n"
        ".seh_endproc\n");

/* `fn(arg)` with the stack pointer at `top`, then back; the frame pointer
 * keeps the old stack, which the unwind data says, so an unwinder walks
 * from the new stack back to the old. The TEB's bounds are the caller's. */
void win_on_stack_raw(void (*fn)(void *), void *arg, char *top);
__asm__(".text\n"
        ".p2align 2\n"
        ".globl win_on_stack_raw\n"
        ".seh_proc win_on_stack_raw\n"
        "win_on_stack_raw:\n"
        "    stp x29, x30, [sp, #-16]!\n"
        "    .seh_save_fplr_x 16\n"
        "    mov x29, sp\n"
        "    .seh_set_fp\n"
        "    .seh_endprologue\n"
        "    mov sp, x2\n"
        "    mov x9, x0\n"
        "    mov x0, x1\n"
        "    blr x9\n"
        "    .seh_startepilogue\n"
        "    mov sp, x29\n"
        "    .seh_set_fp\n"
        "    ldp x29, x30, [sp], #16\n"
        "    .seh_save_fplr_x 16\n"
        "    .seh_endepilogue\n"
        "    ret\n"
        ".seh_endproc\n");

/* The TEB's record of the running stack. */
typedef struct {
    char *base;
    char *limit;
    char *start;
} WinBounds;

static WinBounds win_bounds_get(void) {
    WinBounds b;
    __asm__ volatile("ldr %0, [x18, #0x08]" : "=r"(b.base));
    __asm__ volatile("ldr %0, [x18, #0x10]" : "=r"(b.limit));
    __asm__ volatile("ldr %0, [x18, #0x1478]" : "=r"(b.start));
    return b;
}

static void win_bounds_set(WinBounds b) {
    __asm__ volatile("str %0, [x18, #0x08]" : : "r"(b.base) : "memory");
    __asm__ volatile("str %0, [x18, #0x10]" : : "r"(b.limit) : "memory");
    __asm__ volatile("str %0, [x18, #0x1478]" : : "r"(b.start) : "memory");
}

/* A diagnostic: each thread's last states of the TEB's stack bounds, and
 * the first one that doesn't hold the running stack pointer, or whose
 * committed end lies outside the stack, reported with those before it. */
typedef struct {
    const char *where;
    char *base;
    char *limit;
    char *start;
    char *sp;
} WinNote;

enum { WIN_NOTES = 16 };
static __thread WinNote win_notes[WIN_NOTES];
static __thread unsigned win_note_next;

/* Not inlined: a fiber may come back on another thread, and each call
 * finds its own thread's notes afresh. */
__attribute__((noinline)) static void win_bounds_note(const char *where) {
    WinBounds b = win_bounds_get();
    char *sp = (char *)__builtin_frame_address(0);
    WinNote *n = &win_notes[win_note_next++ % WIN_NOTES];
    *n = (WinNote){where, b.base, b.limit, b.start, sp};
    if (b.start <= b.limit && b.limit <= b.base && b.start <= sp && sp < b.base) {
        return;
    }
    HANDLE err = GetStdHandle(STD_ERROR_HANDLE);
    DWORD wrote;
    static const char head[] = "panic: internal error: stack bounds broken; the thread's last:\n";
    WriteFile(err, head, sizeof head - 1, &wrote, NULL);
    for (unsigned i = 0; i < WIN_NOTES; i++) {
        WinNote *e = &win_notes[(win_note_next + i) % WIN_NOTES];
        if (e->where == NULL) {
            continue;
        }
        char line[200];
        char *at = win_text(line, "  ");
        at = win_text(at, e->where);
        at = win_text(at, " base ");
        at = win_hex(at, (uint64_t)(uintptr_t)e->base);
        at = win_text(at, " limit ");
        at = win_hex(at, (uint64_t)(uintptr_t)e->limit);
        at = win_text(at, " start ");
        at = win_hex(at, (uint64_t)(uintptr_t)e->start);
        at = win_text(at, " sp ");
        at = win_hex(at, (uint64_t)(uintptr_t)e->sp);
        *at++ = '\n';
        WriteFile(err, line, (DWORD)(at - line), &wrote, NULL);
    }
    TerminateProcess(GetCurrentProcess(), 101);
}

void win_task_swap_raw(void **save, void *load);

/* `lugha_rt_task_swap`, its bounds checked on the way out and back. */
void lugha_rt_task_swap(void **save, void *load) {
    win_bounds_note("swap out");
    win_task_swap_raw(save, load);
    win_bounds_note("swap back");
}

/* A task stack's size less one (tasks/stacks.c). */
extern uintptr_t lugha_rt_stack_mask;

/* `lugha_rt_on_stack` (tasks/stacks.c): the TEB says the new stack while
 * `fn` runs on it, and the caller's again after. */
void lugha_rt_on_stack(void (*fn)(void *), void *arg, char *top) {
    win_bounds_note("on_stack in");
    WinBounds saved = win_bounds_get();
    char *stack = (char *)((uintptr_t)top & ~lugha_rt_stack_mask);
    WinBounds there = {stack + lugha_rt_stack_mask + 1, win_committed_low(top), stack};
    win_bounds_set(there);
    win_on_stack_raw(fn, arg, top);
    win_bounds_set(saved);
    win_bounds_note("on_stack out");
}

/* The frame a new fiber's first switch pops (tasks/spawn.c's `fiber_frame`),
 * landing in `lugha_rt_task_boot` with `sp` 16 bytes below `top`, as
 * x86_64.c's: the boot frame's saved pair goes below that. */
static void *cpu_first_frame(Fiber *f) {
    uint64_t *frame = (uint64_t *)(f->top - 16 - 192);
    memset(frame, 0, 192);
    // x19 (slot 0) the fiber; x29 (slot 10) zero; x30 (slot 11) boot, the
    // address the switch's `ret` takes. FPCR 0 (slot 20): round to
    // nearest, no traps.
    frame[0] = (uint64_t)(uintptr_t)f;
    frame[11] = (uint64_t)(uintptr_t)lugha_rt_task_boot;
    // The TEB's bounds: its end, how far it's committed, the reservation.
    frame[21] = (uint64_t)(uintptr_t)(f->top + 16);
    frame[22] = (uint64_t)(uintptr_t)win_committed_low(f->top);
    frame[23] = (uint64_t)(uintptr_t)f->map;
    return frame;
}

#endif
