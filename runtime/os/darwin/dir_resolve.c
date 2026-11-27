/*
 * os/darwin/dir_resolve.c — part of lugha_rt.c: what macOS's folder walk
 * (os/posix/dir_walk.c, joined next) needs of macOS.
 *
 * A `Dir`'s own descriptor, and each folder the walk passes through, is
 * `O_SEARCH`: it needs search permission on the folder only, as Linux's
 * `O_PATH`.
 *
 * A fragment, not compiled on its own: joined after the core.
 */

#include <sys/param.h>

/* How a `Dir`'s own folder is opened: a handle to search it through. */
#define OS_DIR_FLAGS O_SEARCH

/* How the walk opens a folder on the way: macOS refuses a link with
 * `O_NOFOLLOW` (`ELOOP`) whatever else is asked. */
#define WALK_SEARCH O_SEARCH

/* Where the folder is now: `F_GETPATH`, which needs `MAXPATHLEN` bytes. */
static ssize_t os_dir_where(int fd, char *buf, size_t len) {
    char path[MAXPATHLEN];
    if (fcntl(fd, F_GETPATH, path) != 0) {
        return -1;
    }
    size_t n = strlen(path);
    if (n >= len) {
        errno = ENAMETOOLONG;
        return -1;
    }
    memcpy(buf, path, n);
    return (ssize_t)n;
}

static const char *os_dir_message(int code) {
    (void)code;
    return NULL;
}
