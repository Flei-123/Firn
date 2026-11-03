#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/firncam_queue_check.sh -- GATE: FirnCam background job queue on an emulator.
# Three shutter presses in quick succession must give three processed pictures (FIRNCAM_T "final total"),
# the shutter must be free again before the first job ends, and the app must survive leaving with jobs pending.
#   SERIAL=emulator-5592 APK=f.apk bash tools/android/firncam_queue_check.sh
set -u
SERIAL=${SERIAL:-emulator-5554}
APKF=${APK:?APK=<x86_64 apk>}
PKG=org.firn.firncam
ADB="adb -s $SERIAL"
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
FAIL=0
ok() { echo "ok: $*"; }
bad() { echo "BAD: $*"; FAIL=1; }

$ADB wait-for-device
$ADB install -r "$APKF" > /dev/null 2>&1 || { echo "FAIL: install"; exit 1; }
$ADB shell pm grant $PKG android.permission.CAMERA > /dev/null 2>&1
SZ=$($ADB shell wm size | grep -o '[0-9]*x[0-9]*' | tail -1); SW=${SZ%x*}; SH=${SZ#*x}
tapf() { $ADB shell input tap $(( SW * $1 / 1080 )) $(( SH * $2 / 1920 )); }

$ADB shell am force-stop $PKG
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity > /dev/null
sleep 14
for i in 1 2 3; do tapf 540 1705; sleep 2; done
sleep 45
$ADB logcat -d -s FIRNCAM_T:I firncam:I > "$D/log.txt"
finals=$(grep -ac 'final total ms=' "$D/log.txt")
shots=$(grep -ac 'work op=2 stat=0' "$D/log.txt")
echo "captures=$shots finals=$finals"
grep -a 'final total\|preview total\|work op=2' "$D/log.txt" | cut -c1-110 | head -12
[ "$shots" -ge 2 ] && ok "shutter free again during processing ($shots captures)" || bad "only $shots captures"
[ "$finals" -ge "$shots" ] && [ "$finals" -ge 2 ] && ok "every capture processed ($finals)" || bad "processed $finals of $shots"
$ADB shell pidof $PKG > /dev/null && ok "app alive" || bad "app died"

$ADB logcat -c
tapf 540 1705; sleep 3; tapf 540 1705; sleep 2
$ADB shell input keyevent KEYCODE_HOME
sleep 30
$ADB logcat -d -s FIRNCAM_T:I firncam:I AndroidRuntime:E DEBUG:F > "$D/log2.txt"
if grep -aq 'Fatal signal\|FATAL EXCEPTION' "$D/log2.txt"; then bad "crash after leaving with jobs pending"; else ok "no crash after leaving with jobs pending"; fi
echo "after HOME: finals=$(grep -ac 'final total ms=' "$D/log2.txt")"
[ $FAIL = 0 ] && echo "PASS: firncam_queue_check" || echo "FAIL: firncam_queue_check"
exit $FAIL
