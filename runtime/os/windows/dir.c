/*
 * os/windows/dir.c — part of lugha_rt.c on Windows: a `Dir`'s built-ins.
 * A `Dir` is a folder's handle, and every path given to one is looked up
 * beneath it by `win_resolve` (dir_resolve.c), which refuses an absolute
 * path, a `..` above the folder and a link leading out, whatever runs at
 * the same time. There is no unconfined fallback.
 *
 * Listing, making, removing, renaming, and what a path names (its kind,
 * size, mode, syncing) are in dir_names.c, joined right after.
 *
 * Every lookup, read and write runs on a helper thread while the task
 * waits, the `Dir` kept alive until the job is done, as on POSIX. Handles
 * are never inheritable, so a started program receives none. A file
 * stream's handle becomes a C descriptor (`_open_osfhandle`), so `File`
 * streams (io/file.c) work as they do elsewhere.
 *
 * A fragment, not compiled on its own: joined after dir_resolve.c, fs.c
 * (`fs_fail`), bytes.c (`bytes_of`) and input.c (`string_of`).
 */

/* A `Dir`'s folder: its handle, NULL once closed. */
typedef struct {
    HANDLE h;
} LughaDir;

static const char DIR_OUTSIDE[] = "path leads outside the folder";

/* A `Dir` object's visit: freeing it closes its handle. */
static void dir_visit(void *object, int64_t op) {
    LughaDir *d = object;
    if (op == RC_RELEASE && d->h != NULL) {
        CloseHandle(d->h);
        d->h = NULL;
    }
}

/* A counted `Dir` object owning `h`, closed when its last holder goes. */
static LughaDir *dir_wrap(HANDLE h) {
    LughaDir *d = lugha_rt_rc_alloc((int64_t)sizeof *d, dir_visit);
    d->h = h;
    return d;
}

/* Fails with the message for `code`, a failure's `errno`. */
static int32_t dir_fail(LughaString **error, int code) {
    return fs_fail(error, code == EXDEV ? DIR_OUTSIDE : win_message(code));
}

/* Whether `path` may be looked up at all: not empty, no NUL byte. */
static bool dir_path_ok(const LughaString *path, LughaString **error) {
    if (path->len == 0) {
        fs_fail(error, "empty path");
        return false;
    }
    if (fs_has_nul(path)) {
        fs_fail(error, "path contains a NUL byte");
        return false;
    }
    return true;
}

/* The folder handle a job works beneath, kept in `n`. */
static HANDLE dir_job_dir(const Job *job) {
    return (HANDLE)(intptr_t)job->n;
}

/* A handle a job opened (`found`) that its caller never took. */
static void dir_job_drop(Job *job) {
    if (job->found != NULL) {
        CloseHandle(job->found);
    }
}

/* A job on `path` beneath `dir`, the folder kept alive while it runs; or
 * NULL with the message set (an empty path, a NUL byte). */
static Job *dir_job(void (*run)(Job *), const void *dir, const LughaString *path,
                    LughaString **error) {
    if (!dir_path_ok(path, error)) {
        return NULL;
    }
    Job *job = job_new(run);
    job->n = (int64_t)(intptr_t)((const LughaDir *)dir)->h;
    job->hold = (void *)dir;
    job->path = job_copy(path->bytes, (size_t)path->len);
    job->drop = dir_job_drop;
    return job;
}

/* On a helper: the job's path opened beneath its folder as `want` says,
 * or NULL with the failure kept. */
static HANDLE dir_job_open(Job *job, ACCESS_MASK access, int kind, bool create) {
    WinWant want = {access, kind, create};
    HANDLE h = win_resolve(dir_job_dir(job), job->path, &want);
    if (h == NULL) {
        job->err = errno;
    }
    return h;
}

/* A done job's failure as the message, 0; the job freed. */
static int32_t dir_job_failed(Job *job, LughaString **error) {
    int32_t failed = dir_fail(error, job->err);
    job_free(job);
    return failed;
}

