/*
 * os/darwin/dir_names.c — part of lugha_rt.c: `Dir`'s operations on names
 * (make, remove, rename, kind, size, mode, sync) on macOS, refused for now
 * as every folder operation is (os/darwin/dir.c).
 *
 * A fragment, not compiled on its own: joined right after os/darwin/dir.c.
 */

int32_t lugha_rt_dir_make(void *dir, const LughaString *path, int64_t *done, LughaString **error) {
    (void)dir;
    (void)path;
    (void)done;
    return dir_refused(error);
}

int32_t lugha_rt_dir_remove(void *dir, const LughaString *path, int64_t folder, int64_t *done,
                            LughaString **error) {
    (void)dir;
    (void)path;
    (void)folder;
    (void)done;
    return dir_refused(error);
}

int32_t lugha_rt_dir_rename(void *dir, const LughaString *from, const LughaString *to,
                            int64_t *done, LughaString **error) {
    (void)dir;
    (void)from;
    (void)to;
    (void)done;
    return dir_refused(error);
}

/* Nothing is there that can be reached: 0, as for a missing path. */
int64_t lugha_rt_dir_kind(void *dir, const LughaString *path) {
    (void)dir;
    (void)path;
    return 0;
}

int32_t lugha_rt_dir_size(void *dir, const LughaString *path, int64_t *out, LughaString **error) {
    (void)dir;
    (void)path;
    (void)out;
    return dir_refused(error);
}

int32_t lugha_rt_dir_chmod(void *dir, const LughaString *path, int64_t mode, int64_t *done,
                           LughaString **error) {
    (void)dir;
    (void)path;
    (void)mode;
    (void)done;
    return dir_refused(error);
}

int32_t lugha_rt_dir_sync(void *dir, const LughaString *path, int64_t *done, LughaString **error) {
    (void)dir;
    (void)path;
    (void)done;
    return dir_refused(error);
}
