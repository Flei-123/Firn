#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_check.sh -- JARVIS app on an emulator against tools/mini/fake_jarvis.py (host = 10.0.2.2).
#   SERIAL=emulator-55xx bash tools/android/jarvis_check.sh [apk]     (own AVD copy, NOT port 5584)
# STATUS: written 17.10.2026, NOT RUN YET (first lines: install, start with the fake server as argv, screenshot).
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
APK=${1:-/root/jarvis-android-builds/jarvis-0.1.0.apk}
PKG=org.fleitec.jarvis
OUT=${OUT:-$(mktemp -d)}
python3 -c "import websockets" 2>/dev/null || { echo "skip: python3 websockets missing"; exit 0; }
W=$(mktemp -d); trap 'kill $SRV 2>/dev/null; rm -rf "$W"' EXIT
python3 "$ROOT/tools/mini/fake_jarvis.py" "$W/server.log" >"$W/out" 2>&1 & SRV=$!
for _ in $(seq 50); do grep -q '^PORT' "$W/out" && break; sleep 0.1; done
PORT=$(awk '/^PORT/{print $2}' "$W/out")
$ADB uninstall $PKG >/dev/null 2>&1; $ADB install -r "$APK" | tail -1
printf 'http://10.0.2.2:%s\n' "$PORT" >"$W/args.txt"
$ADB shell mkdir -p /sdcard/Android/data/$PKG/files; $ADB push "$W/args.txt" /sdcard/Android/data/$PKG/files/args.txt >/dev/null
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 4
$ADB exec-out screencap -p >"$OUT/1_login.png"; echo "screenshot $OUT/1_login.png"
echo "TODO: tap fields (calibration like gallery_check.sh), type justin / pw, send, check server.log + screenshot"
