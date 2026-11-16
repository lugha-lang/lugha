/*
 * tls.c — `std.tls`'s built-ins: TLS through the system's OpenSSL (3.2 or
 * newer) on memory buffers. Appended to the runtime only for programs that
 * load `std.tls` (they link -lssl -lcrypto); every other program never sees
 * these headers.
 *
 * Each connection is an `SSL` with two memory BIOs: `in` holds what arrived
 * from the peer, `out` what OpenSSL wants sent. The C side never touches a
 * socket, a file or a signal and never waits: Lugha carries the bytes
 * between the BIOs and a `Conn`, so TLS runs over any connection a `Net`
 * gives, a fake one included. One `tls_step` is one OpenSSL call.
 *
 * Certificates are always verified: the chain against the system's roots
 * (plus certificates the program passes as PEM text) and the host name. TLS
 * 1.2 is the oldest version spoken. Connections by key (tls_keys.c) verify
 * the peer's raw public key against a list instead, over TLS 1.3 only.
 *
 * OpenSSL's objects live outside the collector: `malloc`ed here and by
 * OpenSSL, freed by `tls_free` and `tls_free_context`. Lugha holds them as
 * opaque `i64` handles and never passes one after freeing it. C keeps no
 * Lugha array: bytes are copied in and out.
 */
#include <openssl/err.h>
#include <openssl/pem.h>
#include <openssl/ssl.h>
#include <openssl/x509v3.h>

/* Raw public keys and HPKE arrived in OpenSSL 3.2; the driver turns this
 * message into E0332. */
#if OPENSSL_VERSION_NUMBER < 0x30200000L
#error "Lugha needs OpenSSL 3.2 or newer"
#endif

/* The most plaintext one write step takes: one TLS record, so `out` stays
 * about one record long between drains. */
enum { TLS_RECORD = 16384, TLS_MAX_PEM = 16 * 1024 * 1024 };

/* A context: certificates or keys, shared by the connections made from it. */
typedef struct {
    SSL_CTX *ctx;
    int server;
    /* 1 by key, with the public keys a new connection must see proved. */
    int by_key;
    EVP_PKEY **keys;
    int64_t nkeys;
} TlsContext;

typedef struct {
    SSL *ssl;
    /* Owned by `ssl` (`SSL_set_bio`), freed with it. */
    BIO *in;
    BIO *out;
    /* 1 for a connection by key: a failed check names the key, not a certificate. */
    int by_key;
} TlsConn;

/* `(status, bytes)`, as Lugha's `(i64, u8[])`: 1 done (a read's bytes), 0
 * wants more input from the peer. */
typedef struct {
    int64_t status;
    LughaArray *bytes;
} TlsStep;

static int tls_expect(SSL *ssl, EVP_PKEY **keys, int64_t n);
static void tls_free_keys(EVP_PKEY **keys, int64_t n);

/* OpenSSL's reason for the last failure, or `fallback`. */
static int32_t tls_error(LughaString **error, const char *fallback) {
    unsigned long code = ERR_get_error();
    char text[256];
    if (code != 0) {
        ERR_error_string_n(code, text, sizeof text);
        ERR_clear_error();
        return net_error(error, text);
    }
    return net_error(error, fallback);
}

/* A C string of `s`; `NULL` when it holds a NUL byte. */
static const char *tls_text(const LughaString *s) {
    return memchr(s->bytes, 0, (size_t)s->len) == NULL ? s->bytes : NULL;
}

/* Refuses every passphrase prompt: an encrypted PEM key fails to load
 * rather than OpenSSL asking on the terminal. */
static int tls_no_passphrase(char *buf, int size, int rwflag, void *u) {
    (void)buf;
    (void)size;
    (void)rwflag;
    (void)u;
    return 0;
}

/* A read-only BIO over `pem`'s bytes, or NULL when it is too long. */
static BIO *tls_pem(const LughaString *pem) {
    if (pem->len > TLS_MAX_PEM) {
        return NULL;
    }
    return BIO_new_mem_buf(pem->bytes, (int)pem->len);
}

