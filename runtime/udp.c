/*
 * udp.c — part of lugha_rt.c: `std.net`'s UDP sockets, multicast and
 * network interfaces. Sockets are non-blocking and close-on-exec, and a
 * wait parks only its green thread (`wait_fd`), as TCP's do. Datagrams are
 * external input: never longer than the caller's `max`, never silently cut
 * short, and their sender is reported, not trusted.
 *
 * A fragment, not compiled on its own: joined after net.c (`net_error`,
 * `net_resolve`, `wait_fd`, `net_deadline`).
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
    int saved = 0;
    for (struct addrinfo *a = list; a != NULL; a = a->ai_next) {
        int s = socket(a->ai_family, SOCK_DGRAM | SOCK_NONBLOCK | SOCK_CLOEXEC, IPPROTO_UDP);
        int one = 1;
        int ready = s >= 0 && (!shared || (setsockopt(s, SOL_SOCKET, SO_REUSEADDR, &one,
                                                      sizeof one) == 0 &&
                                           setsockopt(s, SOL_SOCKET, SO_REUSEPORT, &one,
                                                      sizeof one) == 0));
        // Linux hands a socket bound to a wildcard address every group any
        // socket on the machine joined; off, it hears only the groups it
        // joined itself, so `leave` means what it says. IPv6's switch is
        // newer (Linux 4.20), so its absence isn't an error.
        int zero = 0;
        ready = ready && (a->ai_family != AF_INET ||
                          setsockopt(s, IPPROTO_IP, IP_MULTICAST_ALL, &zero, sizeof zero) == 0);
#ifdef IPV6_MULTICAST_ALL
        if (ready && a->ai_family == AF_INET6) {
            (void)setsockopt(s, IPPROTO_IPV6, IPV6_MULTICAST_ALL, &zero, sizeof zero);
        }
#endif
        if (ready && bind(s, a->ai_addr, a->ai_addrlen) == 0) {
            freeaddrinfo(list);
            *fd = s;
            return 1;
        }
        saved = errno;
        if (s >= 0) {
            close(s);
        }
    }
    freeaddrinfo(list);
    return net_error(error, strerror(saved));
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
    int64_t deadline = net_deadline(timeout_ms);
    for (;;) {
        ssize_t n = sendto((int)fd, bytes->elems, (size_t)bytes->len, MSG_NOSIGNAL, to->ai_addr,
                           to->ai_addrlen);
        if (n >= 0) {
            freeaddrinfo(to);
            *done = 0;
            return 1;
        }
        if (errno == EAGAIN || errno == EWOULDBLOCK) {
            if (!wait_fd((int)fd, EPOLLOUT, deadline)) {
                freeaddrinfo(to);
                return net_error(error, "timed out");
            }
        } else if (errno != EINTR) {
            int saved = errno;
            freeaddrinfo(to);
            return net_error(error, strerror(saved));
        }
    }
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
    snprintf(text, sizeof text, at->sa_family == AF_INET6 ? "[%s]:%s" : "%s:%s", host, port);
    return string_of(text, (int64_t)strlen(text));
}

/* `$os.udp_recv(fd, max, timeout_ms)`: the next datagram and its sender. One
 * longer than `max` is an error (`MSG_TRUNC` gives its real length). */
int32_t lugha_rt_udp_recv(int64_t fd, int64_t max, int64_t timeout_ms, UdpDatagram *out,
                          LughaString **error) {
    if (max < 1 || max > UDP_MAX) {
        return net_error(error, "max must be from 1 to 65536");
    }
    int64_t deadline = net_deadline(timeout_ms);
    char *buffer = rt_scratch(max);
    for (;;) {
        struct sockaddr_storage from;
        socklen_t len = sizeof from;
        ssize_t n = recvfrom((int)fd, buffer, (size_t)max, MSG_TRUNC, (struct sockaddr *)&from,
                             &len);
        if (n > max) {
            rt_scratch_done(buffer);
            char text[80];
            snprintf(text, sizeof text, "datagram of %zd bytes is longer than max", n);
            return net_error(error, text);
        }
        if (n >= 0) {
            out->bytes = bytes_of(buffer, (int64_t)n);
            out->from = udp_name((struct sockaddr *)&from, len);
            rt_scratch_done(buffer);
            return 1;
        }
        if (errno == EAGAIN || errno == EWOULDBLOCK) {
            if (!wait_fd((int)fd, EPOLLIN, deadline)) {
                rt_scratch_done(buffer);
                return net_error(error, "timed out");
            }
        } else if (errno != EINTR) {
            int saved = errno;
            rt_scratch_done(buffer);
            return net_error(error, strerror(saved));
        }
    }
}

/* The socket's address family, or -1. */
static int udp_family(int fd) {
    struct sockaddr_storage at;
    socklen_t len = sizeof at;
    return getsockname(fd, (struct sockaddr *)&at, &len) == 0 ? at.ss_family : -1;
}

/* An interface's index: 0 for `""` (the system's choice), -1 for no such one. */
static int udp_interface(const LughaString *name) {
    if (name->len == 0) {
        return 0;
    }
    if (name->len >= IF_NAMESIZE || memchr(name->bytes, 0, (size_t)name->len) != NULL) {
        return -1;
    }
    char text[IF_NAMESIZE];
    memcpy(text, name->bytes, (size_t)name->len);
    text[name->len] = 0;
    unsigned index = if_nametoindex(text);
    return index == 0 ? -1 : (int)index;
}

/* `$os.udp_group(fd, group, interface, join)`: joins (1) or leaves (0) a
 * multicast group on an interface; the group's family must be the socket's. */
