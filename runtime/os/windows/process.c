/*
 * os/windows/process.c — part of lugha_rt.c on Windows: `Proc`'s one
 * built-in. Starting processes is security-sensitive: every change needs a
 * review.
 *
 * - No shell: the program found (os/windows/command_line.c) is named to
 *   `CreateProcessW` by its path, so Windows searches nothing, and the
 *   arguments are quoted so each arrives as given. A batch file, a command
 *   holding `"`, a NUL byte anywhere and a command line longer than Windows
 *   allows are refused before anything starts.
 * - The child inherits this program's environment and folder unchanged,
 *   and exactly three handles, named in `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`:
 *   its stdin, stdout and stderr. Nothing else the runtime or C code holds
 *   reaches it, inheritable or not.
 * - stdout and stderr are each read on a thread of their own while stdin is
 *   written, so a child filling one pipe can't deadlock the others; a
 *   child that closes stdin early just ends the input. A captured run
 *   starts with no window.
 * - The exit code is Windows' own: a crash's exception code comes out
 *   negative. There is no timeout and no output limit.
 * - Everything from starting the child to waiting for it runs on a helper
 *   thread, with copies of the command line and input, while the task
 *   waits; a task cancelled meanwhile stops waiting, and the helper still
 *   waits for the child before closing its handles.
 *
 * A fragment, not compiled on its own: joined after command_line.c.
 */

/* `errno` values with no Windows counterpart: a batch file refused. */
enum { WIN_SPAWN_BATCH = 10200 };

/* One output pipe's reading end and what came through it, from `malloc`. */
typedef struct {
    HANDLE from;
    char *bytes;
    size_t len;
    size_t cap;
} WinDrain;

/* On its own thread: reads `arg`'s pipe to its end. */
static void *win_drain(void *arg) {
    WinDrain *d = arg;
    for (;;) {
        job_grow(&d->bytes, &d->cap, d->len, 4096);
        DWORD got = 0;
        if (!ReadFile(d->from, d->bytes + d->len, (DWORD)(d->cap - d->len), &got, NULL) ||
            got == 0) {
            return NULL;
        }
        d->len += got;
    }
}

/* Linux's words for a start's failure. */
static const char *win_spawn_message(int err) {
    switch (err) {
    case WIN_SPAWN_BATCH:
        return "a batch file runs through cmd.exe, a shell, which Proc never starts";
    case ENOENT:
        return "No such file or directory";
    case EACCES:
        return "Permission denied";
    case ENOEXEC:
        return "Exec format error";
    case ENOMEM:
        return "Cannot allocate memory";
    default:
        return "Input/output error";
    }
}

/* `errno` for the last failed start. */
static int win_spawn_errno(void) {
    switch (GetLastError()) {
    case ERROR_FILE_NOT_FOUND:
    case ERROR_PATH_NOT_FOUND:
        return ENOENT;
    case ERROR_ACCESS_DENIED:
        return EACCES;
    case ERROR_BAD_EXE_FORMAT:
    case ERROR_EXE_MACHINE_TYPE_MISMATCH:
        return ENOEXEC;
    case ERROR_NOT_ENOUGH_MEMORY:
    case ERROR_OUTOFMEMORY:
        return ENOMEM;
    default:
        return EIO;
    }
}

