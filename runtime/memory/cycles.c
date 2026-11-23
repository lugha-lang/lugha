/*
 * cycles.c — part of lugha_rt.c: the cycle collector for `Ref` boxes.
 *
 * Only a `Ref` box can close a cycle, and only one whose contents can reach
 * another `Ref` (a function or interface value may hide one): codegen makes
 * those boxes with a cell count (rc.c), so every change to their count
 * reaches the runtime. A cell whose count drops but stays above zero may
 * have just become garbage, held only by a cycle: it joins its task's list
 * of suspects, once.
 *
 * When a task's list reaches its threshold, at the task's next scheduling
 * point, and when the task ends, the task looks at its suspects by trial
 * deletion. It copies the holder counts of everything they reach into a
 * table, subtracts every reference found between those objects, and what
 * keeps a holder is held from outside: it, and all it reaches, is alive.
 * The rest is held only from inside, a dead cycle, and is freed. Real
 * counts aren't written until then.
 *
 * A `Ref` never crosses to another task, and nothing shared reaches one, so
 * the walk stops at shared objects and constants: only the task's own
 * objects are looked at, and no lock is taken. Freeing runs no Lugha code,
 * so a program can't tell when, or whether, it ran.
 *
 * Children are found through each object's visit with `RC_MARK`, which
 * only reports them to `lugha_rt_rc_mark`: while a walk runs, that hands
 * them here instead (`worker->cc_walk`).
 *
 * A fragment, not compiled on its own: joined after spawn.c (fibers) and
 * rc.c (counts, `rc_free_one`).
 */

/* The first threshold, and its bounds as it adapts. */
enum { CC_FIRST = 256, CC_LEAST = 64, CC_MOST = 100000 };

/* Cycles and cells freed by every task, for `LUGHA_RC_REPORT`. */
static int64_t cc_cycles_freed;
static int64_t cc_cells_freed;

static void cc_site_freed(void *object);

/* What a walk does with each child it is handed. */
enum { CC_GATHER, CC_SUBTRACT, CC_ALIVE, CC_GROUP };

/* The objects a collection looks at, with a table from each to its place. */
typedef struct CcGraph {
    void **objects;
    int64_t *trial;      /* holders not yet explained by an inside reference */
    int64_t *group;      /* union-find parent, for counting the cycles */
    bool *alive;
    bool *cell;
    int64_t len, cap;
    int64_t *slots;      /* open addressing: index + 1, 0 for empty */
    int64_t slot_cap;
    int mode;
    int64_t parent;      /* the object whose children the walk visits */
    int64_t *stack;      /* objects whose children are still to visit */
    int64_t stack_len, stack_cap;
} CcGraph;

/* The task whose suspects a cell joins: the running one. */
static Fiber *cc_owner(void) {
    return current != NULL ? current : &main_fiber;
}

static void *cc_grow(void *array, int64_t count, size_t size) {
    void *bigger = realloc(array, (size_t)count * size);
    if (bigger == NULL) {
        die("panic: out of memory\n");
    }
    return bigger;
}

/* Puts cell `object` on the running task's list of suspects; its place in
 * the list goes into its count (rc.c). */
static void cc_suspect(void *object, RcHeader *header, int64_t count) {
    Fiber *f = cc_owner();
    if (f->cc_len >= RC_CELL_MOST) {
        // Far past any threshold: left off, its count stays as it is.
        header->count = count;
        return;
    }
    if (f->cc_len == f->cc_cap) {
        f->cc_cap = f->cc_cap == 0 ? 64 : f->cc_cap * 2;
        f->cc_suspects = cc_grow(f->cc_suspects, f->cc_cap, sizeof(void *));
    }
    header->count = rc_cell_listed(count, f->cc_len);
    f->cc_suspects[f->cc_len++] = object;
}

/* Takes cell `object`, listed at `place`, off the running task's list: it
 * is being freed, or a collection looked at it. The last entry takes its
 * place. A cell listed by another task (only while the program ends) is
 * left where it is: nothing collects that list any more. */
static void cc_unlist(void *object, int64_t place) {
    Fiber *f = cc_owner();
    if (place >= f->cc_len || f->cc_suspects[place] != object) {
        return;
    }
    void *last = f->cc_suspects[--f->cc_len];
    if (last != object) {
        f->cc_suspects[place] = last;
        RcHeader *moved = rc_header(last);
        moved->count = rc_cell_listed(moved->count, place);
    }
}

/* Where `object` is in `g`, or -1. */
static int64_t cc_find(const CcGraph *g, void *object) {
    if (g->slot_cap == 0) {
        return -1;
    }
    uint64_t i = ((uintptr_t)object >> 4) * UINT64_C(0x9E3779B97F4A7C15);
    for (uint64_t n = 0;; n++) {
        int64_t slot = g->slots[(i + n) & (uint64_t)(g->slot_cap - 1)];
        if (slot == 0) {
            return -1;
        }
        if (g->objects[slot - 1] == object) {
            return slot - 1;
        }
    }
}