int32_t lugha_rt_udp_group(int64_t fd, const LughaString *group, const LughaString *iface,
                           int64_t join, int64_t *done, LughaString **error) {
    char text[INET6_ADDRSTRLEN];
    if (group->len >= (int64_t)sizeof text || memchr(group->bytes, 0, (size_t)group->len)) {
        return net_error(error, "group must be an IP address");
    }
    memcpy(text, group->bytes, (size_t)group->len);
    text[group->len] = 0;
    int index = udp_interface(iface);
    if (index < 0) {
        return net_error(error, "no such interface");
    }
    int family = udp_family((int)fd);
    struct in_addr v4;
    struct in6_addr v6;
    int r;
    if (inet_pton(AF_INET, text, &v4) == 1) {
        if (!IN_MULTICAST(ntohl(v4.s_addr))) {
            return net_error(error, "not a multicast group");
        }
        if (family != AF_INET) {
            return net_error(error, "an IPv4 group needs a socket bound to an IPv4 address");
        }
        struct ip_mreqn m = {.imr_multiaddr = v4, .imr_ifindex = index};
        r = setsockopt((int)fd, IPPROTO_IP, join ? IP_ADD_MEMBERSHIP : IP_DROP_MEMBERSHIP, &m,
                       sizeof m);
    } else if (inet_pton(AF_INET6, text, &v6) == 1) {
        if (!IN6_IS_ADDR_MULTICAST(&v6)) {
            return net_error(error, "not a multicast group");
        }
        if (family != AF_INET6) {
            return net_error(error, "an IPv6 group needs a socket bound to an IPv6 address");
        }
        struct ipv6_mreq m = {.ipv6mr_multiaddr = v6, .ipv6mr_interface = (unsigned)index};
        r = setsockopt((int)fd, IPPROTO_IPV6, join ? IPV6_JOIN_GROUP : IPV6_LEAVE_GROUP, &m,
                       sizeof m);
    } else {
        return net_error(error, "group must be an IP address");
    }
    if (r != 0) {
        return net_error(error, strerror(errno));
    }
    *done = 0;
    return 1;
}

/* `$os.udp_option(fd, which, value)`: 0 sets the multicast TTL (IPv6's hop
 * limit), 1 whether multicast sent here loops back to this machine. */
int32_t lugha_rt_udp_option(int64_t fd, int64_t which, int64_t value, int64_t *done,
                            LughaString **error) {
    if (which == 0 && (value < 1 || value > 255)) {
        return net_error(error, "TTL must be from 1 to 255");
    }
    int v = (int)value;
    int six = udp_family((int)fd) == AF_INET6;
    int level = six ? IPPROTO_IPV6 : IPPROTO_IP;
    int name = which == 0 ? (six ? IPV6_MULTICAST_HOPS : IP_MULTICAST_TTL)
                          : (six ? IPV6_MULTICAST_LOOP : IP_MULTICAST_LOOP);
    if (setsockopt((int)fd, level, name, &v, sizeof v) != 0) {
        return net_error(error, strerror(errno));
    }
    *done = 0;
    return 1;
}

/* `$os.udp_multicast_if(fd, interface)`: the interface multicast leaves by. */
int32_t lugha_rt_udp_multicast_if(int64_t fd, const LughaString *iface, int64_t *done,
                                  LughaString **error) {
    int index = udp_interface(iface);
    if (index < 0) {
        return net_error(error, "no such interface");
    }
    int r;
    if (udp_family((int)fd) == AF_INET6) {
        r = setsockopt((int)fd, IPPROTO_IPV6, IPV6_MULTICAST_IF, &index, sizeof index);
    } else {
        struct ip_mreqn m = {.imr_ifindex = index};
        r = setsockopt((int)fd, IPPROTO_IP, IP_MULTICAST_IF, &m, sizeof m);
    }
    if (r != 0) {
        return net_error(error, strerror(errno));
    }
    *done = 0;
    return 1;
}

/* `$os.net_interfaces()`: one line per interface and address,
 * "name\tindex\tup\tmulticast\taddress" (the address empty for an
 * interface's link-layer entry); `std.net` groups them by name. */
int32_t lugha_rt_net_interfaces(LughaString **out, LughaString **error) {
    struct ifaddrs *list = NULL;
    if (getifaddrs(&list) != 0) {
        return net_error(error, strerror(errno));
    }
    size_t cap = 1024, len = 0;
    char *text = rt_scratch((int64_t)cap);
    for (struct ifaddrs *i = list; i != NULL; i = i->ifa_next) {
        char host[NI_MAXHOST] = "";
        int family = i->ifa_addr == NULL ? -1 : i->ifa_addr->sa_family;
        if (family == AF_INET || family == AF_INET6) {
            socklen_t size = family == AF_INET ? sizeof(struct sockaddr_in)
                                               : sizeof(struct sockaddr_in6);
            if (getnameinfo(i->ifa_addr, size, host, sizeof host, NULL, 0, NI_NUMERICHOST) != 0) {
                host[0] = 0;
            }
        }
        char line[IF_NAMESIZE + NI_MAXHOST + 40];
        int n = snprintf(line, sizeof line, "%s\t%u\t%d\t%d\t%s\n", i->ifa_name,
                         if_nametoindex(i->ifa_name), (i->ifa_flags & IFF_UP) != 0,
                         (i->ifa_flags & IFF_MULTICAST) != 0, host);
        if (n < 0 || (size_t)n >= sizeof line) {
            continue;
        }
        if (len + (size_t)n > cap) {
            cap = (len + (size_t)n) * 2;
            char *bigger = rt_scratch((int64_t)cap);
            memcpy(bigger, text, len);
            rt_scratch_done(text);
            text = bigger;
        }
        memcpy(text + len, line, (size_t)n);
        len += (size_t)n;
    }
    freeifaddrs(list);
    *out = string_of(text, (int64_t)len);
    rt_scratch_done(text);
    return 1;
}
