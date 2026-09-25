#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/firnchat_push_check.sh -- FIRNCHAT's push on the emulator,
# the way a person uses a phone: the app in the background, a message from
# another account, a REAL system notification (sender as title, the line as
# text, one per chat, sound + vibration allowed by the channel), a tap on it
# opens THAT chat -- also after Back, after swiping the app out of the
# recent apps, and after the relay went away and came back. No Firebase:
# the relay session of the app IS the push (lib/plat/android/push.fi).
#
#   bash tools/android/firnchat_push_check.sh <firnchat checkout> <apk>
#
# Own relay on RELAY_PORT (default 7782), throw-away identities -- never the
# relay on 7771, never real accounts. SERIAL / ADB as in firnchat_check.sh.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FCDIR=${1:?firnchat checkout}
APK=${2:?apk}
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PORT=${RELAY_PORT:-7782}
PKG=org.firn.firnchat
D=/sdcard/Android/data/$PKG/files
FC=$FCDIR/bin/firnchat
W=$(mktemp -d /tmp/fcpush.XXXX)
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  OK    $1 (got $2)"; PASS=$((PASS+1));
          else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
notes() { $ADB shell dumpsys notification --noredact; }
# title and text of the keyed notification of the chat titled $1
note_text() { notes | awk -v p="$PKG" -v want="$1" '
    /NotificationRecord\(/ { inrec = ($0 ~ "pkg=" p " .*id=100000[0-9] ") ; t="" }
    inrec && /android.title=String \(/ { t=$0; sub(/.*android.title=String \(/,"",t); sub(/\)$/,"",t) }
    inrec && /android.text=String \(/ { x=$0; sub(/.*android.text=String \(/,"",x); sub(/\)$/,"",x);
        if (index(t, want) == 1) { print x; exit } }'; }
note_titles() { notes | awk -v p="$PKG" '
    /NotificationRecord\(/ { inrec = ($0 ~ "pkg=" p " .*id=100000[0-9] ") }
    inrec && /android.title=String \(/ { t=$0; sub(/.*android.title=String \(/,"",t); sub(/\)$/,"",t); print t }' \
    | sort | tr '\n' '|'; }
keyed() { $ADB shell cmd notification list | grep -c "^0|$PKG|100000[0-9]|"; }
out() { $ADB shell cat $D/stdout.txt 2>/dev/null; }
relay() { (cd "$W" && setsid nohup "$FC" serve "$PORT" "$W/data" 0 >> "$W/relay.log" 2>&1 &); sleep 1; }
send() { (cd "$W" && "$FC" send "$BOB" "$2" "$PORT" "$1.id" >/dev/null); }
# tap the notification whose title starts with $1, in the opened shade
# A collapsed group of notifications is ONE row for Android (a tap is the
# summary's), so the group is expanded first, as a person does.
ui() { local i=0; while [ $i -lt 5 ]; do
        $ADB shell uiautomator dump /sdcard/ui.xml 2>&1 | grep -q dumped && break
        sleep 1; i=$((i+1)); done
    $ADB shell cat /sdcard/ui.xml | tr '>' '\n'; }
mid() { echo "$1" | tr '[],' '   ' | awk '{print int(($1+$3)/2), int(($2+$4)/2)}'; }
tap_note() {
    $ADB shell cmd statusbar expand-notifications; sleep 2
    local e; e=$(ui | grep -A12 'text="FirnChat" resource-id="android:id/app_name_text"' \
        | grep 'content-desc="Expand"' | grep -oP 'bounds="\K[^"]+' | head -1)
    [ -n "$e" ] && { $ADB shell input tap $(mid "$e"); sleep 2; }
    local b; b=$(ui | grep -E 'id/(notification_)?title"' | grep "text=\"$1" \
        | grep -oP 'bounds="\K[^"]+' | head -1)
    [ -n "$b" ] || { echo "  (no notification titled $1 in the shade)"; $ADB shell cmd statusbar collapse; return 1; }
    local xy; xy=$(mid "$b"); $ADB shell input tap $(( ${xy% *} + 300 )) ${xy#* }; sleep 5
}

[ -x "$FC" ] || { echo "no $FC (build FIRNCHAT first)"; exit 2; }
relay
(cd "$W" && "$FC" id alice.id >/dev/null && "$FC" id carol.id >/dev/null \
    && "$FC" id bob.id > bob.txt \
    && "$FC" name Alice "$PORT" alice.id >/dev/null \
    && "$FC" name Carol "$PORT" carol.id >/dev/null \
    && "$FC" name "Bob Handy" "$PORT" bob.id >/dev/null)
BOB=$(grep -oP '^key \K[0-9a-f]+' "$W/bob.txt")
send alice "Hallo Bob, erste Nachricht"; send alice "noch was"; send carol "Hi von Carol"

$ADB root >/dev/null 2>&1; sleep 1
$ADB install -r "$APK" >/dev/null || { echo "install failed"; exit 1; }
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 3
$ADB shell am force-stop $PKG
$ADB shell "rm -f $D/bob.id* $D/stdout.txt"
$ADB push "$W/bob.id" $D/bob.id >/dev/null
printf -- '--id\n%s/bob.id\n--host\n10.0.2.2\n--port\n%s\n--font\n/system/fonts/RobotoStatic-Regular.ttf\n' \
    "$D" "$PORT" > "$W/args.txt"
$ADB push "$W/args.txt" $D/args.txt >/dev/null
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 14
check "live session from the phone" "$(grep -c 'watch session opened' "$W/relay.log" | awk '{print ($1>0)?"yes":"no"}')" yes
P1=$($ADB shell pidof $PKG)
mkdir -p "$ROOT/build/android/belege"

echo "== 1. background (Home), messages from two accounts"
$ADB shell input keyevent KEYCODE_HOME; sleep 2
send carol "Hallo von Carol 😀 Grüße"; sleep 5
check "Carol: notification with her line (emoji intact)" "$(note_text Carol)" "Hallo von Carol 😀 Grüße"
send alice "Alice eins"; send alice "Alice zwei"; sleep 5
check "Alice: ONE notification, counted, newest line" "$(note_text 'Alice (2)')" "Alice zwei"
check "one notification per chat" "$(keyed)" 2
check "own group with summary (no Android autogroup)" \
    "$(notes | grep -c "pkg=$PKG .*id=999999 ")/$(notes | grep -c "pkg=$PKG .*ranker_group")" "1/0"
CH=$(notes | grep -o "NotificationChannel{mId='firn_msg'[^}]*" | head -1)
check "channel: importance high" "$(echo "$CH" | grep -oP 'mImportance=\K[0-9]+')" 4
check "channel: sound" "$(echo "$CH" | grep -c 'mSound=content://')" 1
check "channel: vibration allowed" "$(echo "$CH" | grep -oP 'mVibrationEnabled=\K\w+')" true

echo "== 2. tap Carol's notification -> Carol's chat"
tap_note Carol
check "the app is in front" "$($ADB shell dumpsys activity activities | grep -c "topResumedActivity=.*$PKG")" 1
check "tap opened Carol's chat" "$(out | grep '^note-open' | tail -1 | cut -d' ' -f3-)" "Carol"
check "Carol's notification gone, Alice's stays" "$(note_titles)" "Alice (2)|"
$ADB exec-out screencap -p > "$ROOT/build/android/belege/firnchat-push-tap.png"
n=$(keyed); send carol "Carol im Vordergrund"; sleep 5
check "chat on screen: no notification" "$(keyed)" "$n"

echo "== 3. Back: the window is gone, the process stays"
$ADB shell input keyevent KEYCODE_BACK; sleep 3
check "Back: same process" "$($ADB shell pidof $PKG)" "$P1"
send alice "nach Back"; sleep 5
check "after Back: Alice's notification, newest line" "$(note_text 'Alice (3)')" "nach Back"
tap_note Alice
check "tap after Back opened Alice's chat" "$(out | grep '^note-open' | tail -1 | cut -d' ' -f3-)" "Alice"
check "after Back + tap: still the same process" "$($ADB shell pidof $PKG)" "$P1"
check "no notification left" "$(keyed)" 0

echo "== 4. swiped out of the recent apps"
read -r SW SH <<< "$($ADB shell wm size | grep -oP '\d+x\d+' | tail -1 | tr x ' ')"
# a loaded software emulator animates the recents screen slower than 3 s:
# swipe again until the task is really gone (at most 3 times)
for try in 1 2 3; do
    $ADB shell input keyevent KEYCODE_APP_SWITCH; sleep $((2 + 2*try))
    $ADB shell input swipe $((SW/2)) $((SH/2)) $((SW/2)) $((SH/12)) 300; sleep $((2 + try))
    $ADB shell input keyevent KEYCODE_HOME; sleep 2
    [ "$($ADB shell dumpsys activity recents | grep -c "Recent #.*A=.*:$PKG")" = 0 ] && break
done
check "task gone from recents" "$($ADB shell dumpsys activity recents | grep -c "Recent #.*A=.*:$PKG")" 0
check "swiped: the process lives on (foreground service)" "$($ADB shell pidof $PKG)" "$P1"
send carol "nach Wegwischen"; sleep 5
check "swiped: Carol's notification" "$(note_text Carol)" "nach Wegwischen"
tap_note Carol
check "tap after swipe opened Carol's chat" "$(out | grep '^note-open' | tail -1 | cut -d' ' -f3-)" "Carol"
$ADB exec-out screencap -p > "$ROOT/build/android/belege/firnchat-push-swiped.png"

echo "== 5. the relay goes away and comes back (a phone changing networks)"
$ADB shell input keyevent KEYCODE_HOME; sleep 2
r0=$(out | grep -c '^relay-reconnect 1')
pgrep -f "$FC serve $PORT" | xargs -r kill; sleep 6
relay
t=0; while [ $t -lt 40 ] && [ "$(out | grep -c '^relay-reconnect 1')" -le "$r0" ]; do sleep 2; t=$((t+2)); done
check "back live after the relay restart (${t}s)" "$(( $(out | grep -c '^relay-reconnect 1') > r0 ))" 1
send alice "nach Relay-Neustart"; sleep 5
check "after reconnect: Alice's notification" "$(note_text Alice)" "nach Relay-Neustart"

check "no crash" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|panic|ANR in '$PKG)" 0
pgrep -f "$FC serve $PORT" | xargs -r kill
echo "ANDROID FIRNCHAT PUSH: $PASS passed, $FAIL failed (relay log $W/relay.log)"
[ $FAIL -eq 0 ]
