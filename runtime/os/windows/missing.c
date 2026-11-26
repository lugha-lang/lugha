/*
 * os/windows/missing.c — part of lugha_rt.c on Windows: what isn't built
 * for Windows yet, failing closed. Every start of a
 * program `Err("processes aren't supported on Windows yet")`, and every
 * socket operation and option `Err("sockets aren't supported on Windows
 * yet")` (the layer's `os_sock_*` too), so a
 * program never reaches files, programs or the network some other way.
 * Process lockdown isn't applied, and says so.
 *
 * A fragment, not compiled on its own: joined last, after every part it
 * stands in for would have been.
 */

static const char WIN_NO_PROCS[] = "processes aren't supported on Windows yet";
static const char WIN_NO_SOCKETS[] = "sockets aren't supported on Windows yet";

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

int32_t lugha_rt_udp_group(int64_t fd, const LughaString *group, const LughaString *iface,
                           int64_t join, int64_t *done, LughaString **error) {
    (void)fd;
    (void)group;
    (void)iface;
    (void)join;
    (void)done;
    return win_refuse(error, WIN_NO_SOCKETS);
}

int32_t lugha_rt_udp_option(int64_t fd, int64_t which, int64_t value, int64_t *done,
                            LughaString **error) {
    (void)fd;
    (void)which;
    (void)value;
    (void)done;
    return win_refuse(error, WIN_NO_SOCKETS);
}

int32_t lugha_rt_udp_multicast_if(int64_t fd, const LughaString *iface, int64_t *done,
                                  LughaString **error) {
    (void)fd;
    (void)iface;
    (void)done;
    return win_refuse(error, WIN_NO_SOCKETS);
}

int32_t lugha_rt_net_interfaces(LughaString **out, LughaString **error) {
    (void)out;
    return win_refuse(error, WIN_NO_SOCKETS);
}

/* Sockets: none on Windows yet. Every operation fails with the same
 * words, so nothing reaches the network. */
static const char *const OS_NO_SOCKETS = WIN_NO_SOCKETS;

static OsSocket os_sock_open(int family, int type, int protocol, const char **message) {
    (void)family;
    (void)type;
    (void)protocol;
    *message = OS_NO_SOCKETS;
    return OS_FAILED;
}

static int64_t os_sock_listen(OsSocket s, const struct sockaddr *at, socklen_t len, int backlog,
                              const char **message) {
    (void)s;
    (void)at;
    (void)len;
    (void)backlog;
    *message = OS_NO_SOCKETS;
    return OS_FAILED;
}

static int64_t os_sock_bind_udp(OsSocket s, const struct sockaddr *at, socklen_t len, bool shared,
                                const char **message) {
    (void)s;
    (void)at;
    (void)len;
    (void)shared;
    *message = OS_NO_SOCKETS;
    return OS_FAILED;
}

static int64_t os_sock_connect(OsSocket s, const struct sockaddr *at, socklen_t len,
                               int64_t deadline, const char **message) {
    (void)s;
    (void)at;
    (void)len;
    (void)deadline;
    *message = OS_NO_SOCKETS;
    return OS_FAILED;
}

static OsSocket os_sock_accept(OsSocket s, int64_t deadline, const char **message) {
    (void)s;
    (void)deadline;
    *message = OS_NO_SOCKETS;
    return OS_FAILED;
}

static int64_t os_sock_recv(OsSocket s, char *buf, int64_t max, int64_t deadline,
                            const char **message) {
    (void)s;
    (void)buf;
    (void)max;
    (void)deadline;
    *message = OS_NO_SOCKETS;
    return OS_FAILED;
}

static int64_t os_sock_send(OsSocket s, const char *buf, int64_t n, int64_t deadline,
                            const char **message) {
    (void)s;
    (void)buf;
    (void)n;
    (void)deadline;
    *message = OS_NO_SOCKETS;
    return OS_FAILED;
}

static int64_t os_sock_recvfrom(OsSocket s, char *buf, int64_t max, struct sockaddr_storage *from,
                                socklen_t *len, int64_t deadline, const char **message) {
    (void)s;
    (void)buf;
    (void)max;
    (void)from;
    (void)len;
    (void)deadline;
    *message = OS_NO_SOCKETS;
    return OS_FAILED;
}

static int64_t os_sock_sendto(OsSocket s, const char *buf, int64_t n, const struct sockaddr *to,
                              socklen_t len, int64_t deadline, const char **message) {
    (void)s;
    (void)buf;
    (void)n;
    (void)to;
    (void)len;
    (void)deadline;
    *message = OS_NO_SOCKETS;
    return OS_FAILED;
}

static int64_t os_sock_name(OsSocket s, bool peer, struct sockaddr_storage *at, socklen_t *len,
                            const char **message) {
    (void)s;
    (void)peer;
    (void)at;
    (void)len;
    *message = OS_NO_SOCKETS;
    return OS_FAILED;
}

static void os_sock_close(OsSocket s) {
    (void)s;
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
