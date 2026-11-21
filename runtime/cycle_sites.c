/* Where cells were made, for the cycle collector's reports: kept with
 * `LUGHA_RC_REPORT=2` and in a test (`LUGHA_TEST`), never otherwise. Each
 * live cell's address maps to the place it was made; a cell freed by its
 * count leaves the table, so a reused address never finds an old entry,
 * and one freed by a collection counts at its place. */

typedef struct {
    const char *file;
    int64_t line, col, freed;
} CcSite;

static CcSite *cc_sites;
static int64_t cc_sites_len, cc_sites_cap;
/* Open addressing over addresses: `NULL` empty, `CC_GONE` a removed entry. */
#define CC_GONE ((void *)1)
static void **cc_site_keys;
static int64_t *cc_site_vals;
static int64_t cc_site_used, cc_site_cap;
static pthread_mutex_t cc_site_lock = PTHREAD_MUTEX_INITIALIZER;

/* Where `object`'s probe starts. */
static int64_t cc_site_hash(void *object) {
    return (int64_t)((((uintptr_t)object >> 4) * 0x9E3779B97F4A7C15u) >> 1);
}

/* The slot holding `object`, or -1. */
static int64_t cc_site_find(void *object) {
    if (cc_site_cap == 0) {
        return -1;
    }
    for (int64_t i = cc_site_hash(object) % cc_site_cap;; i = (i + 1) % cc_site_cap) {
        if (cc_site_keys[i] == NULL) {
            return -1;
        }
        if (cc_site_keys[i] == object) {
            return i;
        }
    }
}

/* Puts `object` → `site` in the table, which has room. */
static void cc_site_put(void *object, int64_t site) {
    int64_t i = cc_site_hash(object) % cc_site_cap;
    while (cc_site_keys[i] != NULL && cc_site_keys[i] != CC_GONE) {
        i = (i + 1) % cc_site_cap;
    }
    cc_site_keys[i] = object;
    cc_site_vals[i] = site;
}

/* Makes room for one more entry, dropping removed ones. */
static void cc_site_room(void) {
    if ((cc_site_used + 1) * 2 <= cc_site_cap) {
        return;
    }
    void **keys = cc_site_keys;
    int64_t *vals = cc_site_vals;
    int64_t cap = cc_site_cap;
    cc_site_cap = cap == 0 ? 256 : cap * 2;
    cc_site_keys = calloc((size_t)cc_site_cap, sizeof(void *));
    cc_site_vals = calloc((size_t)cc_site_cap, sizeof(int64_t));
    if (cc_site_keys == NULL || cc_site_vals == NULL) {
        die("panic: out of memory\n");
    }
    cc_site_used = 0;
    for (int64_t i = 0; i < cap; i++) {
        if (keys[i] != NULL && keys[i] != CC_GONE) {
            cc_site_put(keys[i], vals[i]);
            cc_site_used++;
        }
    }
    free(keys);
    free(vals);
}

/* Records that cell `object` was made at `file:line:col`. */
static void cc_site(void *object, const char *file, int64_t line, int64_t col) {
    pthread_mutex_lock(&cc_site_lock);
    int64_t site = -1;
    for (int64_t i = 0; i < cc_sites_len; i++) {
        if (cc_sites[i].file == file && cc_sites[i].line == line && cc_sites[i].col == col) {
            site = i;
            break;
        }
    }
    if (site < 0) {
        if (cc_sites_len == cc_sites_cap) {
            cc_sites_cap = cc_sites_cap == 0 ? 16 : cc_sites_cap * 2;
            cc_sites = cc_grow(cc_sites, cc_sites_cap, sizeof(CcSite));
        }
        site = cc_sites_len++;
        cc_sites[site] = (CcSite){.file = file, .line = line, .col = col, .freed = 0};
    }
    cc_site_room();
    cc_site_put(object, site);
    cc_site_used++;
    pthread_mutex_unlock(&cc_site_lock);
}

/* Takes cell `object` out of the table: its place, or -1. */
static int64_t cc_site_take(void *object) {
    int64_t at = cc_site_find(object);
    if (at < 0) {
        return -1;
    }
    cc_site_keys[at] = CC_GONE;
    return cc_site_vals[at];
}

/* Cell `object` was freed by its count: it no longer counts anywhere. */
static void cc_site_forget(void *object) {
    pthread_mutex_lock(&cc_site_lock);
    cc_site_take(object);
    pthread_mutex_unlock(&cc_site_lock);
}

/* A collection freed cell `object`: counted at the place it was made. */
static void cc_site_freed(void *object) {
    pthread_mutex_lock(&cc_site_lock);
    int64_t site = cc_site_take(object);
    if (site >= 0) {
        cc_sites[site].freed++;
    }
    pthread_mutex_unlock(&cc_site_lock);
}

/* The end of `LUGHA_RC_REPORT`'s line: the cycles freed, if any; with
 * `LUGHA_RC_REPORT=2`, the places their cells were made. */
static void cc_report(void) {
    int64_t cycles = __atomic_load_n(&cc_cycles_freed, __ATOMIC_RELAXED);
    int64_t cells = __atomic_load_n(&cc_cells_freed, __ATOMIC_RELAXED);
    if (cycles > 0) {
        fprintf(stderr, "; %" PRId64 " %s freed (%" PRId64 " %s)", cycles,
                cycles == 1 ? "cycle" : "cycles", cells, cells == 1 ? "box" : "boxes");
    }
    fputc('\n', stderr);
    if (!rc_listing) {
        return;
    }
    for (int64_t i = 0; i < cc_sites_len; i++) {
        if (cc_sites[i].freed > 0) {
            fprintf(stderr, "  cycle boxes made at %s:%" PRId64 ":%" PRId64 ": %" PRId64 " freed\n",
                    cc_sites[i].file, cc_sites[i].line, cc_sites[i].col, cc_sites[i].freed);
        }
    }
}

/* A test's line for `lughac test`, at exit, when its run freed cycles:
 * `lugha-cycles: <cycles> <boxes>`, then each place, tab-separated. */
static void cc_test_note(void) {
    int64_t cycles = __atomic_load_n(&cc_cycles_freed, __ATOMIC_RELAXED);
    if (cycles == 0) {
        return;
    }
    int64_t cells = __atomic_load_n(&cc_cells_freed, __ATOMIC_RELAXED);
    fflush(stdout);
    fprintf(stderr, "lugha-cycles: %" PRId64 " %" PRId64, cycles, cells);
    for (int64_t i = 0; i < cc_sites_len; i++) {
        if (cc_sites[i].freed > 0) {
            fprintf(stderr, "\t%s:%" PRId64 ":%" PRId64, cc_sites[i].file, cc_sites[i].line,
                    cc_sites[i].col);
        }
    }
    fputc('\n', stderr);
    fflush(stderr);
}