static void cc_slot(CcGraph *g, int64_t index) {
    uint64_t i = ((uintptr_t)g->objects[index] >> 4) * UINT64_C(0x9E3779B97F4A7C15);
    for (uint64_t n = 0;; n++) {
        int64_t *slot = &g->slots[(i + n) & (uint64_t)(g->slot_cap - 1)];
        if (*slot == 0) {
            *slot = index + 1;
            return;
        }
    }
}

/* The holders of `object` a collection may explain, or -1 for one it
 * leaves alone: freed already (a bug), a constant, shared with another
 * task, or being collected. */
static int64_t cc_holders(void *object) {
    int64_t count = rc_header(object)->count;
    if (count == RC_FREED) {
        die("panic: use of freed memory (reference counting)\n");
    }
    if (rc_cell(count)) {
        return rc_cell_holders(count);
    }
    if (rc_checked(count)) {
        return rc_checked_holders(count);
    }
    if (count < 0 || (count & RC_SHARED) != 0) {
        return -1;
    }
    return count;
}

/* Adds `object` to `g` if it is the task's own and not there yet; its
 * children are visited later. */
static void cc_add(CcGraph *g, void *object) {
    if (object == NULL || cc_find(g, object) >= 0) {
        return;
    }
    int64_t holders = cc_holders(object);
    if (holders < 0) {
        return;
    }
    if (g->len == g->cap) {
        g->cap = g->cap == 0 ? 64 : g->cap * 2;
        g->objects = cc_grow(g->objects, g->cap, sizeof(void *));
        g->trial = cc_grow(g->trial, g->cap, sizeof(int64_t));
        g->group = cc_grow(g->group, g->cap, sizeof(int64_t));
        g->alive = cc_grow(g->alive, g->cap, sizeof(bool));
        g->cell = cc_grow(g->cell, g->cap, sizeof(bool));
    }
    if ((g->len + 1) * 2 > g->slot_cap) {
        g->slot_cap = g->slot_cap == 0 ? 128 : g->slot_cap * 2;
        free(g->slots);
        g->slots = calloc((size_t)g->slot_cap, sizeof(int64_t));
        if (g->slots == NULL) {
            die("panic: out of memory\n");
        }
        for (int64_t i = 0; i < g->len; i++) {
            cc_slot(g, i);
        }
    }
    int64_t index = g->len++;
    g->objects[index] = object;
    g->trial[index] = holders;
    g->group[index] = index;
    g->alive[index] = false;
    g->cell[index] = rc_cell(rc_header(object)->count);
    cc_slot(g, index);
    if (g->stack_len == g->stack_cap) {
        g->stack_cap = g->stack_cap == 0 ? 64 : g->stack_cap * 2;
        g->stack = cc_grow(g->stack, g->stack_cap, sizeof(int64_t));
    }
    g->stack[g->stack_len++] = index;
}

static int64_t cc_root(CcGraph *g, int64_t i) {
    while (g->group[i] != i) {
        g->group[i] = g->group[g->group[i]];
        i = g->group[i];
    }
    return i;
}

/* A child the walk was handed (`lugha_rt_rc_mark`, while `cc_walk` is set). */
static void cc_child(CcGraph *g, void *object) {
    if (object == NULL) {
        return;
    }
    if (g->mode == CC_GATHER) {
        cc_add(g, object);
        return;
    }
    int64_t at = cc_find(g, object);
    if (at < 0) {
        return;
    }
    if (g->mode == CC_SUBTRACT) {
        g->trial[at]--;
    } else if (g->mode == CC_GROUP) {
        // Only dead objects joined to each other make one cycle: a live
        // local object between two of them doesn't.
        if (!g->alive[at]) {
            g->group[cc_root(g, at)] = cc_root(g, g->parent);
        }
    } else if (!g->alive[at]) {
        g->alive[at] = true;
        g->stack[g->stack_len++] = at;
    }
}

/* Visits the children of `g`'s object `index` in the walk's mode. */
static void cc_visit(CcGraph *g, int64_t index) {
    RcVisit visit = rc_header(g->objects[index])->visit;
    if (visit == NULL) {
        return;
    }
    g->parent = index;
    worker->cc_walk = g;
    visit(g->objects[index], RC_MARK);
    worker->cc_walk = NULL;
}

/* Frees `g`'s objects that nothing outside holds; how many cells were among
 * them and in how many separate cycles, through `cells` and `cycles`. */