/* `path` (UTF-8) opened as a folder through Windows' own resolution, from
 * the working folder: the program's starting points, `fs.cwd()` and
 * `fs.dir(path)`. */
static void dir_plain_job(Job *job) {
    wchar_t *wide = malloc((strlen(job->path) + 1) * sizeof *wide);
    if (wide == NULL) {
        die("panic: out of memory\n");
    }
    if (win_wide(job->path, strlen(job->path), wide, (int)strlen(job->path) + 1) < 0) {
        free(wide);
        job->err = EINVAL;
        return;
    }
    HANDLE h = CreateFileW(wide, FILE_TRAVERSE | FILE_LIST_DIRECTORY | SYNCHRONIZE,
                           FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, NULL,
                           OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, NULL);
    free(wide);
    if (h == INVALID_HANDLE_VALUE) {
        job->err = win_errno_last();
        return;
    }
    FILE_ATTRIBUTE_TAG_INFO tag;
    if (!win_tag(h, &tag)) {
        job->err = errno;
        CloseHandle(h);
        return;
    }
    if ((tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY) == 0) {
        job->err = ENOTDIR;
        CloseHandle(h);
        return;
    }
    job->found = h;
}

static int32_t dir_plain(const char *path, size_t len, void **out, LughaString **error) {
    Job *job = job_new(dir_plain_job);
    job->drop = dir_job_drop;
    job->path = job_copy(path, len);
    job_run(job);
    if (job->found == NULL) {
        return dir_job_failed(job, error);
    }
    *out = dir_wrap(job->found);
    job->found = NULL;
    job_free(job);
    return 1;
}

/* The working folder's `Dir`, opened at the first `fs.cwd()` and held
 * here until exit. */
static LughaDir *dir_working;

static void dir_working_done(void) {
    lugha_rt_rc_release(dir_working);
}

/* `$os.dir_cwd()`: the working folder, confined like any `Dir`. */
int32_t lugha_rt_dir_cwd(void **out, LughaString **error) {
    RT_SECTION();
    if (dir_working == NULL) {
        void *opened;
        if (!dir_plain(".", 1, &opened, error)) {
            return 0;
        }
        // Another task may have opened it while this one waited.
        if (dir_working != NULL) {
            lugha_rt_rc_release(opened);
        } else {
            dir_working = opened;
            lugha_rt_rc_mark(dir_working);
            atexit(dir_working_done);
        }
    }
    lugha_rt_rc_retain(dir_working);
    *out = dir_working;
    return 1;
}

/* `$os.dir_open(path)`: `fs.dir(path)`, the one unconfined lookup. */
int32_t lugha_rt_dir_open(const LughaString *path, void **out, LughaString **error) {
    if (!dir_path_ok(path, error)) {
        return 0;
    }
    return dir_plain(path->bytes, (size_t)path->len, out, error);
}

/* On a helper: the job's path opened beneath its folder as `flags` (the
 * access), `value` (the kind) and `lookup` (making a missing file) say. */
static void dir_open_job(Job *job) {
    job->found = dir_job_open(job, (ACCESS_MASK)job->flags, (int)job->value, job->lookup);
    // Emptied when it is opened to be written afresh.
    if (job->found != NULL && job->in_len == 1) {
        FILE_END_OF_FILE_INFO end = {.EndOfFile = {.QuadPart = 0}};
        if (!SetFileInformationByHandle(job->found, FileEndOfFileInfo, &end, sizeof end)) {
            job->err = win_errno_last();
            CloseHandle(job->found);
            job->found = NULL;
        }
    }
}

/* `path` beneath `dir` opened with `access`, as `kind` (0 either, 1 file,
 * 2 folder), made when missing if `create`, emptied if `empty`: 1 and the
 * handle, or 0 and the message. */
