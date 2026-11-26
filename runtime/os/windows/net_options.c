/*
 * os/windows/net_options.c — part of lugha_rt.c on Windows: what sockets
 * say and set beside their operations. Winsock's errors and the resolver's
 * in Linux's words, so programs and tests see one set of messages (Windows'
 * own are localised); `std.net`'s multicast groups and options; and the
 * list of network interfaces, each named as Windows' `if_nametoindex`
 * takes it (`loopback_0`, `ethernet_32768`), with link-local addresses
 * carrying that name after `%`, as on Linux.
 *
 * A fragment, not compiled on its own: joined after io/udp.c (`net_error`)
 * and before os/windows/sockets.c, which uses `win_sock_message`.
 */

/* Linux's words for a Winsock error (glibc's `strerror`), or for any
 * other code "Input/output error". The same pointer for the same error. */
static const char *win_sock_message(int err) {
    switch (err) {
    case WSAECONNREFUSED:
    case ERROR_CONNECTION_REFUSED:
        return "Connection refused";
    case WSAECONNRESET:
    case ERROR_NETNAME_DELETED:
        return "Connection reset by peer";
    case WSAECONNABORTED:
    case ERROR_CONNECTION_ABORTED:
        return "Software caused connection abort";
    case WSAEADDRINUSE:
        return "Address already in use";
    case WSAEADDRNOTAVAIL:
        return "Cannot assign requested address";
    case WSAENETUNREACH:
    case ERROR_NETWORK_UNREACHABLE:
        return "Network is unreachable";
    case WSAEHOSTUNREACH:
    case ERROR_HOST_UNREACHABLE:
        return "No route to host";
    case WSAENETDOWN:
        return "Network is down";
    case WSAEHOSTDOWN:
        return "Host is down";
    case WSAENETRESET:
        return "Network dropped connection on reset";
    case WSAETIMEDOUT:
    case ERROR_SEM_TIMEOUT:
        return "Connection timed out";
    case WSAEACCES:
        return "Permission denied";
    case WSAEMSGSIZE:
        return "Message too long";
    case WSAENOTCONN:
        return "Transport endpoint is not connected";
    case WSAESHUTDOWN:
        return "Cannot send after transport endpoint shutdown";
    case WSAEISCONN:
        return "Transport endpoint is already connected";
    case WSAEAFNOSUPPORT:
        return "Address family not supported by protocol";
    case WSAEPROTONOSUPPORT:
        return "Protocol not supported";
    case WSAEOPNOTSUPP:
        return "Operation not supported";
    case WSAENOTSOCK:
        return "Socket operation on non-socket";
    case WSAEDESTADDRREQ:
        return "Destination address required";
    case WSAENOBUFS:
        return "No buffer space available";
    case WSAEMFILE:
        return "Too many open files";
    case WSAEINVAL:
        return "Invalid argument";
    case WSAENOPROTOOPT:
        return "Protocol not available";
    case WSAEPROTOTYPE:
        return "Protocol wrong type for socket";
    default:
        return "Input/output error";
    }
}

/* A name lookup's failure in glibc's words: Windows' `gai_strerror` gives
 * its own, localised. */
static const char *os_lookup_message(int code) {
    switch (code) {
    case EAI_NONAME:
        return "Name or service not known";
    case EAI_AGAIN:
        return "Temporary failure in name resolution";
    case EAI_FAIL:
        return "Non-recoverable failure in name resolution";
    case WSANO_DATA:
        return "No address associated with hostname";
    case EAI_FAMILY:
        return "ai_family not supported";
    case EAI_SOCKTYPE:
        return "ai_socktype not supported";
    case EAI_SERVICE:
        return "Servname not supported for ai_socktype";
    case EAI_MEMORY:
        return "Memory allocation failure";
    case EAI_BADFLAGS:
        return "Bad value for ai_flags";
    default:
        return "Unknown error";
    }
}

