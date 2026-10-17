#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_check.sh -- the JARVIS app (examples/jarvis_android) on an emulator, against tools/mini/fake_jarvis.py
# (the emulator reaches the host as 10.0.2.2).
#
#   SERIAL=emulator-5590 bash tools/android/jarvis_check.sh [apk]
#
# Own AVD copy "a4b" (NOT the shared "firn" one, NOT port 5584): without SERIAL the script boots it on console port 5590 and
# shuts it down at the end. Nothing is guessed from the app itself (no protocol output): the fake server's log is the proof
# for what the app SENT, tesseract reads what the app SHOWS. Taps use fractions of the screen (portrait, 1080x2340 reference),
# because the layout is fixed and centred; the soft keyboard shifts the login layout, so every field is left with Back first.
# Checks: login, message, streamed reply, 5 tabs, keyboard (Gboard shows, typing, cursor move, selection replaces, select-all
# + copy + paste), mic button (speech -> WAV -> /api/stt), voice-output switch, no crash.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SDK=${SDK:-/root/android-sdk}
ADB0=${ADB:-$SDK/platform-tools/adb}
APK=${1:-/root/jarvis-android-builds/jarvis-0.1.0.apk}
PKG=org.fleitec.jarvis
OUT=${OUT:-$(mktemp -d)}
PASS=0; FAIL=0
check() { # name got want
    if [ "$2" = "$3" ]; then echo "  OK    $1"; PASS=$((PASS+1))
    else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi
}
python3 -c "import websockets" 2>/dev/null || { echo "skip: python3 websockets missing"; exit 0; }
W=$(mktemp -d)
EMU=""
cleanup() {
    kill $SRV 2>/dev/null
    [ -n "$EMU" ] && { $ADB emu kill >/dev/null 2>&1; sleep 2; kill $EMU 2>/dev/null; $ADB0 kill-server >/dev/null 2>&1; }
    rm -rf "$W"
}
trap cleanup EXIT

if [ -z "${SERIAL:-}" ]; then
    AVDHOME=${ANDROID_AVD_HOME:-$HOME/.android/avd}
    if [ ! -d "$AVDHOME/a4b.avd" ]; then # own copy: only the config, a fresh data partition
        mkdir -p "$AVDHOME/a4b.avd"; sed 's/firn/a4b/g' "$AVDHOME/firn.avd/config.ini" >"$AVDHOME/a4b.avd/config.ini"
        printf 'avd.ini.encoding=UTF-8\npath=%s/a4b.avd\npath.rel=avd/a4b.avd\ntarget=android-35\n' "$AVDHOME" >"$AVDHOME/a4b.ini"
    fi
    # own adb server (port 5039): the emulator registers there, so other checks that run `adb` without a serial on the shared
    # server (port 5037) never see it, install nothing into it and cannot steal its screen
    ADB0="$ADB0 -P ${ADB_PORT:-5039}"; $ADB0 start-server >/dev/null 2>&1
    ANDROID_ADB_SERVER_PORT=${ADB_PORT:-5039} ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=$AVDHOME "$SDK/emulator/emulator" -avd a4b -port 5590 -no-window -no-audio \
        -no-boot-anim -no-snapshot -wipe-data -gpu swiftshader_indirect >"$W/emu.log" 2>&1 & EMU=$!
    SERIAL=emulator-5590
