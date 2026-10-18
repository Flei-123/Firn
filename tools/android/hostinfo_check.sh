#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/hostinfo_check.sh -- r382: Configuration.fontScale and WindowInsets over JNI (lib/android/hostinfo.fi), on an emulator.
#
# tools/android/hostinfo/main.fi as an app prints one line (stdout.txt in the app's data dir). The script sets the system font size
# (settings put system font_scale), starts the app and compares the answer. An emulator that is up is used (SERIAL) and left running.
#
#   SERIAL=emulator-5618 bash tools/android/hostinfo_check.sh      (ADB, FIRNC from env)
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
export FIRNLIB=$ROOT/lib FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
PKG=org.firn.hostinfotest
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok    $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL  $1"; [ -n "${2:-}" ] && echo "        $2" | head -6; }
has() { if printf '%s\n' "$2" | grep -qE -- "$3"; then ok "$1"; else bad "$1" "wanted /$3/ in: $2"; fi; }

[ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] || { echo "SKIP: no booted device"; exit 0; }
ABI=$($ADB shell getprop ro.product.cpu.abi | tr -d '\r')
case "$ABI" in arm64-v8a) BA=arm64 ;; x86_64) BA=x86_64 ;; *) echo "SKIP: ABI $ABI"; exit 0 ;; esac
echo "   $ABI API $($ADB shell getprop ro.build.version.sdk | tr -d '\r')"

[ -n "${SKIP_BUILD:-}" ] || bash tools/android/build.sh tools/android/hostinfo/main.fi --name "fUi Hostinfo" --package $PKG --abi "$BA" 2>&1 | tail -1
APK=$ROOT/build/android/fui_hostinfo/fui_hostinfo.apk
$ADB install -r "$APK" | tail -1

run() { # prints the app's line
    $ADB shell am force-stop $PKG
    $ADB shell rm -f /sdcard/Android/data/$PKG/files/stdout.txt
    $ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null 2>&1 || $ADB shell monkey -p $PKG -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
    for i in $(seq 1 20); do
        sleep 1
        L=$($ADB shell cat /sdcard/Android/data/$PKG/files/stdout.txt 2>/dev/null | tr -d '\r' | grep HOSTINFO)
        [ -n "$L" ] && break
    done
    echo "$L"
}
$ADB shell settings put system font_scale 1.0
L=$(run); echo "   $L"
has "default font size: 1000 per mille" "$L" "fontscale_mil=1000"
has "the API level is reported" "$L" "sdk=[0-9]{2}"
has "WindowInsets known (not '?')" "$L" "raw_ins=[0-9]+,[0-9]+,[0-9]+,[0-9]+"
$ADB shell settings put system font_scale 1.3
L=$(run); echo "   $L"
has "font size 1.3 over JNI" "$L" "fontscale_mil=1300"
has "the toolkit's text scale follows (opt-in)" "$L" "text=1300"
$ADB shell settings put system font_scale 0.85
L=$(run); echo "   $L"
has "font size 0.85" "$L" "fontscale_mil=850"
# WHILE THE APP RUNS: a change of the system font size arrives as onConfigurationChanged -> EV_THEME -> a new look
$ADB shell settings put system font_scale 1.0
$ADB shell am force-stop $PKG
$ADB shell rm -f /sdcard/Android/data/$PKG/files/stdout.txt
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null 2>&1
sleep 4
$ADB shell settings put system font_scale 1.5
for i in $(seq 1 15); do sleep 1; LV=$($ADB shell cat /sdcard/Android/data/$PKG/files/stdout.txt 2>/dev/null | tr -d '\r' | grep "LIVE text=1500"); [ -n "$LV" ] && break; done
echo "   $LV"
has "font size changed while running: new text scale arrives" "$LV" "LIVE text=1500"
# ROTATION: bars move to other sides -> the insets are read again
$ADB shell settings put system accelerometer_rotation 0
$ADB shell settings put system user_rotation 1
for i in $(seq 1 15); do sleep 1; LV=$($ADB shell cat /sdcard/Android/data/$PKG/files/stdout.txt 2>/dev/null | tr -d '\r' | grep "^LIVE" | tail -1); echo "$LV" | grep -q "looks=[3-9]" && break; done
echo "   $LV"
has "rotation: the insets were read again (looks >= 3)" "$LV" "looks=[3-9]"
$ADB shell settings put system user_rotation 0
$ADB shell settings put system accelerometer_rotation 1
$ADB shell settings put system font_scale 1.0
$ADB shell am force-stop $PKG
echo "== $PASS ok, $FAIL failed"
[ "$FAIL" = 0 ]
