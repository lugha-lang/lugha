/*
 * cells.c — part of lugha_rt.c: the runtime's cells, the shared state of
 * the library's handles (an open file's descriptor and read-ahead, a
 * socket's descriptor and timeout). A cell is a counted object holding four
 * integers and one byte array; every copy of a handle holds the same cell,
 * so a close through one copy is seen by all. Lugha reaches a cell only
 * through these built-ins, each one step on the runtime's object, never
 * through a `Ref` that two tasks could share.
 *
 * A fragment, not compiled on its own: joined after bytes.c (`bytes_of`)
 * and rc.c (counting).
 */

enum { CELL_SLOTS = 4 };

typedef struct {
    int64_t slots[CELL_SLOTS];
    /* Bytes the handle keeps (a file's read-ahead); NULL for none. */
    LughaArray *bytes;
} LughaCell;

/* A cell's visit: what its bytes hold goes with it. */
static void cell_visit(void *object, int64_t op) {
    LughaCell *c = object;
    if (op == RC_MARK) {
        lugha_rt_rc_mark(c->bytes);
    } else {
        lugha_rt_rc_release(c->bytes);
    }
}

/* The slot `i` names; anything else is a bug in the library. */
static int64_t *cell_slot(LughaCell *c, int64_t i) {
    if (i < 0 || i >= CELL_SLOTS) {
        die("panic: internal error: no such cell slot\n");
    }
    return &c->slots[i];
}

/* `$os.cell_new(a, b, c, d)`: a cell holding those four, and no bytes. */
void *lugha_rt_cell_new(int64_t a, int64_t b, int64_t c, int64_t d) {
    LughaCell *cell = lugha_rt_rc_alloc((int64_t)sizeof *cell, cell_visit);
    cell->slots[0] = a;
    cell->slots[1] = b;
    cell->slots[2] = c;
    cell->slots[3] = d;
    cell->bytes = NULL;
    return cell;
}

/* `$os.cell_get(cell, i)`. */
int64_t lugha_rt_cell_get(LughaCell *cell, int64_t i) {
    RT_SECTION();
    return *cell_slot(cell, i);
}

/* `$os.cell_set(cell, i, v)`. */
void lugha_rt_cell_set(LughaCell *cell, int64_t i, int64_t v) {
    RT_SECTION();
    *cell_slot(cell, i) = v;
}

/* `$os.cell_bytes(cell)`: its bytes, a new holder (empty when it has none). */
LughaArray *lugha_rt_cell_bytes(LughaCell *cell) {
    RT_SECTION();
    if (cell->bytes == NULL) {
        return bytes_of("", 0);
    }
    lugha_rt_rc_retain(cell->bytes);
    return cell->bytes;
}

/* `$os.cell_set_bytes(cell, bytes)`: the cell holds `bytes` now. */
void lugha_rt_cell_set_bytes(LughaCell *cell, LughaArray *bytes) {
    RT_SECTION();
    // Copies of the handle, and so the cell, may be in other tasks.
    lugha_rt_rc_mark(bytes);
    lugha_rt_rc_retain(bytes);
    lugha_rt_rc_release(cell->bytes);
    cell->bytes = bytes;
}
