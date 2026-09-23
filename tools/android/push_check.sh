#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/push_check.sh -- demos/pushdemo on a device/emulator: the
# foreground service written by Firn, the own TCP connection, one
# notification per line -- also after Back closed the activity.
#
#   bash tools/android/push_check.sh [apk]     (ADB, SERIAL, PORT from env)
#
# The relay is tools/android/relay_test.py on THIS machine; the emulator
# reaches it as 10.0.2.2 (the demo's default). For a phone, push args.txt
# with "<ip>\n<port>\n" into the app's files dir first.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PORT=${PORT:-7790}
PKG=org.firn.pushdemo
APK=${1:-$ROOT/build/android/firn_push/firn_push.apk}
LOG=$ROOT/build/android/belege; mkdir -p "$LOG"
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  OK    $1 (got $2)"; PASS=$((PASS+1));
          else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
texts() { $ADB shell dumpsys notification --noredact | grep -c "android.text=String ($1)"; }
relay() { pgrep -f "^python3 $ROOT/tools/android/relay_test.py $PORT" | xargs -r kill 2>/dev/null
          sleep 1; setsid nohup python3 "$ROOT/tools/android/relay_test.py" "$PORT" --spacing 1 "$@" \
              >> "$LOG/relay.log" 2>&1 & }

$ADB root >/dev/null 2>&1; sleep 1
$ADB shell am force-stop $PKG
$ADB install -r "$APK" >/dev/null || { echo "install failed"; exit 1; }
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS
$ADB shell cmd notification cancel_all $PKG >/dev/null 2>&1 || true
T=$(date +%s)
relay "check-$T eins" "check-$T zwei"
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 10
check "service is foreground (remoteMessaging)" \
    "$($ADB shell dumpsys activity services $PKG | grep -c 'isForeground=true.*types=0x00000200')" 1
check "line 1 -> notification" "$(texts "check-$T eins")" 1
check "line 2 -> notification" "$(texts "check-$T zwei")" 1
P1=$($ADB shell pidof $PKG)
$ADB shell input keyevent KEYCODE_BACK; sleep 3
check "Back: activity gone, process stays" "$($ADB shell pidof $PKG)" "$P1"
relay "check-$T nach Back"; sleep 12
check "after Back: reconnect + notification" "$(texts "check-$T nach Back")" 1
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 4
check "reopen: same process" "$($ADB shell pidof $PKG)" "$P1"
check "reopen: activity resumed" \
    "$($ADB shell dumpsys activity activities | grep -c "topResumedActivity=.*$PKG")" 1
check "no crash" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|panic|ANR in '$PKG)" 0
$ADB shell cat /sdcard/Android/data/$PKG/files/stdout.txt 2>/dev/null | head -3
pgrep -f "^python3 $ROOT/tools/android/relay_test.py $PORT" | xargs -r kill 2>/dev/null
echo "ANDROID PUSH: $PASS passed, $FAIL failed"
[ $FAIL -eq 0 ]
