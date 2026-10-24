#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/swipe_check.sh -- finger scrolling on an emulator: `adb shell input swipe` on a vertical
# scroll area, a horizontal scroll area and a text area (tools/android/swipe_app.fi logs "swipe v= h= t= ov= oh=").
#
#   bash tools/android/swipe_check.sh        (ADB, SERIAL, FIRNC from the env)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PKG=org.firn.swipetest
fail=0
. "$ROOT/tools/android/dismiss_anr.sh"
ok()  { echo "   OK      $1"; }
bad() { echo "   FAILED  $1"; fail=1; }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

echo "== 1. build the APK (x86_64) =="
export FIRNLIB=$ROOT/lib FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
bash tools/android/build.sh tools/android/swipe_app.fi --name "Swipe test" \
    --package $PKG --abi x86_64 2>&1 | tail -1
APK=$ROOT/build/android/swipe_test/swipe_test.apk
[ -f "$APK" ] || { echo "no APK"; exit 1; }
$ADB root >/dev/null 2>&1; sleep 2; $ADB wait-for-device
$ADB install -r "$APK" | tail -1
dismiss_anr
$ADB shell settings put system accelerometer_rotation 0
$ADB shell settings put system user_rotation 0
SIZE=$($ADB shell wm size | grep -o '[0-9]*x[0-9]*' | tail -1)
W=${SIZE%x*}; H=${SIZE#*x}
swipe() { # x0 y0 x1 y1 ms, in 1/1000 of the screen
    $ADB shell input swipe $(($1 * W / 1000)) $(($2 * H / 1000)) $(($3 * W / 1000)) $(($4 * H / 1000)) $5
    sleep 2.5
}
last() { $ADB logcat -d -s firn:I 2>/dev/null | grep ' swipe v=' | tail -1; }
field() { last | grep -o " $1=-\?[0-9]*" | tail -1 | sed "s/ $1=//"; }
seen() { $ADB logcat -d -s firn:I 2>/dev/null | grep ' swipe v=' | grep -c "$1"; }

echo "== 2. start =="
$ADB logcat -c
$ADB shell am force-stop $PKG
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
sleep 8
dismiss_anr
$ADB shell input swipe 1 1 2 2 50
sleep 1
check "the app is alive" "$ADB shell pidof $PKG | grep -q ."

echo "== 3. vertical scroll area =="
swipe 500 600 500 250 300
V=$(field v)
check "a swipe up scrolls the area (v=$V > 150)" "[ \"${V:-0}\" -gt 150 ]"
sleep 1
V2=$(field v)
check "the swipe leaves a fling or at least stays put" "[ \"${V2:-0}\" -ge \"${V:-0}\" ]"
swipe 500 250 500 600 300
swipe 500 250 500 600 300
swipe 500 250 500 600 300
check "swiping back reaches the top" "[ \"$(field v)\" = 0 ]"

echo "== 4. rubber band at the top =="
$ADB logcat -c
$ADB shell input swipe $((500 * W / 1000)) $((250 * H / 1000)) $((500 * W / 1000)) $((700 * H / 1000)) 600
sleep 0.1
sleep 3
check "pulling past the top stretched the band (ov < 0 was logged)" "[ $(seen 'ov=-') -gt 0 ]"
check "the band sprang back (ov = 0)" "[ \"$(field ov)\" = 0 ]"
check "the offset stays 0" "[ \"$(field v)\" = 0 ]"

echo "== 5. horizontal scroll area =="
swipe 800 757 200 757 300
HH=$(field h)
check "a sideways swipe scrolls the horizontal area (h=$HH > 150)" "[ \"${HH:-0}\" -gt 150 ]"
check "the vertical area did not move" "[ \"$(field v)\" = 0 ]"

echo "== 6. text area =="
swipe 500 920 500 850 300
T=$(field t)
check "a swipe up scrolls the text area (t=$T > 20)" "[ \"${T:-0}\" -gt 20 ]"
check "the vertical area did not move" "[ \"$(field v)\" = 0 ]"

restart() {
    $ADB logcat -c
    $ADB shell am force-stop $PKG
    $ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
    sleep 8
    dismiss_anr
    $ADB shell input swipe 1 1 2 2 50
    sleep 1
}

echo "== 6b. text area fling =="
restart
swipe 500 942 500 921 2500
TS=$(field t)
restart
swipe 500 942 500 921 150
TF=$(field t)
echo "   INFO    quick flick t=$TF, slow drag t=$TS (fling on the emulator is not asserted yet)"

echo "== 6c. text area rubber band =="
restart
$ADB shell input swipe $((500 * W / 1000)) $((921 * H / 1000)) $((500 * W / 1000)) $((942 * H / 1000)) 600
sleep 3
check "pulling the text area past its top stretched the band (to < 0 was logged)" "[ $(seen 'to=-') -gt 0 ]"
check "the text area band sprang back (to = 0)" "[ \"$(field to)\" = 0 ]"
check "the text area stays at its top (t = 0)" "[ \"$(field t)\" = 0 ]"

echo "== 7. no crash =="
check "the process is alive" "$ADB shell pidof $PKG | grep -q ."

[ $fail -eq 0 ] && echo "SWIPE PASSED" || echo "SWIPE FAILED"
exit $fail
