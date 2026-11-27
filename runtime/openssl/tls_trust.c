/*
 * tls_trust.c — part of `std.tls`'s built-ins, joined after tls.c: where
 * the system's own verifier decides (`OS_TLS_TRUST`), a client hands it
 * the server's chain instead of checking it against OpenSSL's roots.
 *
 * The verifier may wait on the network, so it never runs inside OpenSSL's
 * handshake: the first call of the verify callback keeps the chain and
 * pauses the handshake (`SSL_set_retry_verify`), `tls_step` asks the
 * verifier on a helper thread, then resumes the handshake, and the second
 * call gives the answer. The answer is set as OpenSSL's own error, so a
 * refusal reads as it does where OpenSSL checks.
 *
 * On Android, OpenSSL checks, against the system's own store read here.
 */

#if defined(__ANDROID__)

/* Android's trusted certificates: the store in the TLS module
 * (Android 14 and newer, when it has any), else the system's. Each file
 * is read and added: they are named by OpenSSL's old subject hash, which
 * its folder lookup never matches. Only the system's: not those a user
 * added in the phone's settings. True when one was added. */
static bool tls_android_roots(SSL_CTX *ctx) {
    static const char *const stores[] = {"/apex/com.android.conscrypt/cacerts",
                                         "/system/etc/security/cacerts"};
    X509_STORE *store = SSL_CTX_get_cert_store(ctx);
    for (size_t i = 0; i < sizeof stores / sizeof *stores; i++) {
        DIR *dir = opendir(stores[i]);
        if (dir == NULL) {
            continue;
        }
        int added = 0;
        struct dirent *entry;
        while ((entry = readdir(dir)) != NULL) {
            char path[512];
            int n = snprintf(path, sizeof path, "%s/%s", stores[i], entry->d_name);
            FILE *f = entry->d_name[0] != '.' && n > 0 && (size_t)n < sizeof path
                          ? fopen(path, "re")
                          : NULL;
            X509 *x = f != NULL ? PEM_read_X509(f, NULL, tls_no_passphrase, NULL) : NULL;
            if (f != NULL) {
                fclose(f);
            }
            if (x != NULL) {
                added += X509_STORE_add_cert(store, x) == 1;
                X509_free(x);
            }
        }
        closedir(dir);
        // A file that isn't a certificate leaves an error queued.
        ERR_clear_error();
        if (added > 0) {
            return true;
        }
    }
    return false;
}

#endif

#if OS_TLS_TRUST

/* Appends `x` as DER after its 4-byte big-endian length; false when it
 * doesn't encode. */
static bool trust_put(TrustBytes *b, X509 *x) {
    int size = i2d_X509(x, NULL);
    if (size <= 0) {
        return false;
    }
    unsigned char *grown = realloc(b->bytes, b->len + 4 + (size_t)size);
    if (grown == NULL) {
        die("panic: out of memory\n");
    }
    b->bytes = grown;
    unsigned char *at = grown + b->len;
    at[0] = (unsigned char)(size >> 24);
    at[1] = (unsigned char)(size >> 16);
    at[2] = (unsigned char)(size >> 8);
    at[3] = (unsigned char)size;
    at += 4;
    if (i2d_X509(x, &at) != size) {
        return false;
    }
    b->len += 4 + (size_t)size;
    b->n++;
    return true;
}

static void trust_clear(TrustBytes *b) {
    free(b->bytes);
    b->bytes = NULL;
    b->len = 0;
    b->n = 0;
}

/* On a helper: the verifier's answer for the job's host (`path`) and
 * certificates (`in`: `flags` of the server's, then `n` extra roots). */
static void tls_trust_job(Job *job) {
    job->value = os_tls_trust(job->path, (const uint8_t *)job->in, job->in_len, job->flags,
                              (int)job->n);
}

/* Asks the verifier about the chain `c` kept, waiting as a task waits;
 * the answer is kept for the verify callback's second call. */
