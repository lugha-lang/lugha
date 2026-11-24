/*
 * os/linux/dir_resolve.c — part of lugha_rt.c: how Linux confines a path
 * beneath a folder (os/layer.c's `os_dir_*`). Every path is opened with
 * `openat2` and `RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS`: the kernel
 * refuses an absolute path, a `..` above the folder, and a symbolic link
 * leading out, checking each step, so a folder swapped for a link mid-open
 * is caught too. Without `openat2` (Linux before 5.6, or a sandbox
 * blocking it) every lookup fails: there is no unconfined fallback.
 *
 * A `Dir`'s own descriptor is `O_PATH | O_DIRECTORY`: it needs search
 * permission on the folder only, and reads nothing.
 *
 * A fragment, not compiled on its own: joined after the core and
 * os/linux/os.c (`<linux/openat2.h>`, `<sys/syscall.h>`), before
 * os/posix/dir.c, which calls these.
 */

/* How a `Dir`'s own folder is opened: a handle to search it through. */
#define OS_DIR_FLAGS (O_PATH | O_DIRECTORY)

/* How often a lookup that raced a rename (`EAGAIN`) is tried again. */
enum { DIR_RETRIES = 8 };

static int os_dir_resolve(int dirfd, const char *path, int flags, mode_t mode) {
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

/* The file the kernel resolved, through an `O_PATH` handle on it. */
static int os_dir_stat(int dirfd, const char *path, struct stat *st) {
    int fd = os_dir_resolve(dirfd, path, O_PATH, 0);
    if (fd < 0) {
        return -1;
    }
    int result = fstat(fd, st);
    int code = errno;
    close(fd);
    errno = code;
    return result;
}

/* `fchmodat2`, new in Linux 6.6 (number 452 on every architecture). */
#ifndef SYS_fchmodat2
#define SYS_fchmodat2 452
#endif

/* The mode set through the resolved file's handle: `fchmodat2` with
 * `AT_EMPTY_PATH`, else the handle's `/proc/self/fd` entry on an older
 * kernel. */
static int os_dir_chmod(int dirfd, const char *path, mode_t mode) {
    int fd = os_dir_resolve(dirfd, path, O_PATH, 0);
    if (fd < 0) {
        return -1;
    }
    int ok = syscall(SYS_fchmodat2, fd, "", mode, AT_EMPTY_PATH) == 0;
    if (!ok && errno == ENOSYS) {
        char self[64];
        snprintf(self, sizeof self, "/proc/self/fd/%d", fd);
        ok = chmod(self, mode) == 0;
    }
    int code = errno;
    close(fd);
    errno = code;
    return ok ? 0 : -1;
}

/* Where the folder is now: its `/proc/self/fd` entry read as a link. */
static ssize_t os_dir_where(int fd, char *buf, size_t len) {
    char self[64];
    snprintf(self, sizeof self, "/proc/self/fd/%d", fd);
    ssize_t n = readlink(self, buf, len);
    if (n >= 0 && (size_t)n == len) {
        errno = ENAMETOOLONG;
        return -1;
    }
    return n;
}

static const char *os_dir_message(int code) {
    if (code == ENOSYS) {
        return "this system can't confine a folder: openat2 needs Linux 5.6";
    }
    return NULL;
}
