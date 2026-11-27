/*
 * os/posix/dir_walk.c — part of lugha_rt.c on macOS and Android: how a
 * path is confined beneath a folder (os/layer.c's `os_dir_*`) where the
 * kernel can't do it: macOS has no `openat2` (its own "beneath" flag is
 * missing or silently ignored on the versions Lugha supports), and on
 * Android an app's sandbox ends any program calling it. So the runtime
 * walks the path itself, one name at a time:
 *
 * - each name is opened alone, from a descriptor the walk already holds,
 *   with `O_NOFOLLOW`: the kernel never follows a link and never climbs;
 * - a link is read here and its target walked the same way; an absolute
 *   target, like an absolute path, leads outside (`EXDEV`);
 * - `..` only undoes a name the walk went down through: the walk starts
 *   again from the `Dir` along the names left, each opened as above, so a
 *   folder renamed meanwhile can't take it out; `..` at the `Dir` itself
 *   leads outside.
 *
 * A folder swapped for a link mid-walk is met by the next `O_NOFOLLOW`
 * open, then read as a link and confined like any other. At most
 * `WALK_LINKS` links per lookup and `WALK_MAX` bytes of path, links
 * expanded; at most two descriptors open at a time.
 *
 * Each system's own file, joined just before this one, defines
 * `WALK_SEARCH` (how a folder on the way is opened: for searching only,
 * and refused when it is a link), `OS_DIR_FLAGS`, `os_dir_where` and
 * `os_dir_message` (os/darwin/dir_resolve.c, os/android/dir_resolve.c).
 *
 * A fragment, not compiled on its own: joined after the core, before
 * os/posix/dir.c, which calls these.
 */

/* Links followed in one lookup (Linux's limit); a path's length with its
 * links expanded; one name's length. */
enum { WALK_LINKS = 40, WALK_MAX = 4096, WALK_NAME = 1024 };

/* A lookup under way. */
typedef struct {
    int dirfd;                /* the `Dir`'s folder: never closed here */
    int cur;                  /* the folder reached: `dirfd`, or one opened */
    char done[WALK_MAX + 1];  /* the names gone down through, `/` between */
    size_t done_len;
    char rest[WALK_MAX + 1];  /* the path still to walk, from `at` */
    size_t at;
    int links;
} Walk;

/* Back at the `Dir`: the folder the walk opened, if any, closed. */
static void walk_leave(Walk *w) {
    if (w->cur != w->dirfd) {
        close(w->cur);
    }
    w->cur = w->dirfd;
}

/* One name opened from `at`, never following a link. */
static int walk_openat(int at, const char *name, int flags, mode_t mode) {
    for (;;) {
        int fd = openat(at, name, flags | O_NOFOLLOW | O_CLOEXEC, mode);
        if (fd >= 0 || errno != EINTR) {
            return fd;
        }
    }
}

/* `front` put before what is left to walk, which is empty or starts with
 * `/`: 0, or -1 with `errno`. */
static int walk_prepend(Walk *w, const char *front, size_t front_len) {
    size_t left = strlen(w->rest + w->at);
    if (front_len + left > WALK_MAX) {
        errno = ENAMETOOLONG;
        return -1;
    }
    memmove(w->rest + front_len, w->rest + w->at, left + 1);
    memcpy(w->rest, front, front_len);
    w->at = 0;
    return 0;
}

/* The next name to walk, `/` and `.` skipped: its length, 0 when none is
 * left; `*name` where it starts, `at` just past it. */
static size_t walk_next(Walk *w, const char **name) {
    for (;;) {
        while (w->rest[w->at] == '/') {
            w->at++;
        }
        const char *start = w->rest + w->at;
        size_t n = strcspn(start, "/");
        w->at += n;
        if (n == 1 && start[0] == '.') {
            continue;
        }
        *name = start;
        return n;
    }
}

/* Whether the name just taken is the last: only `/` and `.` follow it.
 * `*folder`: something follows, so it must be a folder, as on Linux. */
static bool walk_ends(const Walk *w, bool *folder) {
    const char *p = w->rest + w->at;
    *folder = *p == '/';
    for (;;) {
        while (*p == '/') {
            p++;
        }
        if (*p == '\0') {
            return true;
        }
        size_t n = strcspn(p, "/");
        if (n != 1 || p[0] != '.') {
            return false;
        }
        p += n;
    }
}

/* If `name` in the folder reached is a link: its target put before what
 * is left to walk, 0. Not a link: 1. Failed: -1 with `errno`. */
static int walk_follow(Walk *w, const char *name) {
    char target[WALK_MAX + 1];
    ssize_t n = readlinkat(w->cur, name, target, sizeof target);
    if (n < 0) {
        return errno == EINVAL ? 1 : -1;
    }
    if (++w->links > WALK_LINKS) {
        errno = ELOOP;
        return -1;
    }
    if ((size_t)n >= sizeof target) {
        errno = ENAMETOOLONG;
        return -1;
    }
    if (n == 0) {
        errno = ENOENT;
        return -1;
    }
    if (target[0] == '/') {
        errno = EXDEV;
        return -1;
    }
    return walk_prepend(w, target, (size_t)n);
}

/* `..`: the last name gone down through undone, and the walk started again
 * from the `Dir` along the names left: 0, or -1 with `errno`. */
