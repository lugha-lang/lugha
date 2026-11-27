/*
 * os/windows/crash.c — part of lugha_rt.c: what Windows does with a fault
 * no program recovers from, in C code or in the runtime: says which, and
 * where, before the program ends.
 *
 * A fragment, not compiled on its own: joined right after os/windows/os.c,
 * which installs it.
 */

/* Appends `v` in hex to `at`, returning the end. */
static char *win_hex(char *at, uint64_t v) {
    char digits[16];
    int n = 0;
    do {
        digits[n++] = "0123456789ABCDEF"[v & 0xf];
        v >>= 4;
    } while (v != 0);
    *at++ = '0';
    *at++ = 'x';
    while (n > 0) {
        *at++ = digits[--n];
    }
    return at;
}

/* Appends `text` to `at`, returning the end. */
static char *win_text(char *at, const char *text) {
    while (*text != '\0') {
        *at++ = *text++;
    }
    return at;
}

/* Whether `code` is a fault no program recovers from: the processor's
 * (a bad address, instruction or division) or the unwinder's own. Not an
 * exception code raises to be caught, such as a panic's, a debugger's
 * message or a thread's name, which pass through untouched. */
static bool win_is_crash(DWORD code) {
    switch (code) {
    case EXCEPTION_ACCESS_VIOLATION:
    case EXCEPTION_ILLEGAL_INSTRUCTION:
    case EXCEPTION_PRIV_INSTRUCTION:
    case EXCEPTION_IN_PAGE_ERROR:
    case EXCEPTION_INT_DIVIDE_BY_ZERO:
    case EXCEPTION_INT_OVERFLOW:
    case EXCEPTION_DATATYPE_MISALIGNMENT:
    case EXCEPTION_ARRAY_BOUNDS_EXCEEDED:
    case EXCEPTION_NONCONTINUABLE_EXCEPTION:
    case EXCEPTION_INVALID_DISPOSITION:
    case 0xC0000028: /* STATUS_BAD_STACK: unwinding found a broken frame */
    case 0xC0000029: /* STATUS_INVALID_UNWIND_TARGET */
        return true;
    default:
        return false;
    }
}

/* A fault no program recovers from: named with its code, what an access
 * violation touched, and where, as an offset into the module it happened
 * in (the executable's, for code lughac built), then the program ends as
 * a panic does. Without this, Windows ends it with the code as its exit
 * status and not a word. Built with no allocation and no formatting
 * library: the heap may be what broke. */
static void win_crash(EXCEPTION_POINTERS *e) {
    static int reported;
    if (__atomic_exchange_n(&reported, 1, __ATOMIC_ACQ_REL) != 0) {
        for (;;) {
            Sleep(INFINITE);
        }
    }
    EXCEPTION_RECORD *r = e->ExceptionRecord;
    char line[256];
    char *at = win_text(line, "panic: internal error: Windows exception ");
    at = win_hex(at, (uint32_t)r->ExceptionCode);
    if (r->ExceptionCode == EXCEPTION_ACCESS_VIOLATION && r->NumberParameters >= 2) {
        ULONG_PTR how = r->ExceptionInformation[0];
        at = win_text(at, how == 1 ? " (writing " : how == 8 ? " (running " : " (reading ");
        at = win_hex(at, (uint64_t)r->ExceptionInformation[1]);
        at = win_text(at, ")");
    }
    at = win_text(at, " at ");
    HMODULE module = NULL;
    uintptr_t pc = (uintptr_t)r->ExceptionAddress;
    if (GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
                               GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                           (LPCWSTR)r->ExceptionAddress, &module) &&
        module != NULL) {
        at = win_text(at, module == GetModuleHandleW(NULL) ? "the program+" : "a library+");
        at = win_hex(at, pc - (uintptr_t)module);
    } else {
        at = win_hex(at, pc);
    }
    *at++ = '\n';
    fflush(stdout);
    DWORD written;
    WriteFile(GetStdHandle(STD_ERROR_HANDLE), line, (DWORD)(at - line), &written, NULL);
    TerminateProcess(GetCurrentProcess(), 101);
}
