#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/screensmoke.sh -- EMULATOR SMOKE OF A fUi SCREEN APP: start, wait, screenshot, crash check.
#
#   /root/jarvis/bin/heavy bash tools/android/screensmoke.sh <apk> <package> <outdir>
#
# Needs the AVD template a31e (as examples/bench_android/smoke.sh). Prints SMOKE <package> PASS|FAIL.
set -uo pipefail
APK=${1:?apk}; PKG=${2:?package}; SHOTS=${3:?outdir}
SDK=/root/android-sdk
AVD=${AVD:-screenf}
EMU_PORT=${EMU_PORT:-5588}
ADB_PORT=${ADB_PORT:-5095}
ADBX="$SDK/platform-tools/adb -P $ADB_PORT"
SER=emulator-$EMU_PORT
ADB="$ADBX -s $SER"
AVDHOME=$HOME/.android/avd
W=$(mktemp -d)
mkdir -p "$SHOTS" "$AVDHOME/$AVD.avd"
sed -e "s/a31e/$AVD/g" "$AVDHOME/a31e.avd/config.ini" >"$AVDHOME/$AVD.avd/config.ini"
printf 'avd.ini.encoding=UTF-8\npath=%s/%s.avd\npath.rel=avd/%s.avd\ntarget=android-35\n' "$AVDHOME" "$AVD" "$AVD" >"$AVDHOME/$AVD.ini"
$ADBX start-server >/dev/null 2>&1
ANDROID_ADB_SERVER_PORT=$ADB_PORT ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=$AVDHOME \
    "$SDK/emulator/emulator" -avd $AVD -port $EMU_PORT -no-window -no-audio -no-boot-anim -no-snapshot \
    -wipe-data -gpu swiftshader_indirect >"$W/emu.log" 2>&1 &
EMU=$!
cleanup() {
    $ADB emu kill >/dev/null 2>&1; sleep 2; kill $EMU 2>/dev/null; $ADBX kill-server >/dev/null 2>&1
    rm -rf "$AVDHOME/$AVD.avd" "$AVDHOME/$AVD.ini" "$W"
}
trap cleanup EXIT
for _ in $(seq 200); do
    [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break
    sleep 3
done
$ADB shell wm size 1080x2400 >/dev/null
$ADB shell wm density 420 >/dev/null
$ADB install -r "$APK" >/dev/null || { echo "SMOKE $PKG FAIL install"; exit 1; }
$ADB logcat -c
$ADB shell am start -W -n "$PKG/android.app.NativeActivity" | grep -E "TotalTime|WaitTime" | tr '\n' ' '; echo
sleep "${WAIT:-12}"
$ADB exec-out screencap -p >"$SHOTS/$PKG.png"
$ADB logcat -d >"$SHOTS/$PKG.log"
PID=$($ADB shell pidof "$PKG" | tr -d '\r')
BAD=$(grep -cE "FATAL|AndroidRuntime|signal 11|SIGSEGV|Fatal signal" "$SHOTS/$PKG.log")
if [ -n "$PID" ] && [ "$BAD" = 0 ]; then echo "SMOKE $PKG PASS pid=$PID"; else echo "SMOKE $PKG FAIL pid=${PID:-none} crashlines=$BAD"; exit 1; fi
