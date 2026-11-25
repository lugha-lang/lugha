/*
 * os/windows/x86_64.c — part of lugha_rt.c: what Windows on x86-64 needs
 * written for its CPU: the task switch and a new task's first entry,
 * running on another stack, and the frame a new task starts from.
 *
 * Windows x64's rules differ from System V's: arguments come in rcx, rdx,
 * r8 and r9, a caller leaves 32 bytes of room above its call for the
 * callee, and rdi, rsi and xmm6–xmm15 are the callee's to keep as well.
 * Windows also records each thread's stack in its TEB: where it ends
 * (`StackBase`, gs:0x08), how far it is committed (`StackLimit`,
 * gs:0x10) and where its reservation begins (`DeallocationStack`,
 * gs:0x1478). The system grows a stack through its guard page only inside
 * those bounds, and unwinding checks each frame lies within them, so a
 * switch carries the three with the registers, as Windows' own fibers do.
 *
 * A fragment, not compiled on its own: joined right after tasks/tasks.c
 * (`Fiber`), beside the other CPUs' files.
 */

#if defined(__x86_64__)

/* Saves the callee-saved registers, MXCSR, the x87 control word and the
 * TEB's stack bounds on the old stack, switches stacks, and restores them
 * from the new one: `lugha_rt_task_swap(save, load)`, in rcx and rdx. */
__asm__(".text\n"
        ".globl lugha_rt_task_swap\n"
        "lugha_rt_task_swap:\n"
        "    pushq %rbp\n"
        "    pushq %rbx\n"
        "    pushq %rdi\n"
        "    pushq %rsi\n"
        "    pushq %r12\n"
        "    pushq %r13\n"
        "    pushq %r14\n"
        "    pushq %r15\n"
        "    pushq %gs:0x08\n"
        "    pushq %gs:0x10\n"
        "    pushq %gs:0x1478\n"
        "    subq $176, %rsp\n"
        "    movaps %xmm6, 0(%rsp)\n"
        "    movaps %xmm7, 16(%rsp)\n"
        "    movaps %xmm8, 32(%rsp)\n"
        "    movaps %xmm9, 48(%rsp)\n"
        "    movaps %xmm10, 64(%rsp)\n"
        "    movaps %xmm11, 80(%rsp)\n"
        "    movaps %xmm12, 96(%rsp)\n"
        "    movaps %xmm13, 112(%rsp)\n"
        "    movaps %xmm14, 128(%rsp)\n"
        "    movaps %xmm15, 144(%rsp)\n"
        "    stmxcsr 160(%rsp)\n"
        "    fnstcw 164(%rsp)\n"
        "    movq %rsp, (%rcx)\n"
        "    movq %rdx, %rsp\n"
        "    ldmxcsr 160(%rsp)\n"
        "    fldcw 164(%rsp)\n"
        "    movaps 0(%rsp), %xmm6\n"
        "    movaps 16(%rsp), %xmm7\n"
        "    movaps 32(%rsp), %xmm8\n"
        "    movaps 48(%rsp), %xmm9\n"
        "    movaps 64(%rsp), %xmm10\n"
        "    movaps 80(%rsp), %xmm11\n"
        "    movaps 96(%rsp), %xmm12\n"
        "    movaps 112(%rsp), %xmm13\n"
        "    movaps 128(%rsp), %xmm14\n"
        "    movaps 144(%rsp), %xmm15\n"
        "    addq $176, %rsp\n"
        "    popq %gs:0x1478\n"
        "    popq %gs:0x10\n"
        "    popq %gs:0x08\n"
        "    popq %r15\n"
        "    popq %r14\n"
        "    popq %r13\n"
        "    popq %r12\n"
        "    popq %rsi\n"
        "    popq %rdi\n"
        "    popq %rbx\n"
        "    popq %rbp\n"
        "    ret\n"
        /* A new fiber's first `ret` lands here, its fiber in r12. Its
         * unwind data allocates the callee's 32 bytes; above them the first
         * frame left a zero return address, so unwinding ends here. */
        ".globl lugha_rt_task_boot\n"
        ".seh_proc lugha_rt_task_boot\n"
        "lugha_rt_task_boot:\n"
        "    subq $32, %rsp\n"
        "    .seh_stackalloc 32\n"
        "    .seh_endprologue\n"
        "    movq %r12, %rcx\n"
        "    call lugha_rt_task_fiber_main\n"
        "    ud2\n"
        ".seh_endproc\n");

