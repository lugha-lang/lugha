/*
 * os/windows/missing.c — part of lugha_rt.c on Windows: what isn't built
 * for Windows yet. Process lockdown isn't applied, and says so.
 *
 * A fragment, not compiled on its own: joined last, after every part it
 * stands in for would have been.
 */

/* `lugha_rt_lockdown(kinds)`: nothing is applied on Windows yet; with
 * `LUGHA_LOCKDOWN=report` it says so, never claiming otherwise. Lugha's
 * own capability checks still hold. */
void lugha_rt_lockdown(int64_t kinds) {
    (void)kinds;
    // Helper threads may start from now on, as after any lockdown.
    helpers_allowed = true;
    const char *report = getenv("LUGHA_LOCKDOWN");
    if (report != NULL && strcmp(report, "report") == 0) {
        fprintf(stderr, "lockdown: not applied on Windows; Lugha's own capability checks hold\n");
    }
}
