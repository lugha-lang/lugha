/*
 * process.c — part of lugha_rt.c: `std.process`'s one built-in.
 * Starting processes is security-sensitive: every change needs a review.
 *
 * - No shell: `posix_spawnp` gets the argument list as given, so no text is
 *   ever split, globbed or interpreted. A command without `/` is found on
 *   `PATH`; one with `/` is a path. A NUL byte anywhere is refused.
 * - The child gets this program's environment unchanged and only fds 0, 1
 *   and 2: the pipes are made close-on-exec, and so is every descriptor the
 *   runtime opens elsewhere (`fopen` mode "e", `O_CLOEXEC`).
 * - stdin, stdout and stderr are served together by `poll`, so a child that
 *   fills one pipe while this side waits on another can't deadlock.
 *   `SIGPIPE` is blocked while writing, and the stdin pipe is quiet
 *   (`os_no_sigpipe`); a child that closes its stdin early just ends the
 *   input.
 * - Death by signal n is code 128 + n. There is no timeout and no output
 *   limit.
 * - Everything from starting the child to waiting for it runs on a helper
 *   thread (helpers.c), with copies of the command, arguments and input,
 *   while the task waits; a task cancelled meanwhile stops waiting, and the
 *   helper still waits for the child, so no zombie is left.
 *
 * A fragment, not compiled on its own: lughac's `link` module joins it after
 * the core (lugha_rt.c), whose includes (`<spawn.h>`, `<poll.h>`,
 * `<sys/wait.h>`, `<signal.h>`, `<fcntl.h>`, `<time.h>`) it relies on, and
 * after bytes.c (`bytes_of`) and input.c (`string_of`).
 */

extern char **environ;

/* A growing byte buffer for one of the child's outputs, from `malloc`. */
typedef struct {
    char *bytes;
    size_t len;
    size_t cap;
} Captured;

/* Fails with `message` (and, when `cmd` is given, `cmd: ` before it). */
static int32_t spawn_fail(LughaString **error, const char *cmd, const char *message) {
    char text[512];
    if (cmd != NULL) {
        snprintf(text, sizeof text, "%s: %s", cmd, message);
    } else {
        snprintf(text, sizeof text, "%s", message);
    }
    *error = string_of(text, (int64_t)strlen(text));
    return 0;
}

/* Reads what `fd` has into `into`; false at end of input or on an error. */
static bool drain(int fd, Captured *into) {
    job_grow(&into->bytes, &into->cap, into->len, 4096);
    ssize_t got = read(fd, into->bytes + into->len, into->cap - into->len);
    if (got > 0) {
        into->len += (size_t)got;
        return true;
    }
    return got < 0 && (errno == EINTR || errno == EAGAIN);
}

/* Feeds `input` to the child's stdin while draining its stdout and stderr
 * into `out` and `err`, until all three are closed; the descriptors are
 * closed here. */
