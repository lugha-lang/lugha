/*
 * os/windows/dir_resolve.c — part of lugha_rt.c on Windows: how a path is
 * confined beneath a folder. Windows has no `openat2`, so the runtime
 * walks the path itself, one name at a time, as on macOS:
 *
 * - each name is opened alone (`NtCreateFile`, relative to a folder handle
 *   the walk already holds) as itself: a reparse point is opened, never
 *   followed (`FILE_OPEN_REPARSE_POINT`), and the handle's attributes say
 *   what it is;
 * - a symbolic link is read here and its target walked the same way; an
 *   absolute target, a junction (whose target is always absolute) and any
 *   other name-redirecting reparse point lead outside (`EXDEV`); a reparse
 *   point that redirects nothing (a cloud placeholder) isn't followed;
 * - `..` only undoes a name the walk went down through: the walk starts
 *   again from the `Dir` along the names left; `..` at the `Dir` itself
 *   leads outside;
 * - what isn't a link is opened again from its own handle (`ReOpenFile`)
 *   with the access the caller wants, so nothing can be swapped in between.
 *
 * Paths take `/` and `\` alike. A path starting with either, or holding
 * `:` (a drive, a drive-relative path, a data stream) or `?` (`\\?\`),
 * leads outside; a name Windows reserves (a device name such as `NUL` or
 * `COM¹`, one ending in `.` or a space, one holding `*"<>|` or a control
 * character) is refused (`WIN_RESERVED`). At most `WALK_LINKS` links and
 * `WALK_MAX` bytes of path per lookup.
 *
 * A fragment, not compiled on its own: joined after paths.c (names,
 * single opens, failures) and before os/windows/dir.c, which calls these.
 */

/* A lookup under way. */
typedef struct {
    HANDLE dir;              /* the `Dir`'s folder: never closed here */
    HANDLE cur;              /* the folder reached: `dir`, or one opened */
    char done[WALK_MAX + 1]; /* the names gone down through, `/` between */
    size_t done_len;
    char rest[WALK_MAX + 1]; /* the path still to walk, from `at`, `/` only */
    size_t at;
    int links;
} Walk;

/* Back at the `Dir`: the folder the walk opened, if any, closed. */
static void walk_leave(Walk *w) {
    if (w->cur != w->dir) {
        CloseHandle(w->cur);
    }
    w->cur = w->dir;
}

/* `front` put before what is left to walk: 0, or -1 with `errno`. */
static int walk_prepend(Walk *w, const char *front, size_t front_len) {
    size_t left = strlen(w->rest + w->at);
    if (front_len + 1 + left > WALK_MAX) {
        errno = ENAMETOOLONG;
        return -1;
    }
    memmove(w->rest + front_len + 1, w->rest + w->at, left + 1);
    memcpy(w->rest, front, front_len);
    w->rest[front_len] = '/';
    w->at = 0;
    return 0;
}

/* The next name, `/` and `.` skipped: its length, 0 when none is left. */
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

/* Whether only `/` and `.` follow the name just taken; `*folder`: a `/`
 * follows it, so it must be a folder. */
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

/* The reparse point `h` (tag `tag`): a relative symbolic link's target put
 * before what is left to walk, 0; anything else -1 with `errno`. */
static int walk_follow(Walk *w, HANDLE h, ULONG tag) {
    if (tag != IO_REPARSE_TAG_SYMLINK) {
        errno = IsReparseTagNameSurrogate(tag) ? EXDEV : WIN_ODD_REPARSE;
        return -1;
    }
    if (++w->links > WALK_LINKS) {
        errno = ELOOP;
        return -1;
    }
    static __thread union {
        WinLink link;
        char bytes[MAXIMUM_REPARSE_DATA_BUFFER_SIZE];
    } data;
    DWORD got = 0;
    if (!DeviceIoControl(h, FSCTL_GET_REPARSE_POINT, NULL, 0, &data, sizeof data, &got, NULL)) {
        errno = win_errno_last();
        return -1;
    }
    WinLink *link = &data.link;
    size_t start = offsetof(WinLink, path) + link->sub_offset;
    if (start + link->sub_len > got || (link->flags & WIN_LINK_RELATIVE) == 0) {
        errno = EXDEV;
        return -1;
    }
    const wchar_t *target = (const wchar_t *)(data.bytes + start);
    int units = link->sub_len / 2;
    char text[WALK_MAX + 1];
    int n = units == 0 ? 0
                       : WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, target, units, text,
                                             WALK_MAX, NULL, NULL);
    if (units > 0 && n <= 0) {
        errno = units * 3 > WALK_MAX ? ENAMETOOLONG : WIN_NOT_UNICODE;
        return -1;
    }
    if (n == 0) {
        errno = ENOENT;
        return -1;
    }
    if (win_outside(text, (size_t)n)) {
        errno = EXDEV;
        return -1;
    }
    for (int i = 0; i < n; i++) {
        if (text[i] == '\\') {
            text[i] = '/';
        }
    }
    return walk_prepend(w, text, (size_t)n);
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

