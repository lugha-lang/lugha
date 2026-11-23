/*
 * dir.c — part of lugha_rt.c: a `Dir`'s built-ins. A `Dir` is a directory
 * descriptor (`O_PATH | O_DIRECTORY`), and every path given to one is
 * opened beneath it with `openat2` and `RESOLVE_BENEATH |
 * RESOLVE_NO_MAGICLINKS`: the kernel refuses an absolute path, a `..` above
 * the folder, and a symbolic link leading out, checking each step, so a
 * folder swapped for a link mid-open is caught too. Without `openat2`
 * (Linux before 5.6, or a sandbox blocking it) every operation fails: there
 * is no unconfined fallback.
 *
 * Making, removing, renaming, and what a path names (its kind, size, mode,
 * syncing) are in dir_names.c, joined right after.
 *
 * Every lookup, read and write runs on a helper thread (helpers.c) while
 * the task waits, the `Dir` kept alive until the job is done; checking a
 * path and making Lugha values stay on the task.
 *
 * Descriptors are close-on-exec, so a started program inherits none. A
 * `Dir`'s descriptor lives in a small counted object and is closed when the
 * last `Dir` holding it is gone. The working folder's `Dir` is opened once
 * and kept until exit.
 *
 * A fragment, not compiled on its own: joined after the core (lugha_rt.c),
 * whose includes (`<linux/openat2.h>`, `<sys/syscall.h>`) it relies on, and
 * after fs.c (`fs_fail`, `fs_has_nul`), bytes.c (`bytes_of`) and input.c
 * (`string_of`).
 */
/* A `Dir`'s folder: its descriptor, -1 once closed. */
typedef struct {
    int fd;
} LughaDir;

static const char DIR_OUTSIDE[] = "path leads outside the folder";

/* How often a lookup that raced a rename (`EAGAIN`) is tried again. */
enum { DIR_RETRIES = 8 };

/* A `Dir` object's visit: freeing it closes its descriptor. */
static void dir_visit(void *object, int64_t op) {
    LughaDir *d = object;
    if (op == RC_RELEASE && d->fd >= 0) {
        close(d->fd);
        d->fd = -1;
    }
}

/* A counted `Dir` object owning `fd`, closed when its last holder goes. */
static LughaDir *dir_wrap(int fd) {
    LughaDir *d = lugha_rt_rc_alloc((int64_t)sizeof *d, dir_visit);
    d->fd = fd;
    return d;
}

/* `path` opened beneath `dirfd` with `flags` (and `mode` when it creates):
 * the descriptor, or -1 with `errno` set. */
static int dir_resolve(int dirfd, const char *path, int flags, mode_t mode) {
    struct open_how how;
    memset(&how, 0, sizeof how);
    how.flags = (uint64_t)(flags | O_CLOEXEC);
    how.mode = (flags & O_CREAT) ? (uint64_t)mode : 0;
    how.resolve = RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS;
    for (int tries = 0;; tries++) {
        long fd = syscall(SYS_openat2, dirfd, path, &how, sizeof how);
        if (fd >= 0) {
            return (int)fd;
        }
        if (errno == EINTR || (errno == EAGAIN && tries < DIR_RETRIES)) {
            continue;
        }
        return -1;
    }
}

