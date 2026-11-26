/*
 * os/windows/sockets.c — part of lugha_rt.c on Windows: the socket
 * operations of the OS layer (os/layer.c), by completion. Every socket is
 * overlapped and tied to the completion port; an operation is started
 * (`WSARecv`, `WSASend`, `AcceptEx`, `ConnectEx`, `WSARecvFrom`,
 * `WSASendTo`) with a `WinIo` in the task's own frame, and the task parks
 * until the poll hands it the operation's packet. Every operation
 * completes through the port, one that finished at once included, so a
 * packet always comes, and comes once.
 *
 * A wait ended otherwise (its deadline, a cancelled scope, another task
 * closing the socket) cancels the operation (`CancelIoEx`) and still
 * waits, uncancellably, for its packet: only then does the task go on, so
 * Windows never writes into a frame or a buffer that has moved on. An
 * operation that finished before the cancel took hold keeps its result:
 * the bytes it moved count.
 *
 * Failures read as Linux's words (os/windows/net_options.c's
 * `win_sock_message`).
 *
 * A fragment, not compiled on its own: joined after io/net.c and io/udp.c
 * (the poll, the parked-socket list), which call these through the layer's
 * declarations, and after net_options.c.
 */

#ifndef SIO_UDP_CONNRESET
#define SIO_UDP_CONNRESET _WSAIOW(IOC_VENDOR, 12)
#endif

/* `OS_FAILED`, with Linux's words for the Winsock error `err`. */
static int64_t win_sock_failed(int err, const char **message) {
    *message = win_sock_message(err);
    return OS_FAILED;
}

/* The token a packet wakes the running task's next wait with: its fiber
 * in the low 48 bits (user space's), the wait's generation's low 16 above. */
static uint64_t win_io_token(void) {
    return (uint64_t)(uintptr_t)current | ((uint64_t)(next_wait_gen(current) & 0xffff) << 48);
}

/* Readies `io` for an operation the running task starts next: true, or
 * false with `OS_CANCELLED` in `*outcome` when the task is cancelled
 * already (nothing is started then). */
static bool win_io_begin(WinIo *io, int64_t *outcome) {
    ensure_started();
    if (current->cancelled != REASON_NONE && !current->unwinding) {
        *outcome = OS_CANCELLED;
        return false;
    }
    may_wait(NULL, 0);
    io_open();
    memset(io, 0, sizeof *io);
    io->packed = win_io_token();
    return true;
}

/* Whether a started operation failed at once: then no packet comes. */
static bool win_io_refused(int got) {
    return got != 0 && WSAGetLastError() != WSA_IO_PENDING;
}

/* `win_io_wait`'s answer when the operation was aborted though its task
 * never cancelled it: Windows' cancelling of another task's read or write
 * on the same socket can abort this one too. Nothing moved, so the task
 * starts it again. */
#define WIN_IO_AGAIN (-100)

/* `ov.Internal` of an operation aborted by a cancel. */
#define WIN_STATUS_CANCELLED 0xC0000120

/* Parks the running task until `io`'s operation on `s` (started) has its
 * packet: 0 when it finished (its result in `io`), `WIN_IO_AGAIN` when
 * someone else's cancel aborted it, else `OS_TIMED_OUT` (`deadline`
 * passed: monotonic ns, < 0 for none), `OS_CANCELLED` or `OS_CLOSED`, the
 * operation cancelled and its packet taken. */
static int64_t win_io_wait(SOCKET s, WinIo *io, int64_t deadline) {
    if (deadline >= 0) {
        add_sleeper(current, deadline);
    }
    net_waits_add((OsSocket)s, next_wait_gen(current), 0, false);
    __atomic_add_fetch(&net_waiting, 1, __ATOMIC_ACQ_REL);
    current->cancellable = true;
    int64_t why = block();
    current->cancellable = false;
    // The section is open again here (`switch_away`), so the list is safe.
    // One operation per wait here: none waits behind another to be woken.
    (void)net_waits_remove();
    // Whatever woke the task, a packet taken already means it finished;
    // else the next wait is the one the packet wakes.
    bool done = __atomic_exchange_n(&io->packed, win_io_token(), __ATOMIC_ACQ_REL) == WIN_IO_DONE;
    if (!done) {
        // Fails only once the operation is over: its packet comes either way.
        CancelIoEx((HANDLE)s, &io->ov);
        do {
            block();
        } while (__atomic_exchange_n(&io->packed, win_io_token(), __ATOMIC_ACQ_REL) !=
                 WIN_IO_DONE);
    }
    __atomic_sub_fetch(&net_waiting, 1, __ATOMIC_ACQ_REL);
    if (done && io->ov.Internal == WIN_STATUS_CANCELLED && io->bytes == 0) {
        return WIN_IO_AGAIN;
    }
    if (done || io->ov.Internal == 0) {
        return 0;
    }
    if (why == SOCKET_CLOSED) {
        return OS_CLOSED;
    }
    if (why == CANCELLED) {
        return OS_CANCELLED;
    }
    return OS_TIMED_OUT;
}

