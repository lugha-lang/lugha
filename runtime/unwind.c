/*
 * unwind.c — part of lugha_rt.c: what a panic does after printing its
 * message. The running task's stack unwinds, frame by frame, through the
 * system unwinder (libgcc's): each Lugha frame's landing pad runs the
 * `defer`s pending at the call in progress and lets go of what the frame
 * holds, then the unwinding goes on. It is a forced unwind: nothing
 * catches a panic, and the stop function ends it at the task's start (a
 * green thread's first frame, marked outermost, or past `main`), where
 * `panic_end` ends the program or the actor. `main`'s own landing pad, in
 * the C `main` lughac emits, ends the program first, after letting go of
 * `main`'s arguments.
 *
 * A panic while a stack is already unwinding (in a `defer`, say) ends the
 * program at once: its message is printed, but nothing is known about
 * what the frames hold. Only a shield entered while unwinding, cut off at
 * its deadline, unwinds again: its own landing pad stops that unwind.
 *
 * A fragment, not compiled on its own: joined after actors.c, whose
 * `panic_end` it calls.
 */

/* "LUGHPANC": a panic's exception class. */
#define PANIC_CLASS 0x4c55474850414e43ULL

/* Whether the main thread unwinds, before any green thread exists. */
static bool main_unwinding;

static bool *unwinding_flag(void) {
    return current == NULL ? &main_unwinding : &current->unwinding;
}

/* Called for every frame the forced unwind passes: frames carry on until
 * the task's start, where the panic ends its task. */
static _Unwind_Reason_Code panic_stop(int version, _Unwind_Action actions,
                                      _Unwind_Exception_Class class,
                                      struct _Unwind_Exception *exception,
                                      struct _Unwind_Context *context, void *arg) {
    (void)version;
    (void)class;
    (void)context;
    (void)arg;
    if ((actions & _UA_END_OF_STACK) == 0) {
        return _URC_NO_REASON;
    }
    Panic *panic = (Panic *)exception;
    // A task a scope started ends there: cancelled, or failing its scope.
    if (current != NULL && current->started_in != NULL) {
        current->in_flight = NULL;
        scope_task_unwound(panic->cancel, panic->text, panic->len);
    }
    *unwinding_flag() = false;
    if (panic->cancel) {
        // `main`, cancelled because a root-scope task failed, or by Ctrl-C.
        fflush(stdout);
        exit(shutting_down ? 130 : 101);
    }
    panic_end(panic->text, panic->len);
    // Only an actor's panic returns from `panic_end`'s caller, and it
    // never does: its fiber ends. Anything else has exited.
    exit(101);
}

/* The `_Unwind_Exception` cleanup a foreign catcher would call; nothing
 * catches a panic, so it is never called. */
static void panic_free(_Unwind_Reason_Code reason, struct _Unwind_Exception *exception) {
    (void)reason;
    (void)exception;
}

/* A panic reached a Lugha function C called (a callback): it never
 * unwinds into C, whose state it can't know, so the program ends here. */
void lugha_rt_unwound_to_c(void) {
    fflush(stdout);
    exit(101);
}

/* Unwinds the current fiber: a panic with its message, or a cancellation
 * (silent). A scope catching it takes `in_flight` over. */
static void start_unwind(bool cancel, const char *text, size_t len) {
    bool *unwinding = unwinding_flag();
    if (*unwinding && !(cancel && shield_cut_off())) {
        exit(101);
    }
    if (*unwinding) {
        // A shield in a `defer` cut off: its pad stops this unwind and
        // gives the first one back.
        current->shield_outer = current->in_flight;
    }
    *unwinding = true;
    Panic *panic = lugha_rc_internal(sizeof(Panic));
    panic->exception.exception_class = PANIC_CLASS;
    panic->exception.exception_cleanup = panic_free;
    panic->cancel = cancel;
    panic->text = lugha_rc_internal(len + 1);
    memcpy(panic->text, text, len);
    panic->len = len;
    if (current != NULL) {
        current->in_flight = panic;
    }
    _Unwind_ForcedUnwind(&panic->exception, panic_stop, NULL);
    // Only if the unwinder couldn't start: end the task as it is.
    panic_end(text, len);
    exit(101);
}

/* A caught unwind that wasn't the catcher's own goes on outward. */
static void resume_unwind(Panic *panic) {
    current->in_flight = panic;
    _Unwind_ForcedUnwind(&panic->exception, panic_stop, NULL);
    exit(101);
}

static void panic_unwind(const char *text, size_t len) {
    start_unwind(false, text, len);
}

/* A cancelled task stops: it unwinds as a panic does, silently. */
static void cancel_unwind(void) {
    start_unwind(true, "cancelled", 9);
}
