/*
 * env.c — part of lugha_rt.c: the environment for `Env`. Values are
 * returned as bytes; the prelude checks them as UTF-8.
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it after the
 * core (lugha_rt.c), whose includes (`<stdlib.h>`, `<unistd.h>`,
 * `<errno.h>`, `<string.h>`) it relies on, and after bytes.c (`bytes_of`)
 * and input.c (`string_of`).
 */

/* `$os.env(name)`: 1 and the variable's bytes, or 0 when it is unset or
 * `name` holds a NUL byte (which would cut the name short). The value is
 * copied at once, so later changes to the environment can't affect it. */
int32_t lugha_rt_env(const LughaString *name, LughaArray **out) {
    if (memchr(name->bytes, 0, (size_t)name->len) != NULL) {
        return 0;
    }
    const char *value = getenv(name->bytes);
    if (value == NULL) {
        return 0;
    }
    *out = bytes_of(value, (int64_t)strlen(value));
    return 1;
}