/* A finished operation's count, or `OS_FAILED` with its error. */
static int64_t win_io_result(SOCKET s, WinIo *io, const char **message) {
    if (io->ov.Internal == 0) {
        return io->bytes;
    }
    DWORD bytes = 0;
    DWORD flags = 0;
    if (WSAGetOverlappedResult(s, &io->ov, &bytes, FALSE, &flags)) {
        return bytes;
    }
    return win_sock_failed(WSAGetLastError(), message);
}

/* A new overlapped socket, not inherited by programs started later, tied
 * to the completion port; a UDP one ignores "port unreachable", as Linux's
 * does. -1 with `*err`. */
static SOCKET win_sock_new(int family, int type, int protocol, int *err) {
    SOCKET s = WSASocketW(family, type, protocol, NULL, 0,
                          WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT);
    if (s == INVALID_SOCKET) {
        *err = WSAGetLastError();
        return INVALID_SOCKET;
    }
    os_poll_open();
    if (CreateIoCompletionPort((HANDLE)s, os_port, OS_TOKEN_IO, 0) == NULL) {
        *err = (int)GetLastError();
        closesocket(s);
        return INVALID_SOCKET;
    }
    if (type == SOCK_DGRAM) {
        BOOL off = FALSE;
        DWORD got = 0;
        // Refused only by a provider without it, which never resets either.
        (void)WSAIoctl(s, SIO_UDP_CONNRESET, &off, sizeof off, NULL, 0, &got, NULL, NULL);
    }
    return s;
}

static OsSocket os_sock_open(int family, int type, int protocol, const char **message) {
    int err = 0;
    SOCKET s = win_sock_new(family, type, protocol, &err);
    return s == INVALID_SOCKET ? win_sock_failed(err, message) : (OsSocket)s;
}

/* `SO_EXCLUSIVEADDRUSE`, not `SO_REUSEADDR`: Windows' `SO_REUSEADDR` would
 * let another program bind the port and take its connections, and a port
 * in `TIME_WAIT` can be bound again without it. */
static int64_t os_sock_listen(OsSocket s, const struct sockaddr *at, socklen_t len, int backlog,
                              const char **message) {
    BOOL one = TRUE;
    if (setsockopt((SOCKET)s, SOL_SOCKET, SO_EXCLUSIVEADDRUSE, (const char *)&one, sizeof one) ==
            0 &&
        bind((SOCKET)s, at, len) == 0 && listen((SOCKET)s, backlog) == 0) {
        return 0;
    }
    return win_sock_failed(WSAGetLastError(), message);
}

/* `shared` sets `SO_REUSEADDR`, which on Windows lets other sockets that
 * also ask bind the same port: what `SO_REUSEPORT` adds on POSIX. */
static int64_t os_sock_bind_udp(OsSocket s, const struct sockaddr *at, socklen_t len, bool shared,
                                const char **message) {
    BOOL one = TRUE;
    if ((!shared || setsockopt((SOCKET)s, SOL_SOCKET, SO_REUSEADDR, (const char *)&one,
                               sizeof one) == 0) &&
        bind((SOCKET)s, at, len) == 0) {
        return 0;
    }
    return win_sock_failed(WSAGetLastError(), message);
}

/* Winsock's extension `id` for `s`, or NULL. */
static void *win_sock_extension(SOCKET s, GUID id) {
    void *fn = NULL;
    DWORD got = 0;
    if (WSAIoctl(s, SIO_GET_EXTENSION_FUNCTION_POINTER, &id, sizeof id, &fn, sizeof fn, &got, NULL,
                 NULL) != 0) {
        return NULL;
    }
    return fn;
}

