/*
 * os/windows/os.c — part of lugha_rt.c: the runtime's operating-system
 * layer for Windows (10 or newer, x64), on Windows' own calls and its C
 * runtime (UCRT); threads are pthreads, through the winpthreads MinGW
 * links in.
 *
 * Events come from one I/O completion port: the two signals other threads
 * raise are packets posted to it with their tokens, and every socket is
 * tied to it, so each overlapped operation's completion arrives there
 * carrying its `WinIo`, which names the task waiting for it
 * (os/windows/sockets.c). Waiting on a word is `WaitOnAddress`.
 *
 * A task's stack is reserved whole and committed as it grows, through a
 * guard page below what is committed, as a thread's own stack is: the task
 * switch keeps the TEB's record of the running stack (x86_64.c), which is
 * where the system looks. A C stack's overflow reaches the guard's end as
 * `EXCEPTION_STACK_OVERFLOW`, reported here as Linux reports it.
 *
 * Text to a console goes as UTF-16 (`WriteConsoleW`), so it shows whatever
 * the console's code page; to anything else the bytes go as they are, and
 * `\n` stays `\n`.
 *
 * A fragment, not compiled on its own: joined right after the core and the
 * layer's declarations.
 */

#if !defined(__x86_64__)
#error "green threads on Windows need a stack switch for this CPU (os/windows/ has x86_64.c)"
#endif

/* `ProcessPrng` (bcryptprimitives.dll): random bytes that can't fail. */
BOOL WINAPI ProcessPrng(PBYTE data, SIZE_T len);

/* The clock deadlines and sleeps are measured on (winpthreads'
 * `clock_gettime`, from the performance counter). */
#define OS_MONOTONIC CLOCK_MONOTONIC

/* The key holding each thread's worker, made before `main`. */
static pthread_key_t os_worker_key;

__attribute__((constructor)) static void os_worker_key_init(void) {
    if (pthread_key_create(&os_worker_key, NULL) != 0) {
        die("panic: can't make the workers' thread key\n");
    }
}

/* This thread's worker, the first time it asks: its own `this_worker`,
 * its address taken here, on the thread, and kept in the key. */
__attribute__((noinline)) static Worker *os_worker_first(void) {
    Worker *w = &this_worker;
    if (pthread_setspecific(os_worker_key, w) != 0) {
        die("panic: can't keep a worker in its thread key\n");
    }
    return w;
}

/* The running thread's worker, read fresh: a task may move to another
 * thread at a switch. */
static inline Worker *os_worker(void) {
    Worker *w = pthread_getspecific(os_worker_key);
    return w != NULL ? w : os_worker_first();
}

/* The completion port, made at the first job or wake-up. */
static HANDLE os_port;
static pthread_once_t os_port_once = PTHREAD_ONCE_INIT;

static void os_port_make(void) {
    os_port = CreateIoCompletionPort(INVALID_HANDLE_VALUE, NULL, 0, 0);
    if (os_port == NULL) {
        die("panic: CreateIoCompletionPort failed\n");
    }
}

static void os_poll_open(void) {
    pthread_once(&os_port_once, os_port_make);
}

/* The completion key every socket is tied to the port with: its packets
 * carry a `WinIo`, whose token says whom they wake. Never a signal's. */
#define OS_TOKEN_IO 3

/* One overlapped operation, in its waiting task's frame: the `OVERLAPPED`
 * Windows completes, and `packed`, the fiber and wait its packet wakes (as
 * the poll packs them, net.c's `poll_found`). The poll stores the byte
 * count, then swaps `packed` for `WIN_IO_DONE` and wakes whom it named;
 * after that swap nothing touches the record, so its task may leave. */
typedef struct {
    OVERLAPPED ov;
    uint64_t packed;
    DWORD bytes;
} WinIo;

/* `packed` once the poll has taken the packet: never a fiber's token. */
#define WIN_IO_DONE 0

/* Waits up to `timeout_ms` (< 0 for ever, 0 not at all) for packets; up
 * to `max` of them, each its token, into `out`. Their count, 0 when none. */
static int os_poll_wait(OsEvent *out, int max, int timeout_ms) {
    OVERLAPPED_ENTRY entries[64];
    ULONG n = 0;
    DWORD wait = timeout_ms < 0 ? INFINITE : (DWORD)timeout_ms;
    if (!GetQueuedCompletionStatusEx(os_port, entries, (ULONG)(max < 64 ? max : 64), &n, wait,
                                     FALSE)) {
        return 0;
    }
    for (ULONG i = 0; i < n; i++) {
        out[i].fd = -1;
        out[i].ready = 0;
        if (entries[i].lpCompletionKey != OS_TOKEN_IO) {
            out[i].token = (uint64_t)entries[i].lpCompletionKey;
            continue;
        }
        WinIo *io = (WinIo *)entries[i].lpOverlapped;
        io->bytes = entries[i].dwNumberOfBytesTransferred;
        out[i].token = __atomic_exchange_n(&io->packed, WIN_IO_DONE, __ATOMIC_ACQ_REL);
    }
    return (int)n;
}

