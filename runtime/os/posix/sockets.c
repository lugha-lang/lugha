/*
 * os/posix/sockets.c — part of lugha_rt.c: the socket operations of the OS
 * layer (os/layer.c) for Linux and macOS. Each is the system call, and when
 * it would block (`EAGAIN`), a wait for the socket to be ready (`wait_fd`)
 * and the call again: readiness is POSIX's model, private to this file.
 * A reader and a writer wait on one socket at once; more tasks waiting
 * the same way take turns. Sockets are non-blocking and close-on-exec from the start (the system's
 * `os_sock_raw`, `os_accept_raw`). A cancelled wait leaves nothing
 * outstanding, so `OS_CANCELLED` comes back at once.
 *
 * A fragment, not compiled on its own: joined after io/net.c and io/udp.c
 * (the poll, the parked-socket list), which call these through the layer's
 * declarations.
 */

/* `wait_fd`'s answer when the waiting task is cancelled: the caller lets
 * go of what it holds, then unwinds (`cancel_unwind`). */
#define WAIT_CANCELLED (-1)

/* `wait_fd`'s answer when another task closed the socket: the caller
 * gives `Err("closed")` and never touches the descriptor again, whose
 * number may already name another file. */
#define WAIT_CLOSED (-2)

/* Who waits in the poll on each descriptor, each way (0 to read, 1 to
 * write): one task per way holds it, so a reader and a writer wait at
 * once; any more waiting the same way queue behind the holder (the wait
 * list, net.c), first come first served, and the next is woken to try
 * again whenever the holder's wait ends. The descriptor is armed for the
 * ways held. Under the runtime's lock, as the poll's wake-ups are. */
typedef struct {
    Fiber *fiber;
    uint32_t gen;
} NetHolder;
typedef struct {
    NetHolder way[2];
} NetFd;
static NetFd *net_fds;
static int64_t net_fds_cap;

/* `fd`'s entry, the table grown to hold it. */
static NetFd *net_fd(int fd) {
    if (fd >= net_fds_cap) {
        int64_t cap = net_fds_cap == 0 ? 64 : net_fds_cap;
        while (cap <= fd) {
            cap *= 2;
        }
        NetFd *bigger = realloc(net_fds, (size_t)cap * sizeof *net_fds);
        if (bigger == NULL) {
            die("panic: out of memory\n");
        }
        memset(bigger + net_fds_cap, 0, (size_t)(cap - net_fds_cap) * sizeof *bigger);
        net_fds = bigger;
        net_fds_cap = cap;
    }
    return &net_fds[fd];
}

/* `fd` armed for the ways someone holds. */
static void net_fd_arm(int fd) {
    NetFd *e = net_fd(fd);
    uint32_t want = (e->way[0].fiber != NULL ? OS_READ : 0) |
                    (e->way[1].fiber != NULL ? OS_WRITE : 0);
    if (want != 0) {
        os_poll_arm(fd, want);
    } else {
        os_poll_disarm(fd);
    }
}

/* The first task queued on `fd` way `dir`, not yet woken, woken to try
 * again: it holds the way next, unless its try succeeds. */
static void net_fd_next(int fd, uint8_t dir) {
    NetWait *first = NULL;
    for (int64_t i = 0; i < net_waits_len; i++) {
        NetWait *w = &net_waits[i];
        if (w->fd == fd && w->dir == dir && w->queued && !w->woken &&
            (first == NULL || w->seq < first->seq)) {
            first = w;
        }
    }
    if (first != NULL) {
        first->woken = true;
        wake_gen(first->fiber, first->gen, 0);
    }
}

/* `fd` closed: nobody holds it any more, so a socket given its number
 * next starts afresh. Those who did are woken by the close. */
static void net_fd_closed(int fd) {
    if (fd < net_fds_cap) {
        net_fds[fd] = (NetFd){0};
    }
}

static void net_fd_ready(int fd, uint32_t ready) {
    if (fd >= net_fds_cap) {
        return;
    }
    NetFd *e = &net_fds[fd];
    bool woke = false;
    for (int dir = 0; dir < 2; dir++) {
        NetHolder *h = &e->way[dir];
        if (h->fiber != NULL && (ready & (dir == 0 ? OS_READ : OS_WRITE)) != 0) {
            wake_gen(h->fiber, h->gen, 0);
            *h = (NetHolder){0};
            woke = true;
        }
    }
    // One shot fired for the whole descriptor on Linux: the way still held
    // is armed again.
    if (woke && (e->way[0].fiber != NULL || e->way[1].fiber != NULL)) {
        net_fd_arm(fd);
    }
}

/* Parks this thread until `fd` is ready for `want` (OS_READ or OS_WRITE),
 * or, behind another task waiting the same way, until its turn to try
 * again: 1, or 0 when `deadline` (monotonic ns; < 0 for none) passes
 * first, or WAIT_CANCELLED, or WAIT_CLOSED. */
