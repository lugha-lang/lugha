/*
 * file.c — part of lugha_rt.c: `std.fs`'s file streams.
 * A `File` is a descriptor; `std.fs` keeps its mode, its read buffer and
 * whether it is closed, so these functions only open, read, write and close.
 * They block the OS thread while they run (disk files are quick; a FIFO
 * stalls the other green threads, as `std.fs` documents).
 *
 * A fragment, not compiled on its own: lughac's link.rs joins it after the
 * core (lugha_rt.c), whose includes (`<fcntl.h>`, `<unistd.h>`, `<errno.h>`,
 * `<string.h>`) it relies on, and after fs.c (`fs_fail`, `fs_has_nul`) and
 * bytes.c (`bytes_of`).
 */

/* The most one `read` asks for, as for sockets: 16 MiB. A stream is opened
 * beneath its `Dir` by dir.c (`dir_file`). */
#define FILE_MAX_READ 16777216

/* `$os.file_read(fd, max)`: 1 and up to `max` bytes (none at the end of the
 * file), or 0 and a message. An interrupted read is retried. */
int32_t lugha_rt_file_read(int64_t fd, int64_t max, LughaArray **out, LughaString **error) {
    if (max < 1 || max > FILE_MAX_READ) {
        return fs_fail(error, "max must be from 1 to 16777216");
    }
    char *buffer = rt_scratch(max);
    for (;;) {
        ssize_t n = read((int)fd, buffer, (size_t)max);
        if (n >= 0) {
            *out = bytes_of(buffer, (int64_t)n);
            rt_scratch_done(buffer);
            return 1;
        }
        if (errno != EINTR) {
            int saved = errno;
            rt_scratch_done(buffer);
            return fs_fail(error, strerror(saved));
        }
    }
}

/* `$os.file_write(fd, bytes)`: 1 once every byte is written, or 0 and the
 * system's message; short and interrupted writes go on where they stopped. */
int32_t lugha_rt_file_write(int64_t fd, const LughaArray *bytes, LughaString **error) {
    int64_t done = 0;
    while (done < bytes->len) {
        ssize_t n = write((int)fd, bytes->elems + done, (size_t)(bytes->len - done));
        if (n >= 0) {
            done += n;
        } else if (errno != EINTR) {
            return fs_fail(error, strerror(errno));
        }
    }
    return 1;
}

/* `$os.file_close(fd)`. A failing `close` is ignored: the descriptor is gone
 * either way (Linux frees it even on EINTR), and nothing is buffered here
 * that could be lost. */
void lugha_rt_file_close(int64_t fd) {
    close((int)fd);
}
