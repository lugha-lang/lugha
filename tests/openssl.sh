#!/bin/sh
# Builds OpenSSL for a target lughac builds for, with zig, into <folder>:
# its headers and static libraries, the folder LUGHA_OPENSSL_<TARGET> names.
# For Android, with the NDK $ANDROID_NDK_HOME (else $ANDROID_NDK_ROOT)
# names instead, for Android 10 (API 29), as lughac builds.
#
#   sh tests/openssl.sh aarch64-apple-darwin "$HOME/openssl-mac"
#
# The release is pinned and checked against its SHA-256 before it is
# unpacked. Nothing is installed outside <folder>, which must be absolute
# and empty or absent. Needs curl, perl, make and zig 0.17 or newer (or,
# for Android, the NDK).
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
    aarch64-pc-windows-gnu) zig=aarch64-windows-gnu; conf=mingwarm64 ;;
    aarch64-linux-android) zig=; conf=android-arm64 ;;
    x86_64-linux-android) zig=; conf=android-x86_64 ;;
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

# This release has no MinGW target for ARM64; the next (3.6.0) added
# `mingwarm64`, given here word for word. This release's assembler scripts
# already speak its `win64` flavour, so AES, SHA and bignums keep ARMv8's
# instructions.
extra=
if [ "$conf" = mingwarm64 ]; then
    cat > "$work/mingwarm64.conf" <<'CONF'
my %targets = (
    "mingwarm64" => {
        inherit_from     => [ "mingw-common" ],
        cflags           => "",
        sys_id           => "MINGWARM64",
        bn_ops           => add("SIXTY_FOUR_BIT"),
        asm_arch         => 'aarch64',
        uplink_arch      => 'armv8',
        perlasm_scheme   => "win64",
        shared_rcflag    => "",
        multilib         => "-arm64",
    },
);
CONF
    extra="--config=$work/mingwarm64.conf"
fi

jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)
cd "$work/openssl-$VERSION"
# Static libraries only, no programs, no loadable modules: what a program
# links. The roots folder is where macOS keeps OpenSSL's (`cert.pem`).
if [ -z "$zig" ]; then
    # OpenSSL's own Android targets find the NDK's clang on PATH.
    ndk=${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}
    if [ -z "$ndk" ]; then
        echo "openssl.sh: building for $target needs \$ANDROID_NDK_HOME or \$ANDROID_NDK_ROOT" >&2
        exit 2
    fi
    PATH="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH" ANDROID_NDK_ROOT="$ndk" \
        ./Configure "$conf" -D__ANDROID_API__=29 no-shared no-module no-apps no-docs \
        no-tests --prefix="$out" --libdir=lib --openssldir=/etc/ssl
    PATH="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH" make -j"$jobs" build_libs
else
    CC="zig cc -target $zig" AR="zig ar" RANLIB="zig ranlib" \
        ./Configure $extra "$conf" no-shared no-module no-apps no-docs no-tests \
        --prefix="$out" --libdir=lib --openssldir=/etc/ssl
    make -j"$jobs" build_libs
fi
make install_dev
echo "OpenSSL $VERSION for $target is in $out"
