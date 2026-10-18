#!/bin/bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_login_de.sh -- proof that the JARVIS app shows the GERMAN "wrong password" text on an emulator whose system
# language is German (de-DE): wrong user/password against tools/mini/fake_jarvis.py (answers 401) -> the screen must say
# "Benutzername oder Passwort stimmt nicht." Pictures: $OUT/de_login.png, de_error.png.  # english: ok
#   SERIAL=emulator-5594 ADB_PORT=5043 OUT=<dir> bash tools/android/jarvis_login_de.sh [apk]   (a running emulator; it is NOT stopped)
# Without SERIAL it boots the AVD like jarvis_e2e.sh (AVD/EMU_PORT/ADB_PORT, with -change-locale de-DE) and stops it again;
# with SERIAL the running emulator must already be German (boot it with -change-locale de-DE).
# The touch/OCR helpers are taken from jarvis_e2e.sh (block "shot() ... app_log()"), so there is one copy of them.
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
SDK=${SDK:-/root/android-sdk}
APK=${1:-$(ls -t /root/jarvis-android-builds/jarvis-*.apk 2>/dev/null | head -1)}
PKG=org.fleitec.jarvis
ACT=$PKG/android.app.NativeActivity
OUT=${OUT:-$(mktemp -d)}
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  OK    $1"; PASS=$((PASS+1)); else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
note() { echo "  NOTE  $*"; }
[ -f "$APK" ] || { echo "no apk: $APK"; exit 2; }
for t in tesseract python3; do command -v $t >/dev/null || { echo "skip: $t missing"; exit 0; }; done
python3 -c "import websockets, PIL" 2>/dev/null || { echo "skip: python3 websockets/PIL missing"; exit 0; }
W=$(mktemp -d)
EMU=""; SRV=""
ADB_PORT=${ADB_PORT:-5043}
AVD=${AVD:-a7e}
ADB0="$SDK/platform-tools/adb -P $ADB_PORT"
cleanup() {
    [ -n "$SRV" ] && kill $SRV 2>/dev/null
    cp "$SLOG" "$OUT/server_de.log" 2>/dev/null
    [ -n "$EMU" ] && { $ADB emu kill >/dev/null 2>&1; sleep 2; kill $EMU 2>/dev/null; $ADB0 kill-server >/dev/null 2>&1; }
    rm -rf "$W"
}
trap cleanup EXIT
if [ -z "${SERIAL:-}" ]; then
    AVDHOME=${ANDROID_AVD_HOME:-$HOME/.android/avd}
    $ADB0 start-server >/dev/null 2>&1
    ANDROID_ADB_SERVER_PORT=$ADB_PORT ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=$AVDHOME "$SDK/emulator/emulator" -avd $AVD -port ${EMU_PORT:-5594} \
        -no-window -no-audio -no-boot-anim -no-snapshot -wipe-data -gpu swiftshader_indirect -change-locale de-DE >"$W/emu.log" 2>&1 & EMU=$!
    SERIAL=emulator-${EMU_PORT:-5594}
fi
ADB="$ADB0 -s $SERIAL"
booted() { [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; }
for _ in $(seq 150); do booted && break; sleep 3; done
booted || { echo "the emulator did not boot"; exit 1; }
mkdir -p "$OUT"

# the system language must be German: an emulator booted here gets it with -change-locale de-DE; a running one (SERIAL) must already have it
# (restarting the framework of a loaded host leaves "System UI is not responding" on the screen)
$ADB root >/dev/null 2>&1; sleep 3
# -change-locale restarts the framework after the first boot: wait until the language is German and the system answers again
for _ in $(seq 60); do [ "$($ADB shell getprop persist.sys.locale 2>/dev/null | tr -d '\r')" = de-DE ] && booted && $ADB shell "ls /sdcard/Android >/dev/null 2>&1 && pm path android >/dev/null 2>&1" && break; sleep 3; done
sleep 20
check "the system language is German" "$($ADB shell getprop persist.sys.locale | tr -d '\r')" de-DE

read -r SW SH < <($ADB shell wm size | tr -d '\r' | sed -n 's/.*: *\([0-9]*\)x\([0-9]*\).*/\1 \2/p' | tail -1)
# helpers (shot, ocr, word_at, tap_word, wait_ocr, kbd, type_into ...) from the E2E script
eval "$(sed -n '/^shot() /,/^app_log() /p' "$HERE/jarvis_e2e.sh" | sed '$d')"
FAKE_POLL_MAX=6 python3 "$ROOT/tools/mini/fake_jarvis.py" "$W/server.log" >"$W/out" 2>&1 & SRV=$!
for _ in $(seq 50); do grep -q '^PORT' "$W/out" && break; sleep 0.1; done
PORT=$(awk '/^PORT/{print $2}' "$W/out")
SLOG=$W/server.log

$ADB root >/dev/null 2>&1; sleep 3
for _ in $(seq 40); do $ADB shell "ls /sdcard/Android >/dev/null 2>&1" && break; sleep 3; done
$ADB uninstall $PKG >/dev/null 2>&1
$ADB install -r "$APK" >/dev/null 2>&1
check "install" "$($ADB shell pm path $PKG | head -c 8)" "package:"
$ADB shell mkdir -p /sdcard/Android/data/$PKG/files
printf 'http://10.0.2.2:%s\n' "$PORT" >"$W/args.txt"
$ADB push "$W/args.txt" /sdcard/Android/data/$PKG/files/args.txt >/dev/null
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS >/dev/null 2>&1
$ADB logcat -c
$ADB shell am start -n $ACT >/dev/null
for _ in $(seq 40); do [ -n "$($ADB shell pidof $PKG)" ] && break; sleep 1; done
check "German login screen shows ('Anmelden')" "$(OCR_TRIES=40 wait_ocr de_login 'Anmelden')" 1
check "German login screen: field 'Passwort'" "$(ocr de_login | grep -c 'Passwort')" 1
type_into "Benutzername" justin; type_into "Passwort" wrongpw
tap_word "Anmelden" 1 1
check "the wrong login reached the server (401)" "$(for _ in $(seq 20); do grep -q '"type": "login"' "$SLOG" && break; sleep 1; done; grep -c '"type": "login"' "$SLOG")" 1
check "the screen says it in German: wrong user name or password" "$(OCR_TRIES=15 wait_ocr de_error 'Passwort stimmt nicht')" 1  # english: ok
check "no English error on the screen" "$(ocr de_error | grep -ci 'Wrong user')" 0
check "no crash in logcat" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|ANR in '$PKG)" 0
$ADB shell am force-stop $PKG
echo "JARVIS LOGIN DE: $PASS passed, $FAIL failed. Pictures in $OUT"
[ $FAIL -eq 0 ]
