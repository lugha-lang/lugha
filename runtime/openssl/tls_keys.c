/*
 * tls_keys.c — `std.tls`'s connections by key: both sides prove an Ed25519
 * key with a raw public key (RFC 7250) instead of a certificate, and each
 * accepts only the keys on its list. Appended after tls.c, for programs that
 * load `std.tls`.
 *
 * TLS 1.3 only: it encrypts the client's key, so others on the network
 * can't see which device is connecting. The list is checked by OpenSSL in
 * the handshake (DANE's "end entity, whole key" match, which is what
 * `SSL_add_expected_rpk` adds), so a refused peer never gets past it; an
 * empty list matches nothing and refuses everyone.
 *
 * Keys cross as bytes. OpenSSL holds the secret key object and clears it
 * when the context is freed.
 */

static const unsigned char TLS_RPK[] = {TLSEXT_cert_type_rpk};

static void tls_free_keys(EVP_PKEY **keys, int64_t n) {
    if (keys == NULL) {
        return;
    }
    for (int64_t i = 0; i < n; i++) {
        EVP_PKEY_free(keys[i]);
    }
    free(keys);
}

/* `accept`'s 32-byte public keys as OpenSSL keys (`*n` of them), or NULL
 * with the reason; an empty list gives a non-NULL empty array. */
static EVP_PKEY **tls_public_keys(const LughaArray *accept, int64_t *n, LughaString **error) {
    LughaArray *const *items = (LughaArray *const *)accept->elems;
    for (int64_t i = 0; i < accept->len; i++) {
        if (items[i]->len != 32) {
            char text[64];
            snprintf(text, sizeof text, "accepted key %lld must be 32 bytes", (long long)i);
            net_error(error, text);
            return NULL;
        }
    }
    EVP_PKEY **keys = calloc((size_t)(accept->len > 0 ? accept->len : 1), sizeof *keys);
    if (keys == NULL) {
        die("panic: out of memory\n");
    }
    for (int64_t i = 0; i < accept->len; i++) {
        keys[i] = EVP_PKEY_new_raw_public_key(EVP_PKEY_ED25519, NULL, items[i]->elems, 32);
        if (keys[i] == NULL) {
            tls_free_keys(keys, i);
            tls_error(error, "can't read an accepted key");
            return NULL;
        }
    }
    *n = accept->len;
    return keys;
}

/* Turns on the key check for one connection and adds every accepted key; 1
 * on success. DANE must be on before `SSL_add_expected_rpk` takes a key, and
 * the connection takes its own reference to each, so a later `set_accept`
 * doesn't reach a handshake already started. */
static int tls_expect(SSL *ssl, EVP_PKEY **keys, int64_t n) {
    if (SSL_dane_enable(ssl, NULL) <= 0) {
        return 0;
    }
    for (int64_t i = 0; i < n; i++) {
        if (SSL_add_expected_rpk(ssl, keys[i]) != 1) {
            return 0;
        }
    }
    return 1;
}

/* A context for one side by key: TLS 1.3, raw public keys only, this
 * side's Ed25519 key, the peer's required. NULL with the reason. */
static SSL_CTX *tls_key_context(int server, const LughaArray *secret, LughaString **error) {
    if (secret->len != 32) {
        net_error(error, "secret key must be 32 bytes");
        return NULL;
    }
    ERR_clear_error();
    SSL_CTX *ctx = SSL_CTX_new(server ? TLS_server_method() : TLS_client_method());
    EVP_PKEY *key =
        EVP_PKEY_new_raw_private_key(EVP_PKEY_ED25519, NULL, secret->elems, (size_t)secret->len);
    int ok = ctx != NULL && key != NULL &&
             SSL_CTX_set_min_proto_version(ctx, TLS1_3_VERSION) == 1 &&
             SSL_CTX_set1_client_cert_type(ctx, TLS_RPK, sizeof TLS_RPK) == 1 &&
             SSL_CTX_set1_server_cert_type(ctx, TLS_RPK, sizeof TLS_RPK) == 1 &&
             SSL_CTX_use_PrivateKey(ctx, key) == 1 && SSL_CTX_dane_enable(ctx) > 0;
    // The context keeps its own reference to the key.
    EVP_PKEY_free(key);
    if (!ok) {
        SSL_CTX_free(ctx);
        tls_error(error, "can't use the secret key");
        return NULL;
    }
    SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER | (server ? SSL_VERIFY_FAIL_IF_NO_PEER_CERT : 0),
                       NULL);
    SSL_CTX_set_options(ctx, SSL_OP_IGNORE_UNEXPECTED_EOF);
    return ctx;
}

/* `$os.tls_key_context(server, secret, accept)`: a context by key for a
 * server (1) or a client (0): this side proves `secret`'s key, and a
 * connection made from it accepts only a peer proving one of `accept`'s. */
int32_t lugha_rt_tls_key_context(int64_t server, const LughaArray *secret,
                                 const LughaArray *accept, int64_t *handle,
                                 LughaString **error) {
    int64_t nkeys = 0;
    EVP_PKEY **keys = tls_public_keys(accept, &nkeys, error);
    if (keys == NULL) {
        return 0;
    }
    SSL_CTX *ctx = tls_key_context(server != 0, secret, error);
    if (ctx == NULL) {
        tls_free_keys(keys, nkeys);
        return 0;
    }
    TlsContext *t = tls_context_of(ctx, server != 0);
    t->by_key = 1;
    t->keys = keys;
    t->nkeys = nkeys;
    *handle = (int64_t)(intptr_t)t;
    return 1;
}

/* `$os.tls_set_accept(context, accept)`: the keys connections made from now
 * on must prove, replacing the old list; a context by certificate has none. */
int32_t lugha_rt_tls_set_accept(int64_t context, const LughaArray *accept, int64_t *done,
                                LughaString **error) {
    TlsContext *t = (TlsContext *)(intptr_t)context;
    if (!t->by_key) {
        return net_error(error, "not a by-key listener");
    }
    int64_t nkeys = 0;
    EVP_PKEY **keys = tls_public_keys(accept, &nkeys, error);
    if (keys == NULL) {
        return 0;
    }
    tls_free_keys(t->keys, t->nkeys);
    t->keys = keys;
    t->nkeys = nkeys;
    *done = 0;
    return 1;
}

/* `$os.tls_peer_key(conn)`: the 32-byte Ed25519 key the peer proved; empty
 * for a certificate connection. */
LughaArray *lugha_rt_tls_peer_key(int64_t conn) {
    TlsConn *c = (TlsConn *)(intptr_t)conn;
    EVP_PKEY *peer = c->by_key ? SSL_get0_peer_rpk(c->ssl) : NULL;
    unsigned char key[32];
    size_t len = sizeof key;
    if (peer == NULL || EVP_PKEY_get_raw_public_key(peer, key, &len) != 1 || len != 32) {
        ERR_clear_error();
        return bytes_of(key, 0);
    }
    return bytes_of(key, 32);
}
