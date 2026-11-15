/*
 * crypto_hpke.c — `std.crypto`'s HPKE (RFC 9180): bytes sealed to an X25519
 * public key, single-shot, base mode, with one suite: DHKEM(X25519,
 * HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305. Appended after crypto.c,
 * for programs that load `std.crypto`.
 *
 * A sealed blob is the 32-byte encapsulated key, then the ciphertext and
 * its 16-byte tag; the plaintext is never empty (OpenSSL refuses one).
 * Each seal makes a fresh ephemeral key inside OpenSSL's context, which
 * clears it when freed. Opened plaintext reaches Lugha only once the tag
 * matches, from a buffer wiped on every path.
 */

enum {
    HPKE_ENC = 32,
    HPKE_MAX_INFO = OSSL_HPKE_MAX_INFOLEN,
};

static OSSL_HPKE_SUITE hpke_suite(void) {
    OSSL_HPKE_SUITE suite = {OSSL_HPKE_KEM_ID_X25519, OSSL_HPKE_KDF_ID_HKDF_SHA256,
                             OSSL_HPKE_AEAD_ID_CHACHA_POLY1305};
    return suite;
}

/* 1 when `pub` is a low-order X25519 point: any clamped scalar times it is
 * the identity, so a fixed scalar's shared secret comes out all zeros (or
 * OpenSSL refuses to derive it). RFC 9180 says such a key must abort. */
static int hpke_low_order(const unsigned char *pub) {
    static const unsigned char scalar[CRYPTO_KEY] = {9};
    static const unsigned char zeros[CRYPTO_KEY];
    ERR_clear_error();
    EVP_PKEY *mine = EVP_PKEY_new_raw_private_key(EVP_PKEY_X25519, NULL, scalar, CRYPTO_KEY);
    EVP_PKEY *theirs = EVP_PKEY_new_raw_public_key(EVP_PKEY_X25519, NULL, pub, CRYPTO_KEY);
    EVP_PKEY_CTX *ctx = mine == NULL ? NULL : EVP_PKEY_CTX_new(mine, NULL);
    unsigned char shared[CRYPTO_KEY];
    size_t len = sizeof shared;
    int ready = ctx != NULL && theirs != NULL && EVP_PKEY_derive_init(ctx) == 1 &&
                EVP_PKEY_derive_set_peer(ctx, theirs) == 1;
    int derived = ready && EVP_PKEY_derive(ctx, shared, &len) == 1 && len == CRYPTO_KEY;
    int low = ready && (!derived || CRYPTO_memcmp(shared, zeros, CRYPTO_KEY) == 0);
    EVP_PKEY_CTX_free(ctx);
    EVP_PKEY_free(theirs);
    EVP_PKEY_free(mine);
    ERR_clear_error();
    return low;
}

/* The checks both directions share: `info` and the sizes. */
static const char *hpke_lengths(const LughaArray *info, const LughaArray *data,
                                const LughaArray *aad) {
    if (info->len > HPKE_MAX_INFO) {
        return "HPKE info must be at most 1024 bytes";
    }
    if (data->len > CRYPTO_MAX_SEAL || aad->len > CRYPTO_MAX_SEAL) {
        return "at most 1073741824 bytes can be sealed";
    }
    return NULL;
}

