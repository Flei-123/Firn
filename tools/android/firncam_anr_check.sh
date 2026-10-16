#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/firncam_anr_check.sh -- GATE: the FirnCam UI thread must never block (ANR "FirnCam is not responding").
#
# Android's input dispatcher gives an app 5 s to read a touch event; FirnCam is a NativeActivity whose main loop (the app
# thread) reads the input queue, so every turn of that loop that takes seconds is an ANR waiting to happen. The app logs
#   firncam: gap ms=N phase=P        one line per main-loop turn longer than 100 ms (P = what ran, P_* in main.fi)
#   firncam: ... max_gap=N gap_phase=P   every 3 s: the longest turn since the last line
# This script installs a FirnCam APK on an emulator, starts it with the zoom sweep FORCED (setprop debug.firncam.sweep 1:
# an emulator camera is no logical multi camera, so the sweep would not run otherwise), taps the screen every 2 s like an
# impatient user, and checks
#   * no am_anr event for the package (the real Android ANR detector)
#   * the longest main-loop turn <= MAX_GAP_MS (default 150; the goal is 100, an emulator under host load jitters)
#   * the app is still alive at the end
# Then it presses HOME, comes back, and runs the same two checks again (pause/resume must not block either).
#
#   SERIAL=emulator-5584 bash tools/android/firncam_anr_check.sh            (builds the APK itself, x86_64)
#   SERIAL=emulator-5584 APK=f.apk bash tools/android/firncam_anr_check.sh   (use a finished x86_64 APK)
# Environment: ADB, SERIAL, APK, FIRNC, MAX_GAP_MS, RUN_SECONDS (default 24). Exit 0 = pass.
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PKG=org.firn.firncam
MAX_GAP_MS=${MAX_GAP_MS:-150}
RUN_SECONDS=${RUN_SECONDS:-24}
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
FAIL=0
ok()  { echo "   OK      $1"; }
bad() { echo "   FAILED  $1"; FAIL=1; }

APKF=${APK:-}
if [ -z "$APKF" ]; then
    export FIRNLIB=$ROOT/lib FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
    bash "$ROOT/tools/android/build.sh" "$ROOT/examples/firncam/main.fi" --camera --name FirnCam --package $PKG \
        --version-code 1 --version-name gate --abi x86_64 --out "$D/f.apk" > "$D/build.log" 2>&1 \
        || { tail -20 "$D/build.log"; echo "FAIL: build"; exit 1; }
    APKF="$D/f.apk"
fi
$ADB wait-for-device
$ADB install -r "$APKF" > /dev/null 2>&1 || { echo "FAIL: install"; exit 1; }
$ADB shell pm grant $PKG android.permission.CAMERA > /dev/null 2>&1
$ADB shell setprop debug.firncam.sweep 1
# a clean start: no cached sweep result (the cache file of the app, when this build has one)
$ADB shell am force-stop $PKG
$ADB shell "run-as $PKG rm -f files/firncam-learned.txt" > /dev/null 2>&1
$ADB shell "rm -f /sdcard/Android/data/$PKG/files/firncam-learned.txt" > /dev/null 2>&1

phase() { # name: run the app for RUN_SECONDS with taps, then check
    local name=$1
    local i=0
    while [ $i -lt $((RUN_SECONDS / 2)) ]; do
        sleep 2
        $ADB shell input tap 240 400
        i=$((i + 1))
    done
    $ADB logcat -d -s firncam:I > "$D/log_$name.txt"
    $ADB logcat -b events -d > "$D/ev_$name.txt"
    local gap; gap=$(grep -ao 'gap ms=[0-9]*' "$D/log_$name.txt" | sed 's/gap ms=//' | sort -n | tail -1)
    local mg; mg=$(grep -ao 'max_gap=[0-9]*' "$D/log_$name.txt" | sed 's/max_gap=//' | sort -n | tail -1)
    local worst=${gap:-0}; [ "${mg:-0}" -gt "$worst" ] && worst=${mg:-0}
    local lines; lines=$(grep -ac 'ui_fps=' "$D/log_$name.txt")
    echo "   [$name] stats lines=$lines  longest gap=${worst} ms  (limit $MAX_GAP_MS)"
    grep -a 'gap ms=' "$D/log_$name.txt" | head -5 | cut -c1-120
    [ "$lines" -ge 2 ] && ok "$name: app runs and logs" || bad "$name: no stats lines (app dead or never ready)"
    [ "$worst" -le "$MAX_GAP_MS" ] && ok "$name: UI thread never blocked > $MAX_GAP_MS ms" || bad "$name: UI thread blocked ${worst} ms"
    if grep -a 'am_anr' "$D/ev_$name.txt" | grep -aq "$PKG"; then bad "$name: Android reported an ANR"; else ok "$name: no ANR event"; fi
}

echo "== 1. cold start with forced zoom sweep, tapping =="
$ADB logcat -c; $ADB logcat -b events -c
$ADB shell am start -n $PKG/android.app.NativeActivity > /dev/null
phase start

echo "== 2. HOME, back, tapping =="
$ADB shell input keyevent KEYCODE_HOME
sleep 3
$ADB logcat -c; $ADB logcat -b events -c
$ADB shell am start -n $PKG/android.app.NativeActivity > /dev/null
phase resume

$ADB shell setprop debug.firncam.sweep 0
$ADB shell am force-stop $PKG
[ $FAIL = 0 ] && echo "PASS: firncam_anr_check" || echo "FAIL: firncam_anr_check"
exit $FAIL
