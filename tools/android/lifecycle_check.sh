#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/lifecycle_check.sh -- PAUSE, RESUME, ROTATION, SCREEN OFF/ON of
# a fui.app program on an emulator (roadmap r114), with adb root not needed.
#
# examples/fui/counter.fi as an APK (lib/@android/fui/apphost.fi). The
# script taps the "+" button, sends the app to the background (HOME), brings
# it back, rotates the screen to landscape and back, switches the screen off
# and on -- and after every step looks at the screenshot (adb screencap):
#   * the picture is the program's (base colour on >= 80 % of the screen)
#   * the state (the counter) survived: the same pixels as before
#   * the program still answers a tap
#   * after the rotation back the picture covers the whole window (a stale
#     EGL surface once clipped it to the landscape height)
#
#   bash tools/android/lifecycle_check.sh        (ADB, SERIAL from the env)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PKG=org.firn.countertest
OUT=${OUT:-/tmp/firn-lifecycle-check}
mkdir -p "$OUT"
fail=0
ok()  { echo "   OK      $1"; }
bad() { echo "   FAILED  $1"; fail=1; }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

echo "== 1. build the APK (x86_64) =="
export FIRNLIB=$ROOT/lib FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
bash tools/android/build.sh examples/fui/counter.fi --name "fUi Counter" \
    --package $PKG --abi x86_64 2>&1 | tail -1
APK=$ROOT/build/android/fui_counter/fui_counter.apk
[ -f "$APK" ] || { echo "no APK"; exit 1; }
$ADB root >/dev/null 2>&1; sleep 2; $ADB wait-for-device
$ADB install -r "$APK" | tail -1

# One screenshot, measured: "<width>x<height> <base share in %> <crop hash> <last row with base>"
measure() { # name
    $ADB exec-out screencap -p > "$OUT/$1.png"
    python3 - "$OUT/$1.png" <<'PY'
import sys, hashlib
from PIL import Image
import numpy as np
im = np.array(Image.open(sys.argv[1]).convert('RGB'))
h, w, _ = im.shape
base = (im == [244, 244, 246]).all(axis=2)
ys = np.where(base.any(axis=1))[0]
crop = im[h * 43 // 100:h * 52 // 100, w * 40 // 100:w * 60 // 100]
print(f"{w}x{h} {int(base.mean() * 100)} {hashlib.md5(crop.tobytes()).hexdigest()[:8]} {int(ys.max()) if len(ys) else 0}")
PY
}
# wait until the picture is the program's (base share >= 70 %) AND does not
# change any more (two equal measurements one second apart: an activity
# transition animates the window), at most 40 s
settle() { # name
    local i=0 m prev=""
    while [ $i -lt 40 ]; do
        m=$(measure "$1")
        if [ "$(echo "$m" | awk '{print $2}')" -ge 70 ] && [ "$m" = "$prev" ]; then
            echo "$m"; return 0
        fi
        prev=$m
        sleep 1; i=$((i + 1))
    done
    echo "$m"; return 1
}
tap_plus() { $ADB shell input tap $((XM * 58 / 100)) $((YM * 54 / 100)); sleep 1; }

$ADB shell settings put system accelerometer_rotation 0
$ADB shell settings put system user_rotation 0
$ADB shell wm user-rotation lock 0 >/dev/null 2>&1
$ADB shell am force-stop $PKG
SIZE=$($ADB shell wm size | grep -o '[0-9]*x[0-9]*' | tail -1)
XM=${SIZE%x*}; YM=${SIZE#*x}

echo "== 2. start, three taps =="
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
M0=$(settle s0); echo "   start: $M0"
check "the program draws its picture" "[ $(echo "$M0" | awk '{print $2}') -ge 70 ]"
for i in 1 2 3; do tap_plus; done
M1=$(settle s1); echo "   after 3 taps: $M1"
check "the counter changed" "[ '$(echo "$M0" | awk '{print $3}')' != '$(echo "$M1" | awk '{print $3}')' ]"

echo "== 3. pause (HOME) and resume =="
$ADB shell input keyevent KEYCODE_HOME; sleep 3
MH=$(measure home); echo "   home: $MH"
check "while paused the screen is the launcher's" "[ $(echo "$MH" | awk '{print $2}') -lt 70 ]"
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
M2=$(settle s2); echo "   resumed: $M2"
check "the picture is back after the resume" "[ $(echo "$M2" | awk '{print $2}') -ge 70 ]"
check "the state survived the pause (same pixels)" "[ '$(echo "$M1" | awk '{print $3}')' = '$(echo "$M2" | awk '{print $3}')' ]"
tap_plus; M3=$(settle s3); echo "   tap after resume: $M3"
check "it still answers a tap" "[ '$(echo "$M2" | awk '{print $3}')' != '$(echo "$M3" | awk '{print $3}')' ]"

echo "== 4. rotation: landscape and back =="
$ADB shell settings put system user_rotation 1; $ADB shell wm user-rotation lock 1 >/dev/null 2>&1; sleep 3
ML=$(settle r1); echo "   landscape: $ML"
check "landscape: the picture covers the window (${XM}x${YM} turned)" "[ '$(echo "$ML" | awk '{print $1}')' = '${YM}x${XM}' ] && [ $(echo "$ML" | awk '{print $2}') -ge 70 ]"
$ADB shell settings put system user_rotation 0; $ADB shell wm user-rotation lock 0 >/dev/null 2>&1; sleep 3
MB=$(settle r2); echo "   portrait again: $MB"
check "portrait again: the whole window is covered, to the bottom" "[ '$(echo "$MB" | awk '{print $1}')' = '${XM}x${YM}' ] && [ $(echo "$MB" | awk '{print $2}') -ge 70 ] && [ $(echo "$MB" | awk '{print $4}') -ge $((YM - 400)) ]"
check "the state survived the rotation" "[ '$(echo "$M3" | awk '{print $3}')' = '$(echo "$MB" | awk '{print $3}')' ]"

echo "== 5. screen off and on =="
$ADB shell input keyevent KEYCODE_POWER; sleep 2
$ADB shell input keyevent KEYCODE_POWER; sleep 2
$ADB shell input keyevent KEYCODE_MENU; sleep 1
M5=$(settle s5); echo "   screen on: $M5"
check "the picture is there after the screen came back" "[ $(echo "$M5" | awk '{print $2}') -ge 70 ]"
check "the state survived" "[ '$(echo "$M3" | awk '{print $3}')' = '$(echo "$M5" | awk '{print $3}')' ]"
tap_plus; M6=$(settle s6); echo "   tap: $M6"
check "it answers a tap after the screen came back" "[ '$(echo "$M5" | awk '{print $3}')' != '$(echo "$M6" | awk '{print $3}')' ]"

[ $fail -eq 0 ] && echo "LIFECYCLE PASSED" || echo "LIFECYCLE FAILED"
exit $fail