/* Fails with the message for `code`, a failed lookup's `errno`. */
static int32_t dir_fail(LughaString **error, int code) {
    if (code == EXDEV) {
        return fs_fail(error, DIR_OUTSIDE);
    }
    if (code == ENOSYS) {
        return fs_fail(error, "this system can't confine a folder: openat2 needs Linux 5.6");
    }
    return fs_fail(error, strerror(code));
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

/* A job on `path` beneath `dir`, the folder kept alive while it runs; or
 * NULL with the message set (an empty path, a NUL byte). */
static Job *dir_job(void (*run)(Job *), const void *dir, const LughaString *path,
                    LughaString **error) {
    if (!dir_path_ok(path, error)) {
        return NULL;
    }
    Job *job = job_new(run);
    job->fd = ((const LughaDir *)dir)->fd;
    job->hold = (void *)dir;
    job->path = job_copy(path->bytes, (size_t)path->len);
    return job;
}

/* On a helper: the job's path opened beneath its folder, or -1 with the
 * failure kept. */
static int dir_job_open(Job *job, int flags, mode_t mode) {
    int fd = dir_resolve(job->fd, job->path, flags, mode);
    if (fd < 0) {
        job->err = errno;
        job->lookup = true;
    }
    return fd;
}

/* A done job's failure as the message, 0; the job freed. */
static int32_t dir_job_failed(Job *job, LughaString **error) {
    int32_t failed = job->lookup ? dir_fail(error, job->err) : fs_fail(error, strerror(job->err));
    job_free(job);
    return failed;
}

/* A descriptor a job opened (`fd2`) that its caller never took. */
static void dir_job_drop(Job *job) {
    if (job->fd2 >= 0) {
        close(job->fd2);
    }
}

/* `path` opened as a folder handle (`O_PATH | O_DIRECTORY`) from the
 * working folder with ordinary resolution: -1 with `errno` set. */
static int dir_open_plain(const char *path) {
    for (;;) {
        int fd = open(path, O_PATH | O_DIRECTORY | O_CLOEXEC);
        if (fd >= 0 || errno != EINTR) {
            return fd;
        }
    }
}

/* On a helper: the job's path opened from the working folder (`fd2`). */
static void dir_plain_job(Job *job) {
    job->fd2 = dir_open_plain(job->path);
    if (job->fd2 < 0) {
        job->err = errno;
    }
}

/* `path` (NUL-free) opened from the working folder as a `Dir`: 1, or 0
 * and the message. */
static int32_t dir_plain(const char *path, size_t len, void **out, LughaString **error) {
    Job *job = job_new(dir_plain_job);
    job->drop = dir_job_drop;
    job->path = job_copy(path, len);
    job_run(job);
    if (job->fd2 < 0) {
        return dir_job_failed(job, error);
    }
    *out = dir_wrap(job->fd2);
    job->fd2 = -1;
    job_free(job);
    return 1;
}

/* The working folder's `Dir`, opened at the first `fs.cwd()` and held
 * here until exit, when the runtime lets go of it. */
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
            // Every task asking for the working folder gets this one.
            lugha_rt_rc_mark(dir_working);
            // Registered after the report at exit, so it runs before it.
            atexit(dir_working_done);
        }
    }
    // The caller gets a holder of its own.
    lugha_rt_rc_retain(dir_working);
    *out = dir_working;
    return 1;
}

/* `$os.dir_open(path)`: `fs.dir(path)`, the one unconfined lookup: `path`
 * as the system resolves it from the working folder. */
int32_t lugha_rt_dir_open(const LughaString *path, void **out, LughaString **error) {
    if (!dir_path_ok(path, error)) {
        return 0;
    }
    return dir_plain(path->bytes, (size_t)path->len, out, error);
}

/* On a helper: the job's path opened beneath its folder with `flags` and
 * mode 0666 when it creates (`fd2`). */
static void dir_open_job(Job *job) {
    job->fd2 = dir_job_open(job, job->flags, 0666);
}

/* `path` opened beneath `dir` with `flags`: 1 and the descriptor, or 0
 * and the message. */
static int32_t dir_open_fd(void *dir, const LughaString *path, int flags, int *fd,
                           LughaString **error) {
    Job *job = dir_job(dir_open_job, dir, path, error);
    if (job == NULL) {
        return 0;
    }
    job->drop = dir_job_drop;
    job->flags = flags;
    job_run(job);
    if (job->fd2 < 0) {
        return dir_job_failed(job, error);
    }
    *fd = job->fd2;
    job->fd2 = -1;
    job_free(job);
    return 1;
}

/* `$os.dir_sub(dir, path)`: a folder beneath `dir`, a narrower `Dir`. */
int32_t lugha_rt_dir_sub(void *dir, const LughaString *path, void **out, LughaString **error) {
    int fd;
    if (!dir_open_fd(dir, path, O_PATH | O_DIRECTORY, &fd, error)) {
        return 0;
    }
    *out = dir_wrap(fd);
    return 1;
}

/* On a helper: everything readable from the job's path, in `out`. */
static void dir_slurp_job(Job *job) {
    int fd = dir_job_open(job, O_RDONLY, 0);
    if (fd < 0) {
        return;
    }
    size_t cap = 0;
    for (;;) {
        job_grow(&job->out, &cap, job->out_len, 4096);
        ssize_t got = read(fd, job->out + job->out_len, cap - job->out_len);
        if (got < 0 && errno == EINTR) {
            continue;
        }
        if (got < 0) {
            job->err = errno;
            break;
        }
        if (got == 0) {
            break;
        }
        job->out_len += (size_t)got;
    }
    close(fd);
}

/* The whole file at `path` beneath `dir`, done: the job, or NULL with the
 * message set. */
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
 * they are, never checked as UTF-8 here. */
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

