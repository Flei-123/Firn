#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/arm64_epoll_check.sh -- tests/2880 (async.loop epoll_event layout) INSIDE the Android emulator's ARM translator.
#
#   bash tools/android/arm64_epoll_check.sh
#   SERIAL=emulator-5600 ADB_PORT=5049   use a running emulator; else an own AVD is started (AVD=a27c EMU_PORT=5710 ADB_PORT=5071)
#
# Why: libndk_translation runs an AArch64 program on an x86-64 kernel, converts the epoll records to the AArch64 layout and
# answers uname(2) with the HOST's "x86_64". Before FIRN a27 the loop believed uname, used the 12 octet record and every
# connection after the first one timed out (arm64 WebSocket/store in the emulator). Here the same test binary runs as
# AArch64 (translated) and as x86-64 (native) in the emulator; both must print "epoll layout ok". Needs no APK.
# Light (one test binary), but the emulator boot is heavy: run through /root/jarvis/bin/heavy.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SDK=${SDK:-/root/android-sdk}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB=$ROOT/lib
ADB_PORT=${ADB_PORT:-5071}; AVD=${AVD:-a27c}; EMU_PORT=${EMU_PORT:-5710}
ADB0="$SDK/platform-tools/adb -P $ADB_PORT"
W=$(mktemp -d); EMU=""; PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  OK    $1"; PASS=$((PASS+1)); else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
cleanup() {
    if [ -n "$EMU" ]; then $ADB emu kill >/dev/null 2>&1; sleep 2; kill $EMU 2>/dev/null; $ADB0 kill-server >/dev/null 2>&1
        rm -rf "$HOME/.android/avd/$AVD.avd" "$HOME/.android/avd/$AVD.ini"; fi
    rm -rf "$W"
}
trap cleanup EXIT
T=$ROOT/tests/2880_async_epoll_layout.fi
"$FIRNC" --target=aarch64-linux -o "$W/t.a64" "$T" || { echo "aarch64 build failed"; exit 1; }
"$FIRNC" --target=x86_64-linux -o "$W/t.x86" "$T" || { echo "x86_64 build failed"; exit 1; }
if [ -z "${SERIAL:-}" ]; then
    AVDHOME=${ANDROID_AVD_HOME:-$HOME/.android/avd}
    IMG=default; [ -d "$SDK/system-images/android-35/google_apis/x86_64" ] && IMG=google_apis
    mkdir -p "$AVDHOME/$AVD.avd"
    sed -e "s/firn/$AVD/g" -e "s#android-35/default/#android-35/$IMG/#" -e "s/^tag.id *=.*/tag.id = $IMG/" "$AVDHOME/firn.avd/config.ini" >"$AVDHOME/$AVD.avd/config.ini"
    printf 'avd.ini.encoding=UTF-8\npath=%s/%s.avd\npath.rel=avd/%s.avd\ntarget=android-35\n' "$AVDHOME" "$AVD" "$AVD" >"$AVDHOME/$AVD.ini"
    $ADB0 start-server >/dev/null 2>&1
    ANDROID_ADB_SERVER_PORT=$ADB_PORT ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=$AVDHOME "$SDK/emulator/emulator" -avd $AVD -port $EMU_PORT \
        -no-window -no-audio -no-boot-anim -no-snapshot -wipe-data -gpu swiftshader_indirect >"$W/emu.log" 2>&1 & EMU=$!
    SERIAL=emulator-$EMU_PORT
fi
ADB="$ADB0 -s $SERIAL"
for _ in $(seq 150); do [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break; sleep 3; done
[ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] || { echo "the emulator did not boot"; exit 1; }
$ADB root >/dev/null 2>&1; sleep 2; $ADB wait-for-device
echo "emulator up: abis $($ADB shell getprop ro.product.cpu.abilist | tr -d '\r')"
$ADB push "$W/t.a64" /data/local/tmp/a27_epoll.a64 >/dev/null; $ADB push "$W/t.x86" /data/local/tmp/a27_epoll.x86 >/dev/null
$ADB shell chmod 755 /data/local/tmp/a27_epoll.a64 /data/local/tmp/a27_epoll.x86
for v in x86 a64; do
    OUT=$($ADB shell "cd /data/local/tmp && ./a27_epoll.$v; echo rc=\$?" | tr -d '\r' | tr '\n' ' ')
    check "tests/2880 as $v in the emulator" "$OUT" "epoll layout ok rc=0 "
done
$ADB shell rm -f /data/local/tmp/a27_epoll.a64 /data/local/tmp/a27_epoll.x86
echo "arm64_epoll_check: $PASS ok, $FAIL failed"
[ "$FAIL" -eq 0 ]
