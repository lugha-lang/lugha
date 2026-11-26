/*
 * os/windows/dir_names.c — part of lugha_rt.c on Windows: a `Dir`'s
 * listing, and its operations on what a path names. Making, removing and
 * renaming act on a path's last part: the folder holding it is reached
 * beneath the `Dir` first (dir_resolve.c), then the one name is opened
 * there as itself, never `.` or `..`, so a link is removed or renamed
 * itself, never followed. Kind, size, mode and syncing act on what the
 * walk resolved beneath the `Dir`, never a link's target outside it.
 *
 * Removing and renaming use Windows 10's POSIX semantics where the file
 * system has them: a removed file goes at once though it is open, and a
 * rename replaces its target in one step; elsewhere the classic ones.
 *
 * Each runs on a helper thread, as dir.c's operations do.
 *
 * A fragment, not compiled on its own: joined right after dir.c
 * (`LughaDir`, the `dir_job` family, `DIR_OUTSIDE`).
 */

/* On a helper: every entry's name but `.` and `..`, as UTF-8 each ending
 * in NUL, in `out`, and how many in `value`. */
static void dir_list_job(Job *job) {
    HANDLE h = dir_job_open(job, FILE_LIST_DIRECTORY, 2, false);
    if (h == NULL) {
        return;
    }
    // Entries come in batches; 8-aligned, as each entry's fields are.
    enum { BATCH = 64 * 1024 };
    char *buffer = malloc(BATCH);
    if (buffer == NULL) {
        die("panic: out of memory\n");
    }
    size_t cap = 0;
    FILE_INFO_BY_HANDLE_CLASS ask = FileIdBothDirectoryRestartInfo;
    while (GetFileInformationByHandleEx(h, ask, buffer, BATCH)) {
        ask = FileIdBothDirectoryInfo;
        size_t at = 0;
        for (;;) {
            FILE_ID_BOTH_DIR_INFO *e = (FILE_ID_BOTH_DIR_INFO *)(buffer + at);
            int units = (int)(e->FileNameLength / 2);
            bool dots = (units == 1 && e->FileName[0] == L'.') ||
                        (units == 2 && e->FileName[0] == L'.' && e->FileName[1] == L'.');
            if (!dots) {
                char name[WALK_NAME * 3];
                int n = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, e->FileName, units,
                                            name, sizeof name, NULL, NULL);
                if (n <= 0) {
                    job->err = WIN_NOT_UNICODE;
                    free(buffer);
                    CloseHandle(h);
                    return;
                }
                job_grow(&job->out, &cap, job->out_len, (size_t)n + 1);
                memcpy(job->out + job->out_len, name, (size_t)n);
                job->out[job->out_len + (size_t)n] = '\0';
                job->out_len += (size_t)n + 1;
                job->value++;
            }
            if (e->NextEntryOffset == 0) {
                break;
            }
            at += e->NextEntryOffset;
        }
    }
    if (GetLastError() != ERROR_NO_MORE_FILES) {
        job->err = win_errno_last();
    }
    free(buffer);
    CloseHandle(h);
}

/* `$os.dir_list(dir, path)`: every entry's name as bytes, without `.` and
 * `..`; a name that isn't valid UTF-16 is an error naming the folder. */
