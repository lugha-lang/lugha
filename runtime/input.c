
/* ---- input.c: command-line arguments and parsing text ---- */

/* `args()`: a fresh `string[]` of the program's arguments on each call, so
 * no program can share or change the runtime's copy. */
LughaArray *lugha_rt_args(void) {
    int64_t count = program_argc < 0 ? 0 : program_argc;
    LughaArray *array =
        lugha_rt_alloc((int64_t)sizeof(LughaArray) + count * (int64_t)sizeof(LughaString *));
    array->len = count;
    array->cap = count;
    LughaString **elems = (LughaString **)(void *)array->elems;
    for (int64_t i = 0; i < count; i++) {
        size_t len = strlen(program_argv[i]);
        LughaString *s = lugha_rt_alloc((int64_t)sizeof(LughaString) + (int64_t)len + 1);
        s->len = (int64_t)len;
        memcpy(s->bytes, program_argv[i], len + 1);
        elems[i] = s;
    }
    return array;
}

/* Skips decimal digits from `*i`; true if there was at least one. */
static bool digits(const LughaString *s, int64_t *i) {
    int64_t start = *i;
    while (*i < s->len && s->bytes[*i] >= '0' && s->bytes[*i] <= '9') {
        (*i)++;
    }
    return *i > start;
}

/* `parse_f64(s)`: `-`? digits (`.` digits)? ([eE] [+-]? digits)? and nothing
 * else, checked here first because `strtod` also takes spaces, hex, `inf`
 * and `nan`. Lugha strings end in a NUL, so `strtod` can read the bytes. */
int32_t lugha_rt_parse_f64(const LughaString *s, double *out) {
    int64_t i = 0;
    if (i < s->len && s->bytes[i] == '-') {
        i++;
    }
    if (!digits(s, &i)) {
        return 0;
    }
    if (i < s->len && s->bytes[i] == '.') {
        i++;
        if (!digits(s, &i)) {
            return 0;
        }
    }
    if (i < s->len && (s->bytes[i] == 'e' || s->bytes[i] == 'E')) {
        i++;
        if (i < s->len && (s->bytes[i] == '+' || s->bytes[i] == '-')) {
            i++;
        }
        if (!digits(s, &i)) {
            return 0;
        }
    }
    if (i != s->len) {
        return 0;
    }
    *out = strtod(s->bytes, NULL);
    return 1;
}

/* ---- I/O: stdin lines, stderr and exit ---- */

/* A new Lugha string holding the `len` bytes at `bytes`. */
static LughaString *string_of(const char *bytes, int64_t len) {
    LughaString *s = lugha_rt_alloc((int64_t)sizeof(LughaString) + len + 1);
    s->len = len;
    memcpy(s->bytes, bytes, (size_t)len);
    s->bytes[len] = '\0';
    return s;
}

/* `read_line()`: 1 and the next stdin line without its `\n` (and a `\r`
 * before it) in `*out`, or 0 at the end of input or on a read error.
 * Stdout is flushed first, so a prompt written with `print` shows. */
int32_t lugha_rt_read_line(LughaString **out) {
    fflush(stdout);
    char *line = NULL;
    size_t cap = 0;
    ssize_t n = getline(&line, &cap, stdin);
    if (n < 0) {
        free(line);
        return 0;
    }
    if (n > 0 && line[n - 1] == '\n') {
        n--;
    }
    if (n > 0 && line[n - 1] == '\r') {
        n--;
    }
    *out = string_of(line, n);
    free(line);
    return 1;
}

/* `eprint(x)` / `eprintln(x)`, with `x` already formatted: stdout is
 * flushed first so a terminal shows output in program order. */
void lugha_rt_eprint(const LughaString *text, int32_t newline) {
    fflush(stdout);
    fwrite(text->bytes, 1, (size_t)text->len, stderr);
    if (newline) {
        fputc('\n', stderr);
    }
}

/* `exit(code)`: stdout flushed, then the process ends with `code`. */
void lugha_rt_exit(int32_t code) {
    fflush(stdout);
    exit(code);
}

/* `flush()`: what `print` wrote so far, out now. A
 * failure is ignored, as `print`'s are. */
void lugha_rt_flush(void) {
    fflush(stdout);
}
