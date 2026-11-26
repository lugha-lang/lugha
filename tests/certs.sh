#!/bin/sh
# Makes the certificates tests/macos/openssl/trust.la and
# tests/windows/openssl/trust.la read, into <folder>,
# with the `openssl` command, fresh for each run, so no key is committed:
#
#   ca.pem             a test CA (ca.key, its key)
#   cert.pem, key.pem  for localhost and 127.0.0.1, signed by the CA
#   other.pem          for other.test, signed by the CA with key.pem
#   old.pem            expired, signed by the CA with key.pem
#   self.pem, self.key self-signed, for 127.0.0.1
#
#   sh tests/certs.sh macos/trust.certs
set -eu

if [ $# -ne 1 ]; then
    echo "usage: sh tests/certs.sh <folder>" >&2
    exit 2
fi
mkdir -p "$1"
cd "$1"
quiet() { "$@" > /dev/null 2>&1 || { echo "certs.sh: $* failed" >&2; exit 1; }; }

# macOS and Windows refuse a TLS server's certificate without serverAuth in its
# extended key usage, even under a root the program adds.
printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n' > local.ext
printf 'subjectAltName=DNS:other.test\nextendedKeyUsage=serverAuth\n' > other.ext
quiet openssl req -x509 -newkey rsa:2048 -nodes -keyout ca.key -out ca.pem -days 2 \
    -subj "/CN=Lugha Test CA"
quiet openssl req -newkey rsa:2048 -nodes -keyout key.pem -out local.csr -subj /CN=localhost
sign() {
    out=$1
    ext=$2
    shift 2
    quiet openssl x509 -req -in local.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
        -out "$out" -extfile "$ext" "$@"
}
sign cert.pem local.ext -days 2
sign other.pem other.ext -days 2
sign old.pem local.ext -not_before 20200101000000Z -not_after 20200102000000Z
quiet openssl req -x509 -newkey rsa:2048 -nodes -keyout self.key -out self.pem -days 2 \
    -subj /CN=localhost -addext subjectAltName=IP:127.0.0.1 -addext extendedKeyUsage=serverAuth
rm -f local.csr local.ext other.ext ca.srl
