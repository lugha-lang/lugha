#!/bin/sh
# Fetches Android's NDK, the release lughac's Android builds are tested
# with, into <folder>: the folder ANDROID_NDK_HOME then names.
#
#   sh tests/ndk.sh "$HOME/Android/ndk"
#   export ANDROID_NDK_HOME="$HOME/Android/ndk/android-ndk-r29"
#
# The release is pinned and checked against its SHA-256 before it is
# unpacked. Nothing is installed outside <folder>, which must be absolute.
# Needs curl and unzip; the NDK's tools run on x86-64 Linux.
set -eu

RELEASE=r29
SHA256=4abbbcdc842f3d4879206e9695d52709603e52dd68d3c1fff04b3b5e7a308ecf
URL="https://dl.google.com/android/repository/android-ndk-$RELEASE-linux.zip"

if [ $# -ne 1 ]; then
    echo "usage: sh tests/ndk.sh <folder>" >&2
    exit 2
fi
out=$1
case $out in
    /*) ;;
    *)
        echo "ndk.sh: $out isn't an absolute path" >&2
        exit 2
        ;;
esac
if [ -e "$out/android-ndk-$RELEASE" ]; then
    echo "ndk.sh: $out/android-ndk-$RELEASE is there already" >&2
    exit 2
fi

mkdir -p "$out"
zip="$out/android-ndk-$RELEASE.zip"
curl -fsSL -o "$zip" "$URL"
sum=$(sha256sum "$zip")
if [ "${sum%% *}" != "$SHA256" ]; then
    rm -f "$zip"
    echo "ndk.sh: android-ndk-$RELEASE-linux.zip doesn't match its SHA-256" >&2
    exit 1
fi
unzip -q "$zip" -d "$out"
rm -f "$zip"
echo "the NDK $RELEASE is in $out/android-ndk-$RELEASE"