/* `ConnectEx` wants its socket bound: to the family's any-address, any port. */
static int64_t os_sock_connect(OsSocket s, const struct sockaddr *at, socklen_t len,
                               int64_t deadline, const char **message) {
    RT_SECTION();
    struct sockaddr_storage any;
    memset(&any, 0, sizeof any);
    any.ss_family = at->sa_family;
    int any_len = at->sa_family == AF_INET6 ? (int)sizeof(struct sockaddr_in6)
                                            : (int)sizeof(struct sockaddr_in);
    if (bind((SOCKET)s, (struct sockaddr *)&any, any_len) != 0) {
        return win_sock_failed(WSAGetLastError(), message);
    }
    LPFN_CONNECTEX connect_ex = (LPFN_CONNECTEX)win_sock_extension((SOCKET)s,
                                                                   (GUID)WSAID_CONNECTEX);
    if (connect_ex == NULL) {
        return win_sock_failed(WSAGetLastError(), message);
    }
    WinIo io;
    int64_t outcome;
    if (!win_io_begin(&io, &outcome)) {
        return outcome;
    }
    int got = connect_ex((SOCKET)s, at, (int)len, NULL, 0, NULL, &io.ov) ? 0 : SOCKET_ERROR;
    if (win_io_refused(got)) {
        return win_sock_failed(WSAGetLastError(), message);
    }
    int64_t waited = win_io_wait((SOCKET)s, &io, deadline);
    if (waited == WIN_IO_AGAIN) {
        // Only this task uses a socket it is connecting: never expected.
        return win_sock_failed(WSA_OPERATION_ABORTED, message);
    }
    if (waited != 0) {
        return waited;
    }
    int64_t done = win_io_result((SOCKET)s, &io, message);
    if (done < 0) {
        return *message == win_sock_message(WSAETIMEDOUT) ? OS_TIMED_OUT : done;
    }
    // Without it the socket has no peer for `getpeername` and `shutdown`.
    setsockopt((SOCKET)s, SOL_SOCKET, SO_UPDATE_CONNECT_CONTEXT, NULL, 0);
    return 0;
}

/* The listener's family, or AF_INET. */
static int win_sock_family(SOCKET s) {
    struct sockaddr_storage at;
    int len = sizeof at;
    return getsockname(s, (struct sockaddr *)&at, &len) == 0 ? at.ss_family : AF_INET;
}

static OsSocket os_sock_accept(OsSocket s, int64_t deadline, const char **message) {
    RT_SECTION();
    LPFN_ACCEPTEX accept_ex = (LPFN_ACCEPTEX)win_sock_extension((SOCKET)s, (GUID)WSAID_ACCEPTEX);
    if (accept_ex == NULL) {
        return win_sock_failed(WSAGetLastError(), message);
    }
    int family = win_sock_family((SOCKET)s);
    enum { ADDR = sizeof(struct sockaddr_storage) + 16 };
    for (;;) {
        int err = 0;
        SOCKET c = win_sock_new(family, SOCK_STREAM, IPPROTO_TCP, &err);
        if (c == INVALID_SOCKET) {
            return win_sock_failed(err, message);
        }
        char addresses[2 * ADDR];
        WinIo io;
        int64_t outcome;
        if (!win_io_begin(&io, &outcome)) {
            closesocket(c);
            return outcome;
        }
        DWORD unused = 0;
        int got = accept_ex((SOCKET)s, c, addresses, 0, ADDR, ADDR, &unused, &io.ov) ? 0
                                                                                     : SOCKET_ERROR;
        if (win_io_refused(got)) {
            closesocket(c);
            return win_sock_failed(WSAGetLastError(), message);
        }
        int64_t waited = win_io_wait((SOCKET)s, &io, deadline);
        if (waited != 0) {
            closesocket(c);
            if (waited == WIN_IO_AGAIN) {
                continue;
            }
            return waited;
        }
        int64_t done = win_io_result((SOCKET)s, &io, message);
        if (done >= 0) {
            SOCKET listener = (SOCKET)s;
            // Gives the connection its addresses, as `accept` would.
            setsockopt(c, SOL_SOCKET, SO_UPDATE_ACCEPT_CONTEXT, (const char *)&listener,
                       sizeof listener);
            return (OsSocket)c;
        }
        closesocket(c);
        // A connection reset before it was taken: the next one, as POSIX does.
        if (*message != win_sock_message(WSAECONNRESET)) {
            return done;
        }
    }
}

/* At most this much per send: a `WSABUF` holds a 32-bit length. */
enum { WIN_SEND_MOST = 1 << 30 };

static int64_t os_sock_recv(OsSocket s, char *buf, int64_t max, int64_t deadline,
                            const char **message) {
    RT_SECTION();
    for (;;) {
        WinIo io;
        int64_t outcome;
        if (!win_io_begin(&io, &outcome)) {
            return outcome;
        }
        WSABUF part = {.len = (ULONG)max, .buf = buf};
        DWORD flags = 0;
        if (win_io_refused(WSARecv((SOCKET)s, &part, 1, NULL, &flags, &io.ov, NULL))) {
            return win_sock_failed(WSAGetLastError(), message);
        }
        int64_t waited = win_io_wait((SOCKET)s, &io, deadline);
        if (waited != WIN_IO_AGAIN) {
            return waited != 0 ? waited : win_io_result((SOCKET)s, &io, message);
        }
    }
}

