/*
 * net.c — part of lugha_rt.c: `std.net`'s built-ins. TCP sockets through
 * the OS layer's operations (`os_sock_*`; os/posix/sockets.c on Linux and
 * macOS, os/windows/sockets.c on Windows), which park only the calling green thread while the others run;
 * the poll an idle worker waits in when nothing is ready (workers.c), and
 * the list of tasks parked on a socket, which closing it wakes. Every
 * failure comes back as 0 and the system's message, never a panic.
 *
 * A fragment, not compiled on its own: joined after tasks.c (the scheduler
 * it extends), bytes.c (`bytes_of`) and input.c (`string_of`).
 */

enum { NET_MAX_READ = 16 * 1024 * 1024, NET_BACKLOG = 128, NET_EVENTS = 64 };

/* An event's fiber: the low 48 bits of its 64 (`wait_fd`). */
#define EVENT_FIBER ((UINT64_C(1) << 48) - 1)

/* Green threads parked on a socket. */
static int64_t net_waiting;

static int32_t net_error(LughaString **error, const char *message) {
    *error = string_of(message, (int64_t)strlen(message));
    return 0;
}

/* Wakes every thread whose socket is ready; waits up to `wait_ns` (< 0
 * forever) for the first, holding no lock while it waits. A thread already
 * woken by its timeout is skipped; helper jobs come back, and the workers'
 * wake-up event is taken (workers.c). */
/* The poller stops counting as parked before what it found wakes tasks
 * (workers.c). */
static void poller_back(void);

/* What one poll found, waking tasks: under the lock, as every wake is. */
static void poll_found(OsEvent *events, int n) {
    RT_SECTION();
    for (int i = 0; i < n; i++) {
        if (events[i].token == OS_TOKEN_HELPERS) {
            jobs_collect();
            continue;
        }
        if (events[i].token == OS_TOKEN_WAKE) {
            wake_taken();
            continue;
        }
        // A fiber and the wait it was armed for: a late event misses.
        uint64_t packed = events[i].token;
        Fiber *f = (Fiber *)(uintptr_t)(packed & EVENT_FIBER);
        if (f != NULL) {
            uint32_t gen = word_gen(fired_of(f));
            uint32_t armed = (uint32_t)(packed >> 48);
            if ((gen & 0xffff) == armed) {
                wake_gen(f, gen, 0);
            } else if (((gen + 1) & 0xffff) == armed) {
                // Ready before its fiber published the wait it armed.
                wake_gen(f, gen + 1, 0);
            }
        }
    }
}

static void poll_sockets(int64_t wait_ns) {
    int ms = -1;
    if (wait_ns >= 0) {
        int64_t rounded = (wait_ns + 999999) / 1000000;
        ms = rounded > INT32_MAX ? INT32_MAX : (int)rounded;
    }
    OsEvent events[NET_EVENTS];
    int n = os_poll_wait(events, NET_EVENTS, ms);
    if (ms != 0) {
        poller_back();
    }
    if (n > 0) {
        poll_found(events, n);
    }
    // Rings whose writers found readers waiting (rings.c): after the
    // wake-up event was emptied, so a ring listed from now on kicks it
    // again and isn't stranded.
    rings_wake();
}

/* The tasks parked on a socket, so that closing it wakes them
 * (`lugha_rt_net_close`): an `epoll` event can't, since closing the
 * descriptor takes it out of the set. On Windows each still takes its
 * aborted operation's completion before it goes on. Under the runtime's lock. */
typedef struct {
    OsSocket fd;
    Fiber *fiber;
    uint32_t gen;
} NetWait;
static NetWait *net_waits;
static int64_t net_waits_len;
static int64_t net_waits_cap;

/* Lists the running task as parked on `fd` for wait `gen`. */
static void net_waits_add(OsSocket fd, uint32_t gen) {
    if (net_waits_len == net_waits_cap) {
        net_waits_cap = net_waits_cap == 0 ? 16 : net_waits_cap * 2;
        NetWait *bigger = realloc(net_waits, (size_t)net_waits_cap * sizeof *net_waits);
        if (bigger == NULL) {
            die("panic: out of memory\n");
        }
        net_waits = bigger;
    }
    net_waits[net_waits_len++] = (NetWait){.fd = fd, .fiber = current, .gen = gen};
}

/* Takes the running task off the list, however its wait ended. */
static void net_waits_remove(void) {
    for (int64_t i = 0; i < net_waits_len; i++) {
        if (net_waits[i].fiber == current) {
            net_waits[i] = net_waits[--net_waits_len];
            return;
        }
    }
}

static int64_t net_deadline(int64_t timeout_ms) {
    if (timeout_ms <= 0) {
        return -1;
    }
    int64_t ns = timeout_ms > INT64_MAX / 2000000 ? INT64_MAX / 2 : timeout_ms * 1000000;
    return now_ns() + ns;
}