int32_t lugha_rt_dir_list(void *dir, const LughaString *path, LughaArray **out,
                          LughaString **error) {
    Job *job = dir_job(dir_list_job, dir, path, error);
    if (job == NULL) {
        return 0;
    }
    job_run(job);
    if (job->err == WIN_NOT_UNICODE) {
        job_free(job);
        size_t room = (size_t)path->len + 64;
        char *message = malloc(room);
        if (message == NULL) {
            die("panic: out of memory\n");
        }
        snprintf(message, room, "a name in %.*s is not valid Unicode", (int)path->len,
                 path->bytes);
        int32_t failed = fs_fail(error, message);
        free(message);
        return failed;
    }
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

/* `path` split into the folder holding its last part and that part, two
 * copies from `malloc` at `parts[0]` and `parts[1]`; false with the
 * message set. The last part must be a name Windows allows, not `.` or
 * `..`; `/` and `\` both separate. */
static bool dir_split(const LughaString *path, char **parts, LughaString **error) {
    if (!dir_path_ok(path, error)) {
        return false;
    }
    if (win_outside(path->bytes, (size_t)path->len)) {
        fs_fail(error, DIR_OUTSIDE);
        return false;
    }
    int64_t n = path->len;
    while (n > 1 && (path->bytes[n - 1] == '/' || path->bytes[n - 1] == '\\')) {
        n--;
    }
    const char *sep = NULL;
    for (int64_t i = n; i > 0; i--) {
        if (path->bytes[i - 1] == '/' || path->bytes[i - 1] == '\\') {
            sep = path->bytes + i - 1;
            break;
        }
    }
    const char *last = sep == NULL ? path->bytes : sep + 1;
    size_t last_len = (size_t)(path->bytes + n - last);
    if (last_len == 0 || (last_len == 1 && last[0] == '.') ||
        (last_len == 2 && last[0] == '.' && last[1] == '.')) {
        fs_fail(error, "a path must end in a name, not `.` or `..`");
        return false;
    }
    if (!win_name_ok(last, last_len)) {
        fs_fail(error, win_message(WIN_RESERVED));
        return false;
    }
    parts[0] = sep == NULL ? job_copy(".", 1) : job_copy(path->bytes, (size_t)(sep - path->bytes));
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
    job->n = (int64_t)(intptr_t)((LughaDir *)dir)->h;
    job->hold = dir;
    return job;
}

/* On a helper: the folder holding the `i`th name, or NULL with the failure
 * kept. */
static HANDLE dir_names_parent(Job *job, int i) {
    WinWant want = {FILE_TRAVERSE | FILE_LIST_DIRECTORY, 2, false};
    HANDLE h = win_resolve(dir_job_dir(job), job->argv[2 * i], &want);
    if (h == NULL) {
        job->err = errno;
    }
    return h;
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

/* On a helper: makes one folder. */
static void dir_make_job(Job *job) {
    HANDLE parent = dir_names_parent(job, 0);
    if (parent == NULL) {
        return;
    }
    HANDLE made;
    NTSTATUS s = win_open_name(parent, job->argv[1], strlen(job->argv[1]), FILE_READ_ATTRIBUTES,
                               FILE_CREATE, FILE_DIRECTORY_FILE, &made);
    if (NT_SUCCESS(s)) {
        CloseHandle(made);
    } else {
        job->err = win_errno_nt(s);
    }
    CloseHandle(parent);
}

/* `$os.dir_make(dir, path)`: one folder. */
int32_t lugha_rt_dir_make(void *dir, const LughaString *path, int64_t *done, LughaString **error) {
    return dir_names_run(dir_names_job(dir_make_job, dir, &path, 1, error), done, error);
}

/* The last name opened from `parent` as itself, for removing or renaming:
 * its handle and whether it is a real folder (a link to one isn't); NULL
 * with the failure kept. */
static HANDLE dir_names_take(Job *job, HANDLE parent, const char *name, bool *folder) {
    HANDLE h;
    NTSTATUS s = win_open_name(parent, name, strlen(name), DELETE | FILE_READ_ATTRIBUTES, FILE_OPEN,
                               0, &h);
    if (!NT_SUCCESS(s)) {
        job->err = win_errno_nt(s);
        return NULL;
    }
    FILE_ATTRIBUTE_TAG_INFO tag;
    if (!win_tag(h, &tag)) {
        job->err = errno;
        CloseHandle(h);
        return NULL;
    }
    *folder = (tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0 &&
              (tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) == 0;
    return h;
}

/* On a helper: removes a name: a file or link (`flags` 0) or an empty
 * folder (1). */
static void dir_remove_job(Job *job) {
    HANDLE parent = dir_names_parent(job, 0);
    if (parent == NULL) {
        return;
    }
    bool folder;
    HANDLE h = dir_names_take(job, parent, job->argv[1], &folder);
    CloseHandle(parent);
    if (h == NULL) {
        return;
    }
    if (folder != (job->flags == 1)) {
        job->err = folder ? EISDIR : ENOTDIR;
        CloseHandle(h);
        return;
    }
    // FileDispositionInformationEx (64): delete, POSIX semantics, even if
    // read-only; else (FAT, older Windows) FileDispositionInformation (13).
    ULONG posix = 0x1 | 0x2 | 0x10;
    bool unknown;
    int err = win_set_info(h, 64, &posix, sizeof posix, &unknown);
    if (err != 0 && unknown) {
        BOOLEAN classic = TRUE;
        err = win_set_info(h, 13, &classic, sizeof classic, &unknown);
    }
    job->err = err;
    CloseHandle(h);
}

/* `$os.dir_remove(dir, path, folder)`: a file or a link itself (`folder`
 * 0), or an empty folder (1). */
int32_t lugha_rt_dir_remove(void *dir, const LughaString *path, int64_t folder, int64_t *done,
                            LughaString **error) {
    Job *job = dir_names_job(dir_remove_job, dir, &path, 1, error);
    if (job != NULL) {
        job->flags = folder ? 1 : 0;
    }
    return dir_names_run(job, done, error);
}

/* On a helper: renames the first name to the second, replacing it. */
static void dir_rename_job(Job *job) {
    HANDLE from = dir_names_parent(job, 0);
    if (from == NULL) {
        return;
    }
    HANDLE to = dir_names_parent(job, 1);
    bool folder;
    HANDLE h = to == NULL ? NULL : dir_names_take(job, from, job->argv[1], &folder);
    CloseHandle(from);
    if (h == NULL) {
        if (to != NULL) {
            CloseHandle(to);
        }
        return;
    }
    wchar_t name[WALK_NAME];
    int units = win_wide(job->argv[3], strlen(job->argv[3]), name, WALK_NAME);
    if (units < 0) {
        job->err = EINVAL;
        CloseHandle(h);
        CloseHandle(to);
        return;
    }
    // Windows wants the whole struct and the name past its first character.
    size_t size = sizeof(FILE_RENAME_INFO) + (size_t)(units + 1) * sizeof(wchar_t);
    FILE_RENAME_INFO *info = calloc(1, size);
    if (info == NULL) {
        die("panic: out of memory\n");
    }
    info->RootDirectory = to;
    info->FileNameLength = (DWORD)units * sizeof(wchar_t);
    memcpy(info->FileName, name, (size_t)(units + 1) * sizeof(wchar_t));
    // FileRenameInformationEx (65): replacing, POSIX semantics; else
    // FileRenameInformation (10), replacing the classic way. The struct is
    // the same, its first field the flags or the replace switch.
    info->Flags = FILE_RENAME_FLAG_REPLACE_IF_EXISTS | FILE_RENAME_FLAG_POSIX_SEMANTICS;
    bool unknown;
    int err = win_set_info(h, 65, info, (ULONG)size, &unknown);
    if (err != 0 && unknown) {
        info->Flags = 0;
        info->ReplaceIfExists = TRUE;
        err = win_set_info(h, 10, info, (ULONG)size, &unknown);
    }
    job->err = err;
    free(info);
    CloseHandle(h);
    CloseHandle(to);
}

/* `$os.dir_rename(dir, from, to)`: replacing `to` in one step, both names
 * beneath `dir`. */
int32_t lugha_rt_dir_rename(void *dir, const LughaString *from, const LughaString *to,
                            int64_t *done, LughaString **error) {
    const LughaString *paths[2] = {from, to};
    return dir_names_run(dir_names_job(dir_rename_job, dir, paths, 2, error), done, error);
}

/* On a helper: what the job's path names, links inside followed: whether
 * a folder in `flags`, its size in `value`. */
static void dir_stat_job(Job *job) {
    HANDLE h = dir_job_open(job, FILE_READ_ATTRIBUTES, 0, false);
    if (h == NULL) {
        return;
    }
    FILE_STANDARD_INFO info;
    if (!GetFileInformationByHandleEx(h, FileStandardInfo, &info, sizeof info)) {
        job->err = win_errno_last();
    } else {
        job->flags = info.Directory ? 1 : 0;
        job->value = info.EndOfFile.QuadPart;
    }
    CloseHandle(h);
}

/* `$os.dir_kind(dir, path)`: 0 when nothing is there (or it can't be
 * reached), 1 for a file, 2 for a folder; links inside followed. */
int64_t lugha_rt_dir_kind(void *dir, const LughaString *path) {
    LughaString *ignored = NULL;
    Job *job = dir_job(dir_stat_job, dir, path, &ignored);
    if (job == NULL) {
        rt_let_go(ignored);
        return 0;
    }
    job_run(job);
    int64_t kind = job->err != 0 ? 0 : job->flags ? 2 : 1;
    job_free(job);
    return kind;
}

/* `$os.dir_size(dir, path)`: a file's size; a folder is `Is a directory`. */
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
    bool folder = job->flags != 0;
    int64_t size = job->value;
    job_free(job);
    if (folder) {
        return fs_fail(error, win_message(EISDIR));
    }
    *out = size;
    return 1;
}

/* On a helper: the file made read-only when `n` lacks the owner's write
 * bit, writable when it has it; Windows has no other mode bits. */
static void dir_chmod_job(Job *job) {
    HANDLE h = dir_job_open(job, FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES, 0, false);
    if (h == NULL) {
        return;
    }
    FILE_BASIC_INFO info;
    if (!GetFileInformationByHandleEx(h, FileBasicInfo, &info, sizeof info)) {
        job->err = win_errno_last();
    } else {
        DWORD attrs = info.FileAttributes & ~(DWORD)FILE_ATTRIBUTE_READONLY;
        if ((job->value & 0200) == 0) {
            attrs |= FILE_ATTRIBUTE_READONLY;
        }
        // Zero leaves a time as it is; NORMAL stands for no attributes.
        info.CreationTime.QuadPart = 0;
        info.LastAccessTime.QuadPart = 0;
        info.LastWriteTime.QuadPart = 0;
        info.ChangeTime.QuadPart = 0;
        info.FileAttributes = attrs == 0 ? FILE_ATTRIBUTE_NORMAL : attrs;
        if (!SetFileInformationByHandle(h, FileBasicInfo, &info, sizeof info)) {
            job->err = win_errno_last();
        }
    }
    CloseHandle(h);
}

/* `$os.dir_chmod(dir, path, mode)`: on Windows, only whether the file is
 * read-only, from the owner's write bit. */
int32_t lugha_rt_dir_chmod(void *dir, const LughaString *path, int64_t mode, int64_t *done,
                           LughaString **error) {
    if (mode < 0 || mode > 07777) {
        return fs_fail(error, "mode must be from 0 to 4095 (0o7777)");
    }
    Job *job = dir_job(dir_chmod_job, dir, path, error);
    if (job != NULL) {
        job->value = mode;
    }
    return dir_names_run(job, done, error);
}

/* On a helper: a file's data written out to the disk; a folder, which
 * Windows won't flush and NTFS journals, is left as it is. */
static void dir_sync_job(Job *job) {
    HANDLE h = dir_job_open(job, FILE_READ_ATTRIBUTES, 0, false);
    if (h == NULL) {
        return;
    }
    FILE_ATTRIBUTE_TAG_INFO tag;
    if (!win_tag(h, &tag)) {
        job->err = errno;
    } else if ((tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY) == 0) {
        HANDLE w = win_reopen(h, GENERIC_WRITE, false);
        if (w == NULL) {
            job->err = errno;
        } else {
            if (!FlushFileBuffers(w)) {
                job->err = win_errno_last();
            }
            CloseHandle(w);
        }
    }
    CloseHandle(h);
}

/* `$os.dir_sync(dir, path)`: a file's data on the disk. */
int32_t lugha_rt_dir_sync(void *dir, const LughaString *path, int64_t *done,
                          LughaString **error) {
    return dir_names_run(dir_job(dir_sync_job, dir, path, error), done, error);
}
