/*
 * rc_check.c — part of lugha_rt.c: the owner check, `LUGHA_RC_CHECK=1`.
 *
 * An object not marked shared belongs to the task that made it; only a
 * marked one may reach another task, whose counts then change atomically.
 * With the check on, every new object's count word also names its maker:
 *
 *   bits 63 and 61 set, 62 clear  (`RC_CHECKED`: negative, so generated
 *                                  code's inline count sends it here);
 *   bits 32–60                    the making task's number;
 *   bits 0–31                     the number of holders.
 *
 * Counting or marking it from another task ends the program, naming both
 * tasks. Marking turns it into an ordinary shared count. The runtime's own
 * retains and releases aren't checked: it lets go of a finished task's
 * leftovers from whichever task runs next.
 *
 * A fragment, not compiled on its own: joined after rc_alloc.c, before
 * rc.c; the task numbers come from tasks.c.
 */

#define RC_CHECKED ((int64_t)(UINT64_C(0xA) << 60))
#define RC_CHECK_TAG ((int64_t)(UINT64_C(0xE) << 60))
#define RC_OWNER_BITS ((int64_t)0x1FFFFFFF)
#define RC_HOLDER_BITS ((int64_t)0xFFFFFFFF)

static bool rc_checking;

/* The running task's number: 1 for `main`, then in the order started. */
static int64_t task_number(void);

static void rc_check_start(void) {
    const char *check = getenv("LUGHA_RC_CHECK");
    rc_checking = check != NULL && strcmp(check, "1") == 0;
}

/* True for a count in the checked form. */
static bool rc_checked(int64_t count) {
    return (count & RC_CHECK_TAG) == RC_CHECKED;
}

/* A new object's count: one holder, and with the check on, its maker. */
static int64_t rc_first_count(void) {
    if (!rc_checking) {
        return 1;
    }
    return RC_CHECKED | ((task_number() & RC_OWNER_BITS) << 32) | 1;
}

static int64_t rc_checked_holders(int64_t count) {
    return count & RC_HOLDER_BITS;
}

/* Ends the program if `count` names a task other than the running one. */
static void rc_check_owner(int64_t count) {
    int64_t maker = (count >> 32) & RC_OWNER_BITS;
    int64_t user = task_number() & RC_OWNER_BITS;
    if (maker != user) {
        fflush(stdout);
        fprintf(stderr,
                "panic: an unshared object reached a second task (made by task %" PRId64
                ", used by task %" PRId64 ")\n",
                maker, user);
        exit(101);
    }
}

/* A checked count changed by `delta` (+1 or -1): the new count, whose
 * holders are zero when it should be freed. */
static int64_t rc_checked_change(int64_t count, int64_t delta, bool check) {
    if (check) {
        rc_check_owner(count);
    }
    return count + delta;
}
