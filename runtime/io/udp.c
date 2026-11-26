/*
 * udp.c — part of lugha_rt.c: `std.net`'s UDP sockets, through the OS
 * layer's operations as TCP's are (io/net.c); a wait parks only its green
 * thread. Multicast and network interfaces are the system's
 * (os/posix/udp_options.c, os/windows/net_options.c). Datagrams are
 * external input: never longer than the caller's `max`, never silently cut
 * short, and their sender is reported, not trusted.
 *
 * A fragment, not compiled on its own: joined after net.c (`net_error`,
 * `net_resolve`, `net_outcome`, `net_deadline`).
 */

enum { UDP_MAX = 65536 };

/* What `recv_from` hands back: a `(u8[], string)` tuple, field by field. */
typedef struct {
    LughaArray *bytes;
    LughaString *from;
} UdpDatagram;

/* `$os.udp_bind(addr, shared)`: a bound socket. `shared` sets
 * `SO_REUSEADDR` and `SO_REUSEPORT` first, so other sockets that also ask
 * may bind the same port (mDNS's 5353 beside Avahi). */
int32_t lugha_rt_udp_bind(const LughaString *addr, int64_t shared, int64_t *fd,
                          LughaString **error) {
    struct addrinfo *list = NULL;
    const char *problem = net_resolve(addr, AI_PASSIVE, &list);
    if (problem != NULL) {
        return net_error(error, problem);
    }
    const char *message = strerror(0);
    for (struct addrinfo *a = list; a != NULL; a = a->ai_next) {
        OsSocket s = os_sock_open(a->ai_family, SOCK_DGRAM, IPPROTO_UDP, &message);
        if (s >= 0 && os_sock_bind_udp(s, a->ai_addr, a->ai_addrlen, shared != 0, &message) == 0) {
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

/* `addr` as a socket address with no lookup, or NULL and a message naming it. */
static struct addrinfo *udp_target(const LughaString *addr, LughaString **error) {
    struct addrinfo *list = NULL;
    if (net_resolve(addr, AI_NUMERICHOST, &list) != NULL || list == NULL) {
        const char *prefix = "address must be an IP address with a port: ";
        size_t n = strlen(prefix);
        char *text = rt_scratch((int64_t)n + addr->len);
        memcpy(text, prefix, n);
        memcpy(text + n, addr->bytes, (size_t)addr->len);
        *error = string_of(text, (int64_t)n + addr->len);
        rt_scratch_done(text);
        return NULL;
    }
    return list;
}

/* `$os.udp_send(fd, bytes, addr, timeout_ms)`: one datagram to an IP address. */
int32_t lugha_rt_udp_send(int64_t fd, const LughaArray *bytes, const LughaString *addr,
                          int64_t timeout_ms, int64_t *done, LughaString **error) {
    struct addrinfo *to = udp_target(addr, error);
    if (to == NULL) {
        return 0;
    }
    const char *message = NULL;
    int64_t sent = os_sock_sendto(fd, (const char *)bytes->elems, bytes->len, to->ai_addr,
                                  to->ai_addrlen, net_deadline(timeout_ms), &message);
    // Let go of before an unwind, which `net_outcome` starts when cancelled.
    freeaddrinfo(to);
    *done = 0;
    return net_outcome(sent, message, error);
}

/* A socket address as "ip:port", "[ipv6%scope]:port" for IPv6. */
static LughaString *udp_name(const struct sockaddr *at, socklen_t len) {
    char host[NI_MAXHOST];
    char port[NI_MAXSERV];
    char text[NI_MAXHOST + NI_MAXSERV + 4];
    if (getnameinfo(at, len, host, sizeof host, port, sizeof port,
                    NI_NUMERICHOST | NI_NUMERICSERV) != 0) {
        return string_of("", 0);
    }
    os_scope_name(host, sizeof host);
    snprintf(text, sizeof text, at->sa_family == AF_INET6 ? "[%s]:%s" : "%s:%s", host, port);
    return string_of(text, (int64_t)strlen(text));
}

/* `$os.udp_recv(fd, max, timeout_ms)`: the next datagram and its sender. One
 * longer than `max` is an error, never cut short (the system gives its
 * real length). */
int32_t lugha_rt_udp_recv(int64_t fd, int64_t max, int64_t timeout_ms, UdpDatagram *out,
                          LughaString **error) {
    if (max < 1 || max > UDP_MAX) {
        return net_error(error, "max must be from 1 to 65536");
    }
    char *buffer = rt_scratch(max);
    struct sockaddr_storage from;
    socklen_t len;
    const char *message = NULL;
    int64_t n = os_sock_recvfrom(fd, buffer, max, &from, &len, net_deadline(timeout_ms), &message);
    if (n > max) {
        rt_scratch_done(buffer);
        char text[80];
        snprintf(text, sizeof text, "datagram of %" PRId64 " bytes is longer than max", n);
        return net_error(error, text);
    }
    if (n >= 0) {
        out->bytes = bytes_of(buffer, n);
        out->from = udp_name((struct sockaddr *)&from, len);
    }
    // Let go of before an unwind, which `net_outcome` starts when cancelled.
    rt_scratch_done(buffer);
    return net_outcome(n, message, error);
}
