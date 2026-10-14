#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/campreview_check.sh -- GPU camera preview (lib/gpu/campreview.fi) on an emulator / device:
# builds examples/firncam_preview, runs it, reads the logcat lines (tag camprev) and checks
#   - the shader's picture against a CPU reference ("check ... psnr=" >= 40 dB)
#   - the frame rate ("fps=" >= MIN_FPS, default 24) and the absence of GL errors.
#   SERIAL=emulator-5584 bash tools/android/campreview_check.sh
# Environment: ADB, SERIAL, FIRNC (see tools/android/build.sh), ABI (default x86_64), SECONDS_RUN (default 25), MIN_FPS.
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
ABI=${ABI:-x86_64}
PKG=org.firn.camprev
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
bash "$ROOT/tools/android/build.sh" "$ROOT/examples/firncam_preview/main.fi" --name CamPrev --package $PKG \
    --camera --abi "$ABI" --out "$D/cp.apk" > "$D/build.log" 2>&1 || { tail -20 "$D/build.log"; echo "FAIL: build"; exit 1; }
$ADB install -r "$D/cp.apk" > /dev/null 2>&1 || { echo "FAIL: install"; exit 1; }
$ADB shell pm grant $PKG android.permission.CAMERA
$ADB shell am force-stop $PKG
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity > /dev/null
sleep "${SECONDS_RUN:-25}"
$ADB logcat -d -s camprev:I > "$D/log.txt"
$ADB shell am force-stop $PKG
grep -a "check\|frames=" "$D/log.txt" | tail -4 | cut -c1-220
FAIL=0
PS=$(grep -a "check " "$D/log.txt" | tail -1 | sed -n 's/.*psnr=\([0-9]*\).*/\1/p')
FPS=$(grep -a "frames=" "$D/log.txt" | tail -1 | sed -n 's/.* fps=\([0-9]*\).*/\1/p')
GLE=$(grep -a "frames=" "$D/log.txt" | tail -1 | sed -n 's/.*glerr=\([0-9]*\).*/\1/p')
[ -n "$PS" ] && [ "$PS" -ge 40 ] || { echo "FAIL: picture check (psnr=${PS:-none})"; FAIL=1; }
[ -n "$FPS" ] && [ "$FPS" -ge "${MIN_FPS:-24}" ] || { echo "FAIL: fps=${FPS:-none}"; FAIL=1; }
[ "${GLE:-1}" = 0 ] || { echo "FAIL: GL error ${GLE:-?}"; FAIL=1; }
[ $FAIL = 0 ] && echo "PASS: campreview"
exit $FAIL
