/*
 * fs.c — part of lugha_rt.c: what the file code shares (dir.c, a `Dir`'s
 * operations, and file.c, its file streams): failing with a message, and
 * refusing a path with a NUL byte in it.
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it after the
 * core (lugha_rt.c), whose includes (`<dirent.h>`, `<sys/stat.h>`,
 * `<errno.h>`, `<string.h>`) it relies on, and after bytes.c (`bytes_of`)
 * and input.c (`string_of`).
 */

/* Fails with `message`: 0, and `*error` set. */
static int32_t fs_fail(LughaString **error, const char *message) {
    *error = string_of(message, (int64_t)strlen(message));
    return 0;
}

/* Whether `path` holds a NUL byte, which would cut it short in C. */
/* Unused on Windows, whose parts that call it aren't built yet. */
__attribute__((unused)) static bool fs_has_nul(const LughaString *path) {
    return memchr(path->bytes, 0, (size_t)path->len) != NULL;
}
