/*
 * os/windows/trust.c — part of lugha_rt.c on Windows: whether a TLS
 * server's certificates are trusted, asked of Windows' own verifier
 * (`CertGetCertificateChain`, then the SSL policy of
 * `CertVerifyCertificateChainPolicy`), which reads the system's
 * certificate stores: the roots a user or a company added, and the ones
 * Microsoft distrusts.
 *
 * crypt32.dll is loaded from the system folder the first time, never
 * linked, so a program that doesn't load `std.tls` doesn't import it.
 * Revocation isn't checked online: a connection never waits on a CRL or
 * OCSP server. Nothing here knows OpenSSL: certificates come in as DER
 * bytes.
 *
 * A fragment, not compiled on its own: joined after os/windows/paths.c
 * (`win_wide`).
 */
#include <wincrypt.h>

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

/* The entry points used, filled once by `trust_load`. */
static struct {
    HCERTSTORE(WINAPI *open_store)(LPCSTR, DWORD, HCRYPTPROV_LEGACY, DWORD, const void *);
    BOOL(WINAPI *add_encoded)(HCERTSTORE, DWORD, const BYTE *, DWORD, DWORD, PCCERT_CONTEXT *);
    BOOL(WINAPI *close_store)(HCERTSTORE, DWORD);
    BOOL(WINAPI *free_context)(PCCERT_CONTEXT);
    BOOL(WINAPI *get_chain)(HCERTCHAINENGINE, PCCERT_CONTEXT, LPFILETIME, HCERTSTORE,
                            PCERT_CHAIN_PARA, DWORD, LPVOID, PCCERT_CHAIN_CONTEXT *);
    void(WINAPI *free_chain)(PCCERT_CHAIN_CONTEXT);
    BOOL(WINAPI *verify_policy)(LPCSTR, PCCERT_CHAIN_CONTEXT, PCERT_CHAIN_POLICY_PARA,
                                PCERT_CHAIN_POLICY_STATUS);
    BOOL(WINAPI *create_engine)(PCERT_CHAIN_ENGINE_CONFIG, HCERTCHAINENGINE *);
    void(WINAPI *free_engine)(HCERTCHAINENGINE);
    LONG(WINAPI *time_validity)(LPFILETIME, PCERT_INFO);
    bool loaded;
} trust_fns;

static pthread_once_t trust_once = PTHREAD_ONCE_INIT;

/* Fills `trust_fns`; `loaded` stays false when crypt32 or any entry point
 * is missing. Only the system folder is searched, so a crypt32.dll beside
 * the program or in the working folder is never loaded. */
static void trust_load(void) {
    HMODULE crypt = LoadLibraryExW(L"crypt32.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (crypt == NULL) {
        return;
    }
    // Each entry point by its documented type: GetProcAddress returns them untyped.
#define TRUST_FN(field, name) *(FARPROC *)&trust_fns.field = GetProcAddress(crypt, name)
    TRUST_FN(open_store, "CertOpenStore");
    TRUST_FN(add_encoded, "CertAddEncodedCertificateToStore");
    TRUST_FN(close_store, "CertCloseStore");
    TRUST_FN(free_context, "CertFreeCertificateContext");
    TRUST_FN(get_chain, "CertGetCertificateChain");
    TRUST_FN(free_chain, "CertFreeCertificateChain");
    TRUST_FN(verify_policy, "CertVerifyCertificateChainPolicy");
    TRUST_FN(create_engine, "CertCreateCertificateChainEngine");
    TRUST_FN(free_engine, "CertFreeCertificateChainEngine");
    TRUST_FN(time_validity, "CertVerifyTimeValidity");
#undef TRUST_FN
    trust_fns.loaded = trust_fns.open_store && trust_fns.add_encoded && trust_fns.close_store &&
                       trust_fns.free_context && trust_fns.get_chain && trust_fns.free_chain &&
                       trust_fns.verify_policy && trust_fns.create_engine &&
                       trust_fns.free_engine && trust_fns.time_validity;
}

/* Whether the system's verifier can be asked: false when it can't be
 * loaded, and then no server is trusted. */
static bool os_tls_trust_ready(void) {
    pthread_once(&trust_once, trust_load);
    return trust_fns.loaded;
}

