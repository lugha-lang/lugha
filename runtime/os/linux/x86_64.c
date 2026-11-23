/*
 * os/linux/x86_64.c — part of lugha_rt.c: what Linux on x86-64 needs written
 * for its CPU: reading the thread pointer, the task switch and a new task's
 * first entry, running on another stack, and the frame a new task starts
 * from. Each system has its own, since the assembler's directives, symbol
 * names, calling rules and thread state differ between systems as well as
 * CPUs.
 *
 * A fragment, not compiled on its own: joined right after tasks/tasks.c
 * (`Fiber`), beside the other CPUs' files; wrapped in `#if`, so only the
 * build's own CPU's is compiled.
 */

#if defined(__x86_64__)

static inline uintptr_t thread_pointer(void) {
    uintptr_t tp;
    __asm__ volatile("movq %%fs:0, %0" : "=r"(tp));
    return tp;
}

/* Saves the callee-saved registers, MXCSR and the x87 control word on the
 * old stack, switches stacks, and restores them from the new one (x86-64
 * System V). */
__asm__(".text\n"
        ".globl lugha_rt_task_swap\n"
        ".type lugha_rt_task_swap, @function\n"
        "lugha_rt_task_swap:\n"
        "    pushq %rbp\n"
        "    pushq %rbx\n"
        "    pushq %r12\n"
        "    pushq %r13\n"
        "    pushq %r14\n"
        "    pushq %r15\n"
        "    subq $8, %rsp\n"
        "    stmxcsr (%rsp)\n"
        "    fnstcw 4(%rsp)\n"
        "    movq %rsp, (%rdi)\n"
        "    movq %rsi, %rsp\n"
        "    ldmxcsr (%rsp)\n"
        "    fldcw 4(%rsp)\n"
        "    addq $8, %rsp\n"
        "    popq %r15\n"
        "    popq %r14\n"
        "    popq %r13\n"
        "    popq %r12\n"
        "    popq %rbx\n"
        "    popq %rbp\n"
        "    ret\n"
        ".size lugha_rt_task_swap, .-lugha_rt_task_swap\n"
        /* A new fiber's first `ret` lands here, its fiber in %r12. */
        ".globl lugha_rt_task_boot\n"
        ".type lugha_rt_task_boot, @function\n"
        "lugha_rt_task_boot:\n"
        /* The fiber's outermost frame: unwinding stops here. */
        "    .cfi_startproc\n"
        "    .cfi_undefined rip\n"
        "    movq %r12, %rdi\n"
        "    call lugha_rt_task_fiber_main\n"
        "    ud2\n"
        "    .cfi_endproc\n"
        ".size lugha_rt_task_boot, .-lugha_rt_task_boot\n");

/* `lugha_rt_on_stack` (tasks/stacks.c). */
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

/* The frame a new fiber's first switch pops (tasks/spawn.c's `fiber_frame`),
 * landing in `lugha_rt_task_boot`; where its stack pointer starts. */
static void *cpu_first_frame(Fiber *f) {
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
    return frame;
}

#endif