fi
ADB="$ADB0 -s $SERIAL"
for _ in $(seq 120); do [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break; sleep 3; done
[ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] || { echo "the emulator did not boot"; exit 1; }
mkdir -p "$OUT"

python3 "$ROOT/tools/mini/fake_jarvis.py" "$W/server.log" >"$W/out" 2>&1 & SRV=$!
for _ in $(seq 50); do grep -q '^PORT' "$W/out" && break; sleep 0.1; done
PORT=$(awk '/^PORT/{print $2}' "$W/out")
SLOG=$W/server.log

# screen -> pixels (reference 1080x2340)
read -r SW SH < <($ADB shell wm size | tr -d '\r' | sed -n 's/.*: *\([0-9]*\)x\([0-9]*\).*/\1 \2/p' | tail -1)
tap() { $ADB shell input tap $(( $1 * SW / 1080 )) $(( $2 * SH / 2340 )); sleep "${3:-1.5}"; }
shot() { $ADB exec-out screencap -p >"$OUT/$1.png"; }
ocr() { tesseract "$OUT/$1.png" stdout 2>/dev/null | tr -s '\n ' ' '; }
kbd() { $ADB shell dumpsys input_method | grep -c 'mInputShown=true'; }
# a loaded host makes the first tap slow (the app builds its IME classes: seconds): wait, do not assume
wait_kbd() { for _ in $(seq 20); do [ "$(kbd)" = 1 ] && break; sleep 1; done; kbd; }
wait_sent() { for _ in $(seq 15); do [ "$(grep -c "\"text\": \"$1\"" "$SLOG")" -ge "${2:-1}" ] && break; sleep 1; done; grep -c "\"text\": \"$1\"" "$SLOG"; }
wait_ocr() { for _ in $(seq ${OCR_TRIES:-12}); do shot "$1"; ocr "$1" | grep -q "$2" && break; sleep 1.5; done; ocr "$1" | grep -c "$2"; }
hide_kbd() { $ADB shell input keyevent KEYCODE_BACK; sleep 1; }
# login layout without keyboard: user 1149, password 1265, sign in 1383 | chat: message field (507,1898), send (884,1898),
# mic is the "+" row; tabs y=2140 at x 160 351 552 736 914
FIELD_X=417; FIELD_Y=1898; SEND_X=904; MIC_X=714; VOICE_X=663; VOICE_Y=391; TAB_Y=2140

$ADB uninstall $PKG >/dev/null 2>&1
$ADB install -r "$APK" >/dev/null 2>&1
check "install" "$($ADB shell pm path $PKG | head -c 8)" "package:"
$ADB shell mkdir -p /sdcard/Android/data/$PKG/files
printf 'http://10.0.2.2:%s\n' "$PORT" >"$W/args.txt"
$ADB push "$W/args.txt" /sdcard/Android/data/$PKG/files/args.txt >/dev/null
$ADB shell pm grant $PKG android.permission.RECORD_AUDIO >/dev/null 2>&1
$ADB logcat -c
sleep 20 # a fresh emulator is still starting its launcher, Gboard and the package manager
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
for _ in $(seq 40); do [ -n "$($ADB shell pidof $PKG)" ] && break; sleep 1; done
check "login screen shows" "$(OCR_TRIES=40 wait_ocr 1_login 'Sign in')" 1
check "process runs" "$([ -n "$($ADB shell pidof $PKG)" ] && echo yes)" yes

# ---- login: the soft keyboard shifts the layout, so each field is left with Back before the next tap
tap 540 1149 1; check "Gboard shows for the user field" "$(wait_kbd)" 1
$ADB shell input text justin; hide_kbd
tap 540 1265; $ADB shell input text pw; hide_kbd
tap 540 1383 1
check "chat screen after login" "$(OCR_TRIES=30 wait_ocr 2_chat 'New chat')" 1
check "login request reached the server" "$(grep -c '"type": "login"' "$SLOG")" 1

# ---- message + streamed reply
tap $FIELD_X $FIELD_Y 1; wait_kbd >/dev/null; $ADB shell input text hello; sleep 1
$ADB shell input keyevent KEYCODE_BACK; sleep 1
tap $SEND_X $FIELD_Y 1
check "message 'hello' reached the server" "$(wait_sent hello)" 1
check "streamed reply visible" "$(wait_ocr 3_reply 'Hallo hello')" 1

# ---- tabs: five different screens, no crash. The tab bar sits right under the content of the placeholder pages (not pinned to
# the bottom as on the chat page): back to Chat from there is a tap at y=1416. Prefs last, then a restart (the session is saved).
HASH_CHAT=$(md5sum <"$OUT/3_reply.png" | cut -c1-8)
n=0; for x in 351 552 736; do
    tap $x $TAB_Y 3; n=$((n+1)); shot tab$n
    h=$(md5sum <"$OUT/tab$n.png" | cut -c1-8)
    check "tab at x=$x differs from the chat" "$([ "$h" != "$HASH_CHAT" ] && echo yes)" yes
    tap 160 1416 3
    check "tab Chat shows the chat again" "$(wait_ocr tabchat$n 'New chat')" 1
done
tap 914 $TAB_Y 3; shot tab4
check "tab Prefs differs from the chat" "$([ "$(md5sum <"$OUT/tab4.png" | cut -c1-8)" != "$HASH_CHAT" ] && echo yes)" yes
$ADB shell am force-stop $PKG; $ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
if [ "$(OCR_TRIES=25 wait_ocr 7_restart 'New chat')" = 1 ]; then echo "  NOTE  restart: saved session restored"; else
    echo "  NOTE  restart: saved session NOT restored on this emulator image (std.secret) - signing in again"
    tap 540 1149 1; wait_kbd >/dev/null; $ADB shell input text justin; hide_kbd
    tap 540 1265 1; $ADB shell input text pw; hide_kbd; tap 540 1383 1
    check "second sign-in works" "$(OCR_TRIES=30 wait_ocr 8_chat_again 'New chat')" 1
fi

# ---- keyboard: typing, cursor, selection, copy/paste. Proof = the exact text the server receives.
tap $FIELD_X $FIELD_Y 1; wait_kbd >/dev/null; $ADB shell input text abd
$ADB shell input keyevent KEYCODE_DPAD_LEFT; $ADB shell input text c      # cursor moved left, inserted: abcd
sleep 1; hide_kbd; tap $SEND_X $FIELD_Y 1
check "cursor move + insert -> 'abcd'" "$(wait_sent abcd 1)" 1
tap $FIELD_X $FIELD_Y 1; wait_kbd >/dev/null; $ADB shell input text abXY
$ADB shell input keycombination 59 21; $ADB shell input keycombination 59 21   # Shift+Left twice: select 'XY'
$ADB shell input text cd                                                       # replaces the selection
sleep 1; hide_kbd; tap $SEND_X $FIELD_Y 1
check "selection replaced -> 'abcd' again" "$(wait_sent abcd 2)" 2
tap $FIELD_X $FIELD_Y 1; wait_kbd >/dev/null; $ADB shell input text copyme
$ADB shell input keycombination 113 29; $ADB shell input keycombination 113 31   # Ctrl+A, Ctrl+C
$ADB shell input keyevent KEYCODE_DEL; $ADB shell input keycombination 113 50   # delete, Ctrl+V
sleep 1; hide_kbd; tap $SEND_X $FIELD_Y 1
check "select all + copy + paste -> 'copyme'" "$(wait_sent copyme 1)" 1

# ---- mic button (speech.fi -> WAV -> POST /api/stt -> text in the input line) and the voice-output switch (tts.fi)
tap $MIC_X $FIELD_Y 3; check "mic button starts recording" "$(wait_ocr 4_rec 'tap Stop')" 1
sleep 2; tap $MIC_X $FIELD_Y 1
for _ in $(seq 15); do grep -q '/api/stt' "$SLOG" && break; sleep 1; done
check "recording uploaded as audio/wav to /api/stt" "$(grep -c '"path": "/api/stt?lang=de", "ctype": "audio/wav"' "$SLOG")" 1
check "transcription lands in the input line" "$(wait_ocr 5_heard 'audio')" 1
tap $VOICE_X $VOICE_Y 2; shot 6_voice; check "voice switch shows 'Voice on'" "$(ocr 6_voice | grep -ci 'oice on')" 1
check "app alive after mic + voice" "$([ -n "$($ADB shell pidof $PKG)" ] && echo yes)" yes

check "no crash in logcat" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|ANR in '$PKG)" 0
$ADB shell am force-stop $PKG
echo "JARVIS APP: $PASS passed, $FAIL failed. Pictures in $OUT"
[ $FAIL -eq 0 ]
