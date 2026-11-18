/*
 * rc.c — part of lugha_rt.c: reference counting, how every program's
 * memory is managed. Every object `lugha_rt_alloc` makes has a 16-byte
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
 * and are still live; `LUGHA_RC_REPORT=2` also lists each live object (its
 * size, count and first bytes), keeping freed ones as tombstones to tell
 * them apart.
 *
 * A fragment, not compiled on its own: joined after the core (lugha_rt.c)
 * and rc_alloc.c.
 */

#define RC_IMMORTAL INT64_MIN
#define RC_FREED (INT64_MIN + 0x5EED)

static int64_t rc_made;
static int64_t rc_freed;
/* With poisoning, freed objects stay as tombstones: never reused. */
static bool rc_poison;

/* With `LUGHA_RC_REPORT=2`, every object made and its size. */
static void **rc_objects;
static int64_t *rc_sizes;
static int64_t rc_listed, rc_room;
static bool rc_listing;

static void rc_list(void *object, int64_t size) {
    if (rc_listed == rc_room) {
        rc_room = rc_room == 0 ? 1024 : rc_room * 2;
        rc_objects = realloc(rc_objects, (size_t)rc_room * sizeof(void *));
        rc_sizes = realloc(rc_sizes, (size_t)rc_room * sizeof(int64_t));
        if (rc_objects == NULL || rc_sizes == NULL) {
            die("panic: out of memory\n");
        }
    }
    rc_objects[rc_listed] = object;
    rc_sizes[rc_listed++] = size;
}

static void rc_report(void) {
    fprintf(stderr, "rc: %" PRId64 " allocated, %" PRId64 " freed, %" PRId64 " live\n", rc_made,
            rc_freed, rc_made - rc_freed);
    for (int64_t i = 0; i < rc_listed; i++) {
        const int64_t *words = rc_objects[i];
        if (words[-1] == RC_FREED) {
            continue;
        }
        /* The first word (a string's or array's length), then the bytes
         * after it, printable ones as they are. */
        fprintf(stderr, "  live: %" PRId64 " bytes, count %" PRId64 ", word %" PRId64 " \"",
                rc_sizes[i], words[-1], rc_sizes[i] >= 8 ? words[0] : 0);
        const unsigned char *bytes = (const unsigned char *)rc_objects[i];
        for (int64_t b = 8; b < rc_sizes[i] && b < 40; b++) {
            fputc(bytes[b] >= 32 && bytes[b] < 127 ? bytes[b] : '.', stderr);
        }
        fputs("\"\n", stderr);
    }
}

