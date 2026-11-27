/*
 * os/android/dir_resolve.c — part of lugha_rt.c on Android: what the folder
 * walk (os/posix/dir_walk.c, joined next) needs of Android. Linux's own
 * confinement (`openat2`, os/linux/dir_resolve.c) isn't used here: an
 * app's sandbox ends any program calling it, without an error to fall
 * back from.
 *
 * A `Dir`'s own descriptor, and each folder the walk passes through, is
 * `O_PATH | O_DIRECTORY`: it needs search permission on the folder only,
 * and reads nothing. `O_DIRECTORY` matters on the way: `O_PATH` with
 * `O_NOFOLLOW` alone would open a link itself rather than refuse it, and
 * with `O_DIRECTORY` a link is refused (`ENOTDIR`), so the walk reads it.
 *
 * A fragment, not compiled on its own: joined after the core and
 * os/linux/os.c.
 */

/* How a `Dir`'s own folder is opened: a handle to search it through. */
#define OS_DIR_FLAGS (O_PATH | O_DIRECTORY)

/* How the walk opens a folder on the way. */
#define WALK_SEARCH (O_PATH | O_DIRECTORY)

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
    (void)code;
    return NULL;
}
