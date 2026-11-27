#!/bin/sh
# Runs what CI built for Android on the device or emulator `adb` reaches:
# the cases in <folder> (each a program beside what it must print), then
# the program suite in <folder>/suite against tests/android/known.txt,
# with tests/suite.sh run on the device itself, from a copy of the
# repository's test folders.
#
#   sh tests/android_run.sh android     (from the repository's root)
#
# Each case runs with the argument `a b`, its .envvars (else nothing) and
# no stdin, in a folder of its own, within a minute: exactly its .stdout,
# its .exit (else 0), and its .stderr somewhere in stderr. `adb` gives the
# program's own exit code and keeps its stdout and stderr apart.
set -u

built=$1
there=/data/local/tmp/lugha
adb shell "rm -rf $there && mkdir -p $there/run $there/tests"
adb push -q "$built" "$there/" > /dev/null
adb push -q tests/suite.sh tests/android "$there/tests/" > /dev/null
adb push -q tests/programs "$there/tests/" > /dev/null
name=$(basename "$built")

status=0
for expected in "$built"/*.stdout; do
    case=$(basename "$expected" .stdout)
    want=0
    if [ -f "$built/$case.exit" ]; then want=$(cat "$built/$case.exit"); fi
    envs=
    if [ -f "$built/$case.envvars" ]; then envs=$(tr '\n' ' ' < "$built/$case.envvars"); fi
    adb shell "mkdir $there/run/$case && cd $there/run/$case && chmod +x ../../$name/$case &&
        env $envs timeout 60 ../../$name/$case 'a b' < /dev/null" > "$built/$case.out" 2> "$built/$case.err"
    got=$?
    if [ "$got" = "$want" ] && cmp -s "$expected" "$built/$case.out" &&
       { [ ! -f "$built/$case.stderr" ] || grep -qF -f "$built/$case.stderr" "$built/$case.err"; }; then
        echo "ok $case"
    else
        echo "FAILED $case: exit $got (want $want)"
        diff -u "$expected" "$built/$case.out"
        cat "$built/$case.err"
        status=1
    fi
done

# Lockdown says what it limited: seccomp, never Landlock, on Android.
report=$(adb shell "cd $there/run && LUGHA_LOCKDOWN=report ../$name/hello 2>&1 > /dev/null")
case $report in
    "lockdown: seccomp applied; files not limited; net refused, proc refused, fs refused"*)
        echo "ok lockdown report" ;;
    *)
        echo "FAILED lockdown report: $report"
        status=1 ;;
esac

# Kotlin calling a Lugha library through JNI, as an app's code would: the
# Kotlin program as DEX, run by Android's own runtime (`app_process`).
if [ -f "$built/jni/classes.dex" ]; then
    jni=$(adb shell "cd $there/$name/jni && app_process -Djava.library.path=$there/$name/jni \
        -cp $there/$name/jni/classes.dex / MainKt 2>&1")
    case $jni in
        *"Panic: index out of bounds"*"four threads answered"*) echo "ok jni" ;;
        *)
            echo "FAILED jni:"
            echo "$jni"
            status=1 ;;
    esac
fi

adb shell "chmod +x $there/$name/suite/*/exe && cd $there &&
    sh tests/suite.sh $name/suite tests/android/known.txt" || status=1
exit $status
