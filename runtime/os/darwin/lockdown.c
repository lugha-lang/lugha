/*
 * os/darwin/lockdown.c — part of lugha_rt.c: process lockdown on macOS.
 * Once the runtime has made what `main` asked for, Seatbelt limits the
 * whole process, C code and started programs included, for the two kinds
 * it refuses reliably: no sockets reaching anything without `Net`, no new
 * processes without `Proc`. Files aren't limited by the kernel on macOS:
 * a profile for them would have to allow name lookups, trusted
 * certificates, time zones and lazily loaded libraries, which change
 * between macOS versions. Lugha's own capability checks hold regardless.
 *
 * Seatbelt (`sandbox_init`, a profile in Apple's SBPL) is in libSystem but
 * left out of the SDK's headers, so it is declared here, as browsers
 * declare it. Once applied it can't be lifted, and children inherit it.
 * If the system refuses the profile the program goes on, unlimited by the
 * kernel, as on a Linux kernel without seccomp; `LUGHA_LOCKDOWN=report`
 * prints what applied.
 *
 * A fragment, not compiled on its own: joined where os/linux/lockdown.c is.
 */

int sandbox_init(const char *profile, uint64_t flags, char **errorbuf);
void sandbox_free_error(char *errorbuf);

/* The capability kinds, one bit each: what the compiler passes. */
enum {
    LOCK_FS = 1 << 0,
    LOCK_NET = 1 << 1,
    LOCK_PROC = 1 << 2,
};

/* Without `Net`: no socket but a UNIX one is made (`socket` and
 * `socketpair` alike), and none connects, binds or sends to an address,
 * a named UNIX socket's included. An unnamed pair has none, so it works. */
static const char LOCK_NO_NET[] = "(deny network*)\n"
                                  "(deny system-socket (require-not (socket-domain AF_UNIX)))\n";

/* Without `Proc`: no fork, vfork, posix_spawn or exec. */
static const char LOCK_NO_PROC[] = "(deny process-fork)\n"
                                   "(deny process-exec*)\n";

/* `lugha_rt_lockdown(kinds)`: limits the process to the capability kinds
 * in `kinds` as far as Seatbelt can; nothing printed unless
 * `LUGHA_LOCKDOWN=report`. */
void lugha_rt_lockdown(int64_t kinds) {
    char profile[256] = "(version 1)\n(allow default)\n";
    if (!(kinds & LOCK_NET)) {
        strcat(profile, LOCK_NO_NET);
    }
    if (!(kinds & LOCK_PROC)) {
        strcat(profile, LOCK_NO_PROC);
    }
    bool needed = (kinds & (LOCK_NET | LOCK_PROC)) != (LOCK_NET | LOCK_PROC);
    char *error = NULL;
    bool applied = needed && sandbox_init(profile, 0, &error) == 0;
    // Helper threads started from now on are limited as this one is.
    helpers_allowed = true;
    const char *report = getenv("LUGHA_LOCKDOWN");
    if (report != NULL && strcmp(report, "report") == 0) {
        if (applied) {
            fprintf(stderr, "lockdown: seatbelt applied");
        } else if (!needed) {
            fprintf(stderr, "lockdown: seatbelt not needed");
        } else {
            fprintf(stderr, "lockdown: seatbelt not applied (%s)",
                    error != NULL ? error : "no reason given");
        }
        fprintf(stderr, ", files not limited; net %s, proc %s, fs %s\n",
                (kinds & LOCK_NET) ? "held" : "refused", (kinds & LOCK_PROC) ? "held" : "refused",
                (kinds & LOCK_FS) ? "held" : "refused");
    }
    if (error != NULL) {
        sandbox_free_error(error);
    }
}
