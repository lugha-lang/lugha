/*
 * trust.c — part of lugha_rt.c on macOS: whether a TLS server's
 * certificates are trusted, asked of the system's own verifier
 * (`SecTrustEvaluateWithError`), which reads the keychain: the roots a
 * user or a company added, and the ones Apple distrusts.
 *
 * The Security and CoreFoundation frameworks are loaded by absolute path
 * the first time, not linked: a program is built on another system, which
 * has no Mac frameworks to link against. Their types are opaque pointers
 * here for the same reason. Nothing here knows OpenSSL: certificates come
 * in as DER bytes.
 *
 * A fragment, not compiled on its own: joined after os.c.
 */
#include <dlfcn.h>

/* A TLS client asks this file, not OpenSSL's roots, whether to trust a
 * server (tls.c). */
#define OS_TLS_TRUST 1

/* `os_tls_trust`'s answers besides 0 (trusted). */
enum {
    OS_TRUST_UNAVAILABLE = -1,
    OS_TRUST_HOST = 1,
    OS_TRUST_EXPIRED,
    OS_TRUST_NOT_YET_VALID,
    OS_TRUST_REVOKED,
    OS_TRUST_NO_ISSUER,
    OS_TRUST_REFUSED,
};

typedef const void *CfRef;

/* The entry points used, filled once by `trust_load`. */
static struct {
    CfRef (*data_create)(CfRef, const uint8_t *, long);
    CfRef (*array_create)(CfRef, const void **, long, const void *);
    CfRef (*string_create)(CfRef, const uint8_t *, long, uint32_t, uint8_t);
    void (*release)(CfRef);
    long (*error_code)(CfRef);
    const void *array_callbacks;
    CfRef (*certificate_create)(CfRef, CfRef);
    CfRef (*policy_ssl)(uint8_t, CfRef);
    int32_t (*trust_create)(CfRef, CfRef, CfRef *);
    int32_t (*set_anchors)(CfRef, CfRef);
    int32_t (*set_anchors_only)(CfRef, uint8_t);
    bool (*evaluate)(CfRef, CfRef *);
    bool loaded;
} trust_fns;

static pthread_once_t trust_once = PTHREAD_ONCE_INIT;

/* Fills `trust_fns`; `loaded` stays false when either framework or any
 * entry point is missing. */
static void trust_load(void) {
    void *cf = dlopen("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation",
                      RTLD_NOW | RTLD_LOCAL);
    void *sec = dlopen("/System/Library/Frameworks/Security.framework/Security",
                       RTLD_NOW | RTLD_LOCAL);
    if (cf == NULL || sec == NULL) {
        return;
    }
    /* Object pointers to functions: what `dlsym` gives on every POSIX system. */
    *(void **)&trust_fns.data_create = dlsym(cf, "CFDataCreate");
    *(void **)&trust_fns.array_create = dlsym(cf, "CFArrayCreate");
    *(void **)&trust_fns.string_create = dlsym(cf, "CFStringCreateWithBytes");
    *(void **)&trust_fns.release = dlsym(cf, "CFRelease");
    *(void **)&trust_fns.error_code = dlsym(cf, "CFErrorGetCode");
    trust_fns.array_callbacks = dlsym(cf, "kCFTypeArrayCallBacks");
    *(void **)&trust_fns.certificate_create = dlsym(sec, "SecCertificateCreateWithData");
    *(void **)&trust_fns.policy_ssl = dlsym(sec, "SecPolicyCreateSSL");
    *(void **)&trust_fns.trust_create = dlsym(sec, "SecTrustCreateWithCertificates");
    *(void **)&trust_fns.set_anchors = dlsym(sec, "SecTrustSetAnchorCertificates");
    *(void **)&trust_fns.set_anchors_only = dlsym(sec, "SecTrustSetAnchorCertificatesOnly");
    *(void **)&trust_fns.evaluate = dlsym(sec, "SecTrustEvaluateWithError");
    trust_fns.loaded = trust_fns.data_create && trust_fns.array_create &&
                       trust_fns.string_create && trust_fns.release && trust_fns.error_code &&
                       trust_fns.array_callbacks && trust_fns.certificate_create &&
                       trust_fns.policy_ssl && trust_fns.trust_create && trust_fns.set_anchors &&
                       trust_fns.set_anchors_only && trust_fns.evaluate;
}

/* Whether the system's verifier can be asked: false when it can't be
 * loaded, and then no server is trusted. */
static bool os_tls_trust_ready(void) {
    pthread_once(&trust_once, trust_load);
    return trust_fns.loaded;
}