static void lugha_rc_start(void) {
    rc_alloc_start();
    const char *poison = getenv("LUGHA_RC_POISON");
    rc_poison = poison != NULL && strcmp(poison, "1") == 0;
    const char *report = getenv("LUGHA_RC_REPORT");
    if (report != NULL && strcmp(report, "2") == 0) {
        rc_poison = true;
        rc_listing = true;
    }
    if (report != NULL && (strcmp(report, "1") == 0 || strcmp(report, "2") == 0)) {
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

/* Visits what a counted object holds, for one operation. */
typedef void (*RcVisit)(void *object, int64_t op);

/* The header before every counted object. */
typedef struct {
    RcVisit visit;
    int64_t count;
} RcHeader;

/* Bit 62 of a count: the object is shared between tasks, and its count
 * changes atomically. A plain count is from 1 to below this. */
#define RC_SHARED ((int64_t)1 << 62)

/* A counted object of `size` bytes, zeroed, held once, with `visit` for
 * what it holds (`NULL`: nothing counted). */
void *lugha_rt_rc_alloc(int64_t size, RcVisit visit) {
    RcHeader *block = rc_block_new(sizeof(RcHeader) + (size_t)(size > 0 ? size : 0));
    block->visit = visit;
    block->count = 1;
    rc_made++;
    if (rc_listing) {
        rc_list(block + 1, size);
    }
    return block + 1;
}

/* `lugha_rt_rc_alloc` with no visit until its maker sets one
 * (`lugha_rt_rc_set_drop`): the runtime's own objects. */
void *lugha_rt_alloc(int64_t size) {
    return lugha_rt_rc_alloc(size, NULL);
}

/* `object`'s header. */
static RcHeader *rc_header(void *object) {
    return (RcHeader *)object - 1;
}

/* What visits `object`'s contents. */
void lugha_rt_rc_set_drop(void *object, RcVisit visit) {
    rc_header(object)->visit = visit;
}

/* The visit `object` has, for a copy of it (array growth, a clone). */
RcVisit lugha_rt_rc_drop_of(const void *object) {
    return rc_header((void *)object)->visit;
}

/* Frees `object`, its count gone to zero: its visit first releases what it
 * holds. A constant is never freed. */
void lugha_rt_rc_free(void *object) {
    RcHeader *header = rc_header(object);
    if (header->count == RC_FREED) {
        die("panic: double free (reference counting)\n");
    }
    if (header->count == RC_IMMORTAL) {
        return;
    }
    // Marked first: a cycle back to this object can't free it twice.
    header->count = RC_FREED;
    if (header->visit != NULL) {
        header->visit(object, RC_RELEASE);
    }
    rc_freed++;
    if (!rc_poison) {
        rc_block_free(header);
    }
}

/* Objects marked shared whose contents are still to be marked: marking
 * walks a list, not the stack, so a long chain can't overflow it. */
static void **rc_marking;
static int64_t rc_marking_len, rc_marking_cap;
static bool rc_walking;

/* Marks `object` shared, and with it everything it reaches that is still
 * local, before it crosses to another task. Called inside a walk (from a
 * visit), it only queues the object. */
void lugha_rt_rc_mark(void *object) {
    if (object == NULL) {
        return;
    }
    RcHeader *header = rc_header(object);
    int64_t count = header->count;
    if (count == RC_FREED) {
        die("panic: use of freed memory (reference counting)\n");
    }
    // A constant, or shared already with everything it reaches.
    if (count < 0 || (count & RC_SHARED) != 0) {
        return;
    }
    header->count = count | RC_SHARED;
    if (rc_marking_len == rc_marking_cap) {
        rc_marking_cap = rc_marking_cap == 0 ? 64 : rc_marking_cap * 2;
        rc_marking = realloc(rc_marking, (size_t)rc_marking_cap * sizeof(void *));
        if (rc_marking == NULL) {
            die("panic: out of memory\n");
        }
    }
    rc_marking[rc_marking_len++] = object;
    if (rc_walking) {
        return;
    }
    rc_walking = true;
    while (rc_marking_len > 0) {
        void *next = rc_marking[--rc_marking_len];
        RcVisit visit = rc_header(next)->visit;
        if (visit != NULL) {
            visit(next, RC_MARK);
        }
    }
    rc_walking = false;
}

/* Releases or marks each element of an array of counted pointers
 * (`string[]`, `u8[][]`, `T[][]`): the visit of every such array. */
void lugha_rt_rc_drop_pointers(void *object, int64_t op) {
    LughaArray *array = object;
    void **items = (void **)array->elems;
    for (int64_t i = 0; i < array->len; i++) {
        if (op == RC_MARK) {
            lugha_rt_rc_mark(items[i]);
        } else {
            lugha_rt_rc_release(items[i]);
        }
    }
}

/* What generated code calls for a count it doesn't handle inline: a
 * shared count changes atomically (and frees at zero), a constant is left
 * alone, a freed object panics. `delta` is +1 or -1. */
void lugha_rt_rc_slow(void *object, int64_t delta) {
    RcHeader *header = rc_header(object);
    int64_t count = __atomic_load_n(&header->count, __ATOMIC_RELAXED);
    if (count == RC_FREED || count == 0) {
        die("panic: use of freed memory (reference counting)\n");
    }
    if (count < 0) {
        return;
    }
    if ((count & RC_SHARED) == 0) {
        // A plain count the caller didn't handle inline: the runtime's own.
        if (delta > 0) {
            header->count = count + 1;
        } else if (count > 1) {
            header->count = count - 1;
        } else {
            lugha_rt_rc_free(object);
        }
        return;
    }
    if (delta > 0) {
        __atomic_fetch_add(&header->count, 1, __ATOMIC_RELAXED);
        return;
    }
    int64_t before = __atomic_fetch_sub(&header->count, 1, __ATOMIC_RELEASE);
    if ((before & ~RC_SHARED) == 1) {
        __atomic_thread_fence(__ATOMIC_ACQUIRE);
        lugha_rt_rc_free(object);
    }
}

/* Takes one holder from `object` (none for `NULL`), freeing it at zero:
 * the runtime's own release; generated code inlines the same. */
void lugha_rt_rc_release(void *object) {
    if (object != NULL) {
        lugha_rt_rc_slow(object, -1);
    }
}

/* Adds one holder to `object` (none for `NULL`). */
void lugha_rt_rc_retain(void *object) {
    if (object != NULL) {
        lugha_rt_rc_slow(object, 1);
    }
}