/* `n` certificates from `certs`, each a 4-byte big-endian length and that
 * many DER bytes, added to a new memory store; NULL when one doesn't
 * parse. `*first` (when asked for) is the first one's context, `*used`
 * how many bytes they took. */
static HCERTSTORE trust_store(const uint8_t *certs, size_t len, int n, PCCERT_CONTEXT *first,
                              size_t *used) {
    HCERTSTORE store = trust_fns.open_store(CERT_STORE_PROV_MEMORY, 0, 0, 0, NULL);
    size_t at = 0;
    bool ok = store != NULL;
    for (int i = 0; ok && i < n; i++) {
        if (len - at < 4) {
            ok = false;
            break;
        }
        size_t size = (size_t)certs[at] << 24 | (size_t)certs[at + 1] << 16 |
                      (size_t)certs[at + 2] << 8 | certs[at + 3];
        at += 4;
        if (size > len - at || size > MAXDWORD) {
            ok = false;
            break;
        }
        PCCERT_CONTEXT *out = i == 0 && first != NULL ? first : NULL;
        ok = trust_fns.add_encoded(store, X509_ASN_ENCODING | PKCS_7_ASN_ENCODING, certs + at,
                                   (DWORD)size, CERT_STORE_ADD_ALWAYS, out);
        at += size;
    }
    *used = at;
    if (!ok && store != NULL) {
        if (first != NULL && *first != NULL) {
            trust_fns.free_context(*first);
            *first = NULL;
        }
        trust_fns.close_store(store, 0);
        return NULL;
    }
    return store;
}

/* Windows' reason, as one of `os_tls_trust`'s answers. */
static int trust_reason(DWORD error, PCCERT_CONTEXT leaf) {
    switch (error) {
    case (DWORD)CERT_E_CN_NO_MATCH:
        return OS_TRUST_HOST;
    case (DWORD)CERT_E_EXPIRED:
        // One reason for both ends of the dates: the leaf's own say which.
        return trust_fns.time_validity(NULL, leaf->pCertInfo) < 0 ? OS_TRUST_NOT_YET_VALID
                                                                  : OS_TRUST_EXPIRED;
    case (DWORD)CRYPT_E_REVOKED:
        return OS_TRUST_REVOKED;
    case (DWORD)CERT_E_UNTRUSTEDROOT:
    case (DWORD)CERT_E_CHAINING:
        return OS_TRUST_NO_ISSUER;
    default:
        return OS_TRUST_REFUSED;
    }
}

/* The chain's own verdict, read before the SSL policy is asked: any error
 * refuses, so a chain that never reaches a trusted root is refused even
 * where a policy wouldn't look (as Go reads it too). 0 when it has none. */
static int trust_chain_status(PCCERT_CHAIN_CONTEXT chain, PCCERT_CONTEXT leaf) {
    DWORD status = chain->TrustStatus.dwErrorStatus;
    if (status == CERT_TRUST_NO_ERROR) {
        return 0;
    }
    if (status & CERT_TRUST_IS_REVOKED) {
        return OS_TRUST_REVOKED;
    }
    if (status & CERT_TRUST_IS_NOT_TIME_VALID) {
        return trust_reason((DWORD)CERT_E_EXPIRED, leaf);
    }
    if (status & (CERT_TRUST_IS_UNTRUSTED_ROOT | CERT_TRUST_IS_PARTIAL_CHAIN)) {
        return OS_TRUST_NO_ISSUER;
    }
    return OS_TRUST_REFUSED;
}

/* The answer for `leaf` shown to `server` (UTF-16; NULL for an IP
 * address, whose match OpenSSL checks), its chain built by `engine`
 * (NULL: the system's roots) with `extra` (what the server sent): the
 * chain's own status, then the SSL policy's; 0 trusted, else why not. */
