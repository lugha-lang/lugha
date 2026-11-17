/*
 * rc_alloc.c — part of lugha_rt.c: the blocks counted objects live in.
 * Most objects are small and short-lived, so a
 * freed block goes back on a list of blocks its size, and the next object
 * that size takes it, without a trip through malloc.
 *
 * - Small blocks (up to `RC_SMALL` bytes, header included) come in 16-byte
 *   classes. Each class carves 64 KB chunks out of one region of address
 *   space reserved at start-up; a chunk's first word is its class, so
 *   freeing a block needs nothing in the block itself.
 * - A block inside the region is small; any other block came from calloc
 *   (a large object, or a small one once the region is used up) and goes
 *   back to free.
 * - Memory taken for small blocks is never returned to the system.
 * - One OS thread allocates (tasks are green threads on it), so the lists
 *   are plain globals.
 *
 * A fragment, not compiled on its own: joined after the core (lugha_rt.c),
 * before rc.c.
 */

#define RC_CHUNK ((uintptr_t)1 << 16)
#define RC_SMALL 272
#define RC_CLASSES (RC_SMALL / 16 + 1)

/* The reserved region: committed by the kernel page by page, as touched. */
static uintptr_t rc_region, rc_region_next, rc_region_end;

/* Freed blocks of each class, linked through their first word. */
static void *rc_lists[RC_CLASSES];

/* The unused end of each class's newest chunk. */
static uintptr_t rc_carve[RC_CLASSES], rc_carve_end[RC_CLASSES];

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

/* A zeroed block of `size` bytes. */
static void *rc_block_new(size_t size) {
    if (size <= RC_SMALL && size > 0) {
        size_t class = (size + 15) / 16;
        void *block = rc_lists[class];
        if (block != NULL) {
            rc_lists[class] = *(void **)block;
            rc_zero(block, class);
            return block;
        }
        size_t room = class * 16;
        if (rc_carve_end[class] - rc_carve[class] < room && rc_region_next < rc_region_end) {
            uintptr_t chunk = rc_region_next;
            rc_region_next += RC_CHUNK;
            *(size_t *)chunk = class;
            // The class word takes the chunk's first 16 bytes, keeping
            // every block 16-byte aligned.
            rc_carve[class] = chunk + 16;
            rc_carve_end[class] = chunk + RC_CHUNK;
        }
        if (rc_carve_end[class] - rc_carve[class] >= room) {
            // Fresh pages from the kernel are already zero.
            void *fresh = (void *)rc_carve[class];
            rc_carve[class] += room;
            return fresh;
        }
    }
    void *block = calloc(1, size > 0 ? size : 1);
    if (block == NULL) {
        die("panic: out of memory\n");
    }
    return block;
}

/* Gives `block` back: to its class's list, or to free. */
static void rc_block_free(void *block) {
    uintptr_t at = (uintptr_t)block;
    if (at >= rc_region && at < rc_region_end) {
        size_t class = *(size_t *)(at & ~(RC_CHUNK - 1));
        *(void **)block = rc_lists[class];
        rc_lists[class] = block;
        return;
    }
    free(block);
}

