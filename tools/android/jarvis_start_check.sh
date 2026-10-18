#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_start_check.sh -- fixed start check: the JARVIS app starts and shows the sign-in page, no crash.
#
#   SERIAL=emulator-5594 ADB_PORT=5043 bash tools/android/jarvis_start_check.sh <apk> [abi]
#
# A running emulator/phone is required (SERIAL). abi (arm64-v8a | x86_64) forces the ABI of the install (`adb install --abi`): on the
# google_apis x86_64 image arm64-v8a runs through libndk_translation, so the arm64 library of an APK really executes there.
# Three ways to start (the way a person starts it, then the explicit launcher component, then the old component name), each:
# process alive after 5 s, the app on top, empty crash buffer, sign-in words on the screen (OCR, when tesseract exists).
set -uo pipefail
APK=${1:?usage: jarvis_start_check.sh <apk> [abi]}
ABI=${2:-}
SDK=${SDK:-/root/android-sdk}
ADB="$SDK/platform-tools/adb -P ${ADB_PORT:-5037} -s ${SERIAL:?set SERIAL}"
PKG=${PKG:-org.fleitec.jarvis}
. "$(dirname "$0")/dismiss_anr.sh"
PASS=0; FAIL=0
check() { # name got want
    if [ "$2" = "$3" ]; then echo "  OK    $1"; PASS=$((PASS+1))
    else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi
}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
$ADB uninstall $PKG >/dev/null 2>&1
if [ -n "$ABI" ]; then $ADB install --abi "$ABI" "$APK" >"$W/inst" 2>&1; else $ADB install "$APK" >"$W/inst" 2>&1; fi
check "install" "$(grep -c "^Success" "$W/inst")" 1
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS >/dev/null 2>&1
run_case() { # name, then the start command
    local name=$1; shift
    $ADB shell am force-stop $PKG; sleep 1
    $ADB logcat -c; $ADB logcat -b crash -c
    "$@" >/dev/null 2>&1
    sleep 5
    dismiss_anr; sleep 1 # a cold emulator may show its own "System UI isn't responding" over the app
    check "$name: process alive after 5 s" "$([ -n "$($ADB shell pidof $PKG | tr -d '\r')" ] && echo yes || echo no)" yes
    check "$name: app on top" "$($ADB shell dumpsys activity activities | grep -m1 topResumedActivity | grep -c "$PKG/")" 1
    check "$name: crash buffer empty" "$($ADB logcat -b crash -d | grep -c -E 'FATAL|SIGSEGV|SIGABRT|Fatal signal')" 0
    if command -v tesseract >/dev/null; then
        local ok=no t
        for t in 1 2 3 4; do # the page may still be drawing or an emulator dialog may pop up late: look again
            $ADB exec-out screencap -p >"$W/shot.png" 2>/dev/null
            tesseract "$W/shot.png" - 2>/dev/null | grep -q -i -E 'sign in|anmelden' && { ok=yes; break; }
            dismiss_anr; sleep 2
        done
        check "$name: sign-in page on screen" "$ok" yes
    fi
}
run_case "launcher intent" $ADB shell monkey -p $PKG -c android.intent.category.LAUNCHER 1
COMP=$($ADB shell cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.LAUNCHER $PKG | tail -1 | tr -d '\r')
echo "  launcher component: $COMP"
run_case "component $COMP" $ADB shell am start -n "$COMP"
run_case "old component name" $ADB shell am start -n $PKG/android.app.NativeActivity
$ADB shell am force-stop $PKG
echo "start check: $PASS ok, $FAIL failed"
[ $FAIL -eq 0 ]
