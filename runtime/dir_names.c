/*
 * dir_names.c — part of lugha_rt.c: a `Dir`'s operations on what a path
 * names. Making, removing and renaming act on a path's last part: the
 * folder holding it is opened beneath the `Dir` first, then the one name is
 * acted on there, never `.` or `..`, and a link is removed itself, never
 * followed. Kind, size, mode and syncing act on the file the kernel
 * resolved beneath the `Dir`, never a link's target outside it.
 *
 * Each runs on a helper thread, as dir.c's operations do.
 *
 * A fragment, not compiled on its own: joined right after dir.c
 * (`LughaDir`, `dir_resolve`, `dir_path_ok`, the `dir_job` family,
 * `DIR_OUTSIDE`).
 */

/* `path` split into the folder holding its last part and that part, two
 * copies from `malloc` at `parts[0]` and `parts[1]`; false with the
 * message set. The last part must be a name: not empty, `.` or `..`. */
static bool dir_split(const LughaString *path, char **parts, LughaString **error) {
    if (!dir_path_ok(path, error)) {
        return false;
    }
    int64_t n = path->len;
    while (n > 1 && path->bytes[n - 1] == '/') {
        n--;
    }
    const char *slash = memrchr(path->bytes, '/', (size_t)n);
    if (slash == path->bytes) {
        fs_fail(error, DIR_OUTSIDE);
        return false;
    }
    const char *last = slash == NULL ? path->bytes : slash + 1;
    size_t last_len = (size_t)(path->bytes + n - last);
    if (last_len == 0 || (last_len == 1 && last[0] == '.') ||
        (last_len == 2 && last[0] == '.' && last[1] == '.')) {
        fs_fail(error, "a path must end in a name, not `.` or `..`");
        return false;
    }
    parts[0] = slash == NULL ? job_copy(".", 1)
                             : job_copy(path->bytes, (size_t)(slash - path->bytes));
    parts[1] = job_copy(last, last_len);
    return true;
}

/* A job on the names in `paths` (`count` of them) beneath `dir`, each split
 * into its folder and last part in `argv`; NULL with the message set. */
static Job *dir_names_job(void (*run)(Job *), void *dir, const LughaString **paths, int count,
                          LughaString **error) {
    Job *job = job_new(run);
    job->argv = calloc((size_t)(2 * count + 1), sizeof(char *));
    if (job->argv == NULL) {
        die("panic: out of memory\n");
    }
    for (int i = 0; i < count; i++) {
        if (!dir_split(paths[i], job->argv + 2 * i, error)) {
            job_free(job);
            return NULL;
        }
    }
    job->fd = ((LughaDir *)dir)->fd;
    job->hold = dir;
    return job;
}

/* On a helper: the folder holding the `i`th name, or -1 with the failure
 * kept. */
static int dir_names_parent(Job *job, int i) {
    int fd = dir_resolve(job->fd, job->argv[2 * i], O_PATH | O_DIRECTORY, 0);
    if (fd < 0) {
        job->err = errno;
        job->lookup = true;
    }
    return fd;
}

/* Runs a names job: 1 with `*done` 0, or 0 and its failure's message. */
static int32_t dir_names_run(Job *job, int64_t *done, LughaString **error) {
    if (job == NULL) {
        return 0;
    }
    job_run(job);
    if (job->err != 0) {
        return dir_job_failed(job, error);
    }
    job_free(job);
    *done = 0;
    return 1;
}

/* On a helper: makes one folder (mode 0777 less the umask). */
static void dir_make_job(Job *job) {
    int parent = dir_names_parent(job, 0);
    if (parent >= 0) {
        if (mkdirat(parent, job->argv[1], 0777) != 0) {
            job->err = errno;
        }
        close(parent);
    }
}

/* `$os.dir_make(dir, path)`: one folder (mode 0777 less the umask). */
int32_t lugha_rt_dir_make(void *dir, const LughaString *path, int64_t *done, LughaString **error) {
    return dir_names_run(dir_names_job(dir_make_job, dir, &path, 1, error), done, error);
}

/* On a helper: removes a name (`flags` 0 or `AT_REMOVEDIR`). */
static void dir_remove_job(Job *job) {
    int parent = dir_names_parent(job, 0);
    if (parent >= 0) {
        if (unlinkat(parent, job->argv[1], job->flags) != 0) {
            job->err = errno;
        }
        close(parent);
    }
}

/* `$os.dir_remove(dir, path, folder)`: a file or a link itself (`folder`
 * 0), or an empty folder (1). */
int32_t lugha_rt_dir_remove(void *dir, const LughaString *path, int64_t folder, int64_t *done,
                            LughaString **error) {
    Job *job = dir_names_job(dir_remove_job, dir, &path, 1, error);
    if (job != NULL) {
        job->flags = folder ? AT_REMOVEDIR : 0;
    }
    return dir_names_run(job, done, error);
}