/* `fn(arg)` with the stack pointer at `top`, then back; the frame pointer
 * keeps the old stack, which the unwind data says, so an unwinder walks
 * from the new stack back to the old. The TEB's bounds are the caller's. */
void win_on_stack_raw(void (*fn)(void *), void *arg, char *top);
__asm__(".text\n"
        ".globl win_on_stack_raw\n"
        ".seh_proc win_on_stack_raw\n"
        "win_on_stack_raw:\n"
        "    pushq %rbp\n"
        "    .seh_pushreg %rbp\n"
        "    movq %rsp, %rbp\n"
        "    .seh_setframe %rbp, 0\n"
        "    .seh_endprologue\n"
        "    movq %r8, %rsp\n"
        "    subq $32, %rsp\n"
        "    movq %rcx, %rax\n"
        "    movq %rdx, %rcx\n"
        "    call *%rax\n"
        "    movq %rbp, %rsp\n"
        "    popq %rbp\n"
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
    __asm__ volatile("movq %%gs:0x08, %0" : "=r"(b.base));
    __asm__ volatile("movq %%gs:0x10, %0" : "=r"(b.limit));
    __asm__ volatile("movq %%gs:0x1478, %0" : "=r"(b.start));
    return b;
}

static void win_bounds_set(WinBounds b) {
    __asm__ volatile("movq %0, %%gs:0x08" : : "r"(b.base) : "memory");
    __asm__ volatile("movq %0, %%gs:0x10" : : "r"(b.limit) : "memory");
    __asm__ volatile("movq %0, %%gs:0x1478" : : "r"(b.start) : "memory");
}

/* A task stack's size less one (tasks/stacks.c). */
extern uintptr_t lugha_rt_stack_mask;

/* `lugha_rt_on_stack` (tasks/stacks.c): the TEB says the new stack while
 * `fn` runs on it, and the caller's again after. */
void lugha_rt_on_stack(void (*fn)(void *), void *arg, char *top) {
    WinBounds saved = win_bounds_get();
    char *stack = (char *)((uintptr_t)top & ~lugha_rt_stack_mask);
    WinBounds there = {stack + lugha_rt_stack_mask + 1, win_committed_low(top), stack};
    win_bounds_set(there);
    win_on_stack_raw(fn, arg, top);
    win_bounds_set(saved);
}

/* The frame a new fiber's first switch pops (tasks/spawn.c's `fiber_frame`),
 * landing in `lugha_rt_task_boot`; where its stack pointer starts. Above
 * the return address, a zero: where unwinding the boot frame ends. */
static void *cpu_first_frame(Fiber *f) {
    uint64_t *end = (uint64_t *)(f->top - 16);
    uint64_t *frame = end - 34;
    memset(frame, 0, 35 * sizeof *frame);
    // xmm6–xmm15 zero (words 0–19), then MXCSR and x87 defaults.
    frame[20] = 0x1f80 | ((uint64_t)0x037f << 32);
    // The TEB's bounds: the reservation, how far it's committed, its end.
    frame[22] = (uint64_t)(uintptr_t)f->map;
    frame[23] = (uint64_t)(uintptr_t)win_committed_low(f->top);
    frame[24] = (uint64_t)(uintptr_t)(f->top + 16);
    // r15, r14, r13, then r12 (the fiber), rsi, rdi, rbx and rbp.
    frame[28] = (uint64_t)(uintptr_t)f;
    frame[33] = (uint64_t)(uintptr_t)lugha_rt_task_boot;
    return frame;
}

#endif
