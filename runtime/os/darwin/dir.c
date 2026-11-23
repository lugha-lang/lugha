/*
 * os/darwin/dir.c — part of lugha_rt.c: `Dir` and `Fs` on macOS, refused
 * for now. Linux confines every path beneath its folder with `openat2`,
 * which macOS lacks; until macOS has its own confinement, every folder
 * operation fails closed, so no program reaches outside a folder, since
 * none reaches a folder at all.
 *
 * A fragment, not compiled on its own: joined where os/linux/dir.c is.
 */

static const char DIR_MACOS[] = "folders aren't supported on macOS yet";

static int32_t dir_refused(LughaString **error) {
    *error = string_of(DIR_MACOS, (int64_t)strlen(DIR_MACOS));
    return 0;
}

int32_t lugha_rt_dir_cwd(void **out, LughaString **error) {
    (void)out;
    return dir_refused(error);
}

int32_t lugha_rt_dir_open(const LughaString *path, void **out, LughaString **error) {
    (void)path;
    (void)out;
    return dir_refused(error);
}

int32_t lugha_rt_dir_sub(void *dir, const LughaString *path, void **out, LughaString **error) {
    (void)dir;
    (void)path;
    (void)out;
    return dir_refused(error);
}

int32_t lugha_rt_dir_read_bytes(void *dir, const LughaString *path, LughaArray **out,
                                LughaString **error) {
    (void)dir;
    (void)path;
    (void)out;
    return dir_refused(error);
}

int32_t lugha_rt_dir_read_text(void *dir, const LughaString *path, LughaString **out,
                               LughaString **error) {
    (void)dir;
    (void)path;
    (void)out;
    return dir_refused(error);
}

int32_t lugha_rt_dir_write(void *dir, const LughaString *path, const LughaArray *bytes,
                           int64_t append, int64_t *count, LughaString **error) {
    (void)dir;
    (void)path;
    (void)bytes;
    (void)append;
    (void)count;
    return dir_refused(error);
}

int32_t lugha_rt_dir_list(void *dir, const LughaString *path, LughaArray **out,
                          LughaString **error) {
    (void)dir;
    (void)path;
    (void)out;
    return dir_refused(error);
}

int32_t lugha_rt_dir_file(void *dir, const LughaString *path, int64_t mode, int64_t *out,
                          LughaString **error) {
    (void)dir;
    (void)path;
    (void)mode;
    (void)out;
    return dir_refused(error);
}

int32_t lugha_rt_dir_path(void *dir, LughaArray **out, LughaString **error) {
    (void)dir;
    (void)out;
    return dir_refused(error);
}
