/*
 * os/darwin/aarch64.c — part of lugha_rt.c: what macOS on Apple Silicon
 * needs written for its CPU: the task switch and a new task's first entry,
 * running on another stack, and the frame a new task starts from; Linux's
 * AArch64 code (os/linux/aarch64.c) in Mach-O form: symbols take a leading
 * `_`, and Mach-O has no `.type` or `.size`. The worker is found without
 * the thread pointer (os/darwin/os.c). Apple's AArch64 rules are AAPCS64's
 * for these registers; x18 is the system's, and nothing here touches it. Each system has its own, since the assembler's directives, symbol
 * names, calling rules and thread state differ between systems as well as
 * CPUs.
 *
 * A fragment, not compiled on its own: joined right after tasks/tasks.c
 * (`Fiber`), beside the other CPUs' files; wrapped in `#if`, so only the
 * build's own CPU's is compiled.
 */

#if defined(__aarch64__)

/* x86_64.c's switch for AArch64: AAPCS64's callee-saved
 * registers — x19–x28, the frame pointer x29, the link register x30 and
 * d8–d15 (the low 64 bits of SIMD registers 8–15) — and FPCR, the
 * rounding mode, in a
 * 176-byte frame, so `sp` stays 16-byte aligned as AAPCS64 requires. */
__asm__(".text\n"
        ".p2align 2\n"
        ".globl _lugha_rt_task_swap\n"
        "_lugha_rt_task_swap:\n"
        "    sub sp, sp, #176\n"
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
        "    mov x9, sp\n"
        "    str x9, [x0]\n"
        "    mov sp, x1\n"
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
        "    add sp, sp, #176\n"
        "    ret\n"
        /* A new fiber's first `ret` lands here, its fiber in x19. */
        ".globl _lugha_rt_task_boot\n"
        "_lugha_rt_task_boot:\n"
        /* The fiber's outermost frame: unwinding stops here. */
        "    .cfi_startproc\n"
        "    .cfi_undefined x30\n"
        "    mov x0, x19\n"
        "    bl _lugha_rt_task_fiber_main\n"
        "    brk #0\n"
        "    .cfi_endproc\n");

/* `lugha_rt_on_stack` (tasks/stacks.c). */
__asm__(".text\n"
        ".p2align 2\n"
        ".globl _lugha_rt_on_stack\n"
        "_lugha_rt_on_stack:\n"
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
        "    .cfi_endproc\n");

/* The frame a new fiber's first switch pops (tasks/spawn.c's `fiber_frame`),
 * landing in `lugha_rt_task_boot`; where its stack pointer starts. */
static void *cpu_first_frame(Fiber *f) {
    /* AArch64: the switch's 176-byte frame, all zero (FPCR 0
     * is round-to-nearest, no traps) but x19 (slot 0) = the fiber and x30
     * (slot 11) = boot, the address its `ret` takes; `sp` ends at `top`, 16-byte aligned. */
    uint64_t *frame = (uint64_t *)(f->top - 176);
    memset(frame, 0, 176);
    frame[0] = (uint64_t)(uintptr_t)f;
    frame[11] = (uint64_t)(uintptr_t)lugha_rt_task_boot;
    return frame;
}

#endif