/* A folder on the way gone down into (`h`): 0, or -1 with `errno`. */
static int walk_down(Walk *w, HANDLE h, const char *name, size_t n) {
    size_t sep = w->done_len > 0 ? 1 : 0;
    if (w->done_len + sep + n > WALK_MAX) {
        CloseHandle(h);
        errno = ENAMETOOLONG;
        return -1;
    }
    if (sep) {
        w->done[w->done_len] = '/';
    }
    memcpy(w->done + w->done_len + sep, name, n);
    w->done_len += sep + n;
    walk_leave(w);
    w->cur = h;
    return 0;
}

/* The last name, missing, made as a file the caller may use: its handle,
 * or NULL with `errno` (`EEXIST` when one appeared meanwhile). */
static HANDLE walk_make(Walk *w, const char *name, size_t n, ACCESS_MASK access) {
    HANDLE h;
    NTSTATUS s = win_open_name(w->cur, name, n, access, FILE_CREATE, FILE_NON_DIRECTORY_FILE, &h);
    if (!NT_SUCCESS(s)) {
        errno = win_errno_nt(s);
        return NULL;
    }
    return h;
}

/* Walks `path` (UTF-8, NUL-free) beneath `dir` and opens what it names as
 * `want` says, links inside followed: its handle, or NULL with `errno`
 * (`EXDEV`: the path leads outside). */
static HANDLE win_resolve(HANDLE dir, const char *path, const WinWant *want) {
    size_t len = strlen(path);
    if (win_outside(path, len)) {
        errno = EXDEV;
        return NULL;
    }
    if (len > WALK_MAX) {
        errno = ENAMETOOLONG;
        return NULL;
    }
    Walk *w = malloc(sizeof *w);
    if (w == NULL) {
        die("panic: out of memory\n");
    }
    memcpy(w->rest, path, len + 1);
    for (size_t i = 0; i < len; i++) {
        if (w->rest[i] == '\\') {
            w->rest[i] = '/';
        }
    }
    w->at = 0;
    w->dir = w->cur = dir;
    w->done_len = 0;
    w->links = 0;
    HANDLE result = NULL;
    // A missing file made meanwhile by someone else is looked at again.
    int retries = 0;
    for (;;) {
        const char *at;
        size_t n = walk_next(w, &at);
        if (n == 0) {
            // The path names the folder reached itself.
            if (want->kind == 1) {
                errno = EISDIR;
                break;
            }
            result = win_reopen(w->cur, want->access, true);
            break;
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
        if (!win_name_ok(name, n)) {
            errno = WIN_RESERVED;
            break;
        }
        bool folder;
        bool final = walk_ends(w, &folder);
        HANDLE h;
        NTSTATUS s = win_open_name(w->cur, name, n, FILE_READ_ATTRIBUTES, FILE_OPEN, 0, &h);
        if (!NT_SUCCESS(s)) {
            errno = win_errno_nt(s);
            if (errno == ENOENT && final && want->create && !folder && want->kind == 1) {
                result = walk_make(w, name, n, want->access);
                if (result == NULL && errno == EEXIST && ++retries < 4) {
                    // Made meanwhile: looked at again, a link followed.
                    w->at = (size_t)(at - w->rest);
                    continue;
                }
            }
            break;
        }
        FILE_ATTRIBUTE_TAG_INFO tag;
        if (!win_tag(h, &tag)) {
            CloseHandle(h);
            break;
        }
        if (tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) {
            int followed = walk_follow(w, h, tag.ReparseTag);
            CloseHandle(h);
            if (followed != 0) {
                break;
            }
            continue;
        }
        bool is_folder = (tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0;
        if (!final) {
            if (!is_folder) {
                CloseHandle(h);
                errno = ENOTDIR;
                break;
            }
            HANDLE next = win_reopen(h, FILE_TRAVERSE | FILE_LIST_DIRECTORY, true);
            CloseHandle(h);
            if (next == NULL || walk_down(w, next, name, n) != 0) {
                break;
            }
            continue;
        }
        if ((folder || want->kind == 2) && !is_folder) {
            errno = ENOTDIR;
        } else if (want->kind == 1 && is_folder) {
            errno = EISDIR;
        } else {
            result = win_reopen(h, want->access, is_folder);
        }
        CloseHandle(h);
        break;
    }
    int code = errno;
    walk_leave(w);
    free(w);
    errno = code;
    return result;
}