/* Every byte, or the first failure: what was sent before it stays sent. */
static int64_t os_sock_send(OsSocket s, const char *buf, int64_t n, int64_t deadline,
                            const char **message) {
    RT_SECTION();
    int64_t sent = 0;
    while (sent < n) {
        WinIo io;
        int64_t outcome;
        if (!win_io_begin(&io, &outcome)) {
            return outcome;
        }
        int64_t left = n - sent;
        WSABUF part = {.len = (ULONG)(left < WIN_SEND_MOST ? left : WIN_SEND_MOST),
                       .buf = (char *)buf + sent};
        if (win_io_refused(WSASend((SOCKET)s, &part, 1, NULL, 0, &io.ov, NULL))) {
            return win_sock_failed(WSAGetLastError(), message);
        }
        int64_t waited = win_io_wait((SOCKET)s, &io, deadline);
        if (waited == WIN_IO_AGAIN) {
            continue;
        }
        if (waited != 0) {
            return waited;
        }
        int64_t got = win_io_result((SOCKET)s, &io, message);
        if (got < 0) {
            return got;
        }
        sent += got;
    }
    return sent;
}

/* Where a datagram longer than the caller's buffer goes on, so its real
 * length comes back: as os/posix/sockets.c's `sock_overflow`. Only
 * Windows writes it and nothing reads it, so operations sharing it lose
 * nothing. */
static char sock_overflow[65536];

/* The datagram's length: its real one when longer than `max`, so the
 * caller can refuse it rather than cut it short. */
static int64_t os_sock_recvfrom(OsSocket s, char *buf, int64_t max, struct sockaddr_storage *from,
                                socklen_t *len, int64_t deadline, const char **message) {
    RT_SECTION();
    WinIo io;
    INT from_len;
    int64_t waited;
    do {
        int64_t outcome;
        if (!win_io_begin(&io, &outcome)) {
            return outcome;
        }
        WSABUF parts[2] = {{.len = (ULONG)max, .buf = buf},
                           {.len = sizeof sock_overflow, .buf = sock_overflow}};
        DWORD flags = 0;
        from_len = sizeof *from;
        if (win_io_refused(WSARecvFrom((SOCKET)s, parts, 2, NULL, &flags,
                                       (struct sockaddr *)from, &from_len, &io.ov, NULL))) {
            return win_sock_failed(WSAGetLastError(), message);
        }
        waited = win_io_wait((SOCKET)s, &io, deadline);
    } while (waited == WIN_IO_AGAIN);
    if (waited != 0) {
        return waited;
    }
    int64_t got = win_io_result((SOCKET)s, &io, message);
    if (got < 0 && *message == win_sock_message(WSAEMSGSIZE)) {
        // Longer than both: UDP can't carry one, so never cut it short.
        *message = "datagram is longer than 65536 bytes";
    }
    *len = from_len;
    return got;
}

static int64_t os_sock_sendto(OsSocket s, const char *buf, int64_t n, const struct sockaddr *to,
                              socklen_t len, int64_t deadline, const char **message) {
    RT_SECTION();
    for (;;) {
        WinIo io;
        int64_t outcome;
        if (!win_io_begin(&io, &outcome)) {
            return outcome;
        }
        WSABUF part = {.len = (ULONG)n, .buf = (char *)buf};
        if (win_io_refused(WSASendTo((SOCKET)s, &part, 1, NULL, 0, to, (int)len, &io.ov, NULL))) {
            return win_sock_failed(WSAGetLastError(), message);
        }
        int64_t waited = win_io_wait((SOCKET)s, &io, deadline);
        if (waited != WIN_IO_AGAIN) {
            return waited != 0 ? waited : win_io_result((SOCKET)s, &io, message);
        }
    }
}

static int64_t os_sock_name(OsSocket s, bool peer, struct sockaddr_storage *at, socklen_t *len,
                            const char **message) {
    int size = sizeof *at;
    int got = peer ? getpeername((SOCKET)s, (struct sockaddr *)at, &size)
                   : getsockname((SOCKET)s, (struct sockaddr *)at, &size);
    *len = size;
    return got == 0 ? 0 : win_sock_failed(WSAGetLastError(), message);
}

/* Aborts the socket's operations: each waiting task still takes its
 * packet before it goes on (`win_io_wait`). */
static void os_sock_close(OsSocket s) {
    closesocket((SOCKET)s);
}