static int32_t dir_open_handle(void *dir, const LughaString *path, ACCESS_MASK access, int kind,
                               bool create, bool empty, HANDLE *out, LughaString **error) {
    Job *job = dir_job(dir_open_job, dir, path, error);
    if (job == NULL) {
        return 0;
    }
    job->flags = (int)access;
    job->value = kind;
    job->lookup = create;
    job->in_len = empty ? 1 : 0;
    job_run(job);
    if (job->found == NULL) {
        return dir_job_failed(job, error);
    }
    *out = job->found;
    job->found = NULL;
    job_free(job);
    return 1;
}

/* `$os.dir_sub(dir, path)`: a folder beneath `dir`, a narrower `Dir`. */
int32_t lugha_rt_dir_sub(void *dir, const LughaString *path, void **out, LughaString **error) {
    HANDLE h;
    if (!dir_open_handle(dir, path, FILE_TRAVERSE | FILE_LIST_DIRECTORY, 2, false, false, &h,
                         error)) {
        return 0;
    }
    *out = dir_wrap(h);
    return 1;
}

/* On a helper: everything readable from the job's path, in `out`. */
static void dir_slurp_job(Job *job) {
    HANDLE h = dir_job_open(job, GENERIC_READ, 1, false);
    if (h == NULL) {
        return;
    }
    size_t cap = 0;
    for (;;) {
        job_grow(&job->out, &cap, job->out_len, 65536);
        DWORD want = cap - job->out_len > 1u << 30 ? 1u << 30 : (DWORD)(cap - job->out_len);
        DWORD got = 0;
        if (!ReadFile(h, job->out + job->out_len, want, &got, NULL)) {
            job->err = win_errno_last();
            break;
        }
        if (got == 0) {
            break;
        }
        job->out_len += got;
    }
    CloseHandle(h);
}

static Job *dir_slurp(void *dir, const LughaString *path, LughaString **error) {
    Job *job = dir_job(dir_slurp_job, dir, path, error);
    if (job == NULL) {
        return NULL;
    }
    job_run(job);
    if (job->err != 0) {
        dir_job_failed(job, error);
        return NULL;
    }
    return job;
}

/* `$os.dir_read_bytes(dir, path)`: the whole file as bytes. */
int32_t lugha_rt_dir_read_bytes(void *dir, const LughaString *path, LughaArray **out,
                                LughaString **error) {
    Job *job = dir_slurp(dir, path, error);
    if (job == NULL) {
        return 0;
    }
    *out = bytes_of(job->out, (int64_t)job->out_len);
    job_free(job);
    return 1;
}

/* `$os.dir_read_text(dir, path)`: the whole file as text, its bytes as
 * they are. */
int32_t lugha_rt_dir_read_text(void *dir, const LughaString *path, LughaString **out,
                               LughaString **error) {
    Job *job = dir_slurp(dir, path, error);
    if (job == NULL) {
        return 0;
    }
    *out = string_of(job->out == NULL ? "" : job->out, (int64_t)job->out_len);
    job_free(job);
    return 1;
}

/* On a helper: `in` written to the job's path, emptied first (`flags` 0)
 * or at its end (1, every write landing there), made when missing. */
static void dir_write_job(Job *job) {
    bool append = job->flags == 1;
    HANDLE h = dir_job_open(job, append ? FILE_APPEND_DATA : GENERIC_WRITE, 1, true);
    if (h == NULL) {
        return;
    }
    FILE_END_OF_FILE_INFO end = {.EndOfFile = {.QuadPart = 0}};
    if (!append && !SetFileInformationByHandle(h, FileEndOfFileInfo, &end, sizeof end)) {
        job->err = win_errno_last();
        CloseHandle(h);
        return;
    }
    size_t done = 0;
    while (done < job->in_len) {
        size_t left = job->in_len - done;
        DWORD wrote = 0;
        if (!WriteFile(h, job->in + done, left > 1u << 30 ? 1u << 30 : (DWORD)left, &wrote, NULL)) {
            job->err = win_errno_last();
            break;
        }
        done += wrote;
    }
    CloseHandle(h);
}