/* Whether each signal (by token) has a packet on its way: one packet
 * stands for every raise until it is taken, as an eventfd's count does. */
static uint32_t os_raised[OS_TOKEN_WAKE + 1];

/* A signal another thread raises to wake the poll: its token. */
static OsSignal os_signal_new(uint64_t token) {
    os_poll_open();
    return token <= OS_TOKEN_WAKE ? (OsSignal)token : -1;
}

static void os_signal_raise(OsSignal s) {
    if (__atomic_exchange_n(&os_raised[s], 1, __ATOMIC_ACQ_REL) == 0 &&
        !PostQueuedCompletionStatus(os_port, 0, (ULONG_PTR)s, NULL)) {
        __atomic_store_n(&os_raised[s], 0, __ATOMIC_RELEASE);
    }
}

/* `s` was reported: lowered, so the next raise posts again. */
static void os_signal_take(OsSignal s) {
    __atomic_store_n(&os_raised[s], 0, __ATOMIC_RELEASE);
}

/* Waits while `*word` is `expected`, or until woken (spuriously too). */
static void os_wait_on(uint32_t *word, uint32_t expected) {
    WaitOnAddress(word, &expected, sizeof expected, INFINITE);
}

/* Wakes up to `n` threads waiting on `word`: one, or every one, which
 * waiters allow, since they look again when woken. */
static void os_wake_on(uint32_t *word, int n) {
    if (n == 1) {
        WakeByAddressSingle(word);
    } else {
        WakeByAddressAll(word);
    }
}

/* The CPUs in every processor group. */
static int os_cpus(void) {
    DWORD n = GetActiveProcessorCount(ALL_PROCESSOR_GROUPS);
    return n > 0 ? (int)n : 1;
}

static bool os_random64(uint64_t *out) {
    return ProcessPrng((PBYTE)out, sizeof *out) != FALSE;
}

/* The running thread's own stack: its lowest address and size. */
/* Unused until sockets and their faults are built for Windows. */
__attribute__((unused)) static bool os_thread_stack(void **base, size_t *size) {
    ULONG_PTR low = 0, high = 0;
    GetCurrentThreadStackLimits(&low, &high);
    *base = (void *)low;
    *size = (size_t)(high - low);
    return high > low;
}

/* No signals: nothing to block on a pipe a helper writes to. */
static void os_no_sigpipe(int fd) {
    (void)fd;
}

/* Whether stdout and stderr are consoles, settled at the start. */
static bool os_console_out, os_console_err;

static bool os_is_console(DWORD which) {
    DWORD mode;
    return GetConsoleMode(GetStdHandle(which), &mode) != 0;
}

/* Bytes in and out unchanged (no `\r\n` for `\n`), consoles noted, and no
 * error dialog box: a crash ends the program, as it does elsewhere. */
static void os_start(void) {
    _setmode(_fileno(stdin), _O_BINARY);
    _setmode(_fileno(stdout), _O_BINARY);
    _setmode(_fileno(stderr), _O_BINARY);
    os_console_out = os_is_console(STD_OUTPUT_HANDLE);
    os_console_err = os_is_console(STD_ERROR_HANDLE);
    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    // Sockets and name lookups use Winsock, which wants starting once.
    WSADATA wsa;
    (void)WSAStartup(MAKEWORD(2, 2), &wsa);
}

/* Memory: reserved, then committed by part (the object region's chunks). */
static void *os_reserve(size_t size) {
    return VirtualAlloc(NULL, size, MEM_RESERVE, PAGE_READWRITE);
}

static void os_commit(void *at, size_t size) {
    if (VirtualAlloc(at, size, MEM_COMMIT, PAGE_READWRITE) == NULL) {
        die("panic: out of memory\n");
    }
}

/* What a fresh task stack has committed at its top, and the guard page
 * below it that the system moves down as the stack grows. */
#define WIN_STACK_FIRST (16 * 1024)
#define WIN_PAGE 4096