static TlsContext *tls_context_of(SSL_CTX *ctx, int server) {
    TlsContext *t = malloc(sizeof *t);
    if (t == NULL) {
        die("panic: out of memory\n");
    }
    t->ctx = ctx;
    t->server = server;
    t->by_key = 0;
    t->keys = NULL;
    t->nkeys = 0;
    return t;
}

/* `$os.tls_client_context(ca_pem)`: a client context trusting the system's
 * roots and every certificate in `ca_pem` (`""` for none). */
int32_t lugha_rt_tls_client_context(const LughaString *ca_pem, int64_t *handle,
                                    LughaString **error) {
    ERR_clear_error();
    SSL_CTX *ctx = SSL_CTX_new(TLS_client_method());
    if (ctx == NULL) {
        return tls_error(error, "TLS unavailable");
    }
    SSL_CTX_set_min_proto_version(ctx, TLS1_2_VERSION);
    SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, NULL);
    /* A peer closing without close_notify reads as the end of the stream. */
    SSL_CTX_set_options(ctx, SSL_OP_IGNORE_UNEXPECTED_EOF);
    if (SSL_CTX_set_default_verify_paths(ctx) != 1) {
        SSL_CTX_free(ctx);
        return tls_error(error, "can't load trusted certificates");
    }
    if (ca_pem->len > 0) {
        BIO *b = tls_pem(ca_pem);
        X509_STORE *store = SSL_CTX_get_cert_store(ctx);
        int added = 0;
        int ok = b != NULL;
        X509 *x;
        while (ok && (x = PEM_read_bio_X509(b, NULL, tls_no_passphrase, NULL)) != NULL) {
            ok = X509_STORE_add_cert(store, x) == 1;
            X509_free(x);
            added++;
        }
        BIO_free(b);
        if (!ok || added == 0) {
            SSL_CTX_free(ctx);
            return tls_error(error, "can't load trusted certificates");
        }
        // The read past the last certificate leaves "no start line" queued.
        ERR_clear_error();
    }
    *handle = (int64_t)(intptr_t)tls_context_of(ctx, 0);
    return 1;
}

/* `$os.tls_server_context(cert_pem, key_pem)`: a server context with the
 * certificate chain (leaf first) and its private key. */
int32_t lugha_rt_tls_server_context(const LughaString *cert_pem, const LughaString *key_pem,
                                    int64_t *handle, LughaString **error) {
    ERR_clear_error();
    SSL_CTX *ctx = SSL_CTX_new(TLS_server_method());
    if (ctx == NULL) {
        return tls_error(error, "TLS unavailable");
    }
    SSL_CTX_set_min_proto_version(ctx, TLS1_2_VERSION);
    BIO *certs = tls_pem(cert_pem);
    BIO *keys = tls_pem(key_pem);
    int ok = certs != NULL && keys != NULL;
    X509 *leaf = ok ? PEM_read_bio_X509(certs, NULL, tls_no_passphrase, NULL) : NULL;
    ok = leaf != NULL && SSL_CTX_use_certificate(ctx, leaf) == 1;
    X509_free(leaf);
    X509 *link;
    while (ok && (link = PEM_read_bio_X509(certs, NULL, tls_no_passphrase, NULL)) != NULL) {
        // The context takes its own reference to each certificate of the chain.
        ok = SSL_CTX_add1_chain_cert(ctx, link) == 1;
        X509_free(link);
    }
    if (ok) {
        // The read past the chain's end leaves "no start line" queued.
        ERR_clear_error();
    }
    EVP_PKEY *key = ok ? PEM_read_bio_PrivateKey(keys, NULL, tls_no_passphrase, NULL) : NULL;
    ok = key != NULL && SSL_CTX_use_PrivateKey(ctx, key) == 1 &&
         SSL_CTX_check_private_key(ctx) == 1;
    EVP_PKEY_free(key);
    BIO_free(certs);
    BIO_free(keys);
    if (!ok) {
        SSL_CTX_free(ctx);
        return tls_error(error, "can't load the certificate or key");
    }
    *handle = (int64_t)(intptr_t)tls_context_of(ctx, 1);
    return 1;
}

