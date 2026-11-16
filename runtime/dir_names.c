/*
 * dir_names.c — part of lugha_rt.c: a `Dir`'s operations on what a path
 * names. Making, removing and renaming act on a path's last part: the
 * folder holding it is opened beneath the `Dir` first, then the one name is
 * acted on there, never `.` or `..`, and a link is removed itself, never
 * followed. Kind, size, mode and syncing act on the file the kernel
 * resolved beneath the `Dir`, never a link's target outside it.
 *
 * A fragment, not compiled on its own: joined right after dir.c
 * (`LughaDir`, `dir_resolve`, `dir_fail`, `dir_path_ok`,
 * `dir_open_beneath`, `DIR_OUTSIDE`).
 */

/* The folder holding `path`'s last part, opened beneath `dir`, with that
 * part in `*last`; -1 with the message set. The last part must be a name:
 * not empty, `.` or `..`. */
static int dir_parent(const void *dir, const LughaString *path, const char **last,
                      LughaString **error) {
    if (!dir_path_ok(path, error)) {
        return -1;
    }
    char *copy = lugha_rt_alloc(path->len + 1);
    memcpy(copy, path->bytes, (size_t)path->len);
    copy[path->len] = '\0';
    int64_t n = path->len;
    while (n > 1 && copy[n - 1] == '/') {
        copy[--n] = '\0';
    }
    char *slash = strrchr(copy, '/');
    const char *parent = ".";
    *last = copy;
    if (slash == copy) {
        fs_fail(error, DIR_OUTSIDE);
        return -1;
    }
    if (slash != NULL) {
        *slash = '\0';
        parent = copy;
        *last = slash + 1;
    }
    if ((*last)[0] == '\0' || strcmp(*last, ".") == 0 || strcmp(*last, "..") == 0) {
        fs_fail(error, "a path must end in a name, not `.` or `..`");
        return -1;
    }
    int fd = dir_resolve(((const LughaDir *)dir)->fd, parent, O_PATH | O_DIRECTORY, 0);
    if (fd < 0) {
        dir_fail(error, errno);
    }
    return fd;
}

/* `$os.dir_make(dir, path)`: one folder (mode 0777 less the umask). */
int32_t lugha_rt_dir_make(void *dir, const LughaString *path, int64_t *done, LughaString **error) {
    const char *last;
    int parent = dir_parent(dir, path, &last, error);
    if (parent < 0) {
        return 0;
    }
    int ok = mkdirat(parent, last, 0777) == 0;
    int saved = errno;
    close(parent);
    *done = 0;
    return ok ? 1 : fs_fail(error, strerror(saved));
}

/* `$os.dir_remove(dir, path, folder)`: a file or a link itself (`folder`
 * 0), or an empty folder (1). */
int32_t lugha_rt_dir_remove(void *dir, const LughaString *path, int64_t folder, int64_t *done,
                            LughaString **error) {
    const char *last;
    int parent = dir_parent(dir, path, &last, error);
    if (parent < 0) {
        return 0;
    }
    int ok = unlinkat(parent, last, folder ? AT_REMOVEDIR : 0) == 0;
    int saved = errno;
    close(parent);
    *done = 0;
    return ok ? 1 : fs_fail(error, strerror(saved));
}

/* `$os.dir_rename(dir, from, to)`: `renameat`, replacing `to` in one step,
 * both names beneath `dir`. */
int32_t lugha_rt_dir_rename(void *dir, const LughaString *from, const LughaString *to,
                            int64_t *done, LughaString **error) {
    const char *from_last;
    const char *to_last;
    int from_parent = dir_parent(dir, from, &from_last, error);
    if (from_parent < 0) {
        return 0;
    }
    int to_parent = dir_parent(dir, to, &to_last, error);
    if (to_parent < 0) {
        close(from_parent);
        return 0;
    }
    int ok = renameat(from_parent, from_last, to_parent, to_last) == 0;
    int saved = errno;
    close(from_parent);
    close(to_parent);
    *done = 0;
    return ok ? 1 : fs_fail(error, strerror(saved));
}

/* `$os.dir_kind(dir, path)`: 0 when nothing is there (or it can't be
 * reached), 1 for a regular file, 2 for a folder, 3 for anything else;
 * links inside followed. */
int64_t lugha_rt_dir_kind(void *dir, const LughaString *path) {
    LughaString *ignored = NULL;
    int fd = dir_open_beneath(dir, path, O_PATH, 0, &ignored);
    if (fd < 0) {
        return 0;
    }
    struct stat info;
    int ok = fstat(fd, &info) == 0;
    close(fd);
    if (!ok) {
        return 0;
    }
    if (S_ISREG(info.st_mode)) {
        return 1;
    }
    return S_ISDIR(info.st_mode) ? 2 : 3;
}

/* `$os.dir_size(dir, path)`: a regular file's size, without opening it for
 * reading; a folder is `EISDIR`'s message, any other kind "not a regular
 * file". */
int32_t lugha_rt_dir_size(void *dir, const LughaString *path, int64_t *out,
                          LughaString **error) {
    int fd = dir_open_beneath(dir, path, O_PATH, 0, error);
    if (fd < 0) {
        return 0;
    }
    struct stat info;
    int ok = fstat(fd, &info) == 0;
    int saved = errno;
    close(fd);
    if (!ok) {
        return fs_fail(error, strerror(saved));
    }
    if (S_ISDIR(info.st_mode)) {
        return fs_fail(error, strerror(EISDIR));
    }
    if (!S_ISREG(info.st_mode)) {
        return fs_fail(error, "not a regular file");
    }
    *out = (int64_t)info.st_size;
    return 1;
}

/* `fchmodat2`, new in Linux 6.6 (number 452 on every architecture). */
#ifndef SYS_fchmodat2
#define SYS_fchmodat2 452
#endif

/* `$os.dir_chmod(dir, path, mode)`: the permission bits of the file the
 * kernel resolved beneath `dir` (never a link's target outside it), through
 * its descriptor: `fchmodat2` with `AT_EMPTY_PATH`, else the descriptor's
 * `/proc/self/fd` entry on an older kernel. */
int32_t lugha_rt_dir_chmod(void *dir, const LughaString *path, int64_t mode, int64_t *done,
                           LughaString **error) {
    if (mode < 0 || mode > 07777) {
        return fs_fail(error, "mode must be from 0 to 4095 (0o7777)");
    }
    int fd = dir_open_beneath(dir, path, O_PATH, 0, error);
    if (fd < 0) {
        return 0;
    }
    int ok = syscall(SYS_fchmodat2, fd, "", (mode_t)mode, AT_EMPTY_PATH) == 0;
    if (!ok && errno == ENOSYS) {
        char self[64];
        snprintf(self, sizeof self, "/proc/self/fd/%d", fd);
        ok = chmod(self, (mode_t)mode) == 0;
    }
    int saved = errno;
    close(fd);
    *done = 0;
    return ok ? 1 : fs_fail(error, strerror(saved));
}

/* `$os.dir_sync(dir, path)`: `fsync(2)` on a file or folder beneath `dir`. */
int32_t lugha_rt_dir_sync(void *dir, const LughaString *path, int64_t *done,
                          LughaString **error) {
    int fd = dir_open_beneath(dir, path, O_RDONLY, 0, error);
    if (fd < 0) {
        return 0;
    }
    int result;
    do {
        result = fsync(fd);
    } while (result != 0 && errno == EINTR);
    int saved = errno;
    close(fd);
    *done = 0;
    return result == 0 ? 1 : fs_fail(error, strerror(saved));
}