static void os_stack_ready(char *stack, size_t size) {
    char *committed = stack + size - WIN_STACK_FIRST;
    DWORD old;
    if (VirtualAlloc(committed, WIN_STACK_FIRST, MEM_COMMIT, PAGE_READWRITE) == NULL ||
        VirtualAlloc(committed - WIN_PAGE, WIN_PAGE, MEM_COMMIT, PAGE_READWRITE) == NULL ||
        !VirtualProtect(committed - WIN_PAGE, WIN_PAGE, PAGE_READWRITE | PAGE_GUARD, &old)) {
        die("panic: out of memory\n");
    }
}

/* Decommitted whole, then ready again as a fresh one. */
static void os_stack_trim(char *stack, size_t size) {
    VirtualFree(stack, size, MEM_DECOMMIT);
    os_stack_ready(stack, size);
}

/* The system's guard already ends every stack: nothing more for C. */
static bool os_stack_guard(char *stack, size_t guard) {
    (void)stack;
    (void)guard;
    return true;
}

static bool os_stack_unguard(char *stack, size_t guard) {
    (void)stack;
    (void)guard;
    return true;
}

/* How far down the stack ending at `top` is committed now: the TEB's
 * `StackLimit` for it (x86_64.c). */
static char *win_committed_low(char *top) {
    MEMORY_BASIC_INFORMATION info;
    if (VirtualQuery(top - 1, &info, sizeof info) == 0) {
        die("panic: internal error: VirtualQuery failed\n");
    }
    return (char *)info.BaseAddress;
}

/* Room the system keeps for the handler once a stack overflowed. */
#define WIN_GUARANTEE (32 * 1024)

/* A C stack's overflow: reported once, without unwinding, as on Linux.
 * Anything else goes on to the system's own handling. */
static LONG CALLBACK os_on_fault(EXCEPTION_POINTERS *e) {
    if (e->ExceptionRecord->ExceptionCode != EXCEPTION_STACK_OVERFLOW) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    static int reported;
    if (__atomic_exchange_n(&reported, 1, __ATOMIC_ACQ_REL) != 0) {
        for (;;) {
            Sleep(INFINITE);
        }
    }
    fflush(stdout);
    static const char message[] = "panic: stack overflow\n";
    DWORD written;
    WriteFile(GetStdHandle(STD_ERROR_HANDLE), message, sizeof message - 1, &written, NULL);
    TerminateProcess(GetCurrentProcess(), 101);
    return EXCEPTION_CONTINUE_SEARCH;
}

static void os_thread_faults(void) {
    ULONG room = WIN_GUARANTEE;
    SetThreadStackGuarantee(&room);
}

static void os_faults_install(void) {
    AddVectoredExceptionHandler(1, os_on_fault);
    os_thread_faults();
}

/* The runtime's Ctrl-C handler, called on a thread the system starts. */
static void (*os_on_interrupt)(void);

static BOOL WINAPI os_console_event(DWORD event) {
    if (event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT) {
        os_on_interrupt();
        return TRUE;
    }
    return FALSE;
}

static void os_interrupts_install(void (*on)(void)) {
    os_on_interrupt = on;
    SetConsoleCtrlHandler(os_console_event, TRUE);
}

/* Ctrl-C comes on a thread of its own: workers needn't leave it. */
static void os_worker_signals(void) {
}

static int os_thread_start(pthread_t *thread, const pthread_attr_t *attr, void *(*run)(void *),
                           void *arg) {
    return pthread_create(thread, attr, run, arg);
}

/* UTF-8 text to a console as UTF-16, after what C's buffer holds; bytes
 * that aren't UTF-8 show as U+FFFD. Anything else takes the bytes. */
static void os_write(FILE *f, const char *text, size_t len) {
    bool console = (f == stdout && os_console_out) || (f == stderr && os_console_err);
    if (!console || len == 0 || len > INT_MAX) {
        fwrite(text, 1, len, f);
        return;
    }
    fflush(f);
    int wide = MultiByteToWideChar(CP_UTF8, 0, text, (int)len, NULL, 0);
    wchar_t small[512];
    wchar_t *buf = wide <= 512 ? small : malloc((size_t)wide * sizeof *buf);
    if (buf == NULL) {
        die("panic: out of memory\n");
    }
    MultiByteToWideChar(CP_UTF8, 0, text, (int)len, buf, wide);
    HANDLE h = GetStdHandle(f == stdout ? STD_OUTPUT_HANDLE : STD_ERROR_HANDLE);
    DWORD written;
    WriteConsoleW(h, buf, (DWORD)wide, &written, NULL);
    if (buf != small) {
        free(buf);
    }
}