static int walk_up(Walk *w) {
    if (w->done_len == 0) {
        errno = EXDEV;
        return -1;
    }
    while (w->done_len > 0 && w->done[w->done_len - 1] != '/') {
        w->done_len--;
    }
    if (w->done_len > 0) {
        w->done_len--;
    }
    walk_leave(w);
    if (w->done_len > 0 && walk_prepend(w, w->done, w->done_len) != 0) {
        return -1;
    }
    w->done_len = 0;
    return 0;
}

/* A folder on the way gone down into (`fd`): 0, or -1 with `errno`. */
static int walk_down(Walk *w, int fd, const char *name, size_t n) {
    size_t sep = w->done_len > 0 ? 1 : 0;
    if (w->done_len + sep + n > WALK_MAX) {
        close(fd);
        errno = ENAMETOOLONG;
        return -1;
    }
    if (sep) {
        w->done[w->done_len] = '/';
    }
    memcpy(w->done + w->done_len + sep, name, n);
    w->done_len += sep + n;
    walk_leave(w);
    w->cur = fd;
    return 0;
}

/* Walks `path` beneath `dirfd`. To open (`last` NULL): the last name opened
 * with `flags` (and `mode`), its descriptor. To find (`last` given): the
 * folder holding the last name left open in `w->cur`, the name in `last`
 * (`.` when the path names that folder itself) and what it names in `st`,
 * itself never a link; 0. Failed: -1 with `errno`, nothing left open. */
static int walk(Walk *w, int dirfd, const char *path, int flags, mode_t mode, char *last,
                struct stat *st) {
    size_t len = strlen(path);
    if (path[0] == '/') {
        errno = EXDEV;
        return -1;
    }
    if (len > WALK_MAX) {
        errno = ENAMETOOLONG;
        return -1;
    }
    memcpy(w->rest, path, len + 1);
    w->at = 0;
    w->dirfd = w->cur = dirfd;
    w->done_len = 0;
    w->links = 0;
    for (;;) {
        const char *at;
        size_t n = walk_next(w, &at);
        if (n == 0) {
            // The path names the folder reached itself.
            if (last != NULL) {
                strcpy(last, ".");
                if (fstatat(w->cur, ".", st, 0) == 0) {
                    return 0;
                }
                break;
            }
            int fd = walk_openat(w->cur, ".", flags, mode);
            if (fd < 0) {
                break;
            }
            walk_leave(w);
            return fd;
        }
        if (n >= WALK_NAME) {
            errno = ENAMETOOLONG;
            break;
        }
        char name[WALK_NAME];
        memcpy(name, at, n);
        name[n] = '\0';
        if (strcmp(name, "..") == 0) {
            if (walk_up(w) != 0) {
                break;
            }
            continue;
        }
        bool folder;
        bool final = walk_ends(w, &folder);
        if (final && last != NULL) {
            if (fstatat(w->cur, name, st, AT_SYMLINK_NOFOLLOW) != 0) {
                break;
            }
            if (S_ISLNK(st->st_mode)) {
                if (walk_follow(w, name) != 0) {
                    break;
                }
                continue;
            }
            if (folder && !S_ISDIR(st->st_mode)) {
                errno = ENOTDIR;
                break;
            }
            strcpy(last, name);
            return 0;
        }
        if (final && folder && (flags & O_CREAT)) {
            errno = EISDIR;
            break;
        }
        int want = !final ? WALK_SEARCH : folder ? flags | O_DIRECTORY : flags;
        int fd = walk_openat(w->cur, name, want, final ? mode : 0);
        if (fd < 0) {
            // A link refused by `O_NOFOLLOW`: `ELOOP`, or `ENOTDIR` when a
            // folder was asked for.
            if (errno != ELOOP && errno != ENOTDIR) {
                break;
            }
            int code = errno;
            int link = walk_follow(w, name);
            if (link == 0) {
                continue;
            }
            if (link == 1) {
                errno = code;
            }
            break;
        }
        if (final) {
            walk_leave(w);
            return fd;
        }
        if (walk_down(w, fd, name, n) != 0) {
            break;
        }
    }
    int code = errno;
    walk_leave(w);
    errno = code;
    return -1;
}

/* A lookup's state, too big for a helper's stack frame to hold twice. */
static Walk *walk_new(void) {
    Walk *w = malloc(sizeof *w);
    if (w == NULL) {
        die("panic: out of memory\n");
    }
    return w;
}

static int os_dir_resolve(int dirfd, const char *path, int flags, mode_t mode) {
    Walk *w = walk_new();
    int fd = walk(w, dirfd, path, flags, mode, NULL, NULL);
    int code = errno;
    free(w);
    errno = code;
    return fd;
}

/* The last name, never a link, looked at where the walk found it. */
static int os_dir_stat(int dirfd, const char *path, struct stat *st) {
    Walk *w = walk_new();
    char last[WALK_NAME];
    int result = walk(w, dirfd, path, 0, 0, last, st);
    int code = errno;
    walk_leave(w);
    free(w);
    errno = code;
    return result;
}

/* The last name's mode set where the walk found it, never following a
 * link: one swapped in after the walk looked has its own mode changed,
 * never its target's. */
static int os_dir_chmod(int dirfd, const char *path, mode_t mode) {
    Walk *w = walk_new();
    char last[WALK_NAME];
    struct stat st;
    int result = walk(w, dirfd, path, 0, 0, last, &st);
    if (result == 0) {
        result = fchmodat(w->cur, last, mode, AT_SYMLINK_NOFOLLOW);
    }
    int code = errno;
    walk_leave(w);
    free(w);
    errno = code;
    return result;
}
