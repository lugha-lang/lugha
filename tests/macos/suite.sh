#!/bin/sh
# Runs the program suite tests/macos_suite.rs built, as tests/programs.rs
# runs each case on Linux: one worker, the case's environment, arguments
# and stdin (or an empty one), from a fresh folder or the repository's
# root; stdout, the exit code and any expected stderr compared byte for
# byte. Known differences (tests/macos/known.txt) are expected to fail.
#
#   sh tests/macos/suite.sh <suite folder>     (from the repository's root)
#
# POSIX sh and perl only: a Mac runner has both and nothing else is needed.

set -u
suite=$(cd "$1" && pwd)
root=$(pwd)
known="$root/tests/macos/known.txt"
seconds=${LUGHA_SUITE_SECONDS:-30}
passed=0 failed=0 expected=0 fixed=0

# perl's alarm outlives its exec: a case still running is ended by SIGALRM.
limit() { perl -e 'alarm shift; exec @ARGV or die "cannot run $ARGV[0]: $!\n"' "$seconds" "$@"; }

is_known() {
    awk -v n="$1" 'index($0, n " — ") == 1 { found = 1 } END { exit !found }' "$known"
}

# Prints how `got` differs from `want`, at most 40 lines.
differs() {
    echo "  $1:"
    diff -u "$2" "$3" | sed -n '3,42s/^/    /p'
}

for dir in "$suite"/*/; do
    dir=${dir%/}
    name=$(cat "$dir/name")
    out=$(mktemp -d)
    cwd=$root
    if [ -f "$dir/scratch" ]; then
        cwd=$(mktemp -d)
    fi
    stdin=/dev/null
    if [ -f "$dir/stdin" ]; then
        stdin=$dir/stdin
    fi
    set -- env LUGHA_WORKERS=1
    if [ -f "$dir/envvars" ]; then
        while IFS= read -r line || [ -n "$line" ]; do
            [ -n "$line" ] && set -- "$@" "$line"
        done < "$dir/envvars"
    fi
    set -- "$@" "$dir/exe"
    if [ -f "$dir/args" ]; then
        while IFS= read -r line || [ -n "$line" ]; do
            set -- "$@" "$line"
        done < "$dir/args"
    fi
    (cd "$cwd" && limit "$@" < "$stdin" > "$out/stdout" 2> "$out/stderr")
    code=$?
    want=$(cat "$dir/exit")
    ok=true
    [ "$code" = "$want" ] || ok=false
    cmp -s "$dir/stdout" "$out/stdout" || ok=false
    if [ -f "$dir/stderr" ]; then
        cmp -s "$dir/stderr" "$out/stderr" || ok=false
    fi
    if $ok; then
        if is_known "$name"; then
            echo "PASSES $name: on tests/macos/known.txt, but passes"
            fixed=$((fixed + 1))
        else
            passed=$((passed + 1))
        fi
    elif is_known "$name"; then
        echo "known $name"
        expected=$((expected + 1))
    else
        echo "FAILED $name"
        [ "$code" = "$want" ] || echo "  exit code: want $want, got $code"
        cmp -s "$dir/stdout" "$out/stdout" || differs stdout "$dir/stdout" "$out/stdout"
        if [ -f "$dir/stderr" ]; then
            cmp -s "$dir/stderr" "$out/stderr" || differs stderr "$dir/stderr" "$out/stderr"
        elif [ "$code" != "$want" ]; then
            sed -n '1,10s/^/  stderr: /p' "$out/stderr"
        fi
        failed=$((failed + 1))
    fi
    rm -rf "$out"
    [ "$cwd" = "$root" ] || rm -rf "$cwd"
done

if [ -s "$suite/skipped.txt" ]; then
    sed 's/^/skipped /' "$suite/skipped.txt"
fi
skipped=$(wc -l < "$suite/skipped.txt" | tr -d ' ')
echo "$passed passed, $failed failed, $expected known, $fixed known but passing, $skipped skipped"
[ "$failed" -eq 0 ] && [ "$fixed" -eq 0 ]