static void spawn_pump(int in_fd, int out_fd, int err_fd, const char *input, size_t input_len,
                       Captured *out, Captured *err) {
    /* Writes may fail with EPIPE; block SIGPIPE so that can't kill us. */
    sigset_t pipe_signal, before;
    sigemptyset(&pipe_signal);
    sigaddset(&pipe_signal, SIGPIPE);
    pthread_sigmask(SIG_BLOCK, &pipe_signal, &before);
    size_t written = 0;
    if (in_fd >= 0) {
        fcntl(in_fd, F_SETFL, fcntl(in_fd, F_GETFL) | O_NONBLOCK);
        if (input_len == 0) {
            close(in_fd);
            in_fd = -1;
        }
    }
    while (out_fd >= 0 || err_fd >= 0 || in_fd >= 0) {
        struct pollfd fds[3];
        nfds_t n = 0;
        int at_out = -1, at_err = -1, at_in = -1;
        if (out_fd >= 0) { at_out = (int)n; fds[n++] = (struct pollfd){out_fd, POLLIN, 0}; }
        if (err_fd >= 0) { at_err = (int)n; fds[n++] = (struct pollfd){err_fd, POLLIN, 0}; }
        if (in_fd >= 0) { at_in = (int)n; fds[n++] = (struct pollfd){in_fd, POLLOUT, 0}; }
        if (poll(fds, n, -1) < 0) {
            if (errno == EINTR) {
                continue;
            }
            break;
        }
        if (at_out >= 0 && fds[at_out].revents != 0 && !drain(out_fd, out)) {
            close(out_fd);
            out_fd = -1;
        }
        if (at_err >= 0 && fds[at_err].revents != 0 && !drain(err_fd, err)) {
            close(err_fd);
            err_fd = -1;
        }
        if (at_in >= 0 && fds[at_in].revents != 0) {
            ssize_t put = write(in_fd, input + written, input_len - written);
            if (put > 0) {
                written += (size_t)put;
            }
            bool more = put >= 0 || errno == EINTR || errno == EAGAIN;
            if (!more || written == input_len) {
                close(in_fd);
                in_fd = -1;
            }
        }
    }
    /* A SIGPIPE raised while blocked is still pending: take it before
     * unblocking, so it can't end this program. */
    sigset_t pending;
    sigpending(&pending);
    if (sigismember(&pending, SIGPIPE)) {
        // Pending, so taken at once: `sigwait` doesn't block here.
        int taken;
        sigwait(&pipe_signal, &taken);
    }
    pthread_sigmask(SIG_SETMASK, &before, NULL);
}

/* On a helper: starts `argv[0]` with `argv` (mode `flags`, input `in`),
 * serves its pipes and waits for it: its code in `value` and its outputs
 * in `out` and `out2`, or the failure in `err`. */
static void spawn_job(Job *job) {
    // Where the system's start can't say a program is missing (Android).
    int why = os_spawn_check(job->argv[0]);
    if (why != 0) {
        job->err = why;
        return;
    }
    int mode = job->flags;
    bool capture = mode != 0;
    int in_pipe[2] = {-1, -1}, out_pipe[2] = {-1, -1}, err_pipe[2] = {-1, -1};
    posix_spawn_file_actions_t actions;
    posix_spawn_file_actions_init(&actions);
    if (capture) {
        if (os_pipe(out_pipe) != 0 || os_pipe(err_pipe) != 0 ||
            (mode == 2 && os_pipe(in_pipe) != 0)) {
            job->err = errno;
            for (int k = 0; k < 2; k++) {
                if (out_pipe[k] >= 0) close(out_pipe[k]);
                if (err_pipe[k] >= 0) close(err_pipe[k]);
                if (in_pipe[k] >= 0) close(in_pipe[k]);
            }
            posix_spawn_file_actions_destroy(&actions);
            return;
        }
        if (mode == 2) {
            // A child that stops reading early is EPIPE to the pump, on
            // every system (os/<system>/os.c).
            os_no_sigpipe(in_pipe[1]);
            posix_spawn_file_actions_adddup2(&actions, in_pipe[0], 0);
        } else {
            posix_spawn_file_actions_addopen(&actions, 0, "/dev/null", O_RDONLY, 0);
        }
        posix_spawn_file_actions_adddup2(&actions, out_pipe[1], 1);
        posix_spawn_file_actions_adddup2(&actions, err_pipe[1], 2);
    }
    // A helper blocks every signal, and a child takes its starter's mask:
    // the child starts with none blocked, as one started by the worker did.
    posix_spawnattr_t attr;
    posix_spawnattr_init(&attr);
    sigset_t none;
    sigemptyset(&none);
    posix_spawnattr_setsigmask(&attr, &none);
    // And what the system needs (os/<system>/os.c): on macOS, every
    // descriptor not named above closed in the child.
    short flags = os_spawn_prepare(&actions, !capture);
    posix_spawnattr_setflags(&attr, (short)(POSIX_SPAWN_SETSIGMASK | flags));
    pid_t pid;
    int spawned = posix_spawnp(&pid, job->argv[0], &actions, &attr, job->argv, environ);
    posix_spawnattr_destroy(&attr);
    posix_spawn_file_actions_destroy(&actions);
    if (capture) {
        close(out_pipe[1]);
        close(err_pipe[1]);
        if (mode == 2) {
            close(in_pipe[0]);
        }
    }
    if (spawned != 0) {
        if (capture) {
            close(out_pipe[0]);
            close(err_pipe[0]);
            if (mode == 2) {
                close(in_pipe[1]);
            }
        }
        job->err = spawned;
        return;
    }
    Captured out = {NULL, 0, 0};
    Captured err = {NULL, 0, 0};
    if (capture) {
        spawn_pump(mode == 2 ? in_pipe[1] : -1, out_pipe[0], err_pipe[0], job->in, job->in_len,
                   &out, &err);
    }
    // Waited for even when its task stopped waiting: no zombie is left.
    int status = 0;
    while (waitpid(pid, &status, 0) < 0) {
        if (errno != EINTR) {
            job->err = errno;
            free(out.bytes);
            free(err.bytes);
            return;
        }
    }
    if (WIFEXITED(status)) {
        job->value = WEXITSTATUS(status);
    } else if (WIFSIGNALED(status)) {
        job->value = 128 + WTERMSIG(status);
    } else {
        job->value = 0;
    }
    job->out = out.bytes;
    job->out_len = out.len;
    job->out2 = err.bytes;
    job->out2_len = err.len;
}

