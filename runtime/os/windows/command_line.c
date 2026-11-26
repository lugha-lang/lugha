/*
 * os/windows/command_line.c — part of lugha_rt.c on Windows: how `Proc`
 * names a program and hands it its arguments, with never a shell.
 *
 * Windows gives a program one string, which the program splits itself;
 * nearly every program splits it by the Microsoft C runtime's rules (as
 * `CommandLineToArgvW` does), and each argument is quoted by those rules
 * here, as Rust's `std::process` and Go's `syscall.EscapeArg` do, so it
 * arrives as given. The program's own name, first, is quoted without
 * escapes, as Windows reads it: a name holding `"` is refused.
 *
 * A bare name is looked up on `PATH` only, each folder in order, as given
 * when it has an extension, else as `.exe` then `.com`: never the current
 * folder or this program's, where Windows' own search would look first. A
 * batch file (`.bat`, `.cmd`) is refused: it runs through cmd.exe, a
 * shell, which would read `&`, `|` and `%` in its arguments.
 *
 * A fragment, not compiled on its own: joined after os/windows/paths.c
 * (`win_wide`) and before os/windows/process.c, which uses these.
 */

/* A command line being built: UTF-8, from `malloc`. */
typedef struct {
    char *bytes;
    size_t len;
    size_t cap;
} WinLine;

/* `n` bytes of `s` onto the end of `line`. */
static void win_line_put(WinLine *line, const char *s, size_t n) {
    job_grow(&line->bytes, &line->cap, line->len, n + 1);
    memcpy(line->bytes + line->len, s, n);
    line->len += n;
    line->bytes[line->len] = '\0';
}

/* `n` backslashes onto `line`. */
static void win_line_slashes(WinLine *line, size_t n) {
    for (size_t i = 0; i < n; i++) {
        win_line_put(line, "\\", 1);
    }
}

/* One argument, quoted when it must be: empty, or holding a space, tab,
 * newline, vertical tab or `"`. Inside quotes a `"` is `\"`, and the
 * backslashes before a `"` (the closing one included) are doubled; any
 * other backslash is itself. */
static void win_line_arg(WinLine *line, const char *arg) {
    if (arg[0] != '\0' && strpbrk(arg, " \t\n\v\"") == NULL) {
        win_line_put(line, arg, strlen(arg));
        return;
    }
    win_line_put(line, "\"", 1);
    const char *p = arg;
    for (;;) {
        size_t slashes = 0;
        while (*p == '\\') {
            slashes++;
            p++;
        }
        if (*p == '\0') {
            win_line_slashes(line, 2 * slashes);
            break;
        }
        if (*p == '"') {
            win_line_slashes(line, 2 * slashes + 1);
        } else {
            win_line_slashes(line, slashes);
        }
        win_line_put(line, p, 1);
        p++;
    }
    win_line_put(line, "\"", 1);
}

/* The whole command line: the program's name quoted as is (it holds no
 * `"`), then each argument. */
static void win_line_build(WinLine *line, char **argv) {
    win_line_put(line, "\"", 1);
    win_line_put(line, argv[0], strlen(argv[0]));
    win_line_put(line, "\"", 1);
    for (char **a = argv + 1; *a != NULL; a++) {
        win_line_put(line, " ", 1);
        win_line_arg(line, *a);
    }
}

/* Whether `name` ends `.bat` or `.cmd`, in any case. */
static bool win_is_batch(const char *name) {
    size_t n = strlen(name);
    return n >= 4 && (_stricmp(name + n - 4, ".bat") == 0 || _stricmp(name + n - 4, ".cmd") == 0);
}

/* Whether `cmd` names a path rather than a program on `PATH`. */
static bool win_is_path(const char *cmd) {
    return strpbrk(cmd, "/\\:") != NULL;
}

/* Whether `path` (UTF-8) is a file, not a folder. */
static bool win_is_file(const char *path) {
    size_t n = strlen(path);
    wchar_t *wide = malloc((n + 1) * sizeof *wide);
    if (wide == NULL) {
        die("panic: out of memory\n");
    }
    DWORD attributes = win_wide(path, n, wide, (int)n + 1) < 0 ? INVALID_FILE_ATTRIBUTES
                                                               : GetFileAttributesW(wide);
    free(wide);
    return attributes != INVALID_FILE_ATTRIBUTES && (attributes & FILE_ATTRIBUTE_DIRECTORY) == 0;
}

/* `folder\name` with `ext` after, from `malloc`, if it is a file; else NULL. */
static char *win_try(const char *folder, size_t folder_len, const char *name, const char *ext) {
    WinLine path = {NULL, 0, 0};
    win_line_put(&path, folder, folder_len);
    if (folder_len > 0 && folder[folder_len - 1] != '\\' && folder[folder_len - 1] != '/') {
        win_line_put(&path, "\\", 1);
    }
    win_line_put(&path, name, strlen(name));
    win_line_put(&path, ext, strlen(ext));
    if (win_is_file(path.bytes)) {
        return path.bytes;
    }
    free(path.bytes);
    return NULL;
}

/* The program `cmd` names, from `malloc`: itself when it is a path, else
 * the first match on `PATH`; NULL when there is none. */
static char *win_find_program(const char *cmd) {
    if (win_is_path(cmd)) {
        return job_copy(cmd, strlen(cmd));
    }
    char *path = os_env("PATH");
    if (path == NULL) {
        return NULL;
    }
    // An extension in the name is used as given; without one, `.exe`, `.com`.
    bool has_ext = strchr(cmd, '.') != NULL;
    static const char *const exts[] = {".exe", ".com"};
    char *found = NULL;
    for (char *at = path; found == NULL && *at != '\0';) {
        size_t n = strcspn(at, ";");
        const char *folder = at;
        size_t folder_len = n;
        // A folder may be written in quotes, for its spaces.
        if (folder_len >= 2 && folder[0] == '"' && folder[folder_len - 1] == '"') {
            folder++;
            folder_len -= 2;
        }
        if (folder_len > 0) {
            if (has_ext) {
                found = win_try(folder, folder_len, cmd, "");
            }
            for (size_t i = 0; found == NULL && !has_ext && i < 2; i++) {
                found = win_try(folder, folder_len, cmd, exts[i]);
            }
        }
        at += n + (at[n] == ';' ? 1 : 0);
    }
    free(path);
    return found;
}
