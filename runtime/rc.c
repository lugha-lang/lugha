/*
 * rc.c — part of lugha_rt.c: the reference-counting prototype
 * (`--memory=rc`). Every object `lugha_rt_alloc` makes has an 8-byte count
 * just before it; the pointer programs hold still points at the object, so
 * no layout changes. Generated code adds and takes away from the count
 * inline, and calls in here only to free, or for an odd count:
 *
 *   - positive: the number of holders;
 *   - `RC_IMMORTAL` (negative): a constant, never counted or freed;
 *   - `RC_FREED` (negative, only with `LUGHA_RC_POISON=1`): freed already,
 *     so touching it again panics instead of corrupting memory.
 *
 * `LUGHA_RC_REPORT=1` prints, at exit, how many objects were made, freed
 * and are still live.
 *
 * A fragment, not compiled on its own: joined after the core (lugha_rt.c),
 * and compiled only with `-DLUGHA_RC`.
 */
#ifdef LUGHA_RC

#define RC_IMMORTAL INT64_MIN
#define RC_FREED (INT64_MIN + 0x5EED)

static int64_t rc_made;
static int64_t rc_freed;
/* With poisoning, freed objects stay as tombstones: never reused. */
static bool rc_poison;

static void rc_report(void) {
    fprintf(stderr, "rc: %" PRId64 " allocated, %" PRId64 " freed, %" PRId64 " live\n", rc_made,
            rc_freed, rc_made - rc_freed);
}

static void lugha_rc_start(void) {
    const char *poison = getenv("LUGHA_RC_POISON");
    rc_poison = poison != NULL && strcmp(poison, "1") == 0;
    const char *report = getenv("LUGHA_RC_REPORT");
    if (report != NULL && strcmp(report, "1") == 0) {
        atexit(rc_report);
    }
}

/* The runtime's own blocks (fibers, handles): zeroed, not counted. */
void *lugha_rc_internal(size_t size) {
    void *memory = calloc(1, size > 0 ? size : 1);
    if (memory == NULL) {
        die("panic: out of memory\n");
    }
    return memory;
}

/* A counted object of `size` bytes, zeroed, held once. */
void *lugha_rt_alloc(int64_t size) {
    int64_t *block = calloc(1, sizeof(int64_t) + (size_t)(size > 0 ? size : 0));
    if (block == NULL) {
        die("panic: out of memory\n");
    }
    block[0] = 1;
    rc_made++;
    return block + 1;
}

/* `object`'s count, read through its header. */
static int64_t *rc_header(void *object) {
    return (int64_t *)object - 1;
}

/* Frees `object`, its count gone to zero: `drop` (when given) first
 * releases what it holds. A constant is never freed. */
void lugha_rt_rc_free(void *object, void (*drop)(void *)) {
    int64_t *header = rc_header(object);
    if (*header == RC_FREED) {
        die("panic: double free (reference counting)\n");
    }
    if (*header == RC_IMMORTAL) {
        return;
    }
    if (drop != NULL) {
        drop(object);
    }
    rc_freed++;
    if (rc_poison) {
        *header = RC_FREED;
        return;
    }
    free(header);
}

/* What generated code calls for a count it doesn't handle inline (zero or
 * negative): a constant is left alone, a freed object panics. `delta` is
 * the change it wanted (+1 or -1). */
void lugha_rt_rc_slow(void *object, int64_t delta) {
    int64_t *header = rc_header(object);
    (void)delta;
    if (*header == RC_FREED || *header == 0) {
        die("panic: use of freed memory (reference counting)\n");
    }
}

#endif