/* On a helper: `in` written to the job's path, opened with `flags`. */
static void dir_write_job(Job *job) {
    int fd = dir_job_open(job, job->flags, 0666);
    if (fd < 0) {
        return;
    }
    size_t done = 0;
    while (done < job->in_len) {
        ssize_t n = write(fd, job->in + done, job->in_len - done);
        if (n < 0 && errno == EINTR) {
            continue;
        }
        if (n < 0) {
            job->err = errno;
            close(fd);
            return;
        }
        done += (size_t)n;
    }
    if (close(fd) != 0 && errno != EINTR) {
        job->err = errno;
    }
}

/* `$os.dir_write(dir, path, bytes, append)`: replaces the file's contents
 * (`append` 0) or adds to its end (1), making it (mode 0666 less the umask)
 * when missing; the bytes written. */
int32_t lugha_rt_dir_write(void *dir, const LughaString *path, const LughaArray *bytes,
                           int64_t append, int64_t *count, LughaString **error) {
    Job *job = dir_job(dir_write_job, dir, path, error);
    if (job == NULL) {
        return 0;
    }
    job->flags = O_WRONLY | O_CREAT | (append ? O_APPEND : O_TRUNC);
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

/* On a helper: every entry's name but `.` and `..`, each ending in NUL,
 * in `out`, and how many in `value`. */
static void dir_list_job(Job *job) {
    int fd = dir_job_open(job, O_RDONLY | O_DIRECTORY, 0);
    if (fd < 0) {
        return;
    }
    DIR *listing = fdopendir(fd);
    if (listing == NULL) {
        job->err = errno;
        close(fd);
        return;
    }
    size_t cap = 0;
    for (;;) {
        errno = 0;
        struct dirent *entry = readdir(listing);
        if (entry == NULL) {
            job->err = errno;
            break;
        }
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }
        size_t n = strlen(entry->d_name) + 1;
        job_grow(&job->out, &cap, job->out_len, n);
        memcpy(job->out + job->out_len, entry->d_name, n);
        job->out_len += n;
        job->value++;
    }
    closedir(listing);
}

/* `$os.dir_list(dir, path)`: every entry's name as bytes, without `.` and
 * `..`, in the order the system gives them. */
int32_t lugha_rt_dir_list(void *dir, const LughaString *path, LughaArray **out,
                          LughaString **error) {
    Job *job = dir_job(dir_list_job, dir, path, error);
    if (job == NULL) {
        return 0;
    }
    job_run(job);
    if (job->err != 0) {
        return dir_job_failed(job, error);
    }
    int64_t len = job->value;
    LughaArray *array =
        lugha_rt_alloc((int64_t)sizeof(LughaArray) + len * (int64_t)sizeof(LughaArray *));
    array->len = len;
    array->cap = len;
    rt_holds_pointers(array);
    LughaArray **names = (LughaArray **)array->elems;
    const char *name = job->out;
    for (int64_t i = 0; i < len; i++) {
        size_t n = strlen(name);
        names[i] = bytes_of(name, (int64_t)n);
        name += n + 1;
    }
    job_free(job);
    *out = array;
    return 1;
}

/* `$os.dir_file(dir, path, mode)`: a file stream's descriptor: mode 0
 * reads, 1 creates or empties, 2 appends (making it). */
int32_t lugha_rt_dir_file(void *dir, const LughaString *path, int64_t mode, int64_t *out,
                          LughaString **error) {
    int flags = mode == 0   ? O_RDONLY
                : mode == 1 ? O_WRONLY | O_CREAT | O_TRUNC
                            : O_WRONLY | O_CREAT | O_APPEND;
    int fd;
    if (!dir_open_fd(dir, path, flags, &fd, error)) {
        return 0;
    }
    *out = fd;
    return 1;
}

/* On a helper: where the job's folder is now, in `out`. */
static void dir_path_job(Job *job) {
    char self[64];
    snprintf(self, sizeof self, "/proc/self/fd/%d", job->fd);
    job->out = malloc(4096);
    if (job->out == NULL) {
        die("panic: out of memory\n");
    }
    ssize_t n = readlink(self, job->out, 4096);
    if (n < 0) {
        job->err = errno;
    } else if (n == 4096) {
        job->err = ENAMETOOLONG;
    } else {
        job->out_len = (size_t)n;
    }
}

/* `$os.dir_path(dir)`: where the folder is now, as bytes. */
int32_t lugha_rt_dir_path(void *dir, LughaArray **out, LughaString **error) {
    Job *job = job_new(dir_path_job);
    job->fd = ((LughaDir *)dir)->fd;
    job->hold = dir;
    job_run(job);
    if (job->err != 0) {
        return dir_job_failed(job, error);
    }
    *out = bytes_of(job->out, (int64_t)job->out_len);
    job_free(job);
    return 1;
}