/* `$os.spawn(cmd, args, input, mode)`: runs `cmd` with `args` and waits.
 * `mode` 0 shares this program's stdin, stdout and stderr; 1 captures
 * stdout and stderr with an empty stdin; 2 also writes `input`, a `u8[]`,
 * to stdin. 1 with `*code` and the outputs' bytes, or 0 and `*error`. */
int32_t lugha_rt_spawn(const LughaString *cmd, const LughaArray *args, const LughaArray *input,
                       int32_t mode, int64_t *code, LughaArray **out, LughaArray **err,
                       LughaString **error) {
    const LughaString **list = (const LughaString **)args->elems;
    if (memchr(cmd->bytes, 0, (size_t)cmd->len) != NULL) {
        return spawn_fail(error, NULL, "command contains a NUL byte");
    }
    for (int64_t i = 0; i < args->len; i++) {
        if (memchr(list[i]->bytes, 0, (size_t)list[i]->len) != NULL) {
            char text[64];
            snprintf(text, sizeof text, "args[%" PRId64 "] contains a NUL byte", i);
            return spawn_fail(error, NULL, text);
        }
    }
    /* The job's own copies: argv (the command, the arguments, NULL) and
     * the input. */
    Job *job = job_new(spawn_job);
    job->argv = calloc((size_t)args->len + 2, sizeof(char *));
    if (job->argv == NULL) {
        die("panic: out of memory\n");
    }
    job->argv[0] = job_copy(cmd->bytes, (size_t)cmd->len);
    for (int64_t i = 0; i < args->len; i++) {
        job->argv[i + 1] = job_copy(list[i]->bytes, (size_t)list[i]->len);
    }
    job->flags = mode;
    if (mode == 2) {
        job->in = job_copy((const char *)input->elems, (size_t)input->len);
        job->in_len = (size_t)input->len;
    }
    /* What this program printed so far comes before what the child prints. */
    fflush(NULL);
    job_run(job);
    if (job->err != 0) {
        int32_t failed = spawn_fail(error, cmd->bytes, strerror(job->err));
        job_free(job);
        return failed;
    }
    *code = job->value;
    *out = bytes_of(job->out, (int64_t)job->out_len);
    *err = bytes_of(job->out2, (int64_t)job->out2_len);
    job_free(job);
    return 1;
}