static int wait_fd(int fd, uint32_t want, int64_t deadline) {
    RT_SECTION();
    ensure_started();
    if (current->cancelled != REASON_NONE && !current->unwinding) {
        return WAIT_CANCELLED;
    }
    may_wait(NULL, 0);
    io_open();
    uint8_t dir = want == OS_READ ? 0 : 1;
    uint32_t gen = next_wait_gen(current);
    NetHolder *slot = &net_fd(fd)->way[dir];
    bool holds = slot->fiber == NULL;
    if (holds) {
        *slot = (NetHolder){.fiber = current, .gen = gen};
        net_fd_arm(fd);
    }
    if (deadline >= 0) {
        add_sleeper(current, deadline);
    }
    net_waits_add(fd, gen, dir, !holds);
    __atomic_add_fetch(&net_waiting, 1, __ATOMIC_ACQ_REL);
    current->cancellable = true;
    int64_t why = block();
    current->cancellable = false;
    __atomic_sub_fetch(&net_waiting, 1, __ATOMIC_ACQ_REL);
    // The section is open again here (`switch_away`), so the lists are safe.
    bool woken = net_waits_remove();
    if (why == SOCKET_CLOSED) {
        // Closed, so out of the poll and its entry emptied (`net_fd_closed`);
        // everyone waiting on it is woken too.
        return WAIT_CLOSED;
    }
    // The table may have grown meanwhile: the entry is looked up again.
    NetHolder *mine = &net_fd(fd)->way[dir];
    if (holds && mine->fiber == current && mine->gen == gen) {
        // Ended otherwise than by the poll: armed for the other way only, so
        // a late event can't wake this thread again.
        *mine = (NetHolder){0};
        net_fd_arm(fd);
    }
    // Its turn over, or given up: the next waiting this way tries.
    if (holds || woken) {
        net_fd_next(fd, dir);
    }
    if (why == TIMED_OUT || why == CANCELLED) {
        return why == CANCELLED ? WAIT_CANCELLED : 0;
    }
    return 1;
}

/* `wait_fd`'s answer as the layer's: `OS_CLOSED`, `OS_CANCELLED`,
 * `OS_TIMED_OUT`, or 0 when the socket is ready. */
static int64_t posix_wait(int s, uint32_t want, int64_t deadline) {
    int ready = wait_fd(s, want, deadline);
    if (ready == WAIT_CLOSED) {
        return OS_CLOSED;
    }
    if (ready == WAIT_CANCELLED) {
        return OS_CANCELLED;
    }
    return ready ? 0 : OS_TIMED_OUT;
}

/* `OS_FAILED`, with the system's words for `err`. */
static int64_t posix_failed(int err, const char **message) {
    *message = strerror(err);
    return OS_FAILED;
}

static OsSocket os_sock_open(int family, int type, int protocol, const char **message) {
    int s = os_sock_raw(family, type, protocol);
    return s >= 0 ? s : posix_failed(errno, message);
}

static int64_t os_sock_listen(OsSocket s, const struct sockaddr *at, socklen_t len, int backlog,
                              const char **message) {
    int one = 1;
    if (setsockopt((int)s, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one) == 0 &&
        bind((int)s, at, len) == 0 && listen((int)s, backlog) == 0) {
        return 0;
    }
    return posix_failed(errno, message);
}

/* `shared` sets `SO_REUSEADDR` and `SO_REUSEPORT` first, so other sockets
 * that also ask may bind the same port (mDNS's 5353 beside Avahi). */
static int64_t os_sock_bind_udp(OsSocket s, const struct sockaddr *at, socklen_t len, bool shared,
                                const char **message) {
    int one = 1;
    bool ready =
        !shared || (setsockopt((int)s, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one) == 0 &&
                    setsockopt((int)s, SOL_SOCKET, SO_REUSEPORT, &one, sizeof one) == 0);
    if (ready && os_udp_own_groups((int)s, at->sa_family) && bind((int)s, at, len) == 0) {
        return 0;
    }
    return posix_failed(errno, message);
}

static int64_t os_sock_connect(OsSocket s, const struct sockaddr *at, socklen_t len,
                               int64_t deadline, const char **message) {
    int err = connect((int)s, at, len) == 0 ? 0 : errno;
    if (err == EINPROGRESS) {
        int64_t waited = posix_wait((int)s, OS_WRITE, deadline);
        if (waited == OS_CLOSED || waited == OS_CANCELLED) {
            return waited;
        }
        socklen_t size = sizeof err;
        if (waited == OS_TIMED_OUT) {
            err = ETIMEDOUT;
        } else if (getsockopt((int)s, SOL_SOCKET, SO_ERROR, &err, &size) != 0) {
            err = errno;
        }
    }
    if (err == 0) {
        return 0;
    }
    return err == ETIMEDOUT ? OS_TIMED_OUT : posix_failed(err, message);
}

