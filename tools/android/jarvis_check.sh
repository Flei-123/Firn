#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_check.sh -- the JARVIS app (examples/jarvis_android) on an emulator, against tools/mini/fake_jarvis.py
# (the emulator reaches the host as 10.0.2.2).
#
#   SERIAL=emulator-5590 bash tools/android/jarvis_check.sh [apk]
#
# Login (since A9): fixed server, fields 'Username or e-mail' / 'Password', 'Anmelden', then the notice page 'Weiter': all found
# by words (tesseract positions), no fixed login coordinates. Chat page: 'New' 'Chats' 'Voice' buttons, tabs Chat/Board/Tasks/Devices/More.
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
    pkill -P $GUARD 2>/dev/null; kill $GUARD 2>/dev/null; kill $SRV 2>/dev/null; cp "$W/server.log" "$OUT/server.log" 2>/dev/null # the proof the checks read stays with the pictures
    [ -n "$EMU" ] && { $ADB emu kill >/dev/null 2>&1; sleep 2; kill $EMU 2>/dev/null; $ADB0 kill-server >/dev/null 2>&1; }
    rm -rf "$W"
}
trap cleanup EXIT

if [ -z "${SERIAL:-}" ]; then
    AVDHOME=${ANDROID_AVD_HOME:-$HOME/.android/avd}
    AVD=${AVD:-a4b}
    if [ ! -d "$AVDHOME/$AVD.avd" ]; then # own copy: only the config, a fresh data partition
        mkdir -p "$AVDHOME/$AVD.avd"; sed "s/firn/$AVD/g" "$AVDHOME/firn.avd/config.ini" >"$AVDHOME/$AVD.avd/config.ini"
        printf 'avd.ini.encoding=UTF-8\npath=%s/%s.avd\npath.rel=avd/%s.avd\ntarget=android-35\n' "$AVDHOME" "$AVD" "$AVD" >"$AVDHOME/$AVD.ini"
    fi
    # own adb server (port 5039): the emulator registers there, so other checks that run `adb` without a serial on the shared
    # server (port 5037) never see it, install nothing into it and cannot steal its screen
    ADB0="$ADB0 -P ${ADB_PORT:-5039}"; $ADB0 start-server >/dev/null 2>&1
    ANDROID_ADB_SERVER_PORT=${ADB_PORT:-5039} ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=$AVDHOME "$SDK/emulator/emulator" -avd $AVD -port ${EMU_PORT:-5590} -no-window -no-audio \
        -no-boot-anim -no-snapshot -wipe-data -gpu swiftshader_indirect >"$W/emu.log" 2>&1 & EMU=$!
    SERIAL=emulator-${EMU_PORT:-5590}
