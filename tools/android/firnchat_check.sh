#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/firnchat_check.sh -- FIRNCHAT's window (src/gui/app.fi) as an
# APK on the emulator, against a FIRNCHAT relay on this machine: the live
# session, the end-to-end encrypted message, and the notification when the
# window is not in front -- also after Back, while only the foreground
# service keeps the process. No Firebase: the relay session IS the push.
#
#   bash tools/android/firnchat_check.sh <firnchat checkout> [apk]
#
# Uses a relay of its own on RELAY_PORT (default 7781) with identities in a
# temporary directory -- never the relay on 7771 and never real accounts.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FCDIR=${1:?firnchat checkout}
APK=${2:-$ROOT/build/android/firnchat/firnchat.apk}
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PORT=${RELAY_PORT:-7781}
PKG=org.firn.firnchat
D=/sdcard/Android/data/$PKG/files
FC=$FCDIR/bin/firnchat
W=$(mktemp -d /tmp/fcchk.XXXX)
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  OK    $1 (got $2)"; PASS=$((PASS+1));
          else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
posted() { $ADB logcat -d | grep -c 'firn-push: notification posted'; }

[ -x "$FC" ] || { echo "no $FC (build FIRNCHAT first)"; exit 2; }
(cd "$W" && setsid nohup "$FC" serve "$PORT" "$W/data" 0 > "$W/relay.log" 2>&1 &)
sleep 1
(cd "$W" && "$FC" id alice.id >/dev/null && "$FC" id bob.id > bob.txt \
    && "$FC" name Alice "$PORT" alice.id >/dev/null \
    && "$FC" name "Bob Handy" "$PORT" bob.id >/dev/null)
BOB=$(grep -oP '^key \K[0-9a-f]+' "$W/bob.txt")
(cd "$W" && "$FC" send "$BOB" "Hallo Bob, erste Nachricht" "$PORT" alice.id >/dev/null)

$ADB root >/dev/null 2>&1; sleep 1
$ADB install -r "$APK" >/dev/null || { echo "install failed"; exit 1; }
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 3
$ADB shell am force-stop $PKG
$ADB shell "rm -f $D/bob.id*"   # pins and session state of an older relay
$ADB push "$W/bob.id" $D/bob.id >/dev/null
printf -- '--id\n%s/bob.id\n--host\n10.0.2.2\n--port\n%s\n--font\n/system/fonts/RobotoStatic-Regular.ttf\n' \
    "$D" "$PORT" > "$W/args.txt"
$ADB push "$W/args.txt" $D/args.txt >/dev/null
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 12
check "relay: watch session from the phone" "$(grep -c 'watch session opened' "$W/relay.log" | awk '{print ($1>0)?"yes":"no"}')" yes
check "foreground service (remoteMessaging)" \
    "$($ADB shell dumpsys activity services $PKG | grep -c 'isForeground=true.*types=0x00000200')" 1
$ADB exec-out screencap -p > "$ROOT/build/android/belege/firnchat-live.png"
P1=$($ADB shell pidof $PKG)
n0=$(posted)
$ADB shell input keyevent KEYCODE_HOME; sleep 2
(cd "$W" && "$FC" send "$BOB" "Nachricht bei Home" "$PORT" alice.id >/dev/null); sleep 6
check "Home + message -> notification" "$(( $(posted) > n0 ))" 1
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 3
n1=$(posted)
(cd "$W" && "$FC" send "$BOB" "Nachricht im Vordergrund" "$PORT" alice.id >/dev/null); sleep 5
check "window in front: no notification" "$(posted)" "$n1"
$ADB shell input keyevent KEYCODE_BACK; sleep 3
check "Back: activity gone, same process" "$($ADB shell pidof $PKG)" "$P1"
n2=$(posted)
(cd "$W" && "$FC" send "$BOB" "Nachricht nach Back" "$PORT" alice.id >/dev/null); sleep 6
check "after Back + message -> notification" "$(( $(posted) > n2 ))" 1
check "after Back: still the same process" "$($ADB shell pidof $PKG)" "$P1"
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 4
check "reopen: the running program gets its window back" "$($ADB shell pidof $PKG)" "$P1"
$ADB exec-out screencap -p > "$ROOT/build/android/belege/firnchat-after.png"
# the phone answers: type into the input line and tap Senden (Enter)
$ADB shell input text "Antwort%svom%sHandy"; sleep 1
$ADB shell input keyevent KEYCODE_ENTER; sleep 3
got=$(cd "$W" && "$FC" hist "$PORT" alice.id 2>/dev/null | grep -c 'Antwort vom Handy')
check "Alice receives the phone's answer (E2E)" "$([ "$got" -ge 1 ] && echo yes)" yes
check "no crash" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|panic|ANR in '$PKG)" 0
pgrep -f "$FC serve $PORT" | xargs -r kill
echo "ANDROID FIRNCHAT: $PASS passed, $FAIL failed (relay log $W/relay.log)"
[ $FAIL -eq 0 ]
