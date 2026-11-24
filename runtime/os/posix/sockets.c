/*
 * os/posix/sockets.c — part of lugha_rt.c: the socket operations of the OS
 * layer (os/layer.c) for Linux and macOS. Each is the system call, and when
 * it would block (`EAGAIN`), a wait for the socket to be ready (`wait_fd`)
 * and the call again: readiness is POSIX's model, private to this file.
 * Sockets are non-blocking and close-on-exec from the start (the system's
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

/* Parks this thread until `fd` is ready for `want` (OS_READ, OS_WRITE):
 * 1, or 0 when `deadline` (monotonic ns; < 0 for none) passes first, or
 * WAIT_CANCELLED. */
static int wait_fd(int fd, uint32_t want, int64_t deadline) {
    RT_SECTION();
    ensure_started();
    if (current->cancelled != REASON_NONE && !current->unwinding) {
        return WAIT_CANCELLED;
    }
    may_wait(NULL, 0);
    io_open();
    // The fiber's address in the low 48 bits (user space's), and the low 16
    // bits of the wait's generation above it.
    uint64_t packed = (uint64_t)(uintptr_t)current |
                      ((uint64_t)(next_wait_gen(current) & 0xffff) << 48);
    os_poll_arm(fd, want, packed);
    if (deadline >= 0) {
        add_sleeper(current, deadline);
    }
    net_waits_add(fd, next_wait_gen(current));
    __atomic_add_fetch(&net_waiting, 1, __ATOMIC_ACQ_REL);
    current->cancellable = true;
    int64_t why = block();
    current->cancellable = false;
    __atomic_sub_fetch(&net_waiting, 1, __ATOMIC_ACQ_REL);
    // The section is open again here (`switch_away`), so the list is safe.
    net_waits_remove();
    if (why == SOCKET_CLOSED) {
        // Closed, so already out of the `epoll` set.
        return WAIT_CLOSED;
    }
    if (why == TIMED_OUT || why == CANCELLED) {
        /* Disarmed, so a late event can't wake this thread again. */
        os_poll_disarm(fd);
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
