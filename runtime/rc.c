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
 *   - `RC_IMMORTAL` (negative): a literal, or a constant once filled
 *     (`lugha_rt_rc_freeze_start`), never counted or freed;
 *   - `RC_FREED` (negative; kept after freeing with `LUGHA_RC_POISON=1`): freed,
 *     so touching it again panics instead of corrupting memory.
 *
 * `LUGHA_RC_REPORT=1` prints, at exit, how many objects were made, freed
 * and are still live; `LUGHA_RC_REPORT=2` also lists each live object (its
 * size, count and first bytes), keeping freed ones as tombstones to tell
 * them apart.
 *
 * A fragment, not compiled on its own: joined after the core (lugha_rt.c),
 * rc_alloc.c and rc_check.c.
 */

#define RC_IMMORTAL INT64_MIN
#define RC_FREED (INT64_MIN + 0x5EED)
/* An object a cycle collection found dead, between finding it and freeing
 * it: the releases among the dead do nothing (cycles.c). */
#define RC_COLLECTING (INT64_MIN + 0x5EEF)

/* With poisoning, freed objects stay as tombstones: never reused. */
static bool rc_poison;

/* With `LUGHA_RC_REPORT=2`, every object made and its size. */
static void **rc_objects;
static int64_t *rc_sizes;
static int64_t rc_listed, rc_room;
static bool rc_listing;
/* Whether cells record where they were made: with `LUGHA_RC_REPORT=2`, or
 * in a test, for `lughac test`'s note (cycle_sites.c). */
static bool cc_sites_on;

/* Whether `LUGHA_RC_REPORT` asked for the count at exit: only then does
 * the end of the entry let go of what the other threads hold. */
static bool rc_reporting;
static pthread_mutex_t rc_list_lock = PTHREAD_MUTEX_INITIALIZER;

static void rc_list(void *object, int64_t size) {
    pthread_mutex_lock(&rc_list_lock);
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
    pthread_mutex_unlock(&rc_list_lock);
}

/* Objects made and freed so far, by every worker. */
/* The counts of threads C made that have exited (callbacks.c), whose
 * records went with them. */
static int64_t rc_retired_made, rc_retired_freed;

static void rc_totals(int64_t *made, int64_t *freed) {
    pthread_mutex_lock(&workers_lock);
    *made = rc_retired_made;
    *freed = rc_retired_freed;
    for (Worker *w = workers; w != NULL; w = w->next_worker) {
        *made += __atomic_load_n(&w->made, __ATOMIC_RELAXED);
        *freed += __atomic_load_n(&w->freed, __ATOMIC_RELAXED);
    }
    pthread_mutex_unlock(&workers_lock);
}

/* The collector's line of the report and a test's note (cycle_sites.c,
 * joined after this fragment). */
static void cc_report(void);
static void cc_test_note(void);

