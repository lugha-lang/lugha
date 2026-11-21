/* Weak references (`Weak<T>`): a `Weak` points to its box's weak cell, a
 * counted object of one word holding the box's address, which counts as
 * no holder of the box. In a program that uses `Weak`, every `Ref` box has
 * one more word after its contents, at `slot` bytes: its cell, or `NULL`.
 * The box holds its cell once and each `Weak` holds it once; freeing the
 * box (by its count or by the cycle collector) empties the cell, so a
 * `Weak` never reaches a freed box. */

/* The word at `slot` bytes into `box`: its weak cell, if it has one. */
static void **weak_slot(void *box, int64_t slot) {
    return (void **)((char *)box + slot);
}

/* `Weak.new(box)`: the box's cell, made on first use, held once more for
 * the new `Weak`. */
void *lugha_rt_weak_new(void *box, int64_t slot) {
    void **at = weak_slot(box, slot);
    if (*at == NULL) {
        // The cell holds nothing counted, so it needs no visit.
        void *cell = lugha_rt_rc_alloc(sizeof(void *), NULL);
        *(void **)cell = box;
        *at = cell;
    }
    lugha_rt_rc_retain(*at);
    return *at;
}

/* `w.get()`: the box, held once more for the caller, or `NULL` once it
 * has been freed. */
void *lugha_rt_weak_get(void *cell) {
    void *box = *(void **)cell;
    lugha_rt_rc_retain(box);
    return box;
}

/* The box's own part of its visit, after its contents': on release it
 * empties its cell and lets go of it; on a walk the cell is its child. */
void lugha_rt_weak_visit(void *box, int64_t slot, int64_t op) {
    void *cell = *weak_slot(box, slot);
    if (cell == NULL) {
        return;
    }
    if (op == RC_MARK) {
        lugha_rt_rc_mark(cell);
        return;
    }
    *(void **)cell = NULL;
    *weak_slot(box, slot) = NULL;
    lugha_rt_rc_release(cell);
}