/* `$os.tls_free_context(context)`: a connection made from it keeps its own
 * reference to OpenSSL's context. */
void lugha_rt_tls_free_context(int64_t context) {
    TlsContext *t = (TlsContext *)(intptr_t)context;
    SSL_CTX_free(t->ctx);
    tls_free_keys(t->keys, t->nkeys);
    free(t);
}

/* `$os.tls_new(context, host)`: a connection about to handshake, on two
 * empty memory BIOs. A client by certificate must see `host` (a name or an
 * IP address) on the server's certificate; by key, one of the context's keys. */
int32_t lugha_rt_tls_new(int64_t context, const LughaString *host, int64_t *handle,
                         LughaString **error) {
    TlsContext *t = (TlsContext *)(intptr_t)context;
    const char *name = tls_text(host);
    if (name == NULL) {
        return net_error(error, "host contains a NUL byte");
    }
    ERR_clear_error();
    SSL *ssl = SSL_new(t->ctx);
    BIO *in = BIO_new(BIO_s_mem());
    BIO *out = BIO_new(BIO_s_mem());
    if (ssl == NULL || in == NULL || out == NULL) {
        SSL_free(ssl);
        BIO_free(in);
        BIO_free(out);
        return tls_error(error, "TLS unavailable");
    }
    /* An empty `in` means "more to come", not the end of the stream. */
    BIO_set_mem_eof_return(in, -1);
    SSL_set_bio(ssl, in, out);
    /* Lugha may hand a retried write's bytes from another address. */
    SSL_set_mode(ssl, SSL_MODE_ACCEPT_MOVING_WRITE_BUFFER);
    if (t->server) {
        SSL_set_accept_state(ssl);
    } else {
        SSL_set_connect_state(ssl);
    }
    int ok = 1;
    if (t->by_key) {
        ok = tls_expect(ssl, t->keys, t->nkeys);
    } else if (!t->server) {
        unsigned char ip[16];
        int is_ip = inet_pton(AF_INET, name, ip) == 1 || inet_pton(AF_INET6, name, ip) == 1;
        ok = is_ip ? X509_VERIFY_PARAM_set1_ip_asc(SSL_get0_param(ssl), name)
                   : SSL_set_tlsext_host_name(ssl, name) == 1 && SSL_set1_host(ssl, name);
        if (!ok) {
            SSL_free(ssl);
            return tls_error(error, "bad host name");
        }
    }
    if (!ok) {
        SSL_free(ssl);
        return tls_error(error, "TLS unavailable");
    }
    TlsConn *c = malloc(sizeof *c);
    if (c == NULL) {
        die("panic: out of memory\n");
    }
    c->ssl = ssl;
    c->in = in;
    c->out = out;
    c->by_key = t->by_key;
    *handle = (int64_t)(intptr_t)c;
    return 1;
}

/* Why the last step failed, as today's messages: the certificate or key the
 * peer showed, else OpenSSL's reason. */
static int32_t tls_failed(TlsConn *c, LughaString **error) {
    long verify = SSL_get_verify_result(c->ssl);
    if (verify != X509_V_OK && c->by_key) {
        ERR_clear_error();
        return net_error(error, "peer key not accepted");
    }
    if (verify != X509_V_OK) {
        char text[300];
        snprintf(text, sizeof text, "certificate verify failed: %s",
                 X509_verify_cert_error_string(verify));
        ERR_clear_error();
        return net_error(error, text);
    }
    return tls_error(error, "TLS failed");
}

