/*
 * resolve.c — part of lugha_rt.c: `Net`'s name lookup, and telling public
 * addresses from private ones for `net.only`.
 * The lookup runs on a helper thread, as `net_connect`'s does (net.c's
 * `net_lookup`). Every failure comes back as 0 and the resolver's message.
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
    struct addrinfo *list = NULL;
    int code = net_lookup(name->bytes, (size_t)name->len, NULL, numeric ? AI_NUMERICHOST : 0,
                          &list);
    if (code != 0) {
        return net_error(error, os_lookup_message(code));
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
           || (a[0] == 192 && a[1] == 0 && a[2] == 2)  /* 192.0.2/24, documentation */
           || (a[0] == 198 && a[1] == 51 && a[2] == 100) /* 198.51.100/24, documentation */
           || (a[0] == 203 && a[1] == 0 && a[2] == 113)  /* 203.0.113/24, documentation */
           || (a[0] == 192 && a[1] == 88 && a[2] == 99)  /* 192.88.99/24, old 6to4 relays */
           || a[0] >= 224;                             /* multicast, reserved, broadcast */
}

/* Whether the IPv6 address `a` (sixteen bytes) is in a range that never
 * reaches the public internet, or tunnels somewhere a relay chooses:
 * unique local, link-local, old site-local, multicast, local-use NAT64,
 * discard, Teredo, benchmarking, ORCHID and documentation. */
static bool net_ipv6_private(const unsigned char *a) {
    static const unsigned char nat64_local[6] = {0x00, 0x64, 0xFF, 0x9B, 0x00, 0x01};
    static const unsigned char discard[8] = {0x01, 0x00, 0, 0, 0, 0, 0, 0};
    unsigned word1 = (unsigned)a[2] << 8 | a[3];
    bool in_2001 = a[0] == 0x20 && a[1] == 0x01;
    return (a[0] & 0xFE) == 0xFC                       /* fc00::/7 */
           || (a[0] == 0xFE && (a[1] & 0x80) == 0x80)   /* fe80::/10, fec0::/10 */
           || a[0] == 0xFF                              /* ff00::/8 */
           || memcmp(a, nat64_local, 6) == 0            /* 64:ff9b:1::/48 */
           || memcmp(a, discard, 8) == 0                /* 100::/64 */
           || (in_2001 && word1 == 0)                   /* 2001::/32, Teredo */
           || (in_2001 && word1 == 2 && a[4] == 0 && a[5] == 0) /* 2001:2::/48 */
           || (in_2001 && (word1 & 0xFFF0) == 0x10)     /* 2001:10::/28, ORCHID */
           || (in_2001 && (word1 & 0xFFF0) == 0x20)     /* 2001:20::/28, ORCHIDv2 */
           || (in_2001 && word1 == 0x0DB8)              /* 2001:db8::/32 */
           || (a[0] == 0x3F && a[1] == 0xFF && (a[2] & 0xF0) == 0); /* 3fff::/20 */
}

/* `$os.net_private(ip)`: 1 when `ip` is not a public unicast address (or
 * isn't an IP address at all), 0 when it is. An IPv4 address carried in an
 * IPv6 one by a fixed rule is judged as that IPv4 address: mapped
 * `::ffff:a.b.c.d`, NAT64's `64:ff9b::a.b.c.d` and 6to4's `2002:AABB:CCDD::`.
 * Every other `::/96` address (`::`, `::1`, the old compatible form) and
 * the ranges of `net_ipv6_private` are not public. No lookup, no socket. */
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
    static const unsigned char nat64[12] = {0x00, 0x64, 0xFF, 0x9B};
    bool high_zero = memcmp(a, zero, 10) == 0;
    if (high_zero && a[10] == 0xFF && a[11] == 0xFF) {
        return net_ipv4_private(a + 12); /* ::ffff:a.b.c.d */
    }
    if (high_zero && a[10] == 0 && a[11] == 0) {
        /* `::`, `::1`, and the old compatible form `::a.b.c.d`. */
        return 1;
    }
    if (memcmp(a, nat64, 12) == 0) {
        return net_ipv4_private(a + 12); /* 64:ff9b::a.b.c.d */
    }
    if (a[0] == 0x20 && a[1] == 0x02) {
        return net_ipv4_private(a + 2); /* 2002:AABB:CCDD::/48 */
    }
    return net_ipv6_private(a);
}
