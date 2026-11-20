
/* Bytes: stdin as bytes, and
 * conversions between `string` and `u8[]`. Text never holds invalid UTF-8:
 * the prelude's `text_from_utf8` checks bytes before
 * `lugha_rt_str_from_bytes` turns them into a string. */

/* A new `u8[]` holding a copy of `len` bytes at `data`. */
static LughaArray *bytes_of(const void *data, int64_t len) {
    LughaArray *array = lugha_rt_alloc((int64_t)sizeof(LughaArray) + len);
    array->len = len;
    array->cap = len;
    if (len > 0) {
        memcpy(array->elems, data, (size_t)len);
    }
    return array;
}

/* A read of stdin failed: `*error` is "stdin: " and the system's text, and
 * stdin's error mark is cleared so a later read tries again. */
static void stdin_failed(int err, LughaString **error) {
    char message[256];
    snprintf(message, sizeof message, "stdin: %s", strerror(err));
    *error = string_of(message, (int64_t)strlen(message));
    clearerr(stdin);
}

/* `read_stdin_bytes()`: 1 and everything left on stdin, or 0 and why a
 * read failed, so a failure never looks like the end of input. */
int32_t lugha_rt_read_stdin_bytes(LughaArray **out, LughaString **error) {
    size_t len = 0;
    size_t cap = 4096;
    char *buffer = malloc(cap);
    if (buffer == NULL) {
        die("panic: out of memory\n");
    }
    size_t got;
    while ((got = fread(buffer + len, 1, cap - len, stdin)) > 0) {
        len += got;
        if (len == cap) {
            cap *= 2;
            char *bigger = realloc(buffer, cap);
            if (bigger == NULL) {
                die("panic: out of memory\n");
            }
            buffer = bigger;
        }
    }
    if (ferror(stdin)) {
        int err = errno != 0 ? errno : EIO;
        free(buffer);
        stdin_failed(err, error);
        return 0;
    }
    *out = bytes_of(buffer, (int64_t)len);
    free(buffer);
    return 1;
}

/* Lines `read_line` has read from stdin, under stdin's lock: one stream for
 * the whole process, so every `Console` counts the same lines. */
static int64_t stdin_lines;

/* On a helper: the next line of stdin into `out`, `value` its length (-1
 * at the end or on an error, `err` set for an error), `n` its number. */
static void read_line_job(Job *job) {
    size_t cap = 0;
    flockfile(stdin);
    errno = 0;
    job->value = (int64_t)getline(&job->out, &cap, stdin);
    if (job->value >= 0) {
        job->n = ++stdin_lines;
    } else if (ferror(stdin)) {
        job->err = errno != 0 ? errno : EIO;
    }
    funlockfile(stdin);
}

/* `read_line()`: 1 with the next stdin line's number and bytes, without its
 * `\n` (and a `\r` before it); 0 at the end of input; -1 and why a read
 * failed. The bytes aren't checked here: the prelude decides what UTF-8 is.
 * Stdout is flushed first, so a prompt written with `print` shows; the read
 * runs on a helper thread, so other tasks run while it waits. */
int32_t lugha_rt_read_line(int64_t *number, LughaArray **out, LughaString **error) {
    fflush(stdout);
    Job *job = job_new(read_line_job);
    job_run(job);
    if (job->value < 0) {
        int err = job->err;
        job_free(job);
        if (err != 0) {
            stdin_failed(err, error);
            return -1;
        }
        return 0;
    }
    int64_t n = job->value;
    const char *line = job->out;
    if (n > 0 && line[n - 1] == '\n') {
        n--;
    }
    if (n > 0 && line[n - 1] == '\r') {
        n--;
    }
    *number = job->n;
    *out = bytes_of(line, n);
    job_free(job);
    return 1;
}

/* `s.to_bytes()`: the string's bytes as a new `u8[]`. */
LughaArray *lugha_rt_str_to_bytes(const LughaString *s) {
    return bytes_of(s->bytes, s->len);
}

/* `$string.from_bytes(bytes)`: bytes the prelude has
 * already checked as UTF-8, copied into a new string. */
LughaString *lugha_rt_str_from_bytes(const LughaArray *bytes) {
    return string_of((const char *)bytes->elems, bytes->len);
}
