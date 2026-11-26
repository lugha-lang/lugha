/*
 * os/windows/lockdown.c — part of lugha_rt.c on Windows: process lockdown.
 * Once the runtime has made what `main` asked for, Windows limits the
 * whole process, C code and started programs included, for the two kinds
 * it can refuse from inside a process:
 *
 * - Without `Proc`, no new processes: the child-process mitigation policy,
 *   and a job allowing one active process, this one. Either alone refuses
 *   a child, so each covers a system that refuses the other. The job's
 *   only handle is closed once this process is in it: nothing left here
 *   can raise its limit, and an unnamed job can't be opened again.
 * - Without `Fs`, files are read-only: the process's own token drops to
 *   Low integrity, which Windows never lets it raise again. A Low process
 *   can read what a user can, and write only where Windows has labelled a
 *   place Low (`%LOCALAPPDATA%Low`). Handles already open (stdout sent to
 *   a file) keep what they were opened for. Programs it starts inherit
 *   the token, so they are Low too.
 *
 * The network can't be refused from inside a process on Windows (that
 * takes administrator rights, or an AppContainer chosen before a program
 * starts), so `Net` is bound by Lugha's own capability checks alone, and
 * the report says so. A part Windows refuses leaves the program
 * unlimited in that part, as on a Linux kernel without seccomp, and
 * `LUGHA_LOCKDOWN=report` says why.
 *
 * A fragment, not compiled on its own: joined last on Windows.
 */

/* The capability kinds, one bit each: what the compiler passes. */
enum {
    LOCK_FS = 1 << 0,
    LOCK_NET = 1 << 1,
    LOCK_PROC = 1 << 2,
};

/* The mitigation policy refusing this process any child: 0, or why not. */
static DWORD lock_child_policy(void) {
    PROCESS_MITIGATION_CHILD_PROCESS_POLICY policy;
    memset(&policy, 0, sizeof policy);
    policy.NoChildProcessCreation = 1;
    return SetProcessMitigationPolicy(ProcessChildProcessPolicy, &policy, sizeof policy)
               ? 0
               : GetLastError();
}

/* This process put in a job of its own allowing one active process, the
 * job's handle then closed: 0, or why not. A job closed with a process in
 * it lives on, its limits with it. */
static DWORD lock_child_job(void) {
    HANDLE job = CreateJobObjectW(NULL, NULL);
    if (job == NULL) {
        return GetLastError();
    }
    JOBOBJECT_BASIC_LIMIT_INFORMATION limits;
    memset(&limits, 0, sizeof limits);
    // No `JOB_OBJECT_LIMIT_BREAKAWAY_OK`: a child can't leave the job.
    limits.LimitFlags = JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    limits.ActiveProcessLimit = 1;
    DWORD error = 0;
    if (!SetInformationJobObject(job, JobObjectBasicLimitInformation, &limits, sizeof limits) ||
        !AssignProcessToJobObject(job, GetCurrentProcess())) {
        error = GetLastError();
    }
    CloseHandle(job);
    return error;
}

/* This process's own primary token lowered to Low integrity: 0, or why
 * not. */
static DWORD lock_low_integrity(void) {
    HANDLE token;
    if (!OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_DEFAULT | TOKEN_QUERY, &token)) {
        return GetLastError();
    }
    DWORD error = 0;
    union {
        SID sid;
        BYTE bytes[SECURITY_MAX_SID_SIZE];
    } low;
    DWORD size = sizeof low;
    if (!CreateWellKnownSid(WinLowLabelSid, NULL, &low.sid, &size)) {
        error = GetLastError();
    } else {
        TOKEN_MANDATORY_LABEL label;
        memset(&label, 0, sizeof label);
        label.Label.Attributes = SE_GROUP_INTEGRITY;
        label.Label.Sid = &low.sid;
        if (!SetTokenInformation(token, TokenIntegrityLevel, &label,
                                 (DWORD)(sizeof label + GetLengthSid(&low.sid)))) {
            error = GetLastError();
        }
    }
    CloseHandle(token);
    return error;
}

/* `lugha_rt_lockdown(kinds)`: limits the process to the capability kinds
 * in `kinds` as far as Windows can; nothing printed unless
 * `LUGHA_LOCKDOWN=report`. */
void lugha_rt_lockdown(int64_t kinds) {
    // Each part's error, or 0 when it applied; unset when it wasn't needed.
    DWORD policy = 0;
    DWORD job = 0;
    DWORD integrity = 0;
    if (!(kinds & LOCK_PROC)) {
        policy = lock_child_policy();
        job = lock_child_job();
    }
    if (!(kinds & LOCK_FS)) {
        integrity = lock_low_integrity();
    }
    // Helper threads started from now on are limited as this one is.
    helpers_allowed = true;
    const char *report = getenv("LUGHA_LOCKDOWN");
    if (report == NULL || strcmp(report, "report") != 0) {
        return;
    }
    fprintf(stderr, "lockdown: ");
    if ((kinds & (LOCK_PROC | LOCK_FS)) == (LOCK_PROC | LOCK_FS)) {
        fprintf(stderr, "nothing needed");
    }
    if (!(kinds & LOCK_PROC)) {
        if (policy == 0 || job == 0) {
            fprintf(stderr, "processes refused");
        } else {
            fprintf(stderr, "processes not refused (Windows refused both: error %lu, %lu)",
                    (unsigned long)policy, (unsigned long)job);
        }
    }
    if (!(kinds & LOCK_FS)) {
        fprintf(stderr, "%s", (kinds & LOCK_PROC) ? "" : ", ");
        if (integrity == 0) {
            fprintf(stderr, "files read-only");
        } else {
            fprintf(stderr, "files not limited (Windows refused: error %lu)",
                    (unsigned long)integrity);
        }
    }
    fprintf(stderr, ", network not limited; net %s, proc %s, fs %s\n",
            (kinds & LOCK_NET) ? "held" : "refused", (kinds & LOCK_PROC) ? "held" : "refused",
            (kinds & LOCK_FS) ? "held" : "refused");
}