/* On a helper: `getaddrinfo` of the job's host (`path`) and port (`path2`,
 * or none) for stream sockets with flags `flags`: the list in `found`, the
 * resolver's code in `value`. */
static void net_lookup_job(Job *job) {
    // One socket type, so each address comes once per family, not per protocol.
    struct addrinfo hints = {.ai_family = AF_UNSPEC, .ai_socktype = SOCK_STREAM};
    hints.ai_flags = job->flags;
    struct addrinfo *list = NULL;
    job->value = getaddrinfo(job->path, job->path2, &hints, &list);
    job->found = list;
}

static void net_lookup_drop(Job *job) {
    if (job->found != NULL) {
        freeaddrinfo(job->found);
    }
}

/* `host` (`len` bytes, no NUL) and `port` (or NULL) looked up with `flags`:
 * on a helper thread, unless only numbers are allowed, which never waits.
 * The resolver's code, and the list in `*out` for the caller to free. */
static int net_lookup(const char *host, size_t len, const char *port, int flags,
                      struct addrinfo **out) {
    Job *job = job_new(net_lookup_job);
    job->drop = net_lookup_drop;
    char scoped[NI_MAXHOST];
    if (os_scope_numeric(host, len, scoped, sizeof scoped)) {
        host = scoped;
        len = strlen(scoped);
    }
    job->path = job_copy(host, len);
    job->path2 = port == NULL ? NULL : job_copy(port, strlen(port));
    job->flags = flags;
    if (flags & AI_NUMERICHOST) {
        job->run(job);
    } else {
        job_run(job);
    }
    int code = (int)job->value;
    *out = job->found;
    job->found = NULL;
    job_free(job);
    return code;
}

/* `addr` ("host:port", "[ipv6-address]:port") resolved with `getaddrinfo`
 * flags `flags` (`AI_PASSIVE` to listen, `AI_NUMERICHOST` for no lookup);
 * a message when it can't be. The list is freed by the caller. */
static const char *net_resolve(const LughaString *addr, int flags, struct addrinfo **out) {
    if (memchr(addr->bytes, 0, (size_t)addr->len) != NULL) {
        return "address contains a NUL byte";
    }
    // The last colon: an IPv6 address has more before it.
    const char *colon = NULL;
    for (int64_t i = addr->len; i > 0 && colon == NULL; i--) {
        if (addr->bytes[i - 1] == ':') {
            colon = addr->bytes + i - 1;
        }
    }
    if (colon == NULL || colon == addr->bytes || colon[1] == 0) {
        return "address must be host:port";
    }
    size_t host_len = (size_t)(colon - addr->bytes);
    const char *host = addr->bytes;
    if (host[0] == '[') {
        if (host_len < 3 || host[host_len - 1] != ']') {
            return "address must be host:port";
        }
        host++;
        host_len -= 2;
    }
    const char *port = colon + 1;
    for (const char *p = port; *p != 0; p++) {
        if (*p < '0' || *p > '9' || p - port > 5) {
            return "port must be a number from 0 to 65535";
        }
    }
    if (atol(port) > 65535) {
        return "port must be a number from 0 to 65535";
    }
    int code = net_lookup(host, host_len, port, flags, out);
    return code == 0 ? NULL : os_lookup_message(code);
}

/* What a socket operation's outcome means to Lugha: 1 when it was done,
 * else 0 and its message; cancelled, the task unwinds from here, so the
 * caller lets go of what it holds before asking. */
static int32_t net_outcome(int64_t got, const char *message, LughaString **error) {
    if (got >= 0) {
        return 1;
    }
    if (got == OS_CANCELLED) {
        cancel_unwind();
    }
    if (got == OS_CLOSED) {
        return net_error(error, "closed");
    }
    if (got == OS_TIMED_OUT) {
        return net_error(error, "timed out");
    }
    return net_error(error, message);
}

/* `$os.net_listen(addr)`: 1 and a listening socket, or 0 and a message. */
int32_t lugha_rt_net_listen(const LughaString *addr, int64_t *fd, LughaString **error) {
    struct addrinfo *list = NULL;
    const char *problem = net_resolve(addr, AI_PASSIVE, &list);
    if (problem != NULL) {
        return net_error(error, problem);
    }
    const char *message = strerror(0);
    for (struct addrinfo *a = list; a != NULL; a = a->ai_next) {
        OsSocket s = os_sock_open(a->ai_family, a->ai_socktype, a->ai_protocol, &message);
        if (s >= 0 && os_sock_listen(s, a->ai_addr, a->ai_addrlen, NET_BACKLOG, &message) == 0) {
            freeaddrinfo(list);
            *fd = s;
            return 1;
        }
        if (s >= 0) {
            os_sock_close(s);
        }
    }
    freeaddrinfo(list);
    return net_error(error, message);
}

/* `$os.net_connect(addr, timeout_ms)`: 1 and a connected socket, or 0 and
 * a message; tries each address the name resolves to in turn. */