/* A pipe whose `child` end alone may be inherited; false on failure. */
static bool win_pipe(HANDLE *ours, HANDLE *child, bool child_reads) {
    HANDLE r, w;
    if (!CreatePipe(&r, &w, NULL, 0)) {
        return false;
    }
    *ours = child_reads ? w : r;
    *child = child_reads ? r : w;
    SetHandleInformation(*child, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
    return true;
}

/* An inheritable copy of this program's handle `which`, or NULL. */
static HANDLE win_std_copy(DWORD which) {
    HANDLE h = GetStdHandle(which);
    HANDLE copy = NULL;
    if (h == NULL || h == INVALID_HANDLE_VALUE ||
        !DuplicateHandle(GetCurrentProcess(), h, GetCurrentProcess(), &copy, 0, TRUE,
                         DUPLICATE_SAME_ACCESS)) {
        return NULL;
    }
    return copy;
}

/* Starts `path` (UTF-8) with `line` and the child's three handles `std`:
 * its process handle, or NULL with `job->err` set. */
static HANDLE win_start(Job *job, const char *path, const char *line, HANDLE std[3],
                        bool capture) {
    size_t path_n = strlen(path), line_n = strlen(line);
    wchar_t *wpath = malloc((path_n + 1) * sizeof *wpath);
    wchar_t *wline = malloc((line_n + 1) * sizeof *wline);
    if (wpath == NULL || wline == NULL) {
        die("panic: out of memory\n");
    }
    HANDLE process = NULL;
    LPPROC_THREAD_ATTRIBUTE_LIST attrs = NULL;
    // Each handle once, none NULL: what the list allows.
    HANDLE list[3];
    int listed = 0;
    for (int i = 0; i < 3; i++) {
        bool seen = std[i] == NULL;
        for (int k = 0; k < listed && !seen; k++) {
            seen = list[k] == std[i];
        }
        if (!seen) {
            list[listed++] = std[i];
        }
    }
    SIZE_T size = 0;
    InitializeProcThreadAttributeList(NULL, 1, 0, &size);
    attrs = malloc(size);
    if (attrs == NULL) {
        die("panic: out of memory\n");
    }
    STARTUPINFOEXW si;
    memset(&si, 0, sizeof si);
    si.StartupInfo.cb = sizeof si;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = std[0];
    si.StartupInfo.hStdOutput = std[1];
    si.StartupInfo.hStdError = std[2];
    PROCESS_INFORMATION pi;
    if (win_wide(path, path_n, wpath, (int)path_n + 1) < 0 ||
        win_wide(line, line_n, wline, (int)line_n + 1) < 0 ||
        !InitializeProcThreadAttributeList(attrs, 1, 0, &size) ||
        (listed > 0 && !UpdateProcThreadAttribute(attrs, 0, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, list,
                                                  (SIZE_T)listed * sizeof(HANDLE), NULL, NULL))) {
        job->err = EIO;
    } else {
        si.lpAttributeList = attrs;
        DWORD flags = EXTENDED_STARTUPINFO_PRESENT | (capture ? CREATE_NO_WINDOW : 0);
        // Always named by its path, so Windows searches nothing.
        if (CreateProcessW(wpath, wline, NULL, NULL, listed > 0, flags, NULL, NULL,
                           &si.StartupInfo, &pi)) {
            CloseHandle(pi.hThread);
            process = pi.hProcess;
        } else {
            job->err = win_spawn_errno();
        }
        DeleteProcThreadAttributeList(attrs);
    }
    free(attrs);
    free(wpath);
    free(wline);
    return process;
}

/* On a helper: finds `argv[0]`, starts it with the command line `path`
 * (mode `flags`, input `in`), serves its pipes and waits for it: its code
 * in `value` and its outputs in `out` and `out2`, or the failure in `err`. */
static void spawn_job(Job *job) {
    int mode = job->flags;
    bool capture = mode != 0;
    char *program = win_find_program(job->argv[0]);
    if (program == NULL) {
        job->err = ENOENT;
        return;
    }
    // Checked on what was found as well as on what was asked for.
    if (win_is_batch(program)) {
        free(program);
        job->err = WIN_SPAWN_BATCH;
        return;
    }
    HANDLE std[3] = {NULL, NULL, NULL};
    HANDLE in = NULL;
    WinDrain out = {NULL, NULL, 0, 0}, err = {NULL, NULL, 0, 0};
    bool ready = true;
    if (!capture) {
        std[0] = win_std_copy(STD_INPUT_HANDLE);
        std[1] = win_std_copy(STD_OUTPUT_HANDLE);
        std[2] = win_std_copy(STD_ERROR_HANDLE);
    } else {
        ready = win_pipe(&out.from, &std[1], false) && win_pipe(&err.from, &std[2], false);
        if (ready && mode == 2) {
            ready = win_pipe(&in, &std[0], true);
        } else if (ready) {
            SECURITY_ATTRIBUTES inherit = {sizeof inherit, NULL, TRUE};
            std[0] = CreateFileW(L"NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                 &inherit, OPEN_EXISTING, 0, NULL);
            ready = std[0] != INVALID_HANDLE_VALUE;
            if (!ready) {
                std[0] = NULL;
            }
        }
        if (!ready) {
            job->err = win_spawn_errno();
        }
    }
    HANDLE process = ready ? win_start(job, program, job->path, std, capture) : NULL;
    free(program);
    // The child's ends are the child's now (or nobody's).
    for (int i = 0; i < 3; i++) {
        if (std[i] != NULL) {
            CloseHandle(std[i]);
        }
    }
    pthread_t readers[2];
    bool reading = process != NULL && capture;
    if (reading) {
        if (pthread_create(&readers[0], NULL, win_drain, &out) != 0 ||
            pthread_create(&readers[1], NULL, win_drain, &err) != 0) {
            die("panic: can't start a thread to read a program's output\n");
        }
    }
    if (in != NULL) {
        size_t written = 0;
        while (process != NULL && written < job->in_len) {
            DWORD put = 0;
            DWORD chunk = (DWORD)(job->in_len - written < (1u << 30) ? job->in_len - written
                                                                      : (1u << 30));
            // A child that closed its stdin early just ends the input.
            if (!WriteFile(in, job->in + written, chunk, &put, NULL)) {
                break;
            }
            written += put;
        }
        CloseHandle(in);
    }
    if (reading) {
        pthread_join(readers[0], NULL);
        pthread_join(readers[1], NULL);
    }
    if (out.from != NULL) {
        CloseHandle(out.from);
    }
    if (err.from != NULL) {
        CloseHandle(err.from);
    }
    if (process == NULL) {
        free(out.bytes);
        free(err.bytes);
        return;
    }
    // Waited for even when its task stopped waiting.
    WaitForSingleObject(process, INFINITE);
    DWORD code = 0;
    GetExitCodeProcess(process, &code);
    CloseHandle(process);
    job->value = (int32_t)code;
    job->out = out.bytes;
    job->out_len = out.len;
    job->out2 = err.bytes;
    job->out2_len = err.len;
}

/* Fails with `message` (and, when `cmd` is given, `cmd: ` before it). */
static int32_t spawn_fail(LughaString **error, const char *cmd, const char *message) {
    char text[512];
    if (cmd != NULL) {
        snprintf(text, sizeof text, "%s: %s", cmd, message);
    } else {
        snprintf(text, sizeof text, "%s", message);
    }
    *error = string_of(text, (int64_t)strlen(text));
    return 0;
}

/* The most UTF-16 units a command line may hold, its NUL aside. */
enum { WIN_LINE_MOST = 32766 };

/* `$os.spawn(cmd, args, input, mode)`: runs `cmd` with `args` and waits.
 * `mode` 0 shares this program's stdin, stdout and stderr; 1 captures
 * stdout and stderr with an empty stdin; 2 also writes `input`, a `u8[]`,
 * to stdin. 1 with `*code` and the outputs' bytes, or 0 and `*error`. */
int32_t lugha_rt_spawn(const LughaString *cmd, const LughaArray *args, const LughaArray *input,
                       int32_t mode, int64_t *code, LughaArray **out, LughaArray **err,
                       LughaString **error) {
    const LughaString **list = (const LughaString **)args->elems;
    if (memchr(cmd->bytes, 0, (size_t)cmd->len) != NULL) {
        return spawn_fail(error, NULL, "command contains a NUL byte");
    }
    for (int64_t i = 0; i < args->len; i++) {
        if (memchr(list[i]->bytes, 0, (size_t)list[i]->len) != NULL) {
            char text[64];
            snprintf(text, sizeof text, "args[%" PRId64 "] contains a NUL byte", i);
            return spawn_fail(error, NULL, text);
        }
    }
    if (memchr(cmd->bytes, '"', (size_t)cmd->len) != NULL) {
        return spawn_fail(error, NULL, "command contains a quote, which no program's name holds");
    }
    /* The job's own copies: argv (the command, the arguments, NULL), the
     * command line in `path`, and the input. */
    Job *job = job_new(spawn_job);
    job->argv = calloc((size_t)args->len + 2, sizeof(char *));
    if (job->argv == NULL) {
        die("panic: out of memory\n");
    }
    job->argv[0] = job_copy(cmd->bytes, (size_t)cmd->len);
    for (int64_t i = 0; i < args->len; i++) {
        job->argv[i + 1] = job_copy(list[i]->bytes, (size_t)list[i]->len);
    }
    if (win_is_batch(job->argv[0])) {
        job_free(job);
        return spawn_fail(error, cmd->bytes, win_spawn_message(WIN_SPAWN_BATCH));
    }
    WinLine line = {NULL, 0, 0};
    win_line_build(&line, job->argv);
    job->path = line.bytes;
    // A UTF-16 unit takes at least one UTF-8 byte: a longer line is over.
    int units = line.len > WIN_LINE_MOST * 3 ? INT_MAX
                    : MultiByteToWideChar(CP_UTF8, 0, line.bytes, (int)line.len, NULL, 0);
    if (units > WIN_LINE_MOST) {
        job_free(job);
        return spawn_fail(error, NULL, "command line is longer than Windows allows");
    }
    job->flags = mode;
    if (mode == 2) {
        job->in = job_copy((const char *)input->elems, (size_t)input->len);
        job->in_len = (size_t)input->len;
    }
    /* What this program printed so far comes before what the child prints. */
    fflush(NULL);
    job_run(job);
    if (job->err != 0) {
        int32_t failed = spawn_fail(error, cmd->bytes, win_spawn_message(job->err));
        job_free(job);
        return failed;
    }
    *code = job->value;
    *out = bytes_of(job->out, (int64_t)job->out_len);
    *err = bytes_of(job->out2, (int64_t)job->out2_len);
    job_free(job);
    return 1;
}