/* `%12` after a link-local address turned into `%loopback_0`, in place. */
static void os_scope_name(char *host, size_t cap) {
    char *mark = strchr(host, '%');
    if (mark == NULL || mark[1] < '0' || mark[1] > '9') {
        return;
    }
    char name[IF_NAMESIZE];
    unsigned long index = strtoul(mark + 1, NULL, 10);
    if (index == 0 || index > UINT32_MAX || if_indextoname((NET_IFINDEX)index, name) == NULL) {
        return;
    }
    size_t at = (size_t)(mark - host) + 1;
    if (at + strlen(name) < cap) {
        memcpy(host + at, name, strlen(name) + 1);
    }
}

/* `host` with a name after `%` turned into its index, for `getaddrinfo`,
 * which takes only numbers there on Windows. */
static bool os_scope_numeric(const char *host, size_t len, char *out, size_t cap) {
    const char *mark = memchr(host, '%', len);
    if (mark == NULL || len >= cap) {
        return false;
    }
    size_t at = (size_t)(mark - host);
    size_t name_len = len - at - 1;
    if (name_len == 0 || name_len >= IF_NAMESIZE || (mark[1] >= '0' && mark[1] <= '9')) {
        return false;
    }
    char name[IF_NAMESIZE];
    memcpy(name, mark + 1, name_len);
    name[name_len] = 0;
    NET_IFINDEX index = if_nametoindex(name);
    if (index == 0) {
        return false;
    }
    memcpy(out, host, at);
    snprintf(out + at, cap - at, "%%%lu", (unsigned long)index);
    return true;
}

/* The socket's address family, or -1. */
static int udp_family(int64_t fd) {
    struct sockaddr_storage at;
    int len = sizeof at;
    return getsockname((SOCKET)fd, (struct sockaddr *)&at, &len) == 0 ? at.ss_family : -1;
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
    NET_IFINDEX index = if_nametoindex(text);
    return index == 0 ? -1 : (int)index;
}

/* 1 and `*done` 0 when `r` is 0, else 0 and the last error's words. */
static int32_t udp_set(int r, int64_t *done, LughaString **error) {
    if (r != 0) {
        return net_error(error, win_sock_message(WSAGetLastError()));
    }
    *done = 0;
    return 1;
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
    int family = udp_family(fd);
    // The classic requests, the interface by index (IPv4's as the address
    // `0.0.0.n`): what every Windows takes, where `MCAST_JOIN_GROUP` isn't.
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
        struct ip_mreq m = {.imr_multiaddr = v4};
        m.imr_interface.s_addr = htonl((u_long)index);
        r = setsockopt((SOCKET)fd, IPPROTO_IP, join ? IP_ADD_MEMBERSHIP : IP_DROP_MEMBERSHIP,
                       (const char *)&m, sizeof m);
    } else if (inet_pton(AF_INET6, text, &v6) == 1) {
        if (!IN6_IS_ADDR_MULTICAST(&v6)) {
            return net_error(error, "not a multicast group");
        }
        if (family != AF_INET6) {
            return net_error(error, "an IPv6 group needs a socket bound to an IPv6 address");
        }
        struct ipv6_mreq m = {.ipv6mr_multiaddr = v6, .ipv6mr_interface = (ULONG)index};
        r = setsockopt((SOCKET)fd, IPPROTO_IPV6, join ? IPV6_ADD_MEMBERSHIP : IPV6_DROP_MEMBERSHIP,
                       (const char *)&m, sizeof m);
    } else {
        return net_error(error, "group must be an IP address");
    }
    return udp_set(r, done, error);
}

/* `$os.udp_option(fd, which, value)`: 0 sets the multicast TTL (IPv6's hop
 * limit), 1 whether multicast sent here loops back to this machine. Each
 * a `DWORD` on Windows. */
int32_t lugha_rt_udp_option(int64_t fd, int64_t which, int64_t value, int64_t *done,
                            LughaString **error) {
    if (which == 0 && (value < 1 || value > 255)) {
        return net_error(error, "TTL must be from 1 to 255");
    }
    DWORD v = (DWORD)value;
    bool six = udp_family(fd) == AF_INET6;
    int level = six ? IPPROTO_IPV6 : IPPROTO_IP;
    int name = which == 0 ? (six ? IPV6_MULTICAST_HOPS : IP_MULTICAST_TTL)
                          : (six ? IPV6_MULTICAST_LOOP : IP_MULTICAST_LOOP);
    return udp_set(setsockopt((SOCKET)fd, level, name, (const char *)&v, sizeof v), done, error);
}

