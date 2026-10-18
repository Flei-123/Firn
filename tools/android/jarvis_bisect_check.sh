#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_bisect_check.sh -- start check of the JARVIS bisect APKs (parallel package names) on ONE emulator, plus the
# crash-page proof for a build that carries jcrash.fi (an early kill -9 and a signal 11 each show the report on the next start).
#
#   bash tools/android/jarvis_bisect_check.sh [--page] <apk>:<package> [<apk>:<package> ...]
#
# Own AVD copy (AVD=a19x, EMU_PORT=5598, ADB_PORT=5047 overridable), booted with -wipe-data and killed at the end. For every pair the
# fixed start check (tools/android/jarvis_start_check.sh) runs once with the arm64 library (libndk_translation) and once with x86_64.
# With --page the pairs ALSO get the crash-page check (needs tesseract). Heavy: run through /root/jarvis/bin/heavy.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SDK=${SDK:-/root/android-sdk}
AVD=${AVD:-a19x}; EMU_PORT=${EMU_PORT:-5598}; ADB_PORT=${ADB_PORT:-5047}
EVID=${EVID:-}
PAGE=0
[ "${1:-}" = "--page" ] && { PAGE=1; shift; }
[ $# -ge 1 ] || { sed -n '3,9p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
ADB0="$SDK/platform-tools/adb -P $ADB_PORT"
SERIAL=emulator-$EMU_PORT
ADB="$ADB0 -s $SERIAL"
W=$(mktemp -d)
EMU=""
cleanup() {
    $ADB emu kill >/dev/null 2>&1
    [ -n "$EMU" ] && { sleep 2; kill "$EMU" 2>/dev/null; }
    $ADB0 kill-server >/dev/null 2>&1
    rm -rf "$W"
    rm -rf "$HOME/.android/avd/$AVD.avd" "$HOME/.android/avd/$AVD.ini"
}
trap cleanup EXIT
AVDHOME=${ANDROID_AVD_HOME:-$HOME/.android/avd}
IMG=default; [ -d "$SDK/system-images/android-35/google_apis/x86_64" ] && IMG=google_apis
mkdir -p "$AVDHOME/$AVD.avd"
sed -e "s/firn/$AVD/g" -e "s#android-35/default/#android-35/$IMG/#" -e "s/^tag.id *=.*/tag.id = $IMG/" "$AVDHOME/firn.avd/config.ini" >"$AVDHOME/$AVD.avd/config.ini"
printf 'avd.ini.encoding=UTF-8\npath=%s/%s.avd\npath.rel=avd/%s.avd\ntarget=android-35\n' "$AVDHOME" "$AVD" "$AVD" >"$AVDHOME/$AVD.ini"
$ADB0 start-server >/dev/null 2>&1
ANDROID_ADB_SERVER_PORT=$ADB_PORT ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=$AVDHOME "$SDK/emulator/emulator" -avd $AVD -port $EMU_PORT \
    -no-window -no-audio -no-boot-anim -no-snapshot -wipe-data -gpu swiftshader_indirect >"$W/emu.log" 2>&1 & EMU=$!
for _ in $(seq 1 120); do
    [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break
    sleep 3
done
[ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] || { echo "the emulator did not boot"; exit 1; }
sleep 5
# google_apis images allow adbd as root: then the shell may kill -9 / kill -SEGV the app (the signal proof needs it)
echo "adb root: $($ADB root 2>&1 | tr -d '\r' | head -1)"; sleep 3
$ADB wait-for-device; sleep 3
echo "shell uid: $($ADB shell id -u | tr -d '\r')"
echo "emulator up: $($ADB shell getprop ro.product.cpu.abilist | tr -d '\r')"

TOTAL_FAIL=0
shot() { $ADB exec-out screencap -p >"$W/shot.png" 2>/dev/null; }
ocr() { shot; tesseract "$W/shot.png" - 2>/dev/null; }
keep() { [ -n "$EVID" ] && { mkdir -p "$EVID"; cp "$W/shot.png" "$EVID/$1.png"; }; }
expect_text() { # name pattern
    local t; t=$(ocr)
    if echo "$t" | grep -q -i -E "$2"; then echo "  OK    $1"; keep "$1"; else echo "  FAIL  $1 (pattern '$2' not on screen)"; keep "FAIL-$1"; TOTAL_FAIL=$((TOTAL_FAIL+1)); fi
}
start_app() { $ADB shell monkey -p "$1" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1; }

crash_page() { # apk pkg abi
    local apk=$1 pkg=$2 abi=$3 tag; tag=$(basename "$apk" .apk)-$abi
    echo "-- crash page: $tag"
    $ADB uninstall "$pkg" >/dev/null 2>&1
    $ADB install --abi "$abi" "$apk" >/dev/null 2>&1
    $ADB shell pm grant "$pkg" android.permission.POST_NOTIFICATIONS >/dev/null 2>&1
    start_app "$pkg"; sleep 9
    local t; t=$(ocr)
    if echo "$t" | grep -q -i -E 'crashed|abgest'; then echo "  FAIL  $tag: crash page on a CLEAN first start"; TOTAL_FAIL=$((TOTAL_FAIL+1)); else echo "  OK    $tag: no crash page on a clean start"; fi
    # run 2: killed hard before the first frame; run 3 shows the report
    $ADB shell am force-stop "$pkg"; sleep 1
    start_app "$pkg"; sleep 1
    local pid
    $ADB shell am force-stop "$pkg"
    sleep 2
    start_app "$pkg"; sleep 9
    expect_text "$tag-early-kill-page" 'last start|crashed|startup stages'
    # run 4: clean start to the first frame, a signal 11 sent to it, restart: the dump is on the screen
    $ADB shell am force-stop "$pkg"; sleep 1
    start_app "$pkg"; sleep 10
    pid=$($ADB shell pidof "$pkg" | tr -d '\r')
    $ADB shell "kill -SEGV $pid 2>/dev/null || su 0 kill -SEGV $pid 2>/dev/null || echo no-kill-permission"; sleep 1
    $ADB shell am force-stop "$pkg"; sleep 1
    start_app "$pkg"; sleep 9
    expect_text "$tag-signal-page" 'signal 11|signal dump|pc_off|base 0x'
    $ADB shell am force-stop "$pkg"
}

for pair in "$@"; do
    apk=${pair%%:*}; pkg=${pair##*:}
    for abi in arm64-v8a x86_64; do
        echo "== $(basename "$apk") $pkg $abi"
        PKG=$pkg SERIAL=$SERIAL ADB_PORT=$ADB_PORT bash "$ROOT/tools/android/jarvis_start_check.sh" "$apk" "$abi" || TOTAL_FAIL=$((TOTAL_FAIL+1))
        [ $PAGE -eq 1 ] && command -v tesseract >/dev/null && crash_page "$apk" "$pkg" "$abi"
    done
    $ADB uninstall "$pkg" >/dev/null 2>&1
done
echo "bisect check: $TOTAL_FAIL failed"
[ $TOTAL_FAIL -eq 0 ]