/* `$os.dir_write(dir, path, bytes, append)`: replaces the file's contents
 * (`append` 0) or adds to its end (1), making it when missing. */
int32_t lugha_rt_dir_write(void *dir, const LughaString *path, const LughaArray *bytes,
                           int64_t append, int64_t *count, LughaString **error) {
    Job *job = dir_job(dir_write_job, dir, path, error);
    if (job == NULL) {
        return 0;
    }
    job->flags = append ? 1 : 0;
    job->in = job_copy((const char *)bytes->elems, (size_t)bytes->len);
    job->in_len = (size_t)bytes->len;
    job_run(job);
    if (job->err != 0) {
        return dir_job_failed(job, error);
    }
    job_free(job);
    *count = bytes->len;
    return 1;
}

/* `$os.dir_file(dir, path, mode)`: a file stream's descriptor: mode 0
 * reads, 1 creates or empties, 2 appends (making it). */
int32_t lugha_rt_dir_file(void *dir, const LughaString *path, int64_t mode, int64_t *out,
                          LughaString **error) {
    ACCESS_MASK access = mode == 0 ? GENERIC_READ : mode == 1 ? GENERIC_WRITE : FILE_APPEND_DATA;
    HANDLE h;
    if (!dir_open_handle(dir, path, access, 1, mode != 0, mode == 1, &h, error)) {
        return 0;
    }
    int flags = _O_BINARY | (mode == 0 ? _O_RDONLY : mode == 1 ? _O_WRONLY : _O_WRONLY | _O_APPEND);
    int fd = _open_osfhandle((intptr_t)h, flags);
    if (fd < 0) {
        CloseHandle(h);
        return dir_fail(error, EMFILE);
    }
    *out = fd;
    return 1;
}

/* On a helper: where the job's folder is now, as UTF-8 in `out`, without
 * the `\\?\` Windows puts first (`\\?\UNC\` becomes `\\`). */
static void dir_path_job(Job *job) {
    // Windows' longest path, too big for a helper's stack.
    enum { LONGEST = 32768 };
    wchar_t *wide = malloc(LONGEST * sizeof *wide);
    if (wide == NULL) {
        die("panic: out of memory\n");
    }
    DWORD n = GetFinalPathNameByHandleW(dir_job_dir(job), wide, LONGEST,
                                        FILE_NAME_NORMALIZED | VOLUME_NAME_DOS);
    if (n == 0 || n >= LONGEST) {
        job->err = n == 0 ? win_errno_last() : ENAMETOOLONG;
        free(wide);
        return;
    }
    const wchar_t *from = wide;
    bool unc = wcsncmp(wide, L"\\\\?\\UNC\\", 8) == 0;
    if (unc) {
        from = wide + 6;
    } else if (wcsncmp(wide, L"\\\\?\\", 4) == 0) {
        from = wide + 4;
    }
    int bytes = WideCharToMultiByte(CP_UTF8, 0, from, -1, NULL, 0, NULL, NULL);
    job->out = malloc((size_t)bytes + 1);
    if (job->out == NULL) {
        die("panic: out of memory\n");
    }
    WideCharToMultiByte(CP_UTF8, 0, from, -1, job->out, bytes, NULL, NULL);
    if (unc) {
        job->out[0] = '\\';
    }
    job->out_len = strlen(job->out);
    free(wide);
}

/* `$os.dir_path(dir)`: where the folder is now, as bytes. */
int32_t lugha_rt_dir_path(void *dir, LughaArray **out, LughaString **error) {
    Job *job = job_new(dir_path_job);
    job->n = (int64_t)(intptr_t)((LughaDir *)dir)->h;
    job->hold = dir;
    job_run(job);
    if (job->err != 0) {
        return dir_job_failed(job, error);
    }
    *out = bytes_of(job->out, (int64_t)job->out_len);
    job_free(job);
    return 1;
}