static OsSocket os_sock_accept(OsSocket s, int64_t deadline, const char **message) {
    for (;;) {
        int c = os_accept_raw((int)s);
        if (c >= 0) {
            return c;
        }
        if (errno == EAGAIN || errno == EWOULDBLOCK) {
            int64_t waited = posix_wait((int)s, OS_READ, deadline);
            if (waited != 0) {
                return waited;
            }
        } else if (errno != EINTR && errno != ECONNABORTED) {
            return posix_failed(errno, message);
        }
    }
}

static int64_t os_sock_recv(OsSocket s, char *buf, int64_t max, int64_t deadline,
                            const char **message) {
    for (;;) {
        ssize_t n = recv((int)s, buf, (size_t)max, 0);
        if (n >= 0) {
            return n;
        }
        if (errno == EAGAIN || errno == EWOULDBLOCK) {
            int64_t waited = posix_wait((int)s, OS_READ, deadline);
            if (waited != 0) {
                return waited;
            }
        } else if (errno != EINTR) {
            return posix_failed(errno, message);
        }
    }
}

/* Every byte, or the first failure: what was sent before it stays sent. */
static int64_t os_sock_send(OsSocket s, const char *buf, int64_t n, int64_t deadline,
                            const char **message) {
    int64_t sent = 0;
    while (sent < n) {
        ssize_t got = send((int)s, buf + sent, (size_t)(n - sent), OS_SEND_FLAGS);
        if (got >= 0) {
            sent += got;
        } else if (errno == EAGAIN || errno == EWOULDBLOCK) {
            int64_t waited = posix_wait((int)s, OS_WRITE, deadline);
            if (waited != 0) {
                return waited;
            }
        } else if (errno != EINTR) {
            return posix_failed(errno, message);
        }
    }
    return sent;
}

/* Where a datagram longer than the caller's buffer goes on: a UDP
 * payload is at most 65,527 bytes, so with this behind the buffer the
 * whole datagram always lands and its real length comes back. One for
 * the process, not per thread: only the kernel writes it and nothing ever
 * reads it, so threads sharing it lose nothing, and no thread pays 64 KiB
 * of thread-local space for it. */
static char sock_overflow[65536];

/* The datagram's length: its real one when longer than `max`, so the
 * caller can refuse it rather than cut it short. One `recvmsg` into the
 * caller's buffer and `sock_overflow` behind it, so the length is the
 * datagram's own, on every system, with no second call to race. */
static int64_t os_sock_recvfrom(OsSocket s, char *buf, int64_t max, struct sockaddr_storage *from,
                                socklen_t *len, int64_t deadline, const char **message) {
    for (;;) {
        struct iovec parts[2] = {{.iov_base = buf, .iov_len = (size_t)max},
                                 {.iov_base = sock_overflow, .iov_len = sizeof sock_overflow}};
        struct msghdr m = {.msg_name = from, .msg_namelen = sizeof *from, .msg_iov = parts,
                           .msg_iovlen = 2};
        ssize_t n = recvmsg((int)s, &m, 0);
        if (n >= 0) {
            *len = m.msg_namelen;
            if ((m.msg_flags & MSG_TRUNC) != 0) {
                // Longer than both: UDP can't carry one, so never cut it short.
                *message = "datagram is longer than 65536 bytes";
                return OS_FAILED;
            }
            return n;
        }
        if (errno == EAGAIN || errno == EWOULDBLOCK) {
            int64_t waited = posix_wait((int)s, OS_READ, deadline);
            if (waited != 0) {
                return waited;
            }
        } else if (errno != EINTR) {
            return posix_failed(errno, message);
        }
    }
}

static int64_t os_sock_sendto(OsSocket s, const char *buf, int64_t n, const struct sockaddr *to,
                              socklen_t len, int64_t deadline, const char **message) {
    for (;;) {
        ssize_t sent = sendto((int)s, buf, (size_t)n, OS_SEND_FLAGS, to, len);
        if (sent >= 0) {
            return sent;
        }
        if (errno == EAGAIN || errno == EWOULDBLOCK) {
            int64_t waited = posix_wait((int)s, OS_WRITE, deadline);
            if (waited != 0) {
                return waited;
            }
        } else if (errno != EINTR) {
            return posix_failed(errno, message);
        }
    }
}

static int64_t os_sock_name(OsSocket s, bool peer, struct sockaddr_storage *at, socklen_t *len,
                            const char **message) {
    *len = sizeof *at;
    int got = peer ? getpeername((int)s, (struct sockaddr *)at, len)
                   : getsockname((int)s, (struct sockaddr *)at, len);
    return got == 0 ? 0 : posix_failed(errno, message);
}

static void os_sock_close(OsSocket s) {
    close((int)s);
}