/* `$os.udp_multicast_if(fd, interface)`: the interface multicast leaves by.
 * IPv4 takes an index as the address `0.0.0.n`, in network order. */
int32_t lugha_rt_udp_multicast_if(int64_t fd, const LughaString *iface, int64_t *done,
                                  LughaString **error) {
    int index = udp_interface(iface);
    if (index < 0) {
        return net_error(error, "no such interface");
    }
    DWORD v = udp_family(fd) == AF_INET6 ? (DWORD)index : htonl((DWORD)index);
    int level = udp_family(fd) == AF_INET6 ? IPPROTO_IPV6 : IPPROTO_IP;
    int name = level == IPPROTO_IPV6 ? IPV6_MULTICAST_IF : IP_MULTICAST_IF;
    return udp_set(setsockopt((SOCKET)fd, level, name, (const char *)&v, sizeof v), done, error);
}

/* One `name\tindex\tup\tmulticast\taddress` line onto `*text`. */
static void win_interface_line(char **text, size_t *len, size_t *cap, const char *line, size_t n) {
    if (*len + n > *cap) {
        *cap = (*len + n) * 2;
        char *bigger = rt_scratch((int64_t)*cap);
        memcpy(bigger, *text, *len);
        rt_scratch_done(*text);
        *text = bigger;
    }
    memcpy(*text + *len, line, n);
    *len += n;
}

/* `$os.net_interfaces()`: one line per interface and address, as on POSIX:
 * each interface once with no address (POSIX's link-layer entry), then
 * once per IPv4 and IPv6 address. */
int32_t lugha_rt_net_interfaces(LughaString **out, LughaString **error) {
    ULONG flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    ULONG size = 16 * 1024;
    IP_ADAPTER_ADDRESSES *list = NULL;
    ULONG got = ERROR_BUFFER_OVERFLOW;
    // The list may grow between asking its size and reading it.
    for (int tries = 0; got == ERROR_BUFFER_OVERFLOW && tries < 4; tries++) {
        free(list);
        list = malloc(size);
        if (list == NULL) {
            die("panic: out of memory\n");
        }
        got = GetAdaptersAddresses(AF_UNSPEC, flags, NULL, list, &size);
    }
    if (got != NO_ERROR) {
        free(list);
        return net_error(error, win_sock_message((int)got));
    }
    size_t cap = 1024, len = 0;
    char *text = rt_scratch((int64_t)cap);
    for (IP_ADAPTER_ADDRESSES *a = list; a != NULL; a = a->Next) {
        char name[IF_NAMESIZE];
        if (ConvertInterfaceLuidToNameA(&a->Luid, name, sizeof name) != NO_ERROR) {
            continue;
        }
        unsigned index = a->IfIndex != 0 ? a->IfIndex : a->Ipv6IfIndex;
        int up = a->OperStatus == IfOperStatusUp;
        int multicast = (a->Flags & IP_ADAPTER_NO_MULTICAST) == 0;
        char line[IF_NAMESIZE + NI_MAXHOST + 40];
        int n = snprintf(line, sizeof line, "%s\t%u\t%d\t%d\t\n", name, index, up, multicast);
        if (n > 0 && (size_t)n < sizeof line) {
            win_interface_line(&text, &len, &cap, line, (size_t)n);
        }
        for (IP_ADAPTER_UNICAST_ADDRESS *u = a->FirstUnicastAddress; u != NULL; u = u->Next) {
            char host[NI_MAXHOST];
            if (getnameinfo(u->Address.lpSockaddr, u->Address.iSockaddrLength, host, sizeof host,
                            NULL, 0, NI_NUMERICHOST) != 0) {
                continue;
            }
            os_scope_name(host, sizeof host);
            n = snprintf(line, sizeof line, "%s\t%u\t%d\t%d\t%s\n", name, index, up, multicast,
                         host);
            if (n > 0 && (size_t)n < sizeof line) {
                win_interface_line(&text, &len, &cap, line, (size_t)n);
            }
        }
    }
    free(list);
    *out = string_of(text, (int64_t)len);
    rt_scratch_done(text);
    return 1;
}
