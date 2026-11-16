/*
 * rc.c — part of lugha_rt.c: the reference-counting prototype
 * (`--memory=rc`). Every object `lugha_rt_alloc` makes has a 16-byte
 * header just before it: the function that releases what the object holds
 * (`NULL` for one holding no counted values), then its count. The pointer
 * programs hold still points at the object, so no layout changes, and
 * releasing any object is the same whatever it holds. Generated code adds
 * and takes away from the count inline, and calls in here only to free,
 * or for an odd count:
 *
 *   - positive: the number of holders;
 *   - `RC_IMMORTAL` (negative): a constant, never counted or freed;
 *   - `RC_FREED` (negative; kept after freeing with `LUGHA_RC_POISON=1`): freed,
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

/* What releases a counted object's contents. */
typedef void (*RcDrop)(void *object);

/* The header before every counted object. */
typedef struct {
    RcDrop drop;
    int64_t count;
} RcHeader;

/* A counted object of `size` bytes, zeroed, held once, with no drop until
 * its maker sets one (`lugha_rt_rc_set_drop`). */
void *lugha_rt_alloc(int64_t size) {
    RcHeader *block = calloc(1, sizeof(RcHeader) + (size_t)(size > 0 ? size : 0));
    if (block == NULL) {
        die("panic: out of memory\n");
    }
    block->count = 1;
    rc_made++;
    return block + 1;
}

/* `object`'s header. */
static RcHeader *rc_header(void *object) {
    return (RcHeader *)object - 1;
}

/* What releases `object`'s contents when it is freed. */
void lugha_rt_rc_set_drop(void *object, RcDrop drop) {
    rc_header(object)->drop = drop;
}

/* The drop `object` has, for a copy of it (array growth, a clone). */
RcDrop lugha_rt_rc_drop_of(const void *object) {
    return rc_header((void *)object)->drop;
}

/* Frees `object`, its count gone to zero: its drop (or `drop`, when
 * given) first releases what it holds. A constant is never freed. */
void lugha_rt_rc_free(void *object, RcDrop drop) {
    RcHeader *header = rc_header(object);
    if (header->count == RC_FREED) {
        die("panic: double free (reference counting)\n");
    }
    if (header->count == RC_IMMORTAL) {
        return;
    }
    RcDrop contents = drop != NULL ? drop : header->drop;
    // Marked first: a cycle back to this object can't free it twice.
    header->count = RC_FREED;
    if (contents != NULL) {
        contents(object);
    }
    rc_freed++;
    if (!rc_poison) {
        free(header);
    }
}

/* Releases each element of an array of counted pointers (`string[]`,
 * `u8[][]`, `T[][]`): the drop of every array whose elements are
 * pointers to counted objects. */
void lugha_rt_rc_drop_pointers(void *object) {
    LughaArray *array = object;
    void **items = (void **)array->elems;
    for (int64_t i = 0; i < array->len; i++) {
        lugha_rt_rc_release(items[i]);
    }
}

/* What generated code calls for a count it doesn't handle inline (zero or
 * negative): a constant is left alone, a freed object panics. `delta` is
 * the change it wanted (+1 or -1). */
void lugha_rt_rc_slow(void *object, int64_t delta) {
    int64_t count = rc_header(object)->count;
    (void)delta;
    if (count == RC_FREED || count == 0) {
        die("panic: use of freed memory (reference counting)\n");
    }
}

/* Takes one holder from `object` (none for `NULL`), freeing it at zero:
 * the runtime's own release; generated code inlines the same. */
void lugha_rt_rc_release(void *object) {
    if (object == NULL) {
        return;
    }
    RcHeader *header = rc_header(object);
    if (header->count > 1) {
        header->count--;
    } else if (header->count == 1) {
        lugha_rt_rc_free(object, NULL);
    } else {
        lugha_rt_rc_slow(object, -1);
    }
}

/* Adds one holder to `object` (none for `NULL`). */
void lugha_rt_rc_retain(void *object) {
    if (object == NULL) {
        return;
    }
    RcHeader *header = rc_header(object);
    if (header->count > 0) {
        header->count++;
    } else {
        lugha_rt_rc_slow(object, 1);
    }
}

#endif