static int trust_check(HCERTCHAINENGINE engine, PCCERT_CONTEXT leaf, HCERTSTORE extra,
                       wchar_t *server) {
    static char server_auth[] = szOID_PKIX_KP_SERVER_AUTH;
    char *usages[] = {server_auth};
    CERT_CHAIN_PARA para;
    memset(&para, 0, sizeof para);
    para.cbSize = sizeof para;
    para.RequestedUsage.dwType = USAGE_MATCH_TYPE_AND;
    para.RequestedUsage.Usage.cUsageIdentifier = 1;
    para.RequestedUsage.Usage.rgpszUsageIdentifier = usages;
    PCCERT_CHAIN_CONTEXT chain = NULL;
    // No revocation flags: nothing is fetched to check revocation.
    if (!trust_fns.get_chain(engine, leaf, NULL, extra, &para, 0, NULL, &chain)) {
        return OS_TRUST_REFUSED;
    }
    int answer = trust_chain_status(chain, leaf);
    if (answer != 0) {
        trust_fns.free_chain(chain);
        return answer;
    }
    SSL_EXTRA_CERT_CHAIN_POLICY_PARA ssl;
    memset(&ssl, 0, sizeof ssl);
    ssl.cbSize = sizeof ssl;
    ssl.dwAuthType = AUTHTYPE_SERVER;
    ssl.fdwChecks = 0;
    ssl.pwszServerName = server;
    CERT_CHAIN_POLICY_PARA policy;
    memset(&policy, 0, sizeof policy);
    policy.cbSize = sizeof policy;
    policy.dwFlags = 0;
    policy.pvExtraPolicyPara = &ssl;
    CERT_CHAIN_POLICY_STATUS status;
    memset(&status, 0, sizeof status);
    status.cbSize = sizeof status;
    answer = OS_TRUST_REFUSED;
    if (trust_fns.verify_policy(CERT_CHAIN_POLICY_SSL, chain, &policy, &status)) {
        answer = status.dwError == 0 ? 0 : trust_reason(status.dwError, leaf);
    }
    trust_fns.free_chain(chain);
    return answer;
}

/* Whether a server named `host` (a name or an IP address) is trusted with
 * the chain it sent: `certs` holds `nchain` certificates, the server's own
 * first, then `nanchors` extra roots (see `trust_store`). 0 when Windows'
 * verifier says yes; otherwise why not, or `OS_TRUST_UNAVAILABLE`. Extra
 * roots join the system's: they are tried, alone, only when the system's
 * roots don't reach the chain's top. Blocks: the verifier may fetch
 * missing issuers, so it's called on a helper thread. */
/* Unused by a program that doesn't load `std.tls`. */
__attribute__((unused)) static int os_tls_trust(const char *host, const uint8_t *certs,
                                                size_t len, int nchain, int nanchors) {
    if (!os_tls_trust_ready()) {
        return OS_TRUST_UNAVAILABLE;
    }
    size_t n = strlen(host);
    wchar_t *server = malloc((n + 1) * sizeof *server);
    if (server == NULL) {
        die("panic: out of memory\n");
    }
    PCCERT_CONTEXT leaf = NULL;
    size_t used = 0, more = 0;
    HCERTSTORE sent = nchain > 0 ? trust_store(certs, len, nchain, &leaf, &used) : NULL;
    HCERTSTORE roots = nanchors > 0 && sent != NULL
                           ? trust_store(certs + used, len - used, nanchors, NULL, &more)
                           : NULL;
    int answer = OS_TRUST_REFUSED;
    bool ready = sent != NULL && leaf != NULL && (nanchors == 0 || roots != NULL) &&
                 win_wide(host, n, server, (int)n + 1) >= 0;
    // An IP address goes to the policy as no name: Windows' and Wine's
    // matches of one differ, and OpenSSL checks it after a yes, as on Linux.
    unsigned char ip[16];
    bool is_ip = inet_pton(AF_INET, host, ip) == 1 || inet_pton(AF_INET6, host, ip) == 1;
    wchar_t *name = is_ip ? NULL : server;
    if (ready) {
        answer = trust_check(NULL, leaf, sent, name);
    }
    if (ready && answer == OS_TRUST_NO_ISSUER && roots != NULL) {
        CERT_CHAIN_ENGINE_CONFIG config;
        memset(&config, 0, sizeof config);
        config.cbSize = sizeof config;
        // Only the program's roots are trusted by this engine.
        config.hExclusiveRoot = roots;
        HCERTCHAINENGINE engine = NULL;
        if (trust_fns.create_engine(&config, &engine)) {
            answer = trust_check(engine, leaf, sent, name);
            trust_fns.free_engine(engine);
        }
    }
    if (leaf != NULL) {
        trust_fns.free_context(leaf);
    }
    if (roots != NULL) {
        trust_fns.close_store(roots, 0);
    }
    if (sent != NULL) {
        trust_fns.close_store(sent, 0);
    }
    free(server);
    return answer;
}