fi
# every adb call is bounded: on a loaded host an `input text` / `dumpsys` can stall for minutes and the whole check seemed to hang
ADB="timeout ${ADB_TIMEOUT:-60} $ADB0 -s $SERIAL"
. "$ROOT/tools/android/dismiss_anr.sh"
# overall guard: a stuck run ends with a clear message instead of blocking a heavy slot until the wrapper's limit
( sleep "${CHECK_MAX_S:-2400}" && { echo "jarvis_check: overall limit reached, stopping" >&2; kill -TERM $$; } ) >/dev/null 2>&1 & GUARD=$!
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
# focused buttons (blue border) defeat full-page OCR: crop the region, scale x3, read one line
ocr_crop() { python3 -I -c "import sys
from PIL import Image
im=Image.open(sys.argv[1]).convert('L').crop(tuple(map(int,sys.argv[2:6])))
im.resize((im.width*3,im.height*3)).save(sys.argv[6])" "$OUT/$1.png" "$2" "$3" "$4" "$5" "$OUT/$1_crop.png" && tesseract "$OUT/$1_crop.png" stdout --psm 7 2>/dev/null | tr -s '\n ' ' '; }
kbd() { $ADB shell dumpsys input_method | grep -c 'mInputShown=true'; }
# a loaded host makes the first tap slow (the app builds its IME classes: seconds): wait, do not assume
wait_kbd() { for _ in $(seq 20); do [ "$(kbd)" = 1 ] && break; sleep 1; done; kbd; }
wait_sent() { for _ in $(seq 15); do [ "$(grep -c "\"text\": \"$1\"" "$SLOG")" -ge "${2:-1}" ] && break; sleep 1; done; grep -c "\"text\": \"$1\"" "$SLOG"; }
# (an emulator "isn't responding" dialog covers the app on a cold, loaded host: tapped away every 4th miss)
wait_ocr() { local i; for i in $(seq ${OCR_TRIES:-12}); do shot "$1"; ocr "$1" | grep -q "$2" && break
        [ $((i % 4)) = 0 ] && dismiss_anr; sleep 1.5; done; ocr "$1" | grep -c "$2"; }
# BACK only while the keyboard is up: without it BACK leaves the app (a slow host shows the keyboard seconds late)
hide_kbd() { if [ "$(kbd)" = 1 ]; then $ADB shell input keyevent KEYCODE_BACK; fi; sleep 1; }
# the centre (pixels) of the n-th word matching a regex in the screenshot $1; empty when there is none
word_at() { python3 -I "$(dirname "${BASH_SOURCE[0]}")/ocr_words.py" "$OUT/$1.png" "$2" "${3:-1}"; }
# touch the n-th word matching a regex ($1); returns 1 when the word is not on the screen
tap_word() { local p; shot _w; p=$(word_at _w "$1" "${2:-1}"); [ -n "$p" ] || { echo "  (no word $1 on the screen)"; return 1; }
    $ADB shell input tap $p; sleep "${3:-1.5}"; }
# type into the field whose hint is $1, then leave the keyboard
type_into() { tap_word "$1" 1 1.5 || return 1; wait_kbd >/dev/null; $ADB shell input text "$2"; sleep 1; hide_kbd; }
# the sign-in page: user, password, "Sign in", then the notice page ("Continue") that follows the first sign-in
login() { type_into "Benutzername" justin; type_into "Passwort" pw; tap_word "Anmelden" 1 1
    if [ "$(OCR_TRIES=20 wait_ocr _notice 'Weiter')" = 1 ]; then tap_word "Weiter" 1 3; fi; }
# chat page (1080x2340, web-like composer with icon buttons): message field (418,1911), mic (790,1911), send (960,1911), "Voice" chip (450,380)
if [ "$($ADB shell settings get secure navigation_mode 2>/dev/null | tr -d '\r')" = 2 ]; then FIELD_Y=1911; else FIELD_Y=1845; fi
FIELD_X=418; SEND_X=960; MIC_X=790; VOICE_X=450; VOICE_Y=380

$ADB uninstall $PKG >/dev/null 2>&1
$ADB install -r "$APK" >/dev/null 2>&1
check "install" "$($ADB shell pm path $PKG | head -c 8)" "package:"
$ADB shell mkdir -p /sdcard/Android/data/$PKG/files
printf 'http://10.0.2.2:%s\n' "$PORT" >"$W/args.txt"
$ADB push "$W/args.txt" /sdcard/Android/data/$PKG/files/args.txt >/dev/null
$ADB shell pm grant $PKG android.permission.RECORD_AUDIO >/dev/null 2>&1
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS >/dev/null 2>&1 # else the system asks after the notice page and covers the chat
$ADB logcat -c
sleep 20 # a fresh emulator is still starting its launcher, Gboard and the package manager
dismiss_anr
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
for _ in $(seq 40); do [ -n "$($ADB shell pidof $PKG)" ] && break; sleep 1; done
check "login screen shows" "$(OCR_TRIES=40 wait_ocr 1_login 'Passwort')" 1
check "process runs" "$([ -n "$($ADB shell pidof $PKG)" ] && echo yes)" yes

# ---- login: found by words; the soft keyboard shifts the layout, so each field is left with Back before the next tap
tap_word "Benutzername" 1 1; check "Gboard shows for the user field" "$(wait_kbd)" 1
$ADB shell input text justin; hide_kbd
type_into "Passwort" pw
tap_word "Anmelden" 1 1
check "the notice page shows after the first sign-in" "$(OCR_TRIES=20 wait_ocr 1b_notice 'Weiter')" 1
tap_word "Weiter" 1 3
check "chat screen after login" "$(OCR_TRIES=30 wait_ocr 2_chat 'Chats')" 1
check "login request reached the server" "$(grep -c '"type": "login"' "$SLOG")" 1

# ---- message + streamed reply
# right after the login the connection is still being built ("offline - reconnecting"): a message sent before it is up is lost
# the green "online" pill (outline, small) is lost by full-page OCR: crop the header corner, scale x3, read the line
online_pill() { local i n=${1:-2b_online}; for i in $(seq 30); do shot $n
        python3 -I -c "import sys
from PIL import Image
im=Image.open(sys.argv[1]).convert('L').crop((820,140,1080,240))
im.resize((im.width*3,im.height*3)).save(sys.argv[2])" "$OUT/$n.png" "$W/pill.png"
        tesseract "$W/pill.png" stdout --psm 7 2>/dev/null | grep -qi 'online' && { echo 1; return; }; sleep 1.5; done; echo 0; }
check "online after the login" "$(online_pill)" 1
tap $FIELD_X $FIELD_Y 1; check "Gboard shows for the message field" "$(wait_kbd)" 1; $ADB shell input text hello; sleep 1
hide_kbd # BACK only while the keyboard is up, else it leaves the app
tap $SEND_X $FIELD_Y 1
check "message 'hello' reached the server" "$(wait_sent hello)" 1
check "streamed reply visible" "$(wait_ocr 3_reply 'Hallo hello')" 1

# ---- tabs: five different screens, no crash. The bar (Chat Board Tasks Devices More) is found by words; "Chat" brings the chat back.
# Then a restart (the session is saved).
HASH_CHAT=$(md5sum <"$OUT/3_reply.png" | cut -c1-8)
n=0; for w in Board Aufgaben "Ger.te" "Me[h]r"; do
    tap_word "$w" 1 3; n=$((n+1)); shot tab$n
    h=$(md5sum <"$OUT/tab$n.png" | cut -c1-8)
    check "tab $w differs from the chat" "$([ "$h" != "$HASH_CHAT" ] && echo yes)" yes
    tap_word "Chat" 1 3
    check "tab Chat shows the chat again" "$(wait_ocr tabchat$n 'Chats')" 1
done
$ADB shell am force-stop $PKG; $ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
if [ "$(OCR_TRIES=25 wait_ocr 7_restart 'Chats')" = 1 ]; then echo "  NOTE  restart: saved session restored"; else
    echo "  NOTE  restart: saved session NOT restored on this emulator image (std.secret) - signing in again"
    login
    check "second sign-in works" "$(OCR_TRIES=30 wait_ocr 8_chat_again 'Chats')" 1
fi

# after the restart the connection is rebuilt ("offline - reconnecting"): a message sent before it is up is not delivered
check "online again after the restart" "$(online_pill 8b_online)" 1 # the pill is small and outlined: full-page OCR misses it, the crop of the header corner reads it (as after the login)

# ---- keyboard: typing, cursor, selection, copy/paste. Proof = the exact text the server receives.
tap $FIELD_X $FIELD_Y 1; wait_kbd >/dev/null; sleep 2; $ADB shell input text abd; sleep 1 # a loaded host reorders fast key events: one step per second
$ADB shell input keyevent KEYCODE_DPAD_LEFT; sleep 1; $ADB shell input text c      # cursor moved left, inserted: abcd
sleep 1; hide_kbd; tap $SEND_X $FIELD_Y 1
check "cursor move + insert -> 'abcd'" "$(wait_sent abcd 1)" 1
tap $FIELD_X $FIELD_Y 1; wait_kbd >/dev/null; sleep 1; $ADB shell input text abXY; sleep 1
$ADB shell input keycombination 59 21; $ADB shell input keycombination 59 21; sleep 1   # Shift+Left twice: select 'XY'
$ADB shell input text cd                                                       # replaces the selection
sleep 1; hide_kbd; tap $SEND_X $FIELD_Y 1
check "selection replaced -> 'abcd' again" "$(wait_sent abcd 2)" 2
tap $FIELD_X $FIELD_Y 1; wait_kbd >/dev/null; $ADB shell input text copyme
$ADB shell input keycombination 113 29; $ADB shell input keycombination 113 31   # Ctrl+A, Ctrl+C
$ADB shell input keyevent KEYCODE_DEL; $ADB shell input keycombination 113 50   # delete, Ctrl+V
sleep 1; hide_kbd; tap $SEND_X $FIELD_Y 1
check "select all + copy + paste -> 'copyme'" "$(wait_sent copyme 1)" 1

# ---- mic button (speech.fi -> WAV -> POST /api/stt -> text in the input line) and the voice-output switch (tts.fi)
tap $MIC_X $FIELD_Y 3; check "mic button starts recording" "$(wait_ocr 4_rec 'Stopp tippen')" 1
sleep 2; tap $MIC_X $FIELD_Y 1
for _ in $(seq 15); do grep -q '/api/stt' "$SLOG" && break; sleep 1; done
check "recording uploaded as audio/wav to /api/stt" "$(grep -c '"path": "/api/stt?lang=de", "ctype": "audio/wav"' "$SLOG")" 1
check "transcription lands in the input line" "$(wait_ocr 5_heard 'audio')" 1
tap_word "Sprache" 1 2; shot 6_voice; check "voice switch shows 'Voice on'" "$(ocr_crop 6_voice 330 320 660 440 | grep -ci 'prache an')" 1 # same word tap + crop as jarvis_e2e.sh (the A21 design moved the chip row; the old fixed pixels hit nothing)
check "app alive after mic + voice" "$([ -n "$($ADB shell pidof $PKG)" ] && echo yes)" yes

check "no crash in logcat" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|ANR in '$PKG)" 0
$ADB shell am force-stop $PKG
echo "JARVIS APP: $PASS passed, $FAIL failed. Pictures in $OUT"
[ $FAIL -eq 0 ]
