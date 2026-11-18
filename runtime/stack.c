/*
 * stack.c — part of lugha_rt.c: the stack overflow handler.
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it after the
 * core (lugha_rt.c), whose includes, types and forward declaration of
 * `install_stack_overflow_handler` it relies on.
 */

/* The lowest address of the main thread's stack; 0 if unknown. */
static uintptr_t stack_low;

/* Where the handler runs once the normal stack is exhausted. */
static char alternate_stack[64 * 1024];

/* How close to the stack's lowest address a fault counts as an overflow. */
#define GUARD_WINDOW (64 * 1024)

/* How much of a worker's C stack is its guard (stacks.c). */
#define C_GUARD_SIZE (64 * 1024)

/* A fault in the faulting thread's C stack guard (C runs there; Lugha
 * stacks check at each function's entry), or next to the process stack's
 * end, is a stack overflow: reported, without unwinding. Anything else
 * keeps the default action, so a real memory error in C code still kills
 * the process with SIGSEGV. */
static void on_segv(int signal, siginfo_t *info, void *context) {
    (void)context;
    uintptr_t at = (uintptr_t)info->si_addr;
    uintptr_t guard = worker->c_stack_guard;
    bool c_overflow = guard != 0 && at >= guard && at < guard + C_GUARD_SIZE;
    bool main_overflow =
        stack_low != 0 && at + GUARD_WINDOW >= stack_low && at < stack_low + GUARD_WINDOW;
    if (c_overflow || main_overflow) {
        /* Not async-signal-safe, but it keeps earlier output, as other panics
         * do; if the overflow happened inside stdio this can hang instead of
         * exiting, which is acceptable for a program already out of stack. */
        fflush(stdout);
        static const char message[] = "panic: stack overflow\n";
        ssize_t written = write(STDERR_FILENO, message, sizeof message - 1);
        /* A failed write is ignored: the program exits either way. */
        (void)written;
        _exit(101);
    }
    /* Restore the default and re-raise: a real fault would repeat anyway, but
     * a SIGSEGV sent by raise() or kill() would otherwise be swallowed. */
    struct sigaction fallback;
    memset(&fallback, 0, sizeof fallback);
    fallback.sa_handler = SIG_DFL;
    sigaction(signal, &fallback, NULL);
    raise(signal);
}

/* Each step can fail; then the program runs without the handler, as before. */
static void install_stack_overflow_handler(void) {
    pthread_attr_t attr;
    void *base;
    size_t size;
    if (pthread_getattr_np(pthread_self(), &attr) != 0) {
        return;
    }
    int got = pthread_attr_getstack(&attr, &base, &size);
    pthread_attr_destroy(&attr);
    if (got != 0) {
        return;
    }
    stack_t alternate;
    memset(&alternate, 0, sizeof alternate);
    alternate.ss_sp = alternate_stack;
    alternate.ss_size = sizeof alternate_stack;
    if (sigaltstack(&alternate, NULL) != 0) {
        return;
    }
    stack_low = (uintptr_t)base;
    struct sigaction action;
    memset(&action, 0, sizeof action);
    action.sa_sigaction = on_segv;
    action.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGSEGV, &action, NULL) != 0) {
        stack_low = 0;
    }
}
