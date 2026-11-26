#!/bin/sh
# Builds OpenSSL for a target lughac builds for, with zig, into <folder>:
# its headers and static libraries, the folder LUGHA_OPENSSL_<TARGET> names.
#
#   sh tests/openssl.sh aarch64-apple-darwin "$HOME/openssl-mac"
#
# The release is pinned and checked against its SHA-256 before it is
# unpacked. Nothing is installed outside <folder>, which must be absolute
# and empty or absent. Needs curl, perl, make and zig 0.17 or newer.
set -eu

VERSION=3.5.4
SHA256=967311f84955316969bdb1d8d4b983718ef42338639c621ec4c34fddef355e99
URL="https://github.com/openssl/openssl/releases/download/openssl-$VERSION/openssl-$VERSION.tar.gz"

if [ $# -ne 2 ]; then
    echo "usage: sh tests/openssl.sh <target> <folder>" >&2
    exit 2
fi
target=$1
out=$2

# zig's name for the target (as lughac's), and OpenSSL's.
case $target in
    aarch64-apple-darwin) zig=aarch64-macos.14.4-none; conf=darwin64-arm64-cc ;;
    x86_64-unknown-linux-musl) zig=x86_64-linux-musl; conf=linux-x86_64 ;;
    aarch64-unknown-linux-musl) zig=aarch64-linux-musl; conf=linux-aarch64 ;;
    x86_64-pc-windows-gnu) zig=x86_64-windows-gnu; conf=mingw64 ;;
    *)
        echo "openssl.sh: no recipe for $target" >&2
        exit 2
        ;;
esac

case $out in
    /*) ;;
    *)
        echo "openssl.sh: $out isn't an absolute path" >&2
        exit 2
        ;;
esac
if [ -e "$out" ] && [ -n "$(ls -A "$out")" ]; then
    echo "openssl.sh: $out isn't empty" >&2
    exit 2
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl -fsSL -o "$work/openssl.tar.gz" "$URL"
if command -v sha256sum >/dev/null 2>&1; then
    sum=$(sha256sum "$work/openssl.tar.gz")
else
    sum=$(shasum -a 256 "$work/openssl.tar.gz")
fi
if [ "${sum%% *}" != "$SHA256" ]; then
    echo "openssl.sh: openssl-$VERSION.tar.gz doesn't match its SHA-256" >&2
    exit 1
fi
tar -xzf "$work/openssl.tar.gz" -C "$work"

jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)
cd "$work/openssl-$VERSION"
# Static libraries only, no programs, no loadable modules: what a program
# links. The roots folder is where macOS keeps OpenSSL's (`cert.pem`).
CC="zig cc -target $zig" AR="zig ar" RANLIB="zig ranlib" \
    ./Configure "$conf" no-shared no-module no-apps no-docs no-tests \
    --prefix="$out" --libdir=lib --openssldir=/etc/ssl
make -j"$jobs" build_libs
make install_dev
echo "OpenSSL $VERSION for $target is in $out"
