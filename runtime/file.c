/*
 * file.c — part of lugha_rt.c: `std.fs`'s file streams.
 * A `File` is a descriptor; `std.fs` keeps its mode, its read buffer and
 * whether it is closed, so these functions only open, read, write and close.
 * Reads and writes run on a helper thread, with a copy of the descriptor,
 * while the task waits and the others run.
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it after the
 * core (lugha_rt.c), whose includes (`<fcntl.h>`, `<unistd.h>`, `<errno.h>`,
 * `<string.h>`) it relies on, and after fs.c (`fs_fail`, `fs_has_nul`) and
 * bytes.c (`bytes_of`).
 */

/* The most one `read` asks for, as for sockets: 16 MiB. A stream is opened
 * beneath its `Dir` by dir.c (`dir_file`). */
#define FILE_MAX_READ 16777216

/* On a helper: `read` into `out`, `n` bytes at most, retried when
 * interrupted; `value` the count, or `err` set. The descriptor is the
 * job's own copy, closed here. */
static void file_read_job(Job *job) {
    job->out = malloc((size_t)job->n);
    if (job->out == NULL) {
        die("panic: out of memory\n");
    }
    ssize_t n;
    do {
        n = read(job->fd, job->out, (size_t)job->n);
    } while (n < 0 && errno == EINTR);
    if (n < 0) {
        job->err = errno;
    }
    job->value = n;
    close(job->fd);
}

/* On a helper: every byte of `in` written, short and interrupted writes
 * going on where they stopped, or `err` set; the descriptor closed. */
static void file_write_job(Job *job) {
    size_t done = 0;
    while (done < job->in_len) {
        ssize_t n = write(job->fd, job->in + done, job->in_len - done);
        if (n >= 0) {
            done += (size_t)n;
        } else if (errno != EINTR) {
            job->err = errno;
            break;
        }
    }
    close(job->fd);
}

/* A copy of `fd` for a job: closing the stream while the job runs (its
 * task cancelled) can't hand the number to another file mid-read. -1 with
 * the message set. */
static int file_dup(int64_t fd, LughaString **error) {
    int copy = fcntl((int)fd, F_DUPFD_CLOEXEC, 0);
    if (copy < 0) {
        fs_fail(error, strerror(errno));
    }
    return copy;
}

/* `$os.file_read(fd, max)`: 1 and up to `max` bytes (none at the end of the
 * file), or 0 and a message. */
int32_t lugha_rt_file_read(int64_t fd, int64_t max, LughaArray **out, LughaString **error) {
    if (max < 1 || max > FILE_MAX_READ) {
        return fs_fail(error, "max must be from 1 to 16777216");
    }
    int copy = file_dup(fd, error);
    if (copy < 0) {
        return 0;
    }
    Job *job = job_new(file_read_job);
    job->fd = copy;
    job->n = max;
    job_run(job);
    int32_t ok = job->err == 0;
    if (ok) {
        *out = bytes_of(job->out, job->value);
    } else {
        fs_fail(error, strerror(job->err));
    }
    job_free(job);
    return ok;
}

/* `$os.file_write(fd, bytes)`: 1 once every byte is written, or 0 and the
 * system's message. */
int32_t lugha_rt_file_write(int64_t fd, const LughaArray *bytes, LughaString **error) {
    int copy = file_dup(fd, error);
    if (copy < 0) {
        return 0;
    }
    Job *job = job_new(file_write_job);
    job->fd = copy;
    job->in = job_copy((const char *)bytes->elems, (size_t)bytes->len);
    job->in_len = (size_t)bytes->len;
    job_run(job);
    int32_t ok = job->err == 0 ? 1 : fs_fail(error, strerror(job->err));
    job_free(job);
    return ok;
}

/* `$os.file_close(fd)`. A failing `close` is ignored: the descriptor is gone
 * either way (Linux frees it even on EINTR), and nothing is buffered here
 * that could be lost. */
void lugha_rt_file_close(int64_t fd) {
    close((int)fd);
}
