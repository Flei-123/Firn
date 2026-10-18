#!/usr/bin/env bash
# Emulator smoke test: boots a private AVD, installs the APK, drives every screen via the intent
# hooks, stores screenshots in shots/ and the BENCH log lines in shots/bench.log.
# Usage: /root/jarvis/bin/heavy bash bench/android_kotlin/smoke.sh <apk> [arm64-v8a|x86_64]
set -uo pipefail
APK=${1:?apk}
ABI=${2:-arm64-v8a}
HERE=$(cd "$(dirname "$0")" && pwd)
SHOTS=$HERE/shots
SDK=/root/android-sdk
AVD=${AVD:-benchb}
EMU_PORT=${EMU_PORT:-5576}
ADB_PORT=${ADB_PORT:-5091}
PKG=com.fleitec.bench.kotlin
ADBX="$SDK/platform-tools/adb -P $ADB_PORT"
SER=emulator-$EMU_PORT
ADB="$ADBX -s $SER"
AVDHOME=$HOME/.android/avd
W=$(mktemp -d)
mkdir -p "$SHOTS"

mkdir -p "$AVDHOME/$AVD.avd"
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
echo "boot done, abilist: $($ADB shell getprop ro.product.cpu.abilist | tr -d '\r')"
# wait for emulator network (DNS) so the net scenario can reach the internet
for _ in $(seq 30); do
  $ADB shell ping -c1 -W2 example.com >/dev/null 2>&1 && { echo "emulator net ok"; break; }
  sleep 2
done
$ADB shell settings put global window_animation_scale 0 >/dev/null
$ADB shell settings put global transition_animation_scale 0 >/dev/null
$ADB shell settings put global animator_duration_scale 1 >/dev/null
$ADB shell wm size 1080x2400 >/dev/null
$ADB shell wm density 420 >/dev/null

if [ "$ABI" = arm64-v8a ]; then $ADB install --abi arm64-v8a -r "$APK" || $ADB install -r "$APK"; else $ADB install -r "$APK"; fi
: >"$SHOTS/bench.log"

# scenario name | wait seconds | am start extras
scenario() {
  local name=$1 wait=$2; shift 2
  if [ -n "${ONLY:-}" ] && ! [[ $name =~ $ONLY ]]; then return; fi
  $ADB shell am force-stop $PKG; sleep 1
  $ADB logcat -c
  $ADB shell am start -W -n $PKG/.MainActivity "$@" | grep -E "TotalTime|WaitTime" | tr '\n' ' '; echo
  sleep "$wait"
  $ADB exec-out screencap -p >"$SHOTS/$name.png"
  { echo "## $name"; $ADB logcat -d -s BENCH:I | grep -E "BENCH|AndroidRuntime|FATAL" ; } >>"$SHOTS/bench.log"
}

scenario list-dark 8 --es tab list --es theme dark
scenario list-light 6 --es tab list --es theme light
scenario list-autoscroll 14 --es tab list --es theme dark --ei autoscroll 1
scenario lab-anim-dark 13 --es tab lab --es lab anim --es theme dark
scenario lab-anim-light 6 --es tab lab --es lab anim --es theme light
scenario lab-text-dark 6 --es tab lab --es lab text --es theme dark
scenario lab-net-dark 30 --es tab lab --es lab net --es theme dark --ez autorun true
scenario lab-cpu-dark ${CPU_WAIT:-150} --es tab lab --es lab cpu --es theme dark --ez autorun true
scenario settings-dark 6 --es tab settings --es theme dark
scenario settings-light 6 --es tab settings --es theme light

echo "--- bench.log"; cat "$SHOTS/bench.log"
$ADB shell dumpsys package $PKG | grep -E "primaryCpuAbi|versionName" | head -3