/* On a helper: renames the first name to the second. */
static void dir_rename_job(Job *job) {
    int from = dir_names_parent(job, 0);
    if (from < 0) {
        return;
    }
    int to = dir_names_parent(job, 1);
    if (to >= 0) {
        if (renameat(from, job->argv[1], to, job->argv[3]) != 0) {
            job->err = errno;
        }
        close(to);
    }
    close(from);
}

/* `$os.dir_rename(dir, from, to)`: `renameat`, replacing `to` in one step,
 * both names beneath `dir`. */
int32_t lugha_rt_dir_rename(void *dir, const LughaString *from, const LughaString *to,
                            int64_t *done, LughaString **error) {
    const LughaString *paths[2] = {from, to};
    return dir_names_run(dir_names_job(dir_rename_job, dir, paths, 2, error), done, error);
}

/* On a helper: what the job's path names, its mode in `flags` and size in
 * `value`; the lookup followed (`O_PATH`, links inside followed). */
static void dir_stat_job(Job *job) {
    int fd = dir_job_open(job, O_PATH, 0);
    if (fd < 0) {
        return;
    }
    struct stat info;
    if (fstat(fd, &info) != 0) {
        job->err = errno;
    } else {
        job->flags = (int)info.st_mode;
        job->value = (int64_t)info.st_size;
    }
    close(fd);
}

/* `$os.dir_kind(dir, path)`: 0 when nothing is there (or it can't be
 * reached), 1 for a regular file, 2 for a folder, 3 for anything else;
 * links inside followed. */
int64_t lugha_rt_dir_kind(void *dir, const LughaString *path) {
    LughaString *ignored = NULL;
    Job *job = dir_job(dir_stat_job, dir, path, &ignored);
    if (job == NULL) {
        rt_let_go(ignored);
        return 0;
    }
    job_run(job);
    mode_t mode = (mode_t)job->flags;
    int64_t kind = job->err != 0 ? 0 : S_ISREG(mode) ? 1 : S_ISDIR(mode) ? 2 : 3;
    job_free(job);
    return kind;
}

/* `$os.dir_size(dir, path)`: a regular file's size, without opening it for
 * reading; a folder is `EISDIR`'s message, any other kind "not a regular
 * file". */
int32_t lugha_rt_dir_size(void *dir, const LughaString *path, int64_t *out,
                          LughaString **error) {
    Job *job = dir_job(dir_stat_job, dir, path, error);
    if (job == NULL) {
        return 0;
    }
    job_run(job);
    if (job->err != 0) {
        return dir_job_failed(job, error);
    }
    mode_t mode = (mode_t)job->flags;
    int64_t size = job->value;
    job_free(job);
    if (S_ISDIR(mode)) {
        return fs_fail(error, strerror(EISDIR));
    }
    if (!S_ISREG(mode)) {
        return fs_fail(error, "not a regular file");
    }
    *out = size;
    return 1;
}

/* `fchmodat2`, new in Linux 6.6 (number 452 on every architecture). */
#ifndef SYS_fchmodat2
#define SYS_fchmodat2 452
#endif

/* On a helper: the permission bits `n` set through the resolved file's
 * descriptor: `fchmodat2` with `AT_EMPTY_PATH`, else the descriptor's
 * `/proc/self/fd` entry on an older kernel. */
static void dir_chmod_job(Job *job) {
    int fd = dir_job_open(job, O_PATH, 0);
    if (fd < 0) {
        return;
    }
    int ok = syscall(SYS_fchmodat2, fd, "", (mode_t)job->n, AT_EMPTY_PATH) == 0;
    if (!ok && errno == ENOSYS) {
        char self[64];
        snprintf(self, sizeof self, "/proc/self/fd/%d", fd);
        ok = chmod(self, (mode_t)job->n) == 0;
    }
    if (!ok) {
        job->err = errno;
    }
    close(fd);
}

/* `$os.dir_chmod(dir, path, mode)`: the permission bits of the file the
 * kernel resolved beneath `dir`, never a link's target outside it. */
int32_t lugha_rt_dir_chmod(void *dir, const LughaString *path, int64_t mode, int64_t *done,
                           LughaString **error) {
    if (mode < 0 || mode > 07777) {
        return fs_fail(error, "mode must be from 0 to 4095 (0o7777)");
    }
    Job *job = dir_job(dir_chmod_job, dir, path, error);
    if (job != NULL) {
        job->n = mode;
    }
    return dir_names_run(job, done, error);
}

/* On a helper: `fsync(2)` on the job's path. */
static void dir_sync_job(Job *job) {
    int fd = dir_job_open(job, O_RDONLY, 0);
    if (fd < 0) {
        return;
    }
    int result;
    do {
        result = fsync(fd);
    } while (result != 0 && errno == EINTR);
    if (result != 0) {
        job->err = errno;
    }
    close(fd);
}

/* `$os.dir_sync(dir, path)`: `fsync(2)` on a file or folder beneath `dir`. */
int32_t lugha_rt_dir_sync(void *dir, const LughaString *path, int64_t *done,
                          LughaString **error) {
    return dir_names_run(dir_job(dir_sync_job, dir, path, error), done, error);
}
