/*
 * os/posix/udp_options.c — part of lugha_rt.c: `std.net`'s multicast
 * groups and options and its list of network interfaces, as Linux and
 * macOS set and read them (setsockopt, getifaddrs). Windows will have its
 * own.
 *
 * A fragment, not compiled on its own: joined after io/udp.c (`net_error`).
 */

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
    // One request for both families, the interface by index: the same on
    // Linux and macOS.
    struct group_req req;
    memset(&req, 0, sizeof req);
    req.gr_interface = (uint32_t)index;
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
        struct sockaddr_in *at = (struct sockaddr_in *)&req.gr_group;
        at->sin_family = AF_INET;
        at->sin_addr = v4;
        os_sockaddr_len((struct sockaddr *)at, sizeof *at);
        r = setsockopt((int)fd, IPPROTO_IP, join ? MCAST_JOIN_GROUP : MCAST_LEAVE_GROUP, &req,
                       sizeof req);
    } else if (inet_pton(AF_INET6, text, &v6) == 1) {
        if (!IN6_IS_ADDR_MULTICAST(&v6)) {
            return net_error(error, "not a multicast group");
        }
        if (family != AF_INET6) {
            return net_error(error, "an IPv6 group needs a socket bound to an IPv6 address");
        }
        struct sockaddr_in6 *at = (struct sockaddr_in6 *)&req.gr_group;
        at->sin6_family = AF_INET6;
        at->sin6_addr = v6;
        os_sockaddr_len((struct sockaddr *)at, sizeof *at);
        r = setsockopt((int)fd, IPPROTO_IPV6, join ? MCAST_JOIN_GROUP : MCAST_LEAVE_GROUP, &req,
                       sizeof req);
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
