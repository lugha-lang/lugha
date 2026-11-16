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
 * Descriptors are close-on-exec, so a started program inherits none. A
 * `Dir`'s descriptor lives in a small collected object and is closed by a
 * finalizer when the last `Dir` holding it is gone. Since that waits for a
 * collection, opening anything that fails for want of descriptors
 * (`EMFILE`) first collects, closing every unreachable folder, and tries
 * once more; and the working folder's `Dir` is opened once and kept.
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

static void dir_finalize(void *object, void *unused) {
    (void)unused;
    LughaDir *d = object;
    if (d->fd >= 0) {
        close(d->fd);
        d->fd = -1;
    }
}

/* A collected `Dir` object owning `fd`, closed when it is collected. */
static LughaDir *dir_wrap(int fd) {
    // Atomic: the object holds no pointers for the collector to follow.
    LughaDir *d = GC_MALLOC_ATOMIC(sizeof *d);
    if (d == NULL) {
        die("panic: out of memory\n");
    }
    d->fd = fd;
    GC_register_finalizer(d, dir_finalize, NULL, NULL, NULL);
    return d;
}

/* After an open failed with `EMFILE`: collects, so the finalizers close the
 * descriptors of folders nothing reaches any more; true when one more try
 * is worth it. */
static bool dir_reclaim(bool *tried) {
    if (errno != EMFILE || *tried) {
        return false;
    }
    *tried = true;
    GC_gcollect();
    GC_invoke_finalizers();
    return true;
}

/* `path` opened beneath `dirfd` with `flags` (and `mode` when it creates):
 * the descriptor, or -1 with `errno` set. */
