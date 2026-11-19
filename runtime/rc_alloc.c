/*
 * rc_alloc.c — part of lugha_rt.c: the blocks counted objects live in.
 * Most objects are small and short-lived, so a
 * freed block goes back on a list of blocks its size, and the next object
 * that size takes it, without a trip through malloc.
 *
 * - Small blocks (up to `RC_SMALL` bytes, header included) come in 16-byte
 *   classes. Each worker carves 64 KB chunks out of one region of address
 *   space reserved at start-up, taken with one atomic add; a chunk's first
 *   16 bytes hold its class and the worker that owns it, so freeing a
 *   block needs nothing in the block itself.
 * - Each worker keeps its own lists (the `Worker` record), so allocating
 *   and freeing its own blocks takes no lock and no atomic. A block of
 *   another worker's chunk is pushed onto that worker's remote list for
 *   its class with one compare-and-swap; the owner takes the whole list
 *   with one swap when its own runs dry, before carving. The owner is the
 *   chunk's, never the count's shared bit: a task that moved to another
 *   worker frees its unshared blocks there.
 * - A block inside the region is small; any other block came from calloc
 *   (a large object, or a small one once the region is used up) and goes
 *   back to free, which any thread may call.
 * - Memory taken for small blocks is never returned to the system.
 *
 * A fragment, not compiled on its own: joined after the core (lugha_rt.c),
 * before rc.c.
 */

/* The reserved region: committed by the kernel page by page, as touched.
 * `rc_region_next` moves by atomic adds; the bounds never change. */
static uintptr_t rc_region, rc_region_next, rc_region_end;

/* A chunk's first 16 bytes. */
typedef struct {
    size_t class;
    Worker *owner;
} RcChunk;

/* The chunk `block` (inside the region) was carved from. */
static RcChunk *rc_chunk_of(const void *block) {
    return (RcChunk *)((uintptr_t)block & ~(RC_CHUNK - 1));
}

/* Reserves the region: as much address space as the system grants, from
 * 64 GB down to 1 GB. Without one, every block comes from calloc. */
static void rc_alloc_start(void) {
    for (size_t size = (size_t)1 << 36; size >= ((size_t)1 << 30); size /= 4) {
        void *at = mmap(NULL, size + RC_CHUNK, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
        if (at != MAP_FAILED) {
            // Chunks are aligned to their size, so a block finds its chunk
            // by clearing its low bits.
            rc_region = ((uintptr_t)at + RC_CHUNK - 1) & ~(RC_CHUNK - 1);
            rc_region_next = rc_region;
            rc_region_end = rc_region + size;
            return;
        }
    }
}

/* Zeroes the first `class` 16-byte pieces of `block`: one fixed-size
 * store each, falling through from the class down, where a `memset` of a
 * size known only at run time would be a call per allocation. */
#define RC_ZERO16(i) memset((char *)block + 16 * (i), 0, 16)
static void rc_zero(void *block, size_t class) {
    switch (class) {
    case 17:
        RC_ZERO16(16);
        /* fall through */
    case 16:
        RC_ZERO16(15);
        /* fall through */
    case 15:
        RC_ZERO16(14);
        /* fall through */
    case 14:
        RC_ZERO16(13);
        /* fall through */
    case 13:
        RC_ZERO16(12);
        /* fall through */
    case 12:
        RC_ZERO16(11);
        /* fall through */
    case 11:
        RC_ZERO16(10);
        /* fall through */
    case 10:
        RC_ZERO16(9);
        /* fall through */
    case 9:
        RC_ZERO16(8);
        /* fall through */
    case 8:
        RC_ZERO16(7);
        /* fall through */
    case 7:
        RC_ZERO16(6);
        /* fall through */
    case 6:
        RC_ZERO16(5);
        /* fall through */
    case 5:
        RC_ZERO16(4);
        /* fall through */
    case 4:
        RC_ZERO16(3);
        /* fall through */
    case 3:
        RC_ZERO16(2);
        /* fall through */
    case 2:
        RC_ZERO16(1);
        /* fall through */
    default:
        RC_ZERO16(0);
    }
}
#undef RC_ZERO16

/* A new chunk for `w`'s class `class`, or 0 once the region is used up. */
static uintptr_t rc_chunk_take(Worker *w, size_t class) {
    if (__atomic_load_n(&rc_region_next, __ATOMIC_RELAXED) >= rc_region_end) {
        return 0;
    }
    uintptr_t chunk = __atomic_fetch_add(&rc_region_next, RC_CHUNK, __ATOMIC_RELAXED);
    if (chunk >= rc_region_end) {
        return 0;
    }
    RcChunk *head = (RcChunk *)chunk;
    head->class = class;
    head->owner = w;
    return chunk;
}

/* A zeroed block of `size` bytes. */
static void *rc_block_new(size_t size) {
    // A thread C made gets no chunks: its record ends with the thread,
    // and blocks freed elsewhere go back to their chunk's owner.
    if (size <= RC_SMALL && size > 0 && !worker->foreign) {
        Worker *w = worker;
        size_t class = (size + 15) / 16;
        void *block = w->lists[class];
        // Run dry: what other threads freed comes back first.
        if (block == NULL && __atomic_load_n(&w->remote[class], __ATOMIC_RELAXED) != NULL) {
            block = __atomic_exchange_n(&w->remote[class], NULL, __ATOMIC_ACQUIRE);
        }
        if (block != NULL) {
            w->lists[class] = *(void **)block;
            rc_zero(block, class);
            return block;
        }
        size_t room = class * 16;
        if (w->carve_end[class] - w->carve[class] < room) {
            uintptr_t chunk = rc_chunk_take(w, class);
            if (chunk != 0) {
                // The header takes the chunk's first 16 bytes, keeping
                // every block 16-byte aligned.
                w->carve[class] = chunk + sizeof(RcChunk);
                w->carve_end[class] = chunk + RC_CHUNK;
            }
        }
        if (w->carve_end[class] - w->carve[class] >= room) {
            // Fresh pages from the kernel are already zero.
            void *fresh = (void *)w->carve[class];
            w->carve[class] += room;
            return fresh;
        }
    }
    void *block = calloc(1, size > 0 ? size : 1);
    if (block == NULL) {
        die("panic: out of memory\n");
    }
    return block;
}

/* Gives `block` back: to its owner's list for its class, or to free. */
static void rc_block_free(void *block) {
    uintptr_t at = (uintptr_t)block;
    if (at >= rc_region && at < rc_region_end) {
        RcChunk *chunk = rc_chunk_of(block);
        Worker *owner = chunk->owner;
        size_t class = chunk->class;
        if (owner == worker) {
            *(void **)block = owner->lists[class];
            owner->lists[class] = block;
            return;
        }
        void *head = __atomic_load_n(&owner->remote[class], __ATOMIC_RELAXED);
        do {
            *(void **)block = head;
        } while (!__atomic_compare_exchange_n(&owner->remote[class], &head, block, true,
                                              __ATOMIC_RELEASE, __ATOMIC_RELAXED));
        return;
    }
    free(block);
}

