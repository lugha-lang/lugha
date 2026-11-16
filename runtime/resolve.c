/*
 * resolve.c — part of lugha_rt.c: `Net`'s name lookup, and telling public
 * addresses from private ones for `net.only`.
 * `getaddrinfo` blocks the OS thread while it runs, as `net_connect`'s
 * lookup does. Every failure comes back as 0 and the resolver's message.
 *
 * A fragment, not compiled on its own: joined after net.c (`net_error`).
 */

/* `$os.net_resolve(name, numeric)`: every address `name` resolves to, as
 * text joined by newlines, in the resolver's order (`std.net` drops
 * repeats), or 0 and the resolver's reason. `numeric` allows IP literals
 * only (`AI_NUMERICHOST`: no lookup). */
int32_t lugha_rt_net_resolve(const LughaString *name, int64_t numeric, LughaString **out,
                             LughaString **error) {
    if (name->len == 0) {
        return net_error(error, "no name to resolve");
    }
    if (memchr(name->bytes, 0, (size_t)name->len) != NULL) {
        return net_error(error, "name contains a NUL byte");
    }
    char *host = rt_scratch(name->len + 1);
    memcpy(host, name->bytes, (size_t)name->len);
    host[name->len] = 0;
    // One socket type, so each address comes once per family, not per protocol.
    struct addrinfo hints = {.ai_family = AF_UNSPEC, .ai_socktype = SOCK_STREAM};
    hints.ai_flags = numeric ? AI_NUMERICHOST : 0;
    struct addrinfo *list = NULL;
    int code = getaddrinfo(host, NULL, &hints, &list);
    rt_scratch_done(host);
    if (code != 0) {
        return net_error(error, gai_strerror(code));
    }
    size_t cap = 64, len = 0;
    char *text = rt_scratch((int64_t)cap);
    for (struct addrinfo *a = list; a != NULL; a = a->ai_next) {
        char one[INET6_ADDRSTRLEN];
        const void *at = a->ai_family == AF_INET6
                             ? (const void *)&((struct sockaddr_in6 *)a->ai_addr)->sin6_addr
                             : (const void *)&((struct sockaddr_in *)a->ai_addr)->sin_addr;
        if ((a->ai_family != AF_INET && a->ai_family != AF_INET6) ||
            inet_ntop(a->ai_family, at, one, sizeof one) == NULL) {
            continue;
        }
        size_t n = strlen(one);
        if (len + n + 2 > cap) {
            cap = (len + n + 2) * 2;
            char *bigger = rt_scratch((int64_t)cap);
            memcpy(bigger, text, len);
            rt_scratch_done(text);
            text = bigger;
        }
        if (len > 0) {
            text[len++] = '\n';
        }
        memcpy(text + len, one, n);
        len += n;
    }
    freeaddrinfo(list);
    if (len == 0) {
        rt_scratch_done(text);
        return net_error(error, "no address for the name");
    }
    *out = string_of(text, (int64_t)len);
    rt_scratch_done(text);
    return 1;
}

/* Whether the IPv4 address `a` (four bytes, network order) is anything but
 * a public unicast address: unspecified, private, shared, loopback,
 * link-local, protocol-assigned, benchmarking, multicast, reserved or
 * broadcast. */
static bool net_ipv4_private(const unsigned char *a) {
    return a[0] == 0                                   /* 0.0.0.0/8 */
           || a[0] == 10                               /* 10/8 */
           || (a[0] == 100 && (a[1] & 0xC0) == 64)     /* 100.64/10 */
           || a[0] == 127                              /* 127/8 */
           || (a[0] == 169 && a[1] == 254)             /* 169.254/16 */
           || (a[0] == 172 && (a[1] & 0xF0) == 16)     /* 172.16/12 */
           || (a[0] == 192 && a[1] == 0 && a[2] == 0)  /* 192.0.0/24 */
           || (a[0] == 192 && a[1] == 168)             /* 192.168/16 */
           || (a[0] == 198 && (a[1] & 0xFE) == 18)     /* 198.18/15 */
           || a[0] >= 224;                             /* multicast, reserved, broadcast */
}

/* `$os.net_private(ip)`: 1 when `ip` is not a public unicast address (or
 * isn't an IP address at all), 0 when it is. IPv6's `::`, `::1`, unique
 * local, link-local and multicast addresses are not public, and an IPv4
 * address in an IPv6 form (mapped `::ffff:a.b.c.d`, or compatible
 * `::a.b.c.d`) is judged as that IPv4 address. No lookup, no socket. */
int64_t lugha_rt_net_private(const LughaString *ip) {
    if (ip->len == 0 || ip->len > 64 || memchr(ip->bytes, 0, (size_t)ip->len) != NULL) {
        return 1;
    }
    unsigned char a[16];
    if (inet_pton(AF_INET, ip->bytes, a) == 1) {
        return net_ipv4_private(a);
    }
    if (inet_pton(AF_INET6, ip->bytes, a) != 1) {
        return 1;
    }
    static const unsigned char zero[16];
    bool high_zero = memcmp(a, zero, 10) == 0;
    if (high_zero && a[10] == 0xFF && a[11] == 0xFF) {
        return net_ipv4_private(a + 12); /* ::ffff:a.b.c.d */
    }
    if (high_zero && a[10] == 0 && a[11] == 0) {
        /* `::`, `::1`, and the old compatible form `::a.b.c.d`. */
        return 1;
    }
    return (a[0] & 0xFE) == 0xFC                 /* fc00::/7 */
           || (a[0] == 0xFE && (a[1] & 0xC0) == 0x80) /* fe80::/10 */
           || a[0] == 0xFF;                      /* ff00::/8 */
}
