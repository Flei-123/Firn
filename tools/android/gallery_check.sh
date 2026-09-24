#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/gallery_check.sh -- demos/x11demo as an APK, operated on a
# real Android (emulator or phone over adb) and checked by what the program
# itself reports (`--protokoll`: "knoten" = where the controls are,
# "zustand" = frame, scroll, switch, checkbox, progress, focus, scale).
#
#   bash tools/android/gallery_check.sh [apk]      (ADB, SERIAL from env)
#
# Nothing is guessed from pixels: every tap goes to the centre of a node
# the program reported, shifted by the offset between screen and content
# that a first calibration tap MEASURES (status bar, cutout, navigation
# bar differ per device and orientation).
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PKG=org.firn.gallery
APK=${1:-$ROOT/build/android/fui_galerie/fui_galerie.apk}
OUT=${OUT:-$ROOT/build/android/belege}
D=/sdcard/Android/data/$PKG/files
mkdir -p "$OUT"
PASS=0; FAIL=0
check() { # name got want
    if [ "$2" = "$3" ]; then echo "  OK    $1 (got $2)"; PASS=$((PASS+1))
    else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi
}
log() { $ADB shell cat $D/stdout.txt 2>/dev/null; }
state() { log | grep '^zustand' | tail -1; }
field() { state | awk -v i="$1" '{print $(i+1)}'; }   # 1=frame 2=scroll 3=switch 4=box 5=progress 6=focus 7=scale
node() { log | grep "^knoten $1 " | tail -1 | awk '{print int($3+$5/2), int($4+$6/2)}'; }

$ADB root >/dev/null 2>&1; sleep 1
$ADB install -r "$APK" >/dev/null || { echo "install failed"; exit 1; }
$ADB shell settings put system accelerometer_rotation 0
$ADB shell settings put system user_rotation 1
$ADB shell am force-stop $PKG
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 3
printf -- '--protokoll\n--dpi=132\n' > "$OUT/args.txt"
$ADB push "$OUT/args.txt" $D/args.txt >/dev/null
$ADB shell am force-stop $PKG
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 5
check "process runs" "$([ -n "$($ADB shell pidof $PKG)" ] && echo yes)" yes
check "first frame" "$([ "$(field 1)" -ge 1 ] 2>/dev/null && echo yes)" yes
check "scale 132 dpi -> 1375" "$(field 7)" 1375

# Calibration: one tap on empty space, the program reports where it landed.
$ADB shell input tap 1500 900; sleep 2
read -r CX CY < <(log | grep '^ereignis 3 ' | tail -1 | awk '{print $4, $5}')
OX=$((1500-CX)); OY=$((900-CY))
echo "  screen = content + ($OX, $OY)"
$ADB exec-out screencap -p > "$OUT/android-gallery-start.png"

tap() { read -r x y < <(node "$1"); $ADB shell input tap $((x+OX)) $((y+OY)); sleep 2; }
s0=$(field 3); tap schalter; check "touch toggles the switch" "$(field 3)" "$((1-s0))"
b0=$(field 4); tap kaestchen; check "touch toggles the checkbox" "$(field 4)" "$((1-b0))"
tap starten; sleep 2; check "Starten runs the progress animation to 1000" "$(field 5)" 1000
$ADB shell input keyevent KEYCODE_TAB; sleep 2
f1=$(field 6); check "Tab moves the focus" "$([ "$f1" != "-1" ] && echo yes)" yes
b1=$(field 4); $ADB shell input keyevent KEYCODE_ENTER; sleep 2
check "Enter activates the focused control" "$(field 4)" "$((1-b1))"
read -r lx ly < <(node liste); $ADB shell input tap $((lx+OX)) $((ly+OY)); sleep 2
$ADB shell input keyevent KEYCODE_PAGE_DOWN; sleep 2
check "PageDown scrolls the list" "$([ "$(field 2)" -gt 0 ] && echo yes)" yes
$ADB exec-out screencap -p > "$OUT/android-gallery-used.png"

$ADB shell settings put system user_rotation 0; sleep 4
check "rotation -> portrait content" "$(log | grep '^ereignis 4 ' | tail -1 | awk '{print ($4<$5)?"portrait":"landscape"}')" portrait
$ADB exec-out screencap -p > "$OUT/android-gallery-portrait.png"
$ADB shell settings put system user_rotation 1; sleep 4
$ADB shell input keyevent KEYCODE_HOME; sleep 2
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 4
check "back from home: repaint" "$(log | grep -c '^ereignis 1 ' | awk '{print ($1>=2)?"yes":"no"}')" yes
$ADB shell input keyevent KEYCODE_BACK; sleep 4
check "Back ends main" "$(log | tail -1)" geschlossen
check "process gone after Back" "$($ADB shell pidof $PKG)" ""
check "no crash in logcat" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|ANR in '$PKG)" 0
echo "paint ms (fUi frame, median of $(log | grep -c '^zeit')): $(log | grep '^zeit' | awk '{print $2}' | sort -n | awk '{a[NR]=$1} END{print a[int(NR/2)+1]}')"
$ADB shell rm -f $D/args.txt
echo "ANDROID GALLERY: $PASS passed, $FAIL failed. Pictures in $OUT"
[ $FAIL -eq 0 ]
