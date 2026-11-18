#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/screensmoke.sh -- EMULATOR SMOKE OF A fUi SCREEN APP: start, wait, screenshot, crash check.
#
#   /root/jarvis/bin/heavy bash tools/android/screensmoke.sh <apk> <package> <outdir>
#
# Needs the AVD template a31e (as examples/bench_android/smoke.sh). Prints SMOKE <package> PASS|FAIL.
set -uo pipefail
APK=${1:?apk}; PKG=${2:?package}; SHOTS=${3:?outdir}
. "$(dirname "$0")/dismiss_anr.sh"
. "$(dirname "$0")/emulator.sh"
mkdir -p "$SHOTS"
emu_start
trap emu_stop EXIT
$ADB install -r "$APK" >/dev/null || { echo "SMOKE $PKG FAIL install"; exit 1; }
$ADB logcat -c
$ADB shell am start -W -n "$PKG/android.app.NativeActivity" | grep -E "TotalTime|WaitTime" | tr '\n' ' '; echo
sleep "${WAIT:-12}"
dismiss_anr && sleep 3
$ADB exec-out screencap -p >"$SHOTS/$PKG.png"
$ADB logcat -d >"$SHOTS/$PKG.log"
PID=$($ADB shell pidof "$PKG" | tr -d '\r')
BAD=$(grep -cE "FATAL|AndroidRuntime|signal 11|SIGSEGV|Fatal signal" "$SHOTS/$PKG.log")
if [ -n "$PID" ] && [ "$BAD" = 0 ]; then echo "SMOKE $PKG PASS pid=$PID"; else echo "SMOKE $PKG FAIL pid=${PID:-none} crashlines=$BAD"; exit 1; fi
