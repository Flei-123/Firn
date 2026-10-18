#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/relaunch_check.sh -- the system RE-CREATES the activity (r775).
#
# Android destroys and re-creates a running activity when the resources under
# it change (a resource overlay comes up, the theme switches): a change in
# CONFIG_ASSETS_PATHS, which no manifest `configChanges` can declare. This
# happens for real shortly after an emulator's first boot (the emulation
# overlay), and the first start of an app then died 100 ms after its window
# showed ("main returned, finishing"). The host (lib/android/activity.fi)
# must keep the PROCESS and hand the program to the new activity.
#
# The script starts examples/fui/counter.fi, toggles an overlay that targets
# the framework twice (each toggle relaunches the activity) and checks that
#   * the process is the SAME (pid) and the log says "activity relaunching"
#   * the picture is the program's again, and it still answers a tap
#
#   bash tools/android/relaunch_check.sh      (ADB, SERIAL from the env;
#   ABI=arm64|x86_64 picks the APK, default x86_64; APK=<file> takes a ready one)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PKG=org.firn.countertest
OUT=${OUT:-$(mktemp -d)}
mkdir -p "$OUT"
ABI=${ABI:-x86_64}
OVERLAY=com.android.internal.display.cutout.emulation.corner
fail=0
ok()  { echo "   OK      $1"; }
bad() { echo "   FAILED  $1"; fail=1; }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

APK=${APK:-}
if [ -z "$APK" ]; then
    echo "== 1. build the APK ($ABI) =="
    export FIRNLIB=$ROOT/lib FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
    bash tools/android/build.sh examples/fui/counter.fi --name "fUi Counter" \
        --package $PKG --abi "$ABI" 2>&1 | tail -1
    APK=$ROOT/build/android/fui_counter/fui_counter.apk
fi
[ -f "$APK" ] || { echo "no APK"; exit 1; }
$ADB install -r "$APK" | tail -1
$ADB shell cmd overlay list 2>/dev/null | grep -q "$OVERLAY" || { echo "SKIPPED: overlay $OVERLAY missing on this image"; exit 0; }
$ADB shell cmd overlay disable "$OVERLAY" >/dev/null 2>&1
$ADB shell am force-stop $PKG

# share of the screen in the program's light base colour, in percent
base_share() {
    $ADB exec-out screencap -p > "$OUT/shot.png"
    python3 - "$OUT/shot.png" <<'PY'
import sys
from PIL import Image
import numpy as np
im = np.array(Image.open(sys.argv[1]).convert('RGB')).astype(int)
print(int((abs(im - [244, 244, 246]).max(axis=2) <= 3).mean() * 100))
PY
}
wait_picture() { # up to 40 s until the program's picture shows
    local i=0
    while [ $i -lt 40 ]; do
        [ "$(base_share)" -ge 70 ] && return 0
        sleep 1; i=$((i + 1))
    done
    return 1
}
pid_of() { $ADB shell pidof $PKG | tr -d '\r' | awk '{print $1}'; }

echo "== 2. start =="
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
check "the picture shows after the start" "wait_picture"
P0=$(pid_of)
check "the process runs (pid $P0)" "[ -n '$P0' ]"

echo "== 3. the system re-creates the activity (overlay on) =="
$ADB shell cmd overlay enable "$OVERLAY" >/dev/null 2>&1
sleep 8
check "the same process lives on" "[ \"\$(pid_of)\" = '$P0' ]"
check "the log says: activity relaunching" "$ADB logcat -d | grep -q 'activity relaunching'"
check "the picture is the program's again" "wait_picture"

echo "== 4. a second time (overlay off) =="
$ADB logcat -d > "$OUT/logcat_step3.txt"
$ADB logcat -c
$ADB shell cmd overlay disable "$OVERLAY" >/dev/null 2>&1
sleep 8
check "the same process lives on" "[ \"\$(pid_of)\" = '$P0' ]"
check "the log says: activity relaunching" "$ADB logcat -d | grep -q 'activity relaunching'"
check "the picture is the program's again" "wait_picture"

echo "== 5. it still answers =="
S1=$(mktemp); $ADB exec-out screencap -p > "$S1"
SIZE=$($ADB shell wm size | grep -o '[0-9]*x[0-9]*' | tail -1)
XM=${SIZE%x*}; YM=${SIZE#*x}
$ADB shell input tap $((XM * 58 / 100)) $((YM * 54 / 100)); sleep 2
S2=$(mktemp); $ADB exec-out screencap -p > "$S2"
check "a tap changes the picture" "! cmp -s $S1 $S2"
rm -f "$S1" "$S2"

$ADB logcat -d > "$OUT/logcat_step4.txt"
$ADB shell am force-stop $PKG
[ $fail -eq 0 ] && echo "RELAUNCH PASSED" || echo "RELAUNCH FAILED"
exit $fail
