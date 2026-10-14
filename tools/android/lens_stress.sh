#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/lens_stress.sh -- lens switch stress test (r542): builds tools/android/lens_stress, runs it on an
# emulator / device and fails on any crash. 12 x lens switches, then 12 x the physical-output path (hook).
#   SERIAL=emulator-5580 bash tools/android/lens_stress.sh
# Environment: ADB, SERIAL, FIRNC (see tools/android/build.sh), ABI (default x86_64), TIMEOUT (seconds, default 240).
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
ABI=${ABI:-x86_64}
TIMEOUT=${TIMEOUT:-240}
PKG=org.firn.lensstress
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
bash "$ROOT/tools/android/build.sh" "$ROOT/tools/android/lens_stress/main.fi" --name LensStress --package $PKG \
    --camera --abi "$ABI" --out "$D/ls.apk" > "$D/build.log" 2>&1 || { tail -20 "$D/build.log"; echo "FAIL: build"; exit 1; }
$ADB install -r "$D/ls.apk" > /dev/null 2>&1 || { echo "FAIL: install"; exit 1; }
$ADB shell pm grant $PKG android.permission.CAMERA
$ADB shell am force-stop $PKG
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity > /dev/null
T=0; RES=""
while [ $T -lt "$TIMEOUT" ]; do
    sleep 3; T=$((T + 3))
    $ADB logcat -d > "$D/log.txt"
    if grep -q "Fatal signal\|FATAL EXCEPTION" "$D/log.txt"; then RES=crash; break; fi
    if ! $ADB shell pidof $PKG > /dev/null 2>&1 && ! grep -q "RESULT done" "$D/log.txt"; then RES=died; break; fi
    if grep -q "RESULT done" "$D/log.txt"; then RES=done; break; fi
done
grep -a "lens-stress\|Fatal signal\|backtrace\|#0[0-9] pc" "$D/log.txt" | tail -${TAIL:-25}
$ADB shell am force-stop $PKG
if [ "$RES" = done ] && grep -q "RESULT bad=0" "$D/log.txt"; then echo "PASS: lens stress"; exit 0; fi
echo "FAIL: lens stress ($RES)"; exit 1