int32_t lugha_rt_net_connect(const LughaString *addr, int64_t timeout_ms, int64_t *fd,
                             LughaString **error) {
    struct addrinfo *list = NULL;
    const char *problem = net_resolve(addr, 0, &list);
    if (problem != NULL) {
        return net_error(error, problem);
    }
    int64_t deadline = net_deadline(timeout_ms);
    const char *message = "no address to connect to";
    for (struct addrinfo *a = list; a != NULL; a = a->ai_next) {
        OsSocket s = os_sock_open(a->ai_family, a->ai_socktype, a->ai_protocol, &message);
        if (s < 0) {
            continue;
        }
        int64_t got = os_sock_connect(s, a->ai_addr, a->ai_addrlen, deadline, &message);
        if (got == 0) {
            freeaddrinfo(list);
            *fd = s;
            return 1;
        }
        os_sock_close(s);
        if (got == OS_CLOSED || got == OS_CANCELLED) {
            freeaddrinfo(list);
            return net_outcome(got, message, error);
        }
        if (got == OS_TIMED_OUT) {
            message = "timed out";
        }
    }
    freeaddrinfo(list);
    return net_error(error, message);
}

/* `$os.net_accept(fd, timeout_ms)`: 1 and the next connection, or 0 and a message. */
int32_t lugha_rt_net_accept(int64_t fd, int64_t timeout_ms, int64_t *conn, LughaString **error) {
    const char *message = NULL;
    OsSocket c = os_sock_accept(fd, net_deadline(timeout_ms), &message);
    if (c >= 0) {
        *conn = c;
    }
    return net_outcome(c, message, error);
}

/* `$os.net_read(fd, max, timeout_ms)`: 1 and up to `max` bytes (none at the
 * end of the stream), or 0 and a message. */
int32_t lugha_rt_net_read(int64_t fd, int64_t max, int64_t timeout_ms, LughaArray **out,
                          LughaString **error) {
    if (max < 1 || max > NET_MAX_READ) {
        return net_error(error, "read size must be from 1 to 16777216 bytes");
    }
    char *buffer = rt_scratch(max);
    const char *message = NULL;
    int64_t n = os_sock_recv(fd, buffer, max, net_deadline(timeout_ms), &message);
    if (n >= 0) {
        *out = bytes_of(buffer, n);
    }
    // Let go of before an unwind, which `net_outcome` starts when cancelled.
    rt_scratch_done(buffer);
    return net_outcome(n, message, error);
}

/* `$os.net_write(fd, bytes, timeout_ms)`: 1 once every byte is sent, or 0
 * and a message; a closed peer is an error, never SIGPIPE. */
int32_t lugha_rt_net_write(int64_t fd, const LughaArray *bytes, int64_t timeout_ms,
                           LughaString **error) {
    const char *message = NULL;
    int64_t sent =
        os_sock_send(fd, (const char *)bytes->elems, bytes->len, net_deadline(timeout_ms), &message);
    return net_outcome(sent, message, error);
}

/* `$os.net_close(fd)`: the tasks parked on it are woken with
 * `Err("closed")` first, since closing also takes it out of the poll and
 * no event would reach them; then it is closed. Under the runtime's lock,
 * so no task parks on it in between. */
void lugha_rt_net_close(int64_t fd) {
    RT_SECTION();
    for (int64_t i = 0; i < net_waits_len; i++) {
        if (net_waits[i].fd == fd) {
            wake_gen(net_waits[i].fiber, net_waits[i].gen, SOCKET_CLOSED);
        }
    }
    os_sock_close(fd);
}

/* `$os.net_name(fd, peer)`: 1 and "host:port" of this end (or the peer's),
 * "[address]:port" for IPv6; or 0 and a message. */
int32_t lugha_rt_net_name(int64_t fd, int64_t peer, LughaString **out, LughaString **error) {
    struct sockaddr_storage at;
    socklen_t len;
    const char *message = NULL;
    if (os_sock_name(fd, peer != 0, &at, &len, &message) != 0) {
        return net_error(error, message);
    }
    char host[INET6_ADDRSTRLEN];
    char text[INET6_ADDRSTRLEN + 16];
    int port;
    if (at.ss_family == AF_INET6) {
        struct sockaddr_in6 *v6 = (struct sockaddr_in6 *)&at;
        inet_ntop(AF_INET6, &v6->sin6_addr, host, sizeof host);
        port = ntohs(v6->sin6_port);
        snprintf(text, sizeof text, "[%s]:%d", host, port);
    } else {
        struct sockaddr_in *v4 = (struct sockaddr_in *)&at;
        inet_ntop(AF_INET, &v4->sin_addr, host, sizeof host);
        port = ntohs(v4->sin_port);
        snprintf(text, sizeof text, "%s:%d", host, port);
    }
    *out = string_of(text, (int64_t)strlen(text));
    return 1;
}