static void tls_trust_ask(TlsConn *c) {
    Job *job = job_new(tls_trust_job);
    job->path = job_copy(c->host, strlen(c->host));
    job->in_len = c->chain.len + c->anchors.len;
    job->in = malloc(job->in_len > 0 ? job->in_len : 1);
    if (job->in == NULL) {
        die("panic: out of memory\n");
    }
    memcpy(job->in, c->chain.bytes, c->chain.len);
    if (c->anchors.len > 0) {
        memcpy(job->in + c->chain.len, c->anchors.bytes, c->anchors.len);
    }
    job->flags = c->chain.n;
    job->n = c->anchors.n;
    job_run(job);
    c->trust = (int)job->value;
    job_free(job);
    trust_clear(&c->chain);
}

/* The verifier's answer as OpenSSL's error for `leaf`, shown to `host`. */
static int tls_trust_error(int answer, X509 *leaf, const char *host, int is_ip) {
    switch (answer) {
    case 0:
        // Checked again here, as the verifier's policy already did.
        if (is_ip ? X509_check_ip_asc(leaf, host, 0) != 1
                  : X509_check_host(leaf, host, strlen(host), 0, NULL) != 1) {
            return is_ip ? X509_V_ERR_IP_ADDRESS_MISMATCH : X509_V_ERR_HOSTNAME_MISMATCH;
        }
        return X509_V_OK;
    case OS_TRUST_HOST:
        return is_ip ? X509_V_ERR_IP_ADDRESS_MISMATCH : X509_V_ERR_HOSTNAME_MISMATCH;
    case OS_TRUST_EXPIRED:
        return X509_V_ERR_CERT_HAS_EXPIRED;
    case OS_TRUST_NOT_YET_VALID:
        return X509_V_ERR_CERT_NOT_YET_VALID;
    case OS_TRUST_REVOKED:
        return X509_V_ERR_CERT_REVOKED;
    case OS_TRUST_NO_ISSUER:
        // A certificate that signed itself, alone: OpenSSL's own words for it.
        return X509_check_issued(leaf, leaf) == X509_V_OK
                   ? X509_V_ERR_DEPTH_ZERO_SELF_SIGNED_CERT
                   : X509_V_ERR_UNABLE_TO_GET_ISSUER_CERT_LOCALLY;
    default:
        return X509_V_ERR_CERT_UNTRUSTED;
    }
}

/* OpenSSL's whole-chain check, replaced: 1 trusted, 0 refused (the reason
 * set on `x`). Pausing for `tls_trust_ask` is also 1, after
 * `SSL_set_retry_verify`: OpenSSL treats any other answer as a refusal, and
 * the pause, not this answer, then decides what happens next. */
static int tls_trust_verify(X509_STORE_CTX *x, void *arg) {
    (void)arg;
    SSL *ssl = X509_STORE_CTX_get_ex_data(x, SSL_get_ex_data_X509_STORE_CTX_idx());
    TlsConn *c = ssl == NULL ? NULL : SSL_get_app_data(ssl);
    X509 *leaf = X509_STORE_CTX_get0_cert(x);
    if (c == NULL || leaf == NULL) {
        X509_STORE_CTX_set_error(x, X509_V_ERR_UNSPECIFIED);
        return 0;
    }
    if (c->trust == TRUST_UNASKED) {
        // The server's own certificate first, then what else it sent.
        trust_clear(&c->chain);
        bool ok = trust_put(&c->chain, leaf);
        STACK_OF(X509) *sent = X509_STORE_CTX_get0_untrusted(x);
        for (int i = 0; ok && sent != NULL && i < sk_X509_num(sent); i++) {
            X509 *one = sk_X509_value(sent, i);
            ok = X509_cmp(one, leaf) == 0 || trust_put(&c->chain, one);
        }
        if (!ok || !SSL_set_retry_verify(ssl)) {
            trust_clear(&c->chain);
            X509_STORE_CTX_set_error(x, X509_V_ERR_UNSPECIFIED);
            return 0;
        }
        return 1;
    }
    unsigned char ip[16];
    int is_ip = inet_pton(AF_INET, c->host, ip) == 1 || inet_pton(AF_INET6, c->host, ip) == 1;
    int error = tls_trust_error(c->trust, leaf, c->host, is_ip);
    X509_STORE_CTX_set_error(x, error);
    return error == X509_V_OK;
}

#endif
