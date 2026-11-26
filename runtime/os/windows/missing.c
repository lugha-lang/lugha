/*
 * os/windows/missing.c — part of lugha_rt.c on Windows: what isn't built
 * for Windows yet, failing closed. Every start of a
 * program is `Err("processes aren't supported on Windows yet")`, so a
 * program never starts another some other way.
 * Process lockdown isn't applied, and says so.
 *
 * A fragment, not compiled on its own: joined last, after every part it
 * stands in for would have been.
 */

static const char WIN_NO_PROCS[] = "processes aren't supported on Windows yet";

/* 0 and `message` as the error, as every refusal here. */
static int32_t win_refuse(LughaString **error, const char *message) {
    *error = string_of(message, (int64_t)strlen(message));
    return 0;
}

int32_t lugha_rt_spawn(const LughaString *cmd, const LughaArray *args, const LughaArray *input,
                       int32_t mode, int64_t *code, LughaArray **out, LughaArray **err,
                       LughaString **error) {
    (void)cmd;
    (void)args;
    (void)input;
    (void)mode;
    (void)code;
    (void)out;
    (void)err;
    return win_refuse(error, WIN_NO_PROCS);
}

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