static void rc_report(void) {
    int64_t made, freed;
    rc_totals(&made, &freed);
    fprintf(stderr, "rc: %" PRId64 " allocated, %" PRId64 " freed, %" PRId64 " live", made, freed,
            made - freed);
    cc_report();
    for (int64_t i = 0; i < rc_listed; i++) {
        const int64_t *words = rc_objects[i];
        if (words[-1] == RC_FREED || words[-1] == RC_IMMORTAL) {
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
    rc_check_start();
    const char *poison = getenv("LUGHA_RC_POISON");
    rc_poison = poison != NULL && strcmp(poison, "1") == 0;
    const char *report = getenv("LUGHA_RC_REPORT");
    if (report != NULL && strcmp(report, "2") == 0) {
        rc_poison = true;
        rc_listing = true;
    }
    if (report != NULL && (strcmp(report, "1") == 0 || strcmp(report, "2") == 0)) {
        rc_reporting = true;
        atexit(rc_report);
    }
    cc_sites_on = rc_listing;
    if (getenv("LUGHA_TEST") != NULL) {
        cc_sites_on = true;
        atexit(cc_test_note);
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

/* A cell: a `Ref` box whose contents can reach another `Ref`, so that it
 * may be part of a cycle (cycles.c). Its count is
 *
 *   bits 63 and 62 set, 61 clear  (`RC_CELL`: negative, so generated code's
 *                                  inline count sends it here);
 *   bit 60                        on its task's list of suspects;
 *   bits 32–59                    its place in that list;
 *   bits 0–31                     the number of holders.
 *
 * A cell never crosses to another task, so its count is never atomic. */
#define RC_CELL ((int64_t)(UINT64_C(0xC) << 60))
#define RC_CELL_LISTED ((int64_t)1 << 60)
#define RC_CELL_MOST ((int64_t)1 << 28)
#define RC_CELL_PLACES ((RC_CELL_MOST - 1) << 32)
#define RC_CELL_HOLDERS ((int64_t)0xFFFFFFFF)

static bool rc_cell(int64_t count) {
    return (count & RC_CHECK_TAG) == RC_CELL;
}

static int64_t rc_cell_holders(int64_t count) {
    return count & RC_CELL_HOLDERS;
}

static bool rc_cell_is_listed(int64_t count) {
    return (count & RC_CELL_LISTED) != 0;
}

static int64_t rc_cell_place(int64_t count) {
    return (count & RC_CELL_PLACES) >> 32;
}

static int64_t rc_cell_listed(int64_t count, int64_t place) {
    return (count & ~RC_CELL_PLACES) | RC_CELL_LISTED | (place << 32);
}

static int64_t rc_cell_unlisted(int64_t count) {
    return count & ~(RC_CELL_PLACES | RC_CELL_LISTED);
}

/* The rest of the collector (cycles.c). */
struct CcGraph;
static void cc_suspect(void *object, RcHeader *header, int64_t count);
static void cc_unlist(void *object, int64_t place);
static void cc_child(struct CcGraph *g, void *object);
static void cc_site(void *object, const char *file, int64_t line, int64_t col);
static void cc_site_forget(void *object);

/* A counted object of `size` bytes, zeroed, held once, with `visit` for
 * what it holds (`NULL`: nothing counted). */
void *lugha_rt_rc_alloc(int64_t size, RcVisit visit) {
    RcHeader *block = rc_block_new(sizeof(RcHeader) + (size_t)(size > 0 ? size : 0));
    block->visit = visit;
    block->count = rc_first_count();
    worker->made++;
    if (rc_listing) {
        rc_list(block + 1, size);
    }
    return block + 1;
}

/* `Ref.new` of a box that may be part of a cycle: a cell, made by the
 * running task at `file:line:col`, which `LUGHA_RC_REPORT=2` names. */
void *lugha_rt_rc_cell_alloc(int64_t size, RcVisit visit, const char *file, int64_t line,
                             int64_t col) {
    void *object = lugha_rt_rc_alloc(size, visit);
    ((RcHeader *)object - 1)->count = RC_CELL | 1;
    if (cc_sites_on) {
        cc_site(object, file, line, col);
    }
    return object;
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

/* Visits `object`, already marked freed, so it releases what it holds,
 * then gives its block back. */
static void rc_free_one(void *object) {
    RcHeader *header = rc_header(object);
    if (header->visit != NULL) {
        header->visit(object, RC_RELEASE);
    }
    worker->freed++;
    if (!rc_poison) {
        rc_block_free(header);
    }
}

/* Frees `object`, its count gone to zero: its visit first releases what it
 * holds. A constant is never freed. Called from a visit, while this worker
 * is already freeing, it only queues the object: the outermost call frees
 * the queue, one object at a time, so a long chain (a list a million nodes
 * deep) is freed in a loop rather than a million nested calls. */
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
    if (worker->releasing) {
        if (worker->freeing_len == worker->freeing_cap) {
            worker->freeing_cap = worker->freeing_cap == 0 ? 64 : worker->freeing_cap * 2;
            worker->freeing = realloc(worker->freeing, (size_t)worker->freeing_cap * sizeof(void *));
            if (worker->freeing == NULL) {
                die("panic: out of memory\n");
            }
        }
        worker->freeing[worker->freeing_len++] = object;
        return;
    }
    // A visit never runs Lugha code, so nothing unwinds out of this loop
    // and leaves `releasing` set.
    worker->releasing = true;
    rc_free_one(object);
    while (worker->freeing_len > 0) {
        rc_free_one(worker->freeing[--worker->freeing_len]);
    }
    worker->releasing = false;
}


/* Marks `object` shared, and with it everything it reaches that is still
 * local, before it crosses to another task. Called inside a walk (from a
 * visit), it only queues the object. */
void lugha_rt_rc_mark(void *object) {
    // A cycle collection's walk: a child, for the collector.
    if (worker->cc_walk != NULL) {
        cc_child(worker->cc_walk, object);
        return;
    }
    if (object == NULL) {
        return;
    }
    RcHeader *header = rc_header(object);
    int64_t count = header->count;
    if (count == RC_FREED) {
        die("panic: use of freed memory (reference counting)\n");
    }
    if (rc_checked(count)) {
        // Marked by the task that made it, or it reached another unmarked.
        rc_check_owner(count);
        count = rc_checked_holders(count);
    }
    // A constant, or shared already with everything it reaches.
    if (count < 0 || (!worker->freezing && (count & RC_SHARED) != 0)) {
        return;
    }
    if (worker->freezing) {
        // Never freed from now on: the report counts it as let go of.
        header->count = RC_IMMORTAL;
        worker->freed++;
    } else {
        header->count = count | RC_SHARED;
    }
    if (worker->marking_len == worker->marking_cap) {
        worker->marking_cap = worker->marking_cap == 0 ? 64 : worker->marking_cap * 2;
        worker->marking = realloc(worker->marking, (size_t)worker->marking_cap * sizeof(void *));
        if (worker->marking == NULL) {
            die("panic: out of memory\n");
        }
    }
    worker->marking[worker->marking_len++] = object;
    if (worker->walking) {
        return;
    }
    worker->walking = true;
    while (worker->marking_len > 0) {
        void *next = worker->marking[--worker->marking_len];
        RcVisit visit = rc_header(next)->visit;
        if (visit != NULL) {
            visit(next, RC_MARK);
        }
    }
    worker->walking = false;
}

/* Constants, once filled: everything `lugha_rt_rc_mark` reaches until
 * `lugha_rt_rc_freeze_end` becomes immortal, its count never written
 * again, so tasks reading a constant never contend for it. */
void lugha_rt_rc_freeze_start(void) {
    worker->freezing = true;
}

void lugha_rt_rc_freeze_end(void) {
    worker->freezing = false;
}

/* Marks what `object` holds shared, `object` itself already marked: a
 * shared block whose owner stored new values in it (an actor's state). */
static void rc_mark_contents(void *object) {
    if (object == NULL) {
        return;
    }
    RcVisit visit = rc_header(object)->visit;
    if (visit != NULL) {
        visit(object, RC_MARK);
    }
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

/* A cell's count changed by `delta`: freed at no holders; a drop that
 * leaves some makes it a suspect, once (cycles.c). */
static void rc_cell_change(void *object, RcHeader *header, int64_t count, int64_t delta) {
    int64_t holders = rc_cell_holders(count) + delta;
    if (holders <= 0) {
        if (rc_cell_is_listed(count)) {
            cc_unlist(object, rc_cell_place(count));
        }
        if (cc_sites_on) {
            cc_site_forget(object);
        }
        lugha_rt_rc_free(object);
        return;
    }
    int64_t next = (count & ~RC_CELL_HOLDERS) | holders;
    if (delta < 0 && !rc_cell_is_listed(next)) {
        cc_suspect(object, header, next);
        return;
    }
    header->count = next;
}

/* A count generated code doesn't handle inline, or the runtime's own: a
 * shared count changes atomically (and frees at zero), a constant is left
 * alone, a freed object panics. `delta` is +1 or -1; `check`: whether the
 * owner check applies (generated code's counts). */
static void rc_change(void *object, int64_t delta, bool check) {
    RcHeader *header = rc_header(object);
    int64_t count = __atomic_load_n(&header->count, __ATOMIC_RELAXED);
    if (count == RC_FREED || count == 0) {
        die("panic: use of freed memory (reference counting)\n");
    }
    if (rc_cell(count)) {
        rc_cell_change(object, header, count, delta);
        return;
    }
    if (rc_checked(count)) {
        count = rc_checked_change(count, delta, check);
        __atomic_store_n(&header->count, count, __ATOMIC_RELAXED);
        if (rc_checked_holders(count) == 0) {
            lugha_rt_rc_free(object);
        }
        return;
    }
    if (count < 0) {
        return;
    }
    if ((count & RC_SHARED) == 0) {
        // A plain count the caller didn't handle inline: the runtime's own.
        if (delta > 0) {
            __atomic_store_n(&header->count, count + 1, __ATOMIC_RELAXED);
        } else if (count > 1) {
            __atomic_store_n(&header->count, count - 1, __ATOMIC_RELAXED);
        } else {
            lugha_rt_rc_free(object);
        }
        return;
    }
    if (delta > 0) {
        __atomic_fetch_add(&header->count, 1, __ATOMIC_RELAXED);
        return;
    }
    // Acquire as well as release, rather than a fence before the free:
    // the same instruction on x86, and ThreadSanitizer understands it.
    int64_t before = __atomic_fetch_sub(&header->count, 1, __ATOMIC_ACQ_REL);
    if ((before & ~RC_SHARED) == 1) {
        lugha_rt_rc_free(object);
    }
}

/* What generated code calls for a count it doesn't handle inline. */
void lugha_rt_rc_slow(void *object, int64_t delta) {
    rc_change(object, delta, true);
}

/* Takes one holder from `object` (none for `NULL`), freeing it at zero:
 * the runtime's own release; generated code inlines the same. */
void lugha_rt_rc_release(void *object) {
    if (object != NULL) {
        rc_change(object, -1, false);
    }
}

/* Adds one holder to `object` (none for `NULL`). */
void lugha_rt_rc_retain(void *object) {
    if (object != NULL) {
        rc_change(object, 1, false);
    }
}