/* `n` certificates from `certs`, each a 4-byte big-endian length and that
 * many DER bytes, as a CFArray of SecCertificates; NULL when one doesn't
 * parse. `*used` is how many bytes they took. */
static CfRef trust_certificates(const uint8_t *certs, size_t len, int n, size_t *used) {
    CfRef *list = calloc(n > 0 ? (size_t)n : 1, sizeof *list);
    if (list == NULL) {
        die("panic: out of memory\n");
    }
    size_t at = 0;
    int made = 0;
    bool ok = true;
    for (; ok && made < n; made++) {
        if (len - at < 4) {
            ok = false;
            break;
        }
        size_t size = (size_t)certs[at] << 24 | (size_t)certs[at + 1] << 16 |
                      (size_t)certs[at + 2] << 8 | certs[at + 3];
        at += 4;
        if (size > len - at) {
            ok = false;
            break;
        }
        CfRef der = trust_fns.data_create(NULL, certs + at, (long)size);
        list[made] = der == NULL ? NULL : trust_fns.certificate_create(NULL, der);
        if (der != NULL) {
            trust_fns.release(der);
        }
        ok = list[made] != NULL;
        at += size;
    }
    CfRef array = ok ? trust_fns.array_create(NULL, (const void **)list, n,
                                              trust_fns.array_callbacks)
                     : NULL;
    // The array holds its own references (`kCFTypeArrayCallBacks`).
    for (int i = 0; i < made; i++) {
        if (list[i] != NULL) {
            trust_fns.release(list[i]);
        }
    }
    free(list);
    *used = at;
    return array;
}

/* Apple's reason, as one of `os_tls_trust`'s answers. */
static int trust_reason(long code) {
    switch (code) {
    case -67602: /* errSecHostNameMismatch */
        return OS_TRUST_HOST;
    case -67818: /* errSecCertificateExpired */
        return OS_TRUST_EXPIRED;
    case -67819: /* errSecCertificateNotValidYet */
        return OS_TRUST_NOT_YET_VALID;
    case -67820: /* errSecCertificateRevoked */
        return OS_TRUST_REVOKED;
    case -67843: /* errSecNotTrusted */
    case -25318: /* errSecCreateChainFailed */
        return OS_TRUST_NO_ISSUER;
    default:
        return OS_TRUST_REFUSED;
    }
}

/* Whether a server named `host` (a name or an IP address) is trusted with
 * the chain it sent: `certs` holds `nchain` certificates, the server's own
 * first, then the program's extra roots (see `trust_certificates`). 0 when
 * the system's verifier says yes; otherwise why not, or
 * `OS_TRUST_UNAVAILABLE`. Blocks: the verifier may fetch over the network,
 * so it's called on a helper thread. */
/* Unused by a program that doesn't load `std.tls`. */
__attribute__((unused)) static int os_tls_trust(const char *host, const uint8_t *certs, size_t len, int nchain,
                        int nanchors) {
    if (!os_tls_trust_ready()) {
        return OS_TRUST_UNAVAILABLE;
    }
    size_t used = 0, more = 0;
    CfRef chain = trust_certificates(certs, len, nchain, &used);
    CfRef anchors = nanchors > 0 && chain != NULL
                        ? trust_certificates(certs + used, len - used, nanchors, &more)
                        : NULL;
    /* kCFStringEncodingUTF8 */
    CfRef name = trust_fns.string_create(NULL, (const uint8_t *)host, (long)strlen(host),
                                         0x08000100, 0);
    CfRef policy = name == NULL ? NULL : trust_fns.policy_ssl(1, name);
    CfRef trust = NULL;
    int answer = OS_TRUST_REFUSED;
    bool ready = chain != NULL && policy != NULL && (nanchors == 0 || anchors != NULL) &&
                 trust_fns.trust_create(chain, policy, &trust) == 0 && trust != NULL;
    // Extra roots join the system's, never replace them.
    if (ready && anchors != NULL) {
        ready = trust_fns.set_anchors(trust, anchors) == 0 &&
                trust_fns.set_anchors_only(trust, 0) == 0;
    }
    if (ready) {
        CfRef error = NULL;
        if (trust_fns.evaluate(trust, &error)) {
            answer = 0;
        } else {
            answer = error == NULL ? OS_TRUST_REFUSED : trust_reason(trust_fns.error_code(error));
        }
        if (error != NULL) {
            trust_fns.release(error);
        }
    }
    CfRef held[] = {trust, policy, name, anchors, chain};
    for (size_t i = 0; i < sizeof held / sizeof held[0]; i++) {
        if (held[i] != NULL) {
            trust_fns.release(held[i]);
        }
    }
    return answer;
}
