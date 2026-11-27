#!/bin/sh
# Fetches Kotlin's compiler, the release lughac's `--jni` tests compile
# with, into <folder>: its `kotlinc/bin` then goes on PATH.
#
#   sh tests/kotlin.sh "$HOME/kotlin"
#   export PATH="$HOME/kotlin/kotlinc/bin:$PATH"
#
# The release is pinned and checked against its SHA-256 before it is
# unpacked. Nothing is installed outside <folder>, which must be absolute.
# Needs curl, unzip and a JDK to run it.
set -eu

VERSION=2.4.21
SHA256=7cc140e76daf416a0424a5557f99afd7ca89ebd463ea1101261e3e01c33e75cd
URL="https://github.com/JetBrains/kotlin/releases/download/v$VERSION/kotlin-compiler-$VERSION.zip"

if [ $# -ne 1 ]; then
    echo "usage: sh tests/kotlin.sh <folder>" >&2
    exit 2
fi
out=$1
case $out in
    /*) ;;
    *)
        echo "kotlin.sh: $out isn't an absolute path" >&2
        exit 2
        ;;
esac
if [ -e "$out/kotlinc" ]; then
    echo "kotlin.sh: $out/kotlinc is there already" >&2
    exit 2
fi

mkdir -p "$out"
zip="$out/kotlin-compiler-$VERSION.zip"
curl -fsSL -o "$zip" "$URL"
sum=$(sha256sum "$zip")
if [ "${sum%% *}" != "$SHA256" ]; then
    rm -f "$zip"
    echo "kotlin.sh: kotlin-compiler-$VERSION.zip doesn't match its SHA-256" >&2
    exit 1
fi
unzip -q "$zip" -d "$out"
rm -f "$zip"
echo "Kotlin $VERSION is in $out/kotlinc"