/* `$os.crypto_hpke_seal(public, info, plaintext, aad)`: `enc ‖ ct`. */
int32_t lugha_rt_crypto_hpke_seal(const LughaArray *pub, const LughaArray *info,
                                  const LughaArray *plain, const LughaArray *aad,
                                  LughaArray **out, LughaString **error) {
    const char *wrong = hpke_lengths(info, plain, aad);
    if (pub->len != CRYPTO_KEY) {
        wrong = "public key must be 32 bytes";
    } else if (plain->len == 0) {
        // OpenSSL's HPKE refuses an empty plaintext.
        wrong = "HPKE plaintext must be at least 1 byte";
    }
    if (wrong != NULL) {
        return net_error(error, wrong);
    }
    if (hpke_low_order(pub->elems)) {
        return net_error(error, CRYPTO_LOW_ORDER);
    }
    // Neither the encapsulated key nor the ciphertext is secret.
    size_t total = HPKE_ENC + (size_t)plain->len + CRYPTO_TAG;
    unsigned char *sealed = lugha_rt_alloc((int64_t)total);
    size_t enclen = HPKE_ENC;
    size_t ctlen = total - HPKE_ENC;
    ERR_clear_error();
    OSSL_HPKE_CTX *ctx =
        OSSL_HPKE_CTX_new(OSSL_HPKE_MODE_BASE, hpke_suite(), OSSL_HPKE_ROLE_SENDER, NULL, NULL);
    int ok = ctx != NULL &&
             OSSL_HPKE_encap(ctx, sealed, &enclen, pub->elems, CRYPTO_KEY, info->elems,
                             (size_t)info->len) == 1 &&
             enclen == HPKE_ENC &&
             OSSL_HPKE_seal(ctx, sealed + HPKE_ENC, &ctlen, aad->elems, (size_t)aad->len,
                            plain->elems, (size_t)plain->len) == 1 &&
             ctlen == total - HPKE_ENC;
    // Freeing the context clears the ephemeral secret and the derived keys.
    OSSL_HPKE_CTX_free(ctx);
    if (!ok) {
        return crypto_error(error, "sealing failed");
    }
    *out = bytes_of(sealed, (int64_t)total);
    return 1;
}

/* `$os.crypto_hpke_open(secret, info, sealed, aad)`: the plaintext, only
 * once the tag matches; a wrong tag is `CRYPTO_NOT_AUTHENTIC`. */
int32_t lugha_rt_crypto_hpke_open(const LughaArray *secret, const LughaArray *info,
                                  const LughaArray *sealed, const LughaArray *aad,
                                  LughaArray **out, LughaString **error) {
    const char *wrong = hpke_lengths(info, sealed, aad);
    if (secret->len != CRYPTO_KEY) {
        wrong = "secret key must be 32 bytes";
    } else if (sealed->len < HPKE_ENC + CRYPTO_TAG + 1) {
        wrong = "sealed bytes must be at least 49";
    }
    if (wrong != NULL) {
        return net_error(error, wrong);
    }
    if (hpke_low_order(sealed->elems)) {
        return net_error(error, CRYPTO_LOW_ORDER);
    }
    size_t ctlen = (size_t)sealed->len - HPKE_ENC;
    size_t len = ctlen - CRYPTO_TAG;
    // OpenSSL's buffer: plaintext from a forged input never reaches Lugha.
    unsigned char *plain = OPENSSL_malloc(len > 0 ? len : 1);
    size_t ptlen = len;
    ERR_clear_error();
    EVP_PKEY *key =
        EVP_PKEY_new_raw_private_key(EVP_PKEY_X25519, NULL, secret->elems, CRYPTO_KEY);
    OSSL_HPKE_CTX *ctx =
        plain == NULL || key == NULL
            ? NULL
            : OSSL_HPKE_CTX_new(OSSL_HPKE_MODE_BASE, hpke_suite(), OSSL_HPKE_ROLE_RECEIVER,
                                NULL, NULL);
    int ready = ctx != NULL && OSSL_HPKE_decap(ctx, sealed->elems, HPKE_ENC, key, info->elems,
                                               (size_t)info->len) == 1;
    // A wrong secret or info still decapsulates, to other keys: only the
    // tag check below can tell, so its failure is the input not matching.
    int authentic = ready &&
                    OSSL_HPKE_open(ctx, plain, &ptlen, aad->elems, (size_t)aad->len,
                                   sealed->elems + HPKE_ENC, ctlen) == 1 &&
                    ptlen == len;
    OSSL_HPKE_CTX_free(ctx);
    EVP_PKEY_free(key);
    if (authentic) {
        *out = bytes_of(plain, (int64_t)len);
    }
    OPENSSL_clear_free(plain, len > 0 ? len : 1);
    if (!ready) {
        return crypto_error(error, "opening failed");
    }
    if (!authentic) {
        ERR_clear_error();
        return net_error(error, CRYPTO_NOT_AUTHENTIC);
    }
    return 1;
}
