#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/accept.sh -- the Android acceptance run: gallery, ime inject, keyboard and swipe checks one after
# another on the SAME emulator, one summary line at the end.
#
#   bash tools/android/accept.sh                    boots its own emulator (port ACCEPT_PORT, default 5590) and kills it at the end
#   SERIAL=emulator-5554 bash tools/android/accept.sh   uses a running device
#   ACCEPT_ONLY="swipe ime" bash tools/android/accept.sh   runs only the named checks (gallery ime keyboard swipe)
#   OUT=<dir> keeps the logs of every check (default: a temp dir that is removed)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export ADB=${ADB:-/root/android-sdk/platform-tools/adb}
export FIRNLIB=$ROOT/lib FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
KEEP_OUT=${OUT:-}
WORK=${OUT:-$(mktemp -d)}
mkdir -p "$WORK"
ONLY=${ACCEPT_ONLY:-"gallery ime keyboard swipe"}
EMU_PID=""
OWN_SERIAL=""

cleanup() {
    if [ -n "$OWN_SERIAL" ]; then
        $ADB -s "$OWN_SERIAL" emu kill >/dev/null 2>&1
        [ -n "$EMU_PID" ] && wait "$EMU_PID" 2>/dev/null
    fi
    [ -z "$KEEP_OUT" ] && rm -rf "$WORK"
}
trap cleanup EXIT

boot_emulator() {
    local port=${ACCEPT_PORT:-5590}
    OWN_SERIAL=emulator-$port
    ANDROID_AVD_HOME=${ANDROID_AVD_HOME:-$HOME/.android/avd} /root/android-sdk/emulator/emulator -avd "${ACCEPT_AVD:-a13e}" \
        -port "$port" -read-only -no-snapshot -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect \
        > "$WORK/emulator.log" 2>&1 &
    EMU_PID=$!
    export SERIAL=$OWN_SERIAL
    local n=0
    until [ "$($ADB -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do
        n=$((n + 1))
        if [ $n -gt 90 ]; then
            echo "emulator did not boot"
            exit 2
        fi
        sleep 4
    done
    sleep 5
}

[ -z "${SERIAL:-}" ] && boot_emulator
export SERIAL
echo "== device $SERIAL =="

declare -A RESULT
declare -A WHY
PASS=0
FAIL=0

run_check() {
    local name=$1
    shift
    case " $ONLY " in *" $name "*) ;; *) return ;; esac
    echo "== $name =="
    local log=$WORK/$name.log
    mkdir -p "$WORK/$name-out"
    OUT=$WORK/$name-out "$@" > "$log" 2>&1
    local rc=$?
    local fails
    fails=$(grep -cE '^\s*(FAILED|FAIL)\b' "$log")
    if [ $rc -eq 0 ] && [ "$fails" = 0 ]; then
        RESULT[$name]=PASS
        PASS=$((PASS + 1))
    else
        RESULT[$name]=FAIL
        FAIL=$((FAIL + 1))
        WHY[$name]=$(grep -E '^\s*(FAILED|FAIL)\b' "$log" | head -3 | tr '\n' ';')
        echo "  $name failed (rc=$rc): ${WHY[$name]}"
        tail -5 "$log" | sed 's/^/  | /'
    fi
    tail -2 "$log"
}

build_gallery() {
    bash tools/android/build.sh demos/x11demo/main.fi --name "fUi Galerie" --package org.firn.gallery --abi x86_64 2>&1 | tail -1
    bash tools/android/gallery_check.sh "$ROOT/build/android/fui_galerie/fui_galerie.apk"
}

run_check gallery build_gallery
run_check ime bash tools/android/ime_inject_check.sh
run_check keyboard bash tools/android/keyboard_check.sh
run_check swipe bash tools/android/swipe_check.sh

SUMMARY=""
for n in gallery ime keyboard swipe; do
    [ -n "${RESULT[$n]:-}" ] && SUMMARY="$SUMMARY $n=${RESULT[$n]}"
done
echo "ANDROID ACCEPT: $PASS passed, $FAIL failed ($SUMMARY )"
[ $FAIL -eq 0 ]