static void cc_free_dead(CcGraph *g, int64_t *cells, int64_t *cycles) {
    int64_t dead = 0;
    for (int64_t i = 0; i < g->len; i++) {
        if (!g->alive[i]) {
            g->objects[dead] = g->objects[i];
            g->cell[dead] = g->cell[i];
            g->group[dead] = cc_root(g, i);
            dead++;
        }
    }
    // Counted before anything changes: a cycle is a group of the dead,
    // named by its root's place.
    bool *seen = calloc((size_t)(g->len > 0 ? g->len : 1), sizeof(bool));
    if (seen == NULL) {
        die("panic: out of memory\n");
    }
    for (int64_t i = 0; i < dead; i++) {
        if (!seen[g->group[i]]) {
            seen[g->group[i]] = true;
            *cycles += 1;
        }
        *cells += g->cell[i] ? 1 : 0;
    }
    free(seen);
    // Every dead object is marked first, so the releases between them do
    // nothing: each is visited exactly once, below, and freed once.
    for (int64_t i = 0; i < dead; i++) {
        RcHeader *header = rc_header(g->objects[i]);
        if (g->cell[i] && rc_cell_is_listed(header->count)) {
            cc_unlist(g->objects[i], rc_cell_place(header->count));
        }
        if (g->cell[i] && cc_sites_on) {
            cc_site_freed(g->objects[i]);
        }
        header->count = RC_COLLECTING;
    }
    bool was_releasing = worker->releasing;
    worker->releasing = true;
    for (int64_t i = 0; i < dead; i++) {
        RcVisit visit = rc_header(g->objects[i])->visit;
        if (visit != NULL) {
            visit(g->objects[i], RC_RELEASE);
        }
    }
    // What they held that has no other holder now; none of it is dead.
    while (worker->freeing_len > 0) {
        rc_free_one(worker->freeing[--worker->freeing_len]);
    }
    worker->releasing = was_releasing;
    for (int64_t i = 0; i < dead; i++) {
        RcHeader *header = rc_header(g->objects[i]);
        header->count = RC_FREED;
        worker->freed++;
        if (!rc_poison) {
            rc_block_free(header);
        }
    }
}

/* Looks at the running task's suspects now: frees what only cycles hold,
 * takes every suspect off the list, and sets the next threshold by how
 * much of what it looked at was dead. */
static void cc_collect(void) {
    Fiber *f = cc_owner();
    if (f->cc_len == 0 || worker->cc_walk != NULL || worker->releasing) {
        return;
    }
    CcGraph g = {0};
    for (int64_t i = 0; i < f->cc_len; i++) {
        cc_add(&g, f->cc_suspects[i]);
    }
    // Every suspect is off the list; a live one rejoins at its next drop.
    for (int64_t i = 0; i < f->cc_len; i++) {
        RcHeader *header = rc_header(f->cc_suspects[i]);
        if (rc_cell(header->count)) {
            header->count = rc_cell_unlisted(header->count);
        }
    }
    f->cc_len = 0;
    g.mode = CC_GATHER;
    while (g.stack_len > 0) {
        cc_visit(&g, g.stack[--g.stack_len]);
    }
    g.mode = CC_SUBTRACT;
    for (int64_t i = 0; i < g.len; i++) {
        cc_visit(&g, i);
    }
    g.mode = CC_ALIVE;
    // Each object is pushed at most once in this phase.
    if (g.stack_cap < g.len) {
        g.stack_cap = g.len;
        g.stack = cc_grow(g.stack, g.stack_cap, sizeof(int64_t));
    }
    for (int64_t i = 0; i < g.len; i++) {
        if (g.trial[i] > 0 && !g.alive[i]) {
            g.alive[i] = true;
            g.stack[g.stack_len++] = i;
            while (g.stack_len > 0) {
                cc_visit(&g, g.stack[--g.stack_len]);
            }
        }
    }
    g.mode = CC_GROUP;
    for (int64_t i = 0; i < g.len; i++) {
        if (!g.alive[i]) {
            cc_visit(&g, i);
        }
    }
    int64_t looked = g.len, cells = 0, cycles = 0;
    int64_t before = worker->freed;
    cc_free_dead(&g, &cells, &cycles);
    int64_t freed = worker->freed - before;
    __atomic_add_fetch(&cc_cycles_freed, cycles, __ATOMIC_RELAXED);
    __atomic_add_fetch(&cc_cells_freed, cells, __ATOMIC_RELAXED);
    // Adapts as Nim's ORC does: sooner while it finds garbage, later while
    // it finds little.
    int64_t threshold = f->cc_threshold == 0 ? CC_FIRST : f->cc_threshold;
    if (freed * 2 >= looked) {
        threshold = threshold * 2 / 3;
    } else {
        threshold = threshold * 3 / 2;
    }
    f->cc_threshold = threshold < CC_LEAST ? CC_LEAST : threshold > CC_MOST ? CC_MOST : threshold;
    free(g.objects);
    free(g.trial);
    free(g.group);
    free(g.alive);
    free(g.cell);
    free(g.slots);
    free(g.stack);
}

/* At a scheduling point: a collection, if the running task's list is long
 * enough. */
static void cc_maybe_collect(void) {
    Fiber *f = cc_owner();
    int64_t threshold = f->cc_threshold == 0 ? CC_FIRST : f->cc_threshold;
    if (f->cc_len >= threshold) {
        cc_collect();
    }
}

/* A task ends, its own values let go of: its last collection, and its list
 * given back. */
static void cc_task_done(Fiber *f) {
    if (f->cc_len > 0) {
        cc_collect();
    }
    free(f->cc_suspects);
    f->cc_suspects = NULL;
    f->cc_len = 0;
    f->cc_cap = 0;
}