static int dir_resolve(int dirfd, const char *path, int flags, mode_t mode) {
    struct open_how how;
    memset(&how, 0, sizeof how);
    how.flags = (uint64_t)(flags | O_CLOEXEC);
    how.mode = (flags & O_CREAT) ? (uint64_t)mode : 0;
    how.resolve = RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS;
    bool reclaimed = false;
    for (int tries = 0;; tries++) {
        long fd = syscall(SYS_openat2, dirfd, path, &how, sizeof how);
        if (fd >= 0) {
            return (int)fd;
        }
        if (errno == EINTR || (errno == EAGAIN && tries < DIR_RETRIES) ||
            dir_reclaim(&reclaimed)) {
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

/* `path` opened beneath `dir`, or -1 with the message set. */
static int dir_open_beneath(const void *dir, const LughaString *path, int flags, mode_t mode,
                            LughaString **error) {
    if (!dir_path_ok(path, error)) {
        return -1;
    }
    int fd = dir_resolve(((const LughaDir *)dir)->fd, path->bytes, flags, mode);
    if (fd < 0) {
        dir_fail(error, errno);
    }
    return fd;
}

/* `path` opened as a folder handle (`O_PATH | O_DIRECTORY`) from the
 * working folder with ordinary resolution: -1 with `errno` set. */
static int dir_open_plain(const char *path) {
    bool reclaimed = false;
    for (;;) {
        int fd = open(path, O_PATH | O_DIRECTORY | O_CLOEXEC);
        if (fd >= 0 || (errno != EINTR && !dir_reclaim(&reclaimed))) {
            return fd;
        }
    }
}

/* The working folder's `Dir`, opened at the first `fs.cwd()` and kept: a
 * static the collector scans, so it is never finalized. */
static LughaDir *dir_working;

/* `$os.dir_cwd()`: the working folder, confined like any `Dir`. */
int32_t lugha_rt_dir_cwd(void **out, LughaString **error) {
    if (dir_working == NULL) {
        int fd = dir_open_plain(".");
        if (fd < 0) {
            return fs_fail(error, strerror(errno));
        }
        dir_working = dir_wrap(fd);
    }
    *out = dir_working;
    return 1;
}

/* `$os.dir_open(path)`: `fs.dir(path)`, the one unconfined lookup: `path`
 * as the system resolves it from the working folder. */
int32_t lugha_rt_dir_open(const LughaString *path, void **out, LughaString **error) {
    if (!dir_path_ok(path, error)) {
        return 0;
    }
    int fd = dir_open_plain(path->bytes);
    if (fd < 0) {
        return fs_fail(error, strerror(errno));
    }
    *out = dir_wrap(fd);
    return 1;
}

/* `$os.dir_sub(dir, path)`: a folder beneath `dir`, a narrower `Dir`. */
int32_t lugha_rt_dir_sub(void *dir, const LughaString *path, void **out, LughaString **error) {
    int fd = dir_open_beneath(dir, path, O_PATH | O_DIRECTORY, 0, error);
    if (fd < 0) {
        return 0;
    }
    *out = dir_wrap(fd);
    return 1;
}

/* Everything readable from `fd`, which it closes: 1 and the bytes in a
 * `malloc`ed buffer (`*len` of them), or 0 and the message. */
static int32_t dir_slurp(int fd, char **data, size_t *len, LughaString **error) {
    size_t cap = 4096;
    char *buffer = malloc(cap);
    if (buffer == NULL) {
        die("panic: out of memory\n");
    }
    size_t used = 0;
    for (;;) {
        if (used == cap) {
            cap *= 2;
            char *bigger = realloc(buffer, cap);
            if (bigger == NULL) {
                die("panic: out of memory\n");
            }
            buffer = bigger;
        }
        ssize_t got = read(fd, buffer + used, cap - used);
        if (got < 0 && errno == EINTR) {
            continue;
        }
        if (got < 0) {
            int saved = errno;
            close(fd);
            free(buffer);
            return fs_fail(error, strerror(saved));
        }
        if (got == 0) {
            break;
        }
        used += (size_t)got;
    }
    close(fd);
    *data = buffer;
    *len = used;
    return 1;
}

/* `$os.dir_read_bytes(dir, path)`: the whole file as bytes. */
int32_t lugha_rt_dir_read_bytes(void *dir, const LughaString *path, LughaArray **out,
                                LughaString **error) {
    int fd = dir_open_beneath(dir, path, O_RDONLY, 0, error);
    char *data;
    size_t len;
    if (fd < 0 || !dir_slurp(fd, &data, &len, error)) {
        return 0;
    }
    *out = bytes_of(data, (int64_t)len);
    free(data);
    return 1;
}

/* `$os.dir_read_text(dir, path)`: the whole file as text, its bytes as
 * they are, never checked as UTF-8 here. */
int32_t lugha_rt_dir_read_text(void *dir, const LughaString *path, LughaString **out,
                               LughaString **error) {
    int fd = dir_open_beneath(dir, path, O_RDONLY, 0, error);
    char *data;
    size_t len;
    if (fd < 0 || !dir_slurp(fd, &data, &len, error)) {
        return 0;
    }
    *out = string_of(data, (int64_t)len);
    free(data);
    return 1;
}

/* `$os.dir_write(dir, path, bytes, append)`: replaces the file's contents
 * (`append` 0) or adds to its end (1), making it (mode 0666 less the umask)
 * when missing; the bytes written. */
int32_t lugha_rt_dir_write(void *dir, const LughaString *path, const LughaArray *bytes,
                           int64_t append, int64_t *count, LughaString **error) {
    int flags = O_WRONLY | O_CREAT | (append ? O_APPEND : O_TRUNC);
    int fd = dir_open_beneath(dir, path, flags, 0666, error);
    if (fd < 0) {
        return 0;
    }
    int64_t done = 0;
    while (done < bytes->len) {
        ssize_t n = write(fd, bytes->elems + done, (size_t)(bytes->len - done));
        if (n < 0 && errno == EINTR) {
            continue;
        }
        if (n < 0) {
            int saved = errno;
            close(fd);
            return fs_fail(error, strerror(saved));
        }
        done += n;
    }
    if (close(fd) != 0 && errno != EINTR) {
        return fs_fail(error, strerror(errno));
    }
    *count = bytes->len;
    return 1;
}

/* `$os.dir_list(dir, path)`: every entry's name as bytes, without `.` and
 * `..`, in the order the system gives them. */
int32_t lugha_rt_dir_list(void *dir, const LughaString *path, LughaArray **out,
                          LughaString **error) {
    int fd = dir_open_beneath(dir, path, O_RDONLY | O_DIRECTORY, 0, error);
    if (fd < 0) {
        return 0;
    }
    DIR *listing = fdopendir(fd);
    if (listing == NULL) {
        int saved = errno;
        close(fd);
        return fs_fail(error, strerror(saved));
    }
    int64_t len = 0;
    int64_t cap = 16;
    LughaArray **names = lugha_rt_alloc(cap * (int64_t)sizeof(LughaArray *));
    for (;;) {
        errno = 0;
        struct dirent *entry = readdir(listing);
        if (entry == NULL) {
            break;
        }
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }
        if (len == cap) {
            if (cap > INT64_MAX / 2 / (int64_t)sizeof(LughaArray *)) {
                die("panic: out of memory\n");
            }
            LughaArray **bigger = lugha_rt_alloc(2 * cap * (int64_t)sizeof(LughaArray *));
            memcpy(bigger, names, (size_t)len * sizeof(LughaArray *));
            names = bigger;
            cap *= 2;
        }
        names[len++] = bytes_of(entry->d_name, (int64_t)strlen(entry->d_name));
    }
    int failed = errno;
    closedir(listing);
    if (failed != 0) {
        return fs_fail(error, strerror(failed));
    }
    LughaArray *array =
        lugha_rt_alloc((int64_t)sizeof(LughaArray) + len * (int64_t)sizeof(LughaArray *));
    array->len = len;
    array->cap = len;
    memcpy(array->elems, names, (size_t)len * sizeof(LughaArray *));
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
    int fd = dir_open_beneath(dir, path, flags, 0666, error);
    if (fd < 0) {
        return 0;
    }
    *out = fd;
    return 1;
}

/* `$os.dir_path(dir)`: where the folder is now, as bytes. */
int32_t lugha_rt_dir_path(void *dir, LughaArray **out, LughaString **error) {
    char self[64];
    snprintf(self, sizeof self, "/proc/self/fd/%d", ((LughaDir *)dir)->fd);
    char target[4096];
    ssize_t n = readlink(self, target, sizeof target);
    if (n < 0) {
        return fs_fail(error, strerror(errno));
    }
    if ((size_t)n == sizeof target) {
        return fs_fail(error, strerror(ENAMETOOLONG));
    }
    *out = bytes_of(target, (int64_t)n);
    return 1;
}