/* Appends `c` to the line being read, growing it as `getline` does. */
static void os_line_put(char **line, size_t *cap, size_t *len, char c) {
    if (*len + 2 > *cap) {
        size_t bigger = *cap < 128 ? 128 : *cap * 2;
        char *grown = realloc(*line, bigger);
        if (grown == NULL) {
            die("panic: out of memory\n");
        }
        *line = grown;
        *cap = bigger;
    }
    (*line)[(*len)++] = c;
    (*line)[*len] = '\0';
}

/* A console's next line, read as UTF-16 and given as UTF-8 with its line
 * end; -1 at the end of input (Ctrl-Z at a line's start) or an error. */
static int64_t os_console_line(char **line, size_t *cap) {
    HANDLE in = GetStdHandle(STD_INPUT_HANDLE);
    size_t len = 0;
    for (;;) {
        wchar_t got[256];
        DWORD n = 0;
        if (!ReadConsoleW(in, got, 256, &n, NULL) || n == 0) {
            return len > 0 ? (int64_t)len : -1;
        }
        if (len == 0 && got[0] == 0x1a) {
            return -1;
        }
        char utf8[256 * 3];
        int bytes = WideCharToMultiByte(CP_UTF8, 0, got, (int)n, utf8, sizeof utf8, NULL, NULL);
        for (int i = 0; i < bytes; i++) {
            os_line_put(line, cap, &len, utf8[i]);
        }
        if (got[n - 1] == L'\n') {
            return (int64_t)len;
        }
    }
}

/* `getline`, which Windows' C runtime lacks; a console read as UTF-16.
 * The caller holds the stream's lock. */
static int64_t os_read_line(FILE *f, char **line, size_t *cap) {
    DWORD mode;
    if (f == stdin && GetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), &mode)) {
        return os_console_line(line, cap);
    }
    size_t len = 0;
    int c;
    while ((c = _getc_nolock(f)) != EOF) {
        os_line_put(line, cap, &len, (char)c);
        if (c == '\n') {
            break;
        }
    }
    return len == 0 ? -1 : (int64_t)len;
}

/* The C runtime's own stream lock, which nests. */
static void os_stream_lock(FILE *f) {
    _lock_file(f);
}

static void os_stream_unlock(FILE *f) {
    _unlock_file(f);
}

/* UTF-16 `wide` as UTF-8 from `malloc`, or NULL when it isn't valid. */
static char *win_utf8(const wchar_t *wide) {
    int n = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, -1, NULL, 0, NULL, NULL);
    char *text = n > 0 ? malloc((size_t)n) : NULL;
    if (text != NULL) {
        WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, -1, text, n, NULL, NULL);
    }
    return text;
}

/* The arguments as UCRT split them (`__wargv`, once asked for), from
 * their UTF-16 originals, not the code page C's `argv` was converted to;
 * kept for the program's life. */
static void os_args(int32_t *argc, char ***argv) {
    if (_configure_wide_argv(_crt_argv_unexpanded_arguments) != 0 || __wargv == NULL) {
        return;
    }
    char **utf8 = calloc((size_t)__argc + 1, sizeof *utf8);
    if (utf8 == NULL) {
        die("panic: out of memory\n");
    }
    for (int i = 0; i < __argc; i++) {
        utf8[i] = win_utf8(__wargv[i]);
        // An argument that isn't valid UTF-16 keeps C's own form.
        if (utf8[i] == NULL) {
            utf8[i] = i < *argc ? (*argv)[i] : "";
        }
    }
    *argc = __argc;
    *argv = utf8;
}

/* A variable's UTF-16 value, as UTF-8. */
static char *os_env(const char *name) {
    int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, name, -1, NULL, 0);
    if (n <= 0) {
        return NULL;
    }
    wchar_t *wide = malloc((size_t)n * sizeof *wide);
    if (wide == NULL) {
        die("panic: out of memory\n");
    }
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, name, -1, wide, n);
    const wchar_t *value = _wgetenv(wide);
    free(wide);
    return value == NULL ? NULL : win_utf8(value);
}

/* A program started without a stream finds its writes simply fail. */
static void os_std_streams_open(void) {
}

static int os_dup(int fd) {
    return _dup(fd);
}

/* The local time's fields read back as if they were UTC: the difference
 * is the offset. */
static bool os_local_offset(int64_t seconds, int64_t *out) {
    time_t t = (time_t)seconds;
    struct tm parts;
    if ((int64_t)t != seconds || localtime_s(&parts, &t) != 0) {
        return false;
    }
    time_t as_utc = _mkgmtime(&parts);
    if (as_utc == (time_t)-1) {
        return false;
    }
    *out = (int64_t)(as_utc - t);
    return true;
}