/* `$os.tls_step(conn, op, data, max)`: one OpenSSL call: the handshake (op
 * 0), a read of up to `max` bytes (1), a write of `data`, at most one record
 * (2), or sending close_notify (3). `(1, bytes)` when it finished (a read's
 * bytes; `[]` at the end of the stream), `(0, [])` when it needs more of the
 * peer's bytes first, or 0 and the reason. */
int32_t lugha_rt_tls_step(int64_t conn, int64_t op, const LughaArray *data, int64_t max,
                          TlsStep *out, LughaString **error) {
    static const unsigned char none[1];
    TlsConn *c = (TlsConn *)(intptr_t)conn;
    char *buffer = NULL;
    if (op == 1 && (max < 1 || max > NET_MAX_READ)) {
        return net_error(error, "read size must be from 1 to 16777216 bytes");
    }
    if (op == 2 && (data->len < 1 || data->len > TLS_RECORD)) {
        return net_error(error, "a write step takes 1 to 16384 bytes");
    }
    if (op < 0 || op > 3) {
        return net_error(error, "no such TLS step");
    }
    ERR_clear_error();
    int r;
    if (op == 0) {
        r = SSL_do_handshake(c->ssl);
    } else if (op == 1) {
        buffer = lugha_rt_alloc(max);
        r = SSL_read(c->ssl, buffer, (int)max);
    } else if (op == 2) {
        r = SSL_write(c->ssl, data->elems, (int)data->len);
    } else {
        /* 0 (sent, the peer's reply not awaited) counts as done. */
        r = SSL_shutdown(c->ssl);
        if (r == 0) {
            r = 1;
        }
    }
    if (r > 0) {
        out->status = 1;
        out->bytes = op == 1 ? bytes_of(buffer, r) : bytes_of(none, 0);
        return 1;
    }
    int why = SSL_get_error(c->ssl, r);
    if (why == SSL_ERROR_WANT_READ) {
        out->status = 0;
        out->bytes = bytes_of(none, 0);
        return 1;
    }
    if (why == SSL_ERROR_ZERO_RETURN) {
        out->status = 1;
        out->bytes = bytes_of(none, 0);
        return 1;
    }
    return tls_failed(c, error);
}

/* `$os.tls_outgoing(conn)`: everything OpenSSL wrote for the peer since the
 * last call, drained from `out`. */
LughaArray *lugha_rt_tls_outgoing(int64_t conn) {
    static const unsigned char none[1];
    TlsConn *c = (TlsConn *)(intptr_t)conn;
    size_t pending = BIO_ctrl_pending(c->out);
    if (pending == 0) {
        return bytes_of(none, 0);
    }
    if (pending > INT32_MAX) {
        die("panic: TLS output too large\n");
    }
    char *buffer = lugha_rt_alloc((int64_t)pending);
    int n = BIO_read(c->out, buffer, (int)pending);
    return bytes_of(buffer, n > 0 ? n : 0);
}

/* `$os.tls_incoming(conn, bytes)`: the peer's bytes, copied into `in`. */
void lugha_rt_tls_incoming(int64_t conn, const LughaArray *bytes) {
    TlsConn *c = (TlsConn *)(intptr_t)conn;
    int64_t done = 0;
    while (done < bytes->len) {
        int64_t left = bytes->len - done;
        int chunk = left > INT32_MAX ? INT32_MAX : (int)left;
        int n = BIO_write(c->in, bytes->elems + done, chunk);
        if (n <= 0) {
            die("panic: out of memory\n");
        }
        done += n;
    }
}

/* `$os.tls_incoming_end(conn)`: the peer closed; reading past what `in`
 * holds is now the end of the stream. */
void lugha_rt_tls_incoming_end(int64_t conn) {
    TlsConn *c = (TlsConn *)(intptr_t)conn;
    BIO_set_mem_eof_return(c->in, 0);
}

/* `$os.tls_free(conn)`: the connection and both its BIOs. */
void lugha_rt_tls_free(int64_t conn) {
    TlsConn *c = (TlsConn *)(intptr_t)conn;
    SSL_free(c->ssl);
    ERR_clear_error();
    free(c);
}
