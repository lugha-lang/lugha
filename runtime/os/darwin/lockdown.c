/*
 * os/darwin/lockdown.c — part of lugha_rt.c: process lockdown on macOS,
 * not applied for now. Linux's (os/linux/lockdown.c) uses seccomp and
 * Landlock, which macOS lacks; until macOS's answer is decided, nothing is
 * limited by the kernel, as on a Linux kernel without Landlock, and
 * `LUGHA_LOCKDOWN=report` says so. Lugha's own capability checks hold.
 *
 * A fragment, not compiled on its own: joined where os/linux/lockdown.c is.
 */

/* The capability kinds, one bit each: what the compiler passes. */
enum {
    LOCK_FS = 1 << 0,
    LOCK_NET = 1 << 1,
    LOCK_PROC = 1 << 2,
};

/* `lugha_rt_lockdown(kinds)`: limits nothing on macOS; helper threads may
 * start from now on, as after Linux's lockdown. */
void lugha_rt_lockdown(int64_t kinds) {
    helpers_allowed = true;
    const char *report = getenv("LUGHA_LOCKDOWN");
    if (report != NULL && strcmp(report, "report") == 0) {
        fprintf(stderr, "lockdown: not applied on macOS; net %s, proc %s, fs %s\n",
                (kinds & LOCK_NET) ? "held" : "refused", (kinds & LOCK_PROC) ? "held" : "refused",
                (kinds & LOCK_FS) ? "held" : "refused");
    }
}
