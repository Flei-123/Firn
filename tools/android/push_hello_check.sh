#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/push_hello_check.sh -- push_set_hello of lib/plat/android/push.fi
# on a device/emulator (demos/pushdemo with a hello line as 5th argument).
#
#   bash tools/android/push_hello_check.sh [apk]     (ADB, SERIAL, PORT from env)
#
# Checks: the relay receives the hello line right after the connect (and again
# after a reconnect), a line "<title>: <body>" becomes title + text, a line
# without ": " keeps the fixed title, no crash. Without a hello nothing changes
# (tools/android/push_check.sh and firnchat_push_check.sh cover that).
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PORT=${PORT:-7791}
PKG=org.firn.pushdemo
APK=${1:-$ROOT/build/android/firn_push/firn_push.apk}
D=/sdcard/Android/data/$PKG/files
W=$(mktemp -d "${TMPDIR:-/tmp}/pushhello.XXXXXX")
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  OK    $1 (got $2)"; PASS=$((PASS+1));
          else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
stop_relay() { pgrep -f "^python3 $ROOT/tools/android/relay_test.py $PORT" | xargs -r kill 2>/dev/null; sleep 1; }
cleanup() { stop_relay; $ADB shell am force-stop $PKG >/dev/null 2>&1; rm -f "$W/args.txt"; $ADB shell rm -f $D/args.txt >/dev/null 2>&1; rm -rf "$W"; }
trap cleanup EXIT
relay() { stop_relay; setsid nohup python3 "$ROOT/tools/android/relay_test.py" "$PORT" --spacing 1 "$@" >> "$W/relay.log" 2>&1 & sleep 1; }
# notification text lines of this package
note_has() { $ADB shell dumpsys notification --noredact | grep -c "android.$1=String ($2)"; }

T=$(date +%s)
TOKEN=0123456789abcdef0123456789abcdef
HELLO="HELLO $TOKEN 1"
$ADB root >/dev/null 2>&1; sleep 1
$ADB shell am force-stop $PKG
$ADB install -r "$APK" >/dev/null || { echo "install failed"; exit 1; }
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS
$ADB shell cmd notification cancel_all $PKG >/dev/null 2>&1 || true
# host, port, name server (-: none), pin (-: none), hello
printf '10.0.2.2\n%s\n-\n-\n%s\n' "$PORT" "$HELLO" > "$W/args.txt"
$ADB shell mkdir -p $D
$ADB push "$W/args.txt" $D/args.txt >/dev/null
relay "Call $T: pick up Grandpa" "no title $T"
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 10
check "relay got the hello line" "$(grep -c "^recv $HELLO\$" "$W/relay.log")" 1
check "title split: title" "$(note_has title "Call $T")" 1
check "title split: text" "$(note_has text "pick up Grandpa")" 1
check "no ': ' -> whole line as text" "$(note_has text "no title $T")" 1
# reconnect: a new connection must say hello again
relay "two $T: after restart"
sleep 12
check "hello again after reconnect" "$(grep -c "^recv $HELLO\$" "$W/relay.log")" 2
check "line after reconnect" "$(note_has text "after restart")" 1
check "no crash" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|panic|ANR in '$PKG)" 0
echo "ANDROID PUSH HELLO: $PASS passed, $FAIL failed"
[ $FAIL -eq 0 ]
