#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_e2e.sh -- the JARVIS app (examples/jarvis_android) end to end on an emulator, against tools/mini/fake_jarvis.py
# (the emulator reaches the host as 10.0.2.2): the way a person uses the app, phone-sized, with REAL touches and the system's own
# notification service as the judge.
#
#   bash tools/android/jarvis_e2e.sh [apk]          (default: the newest /root/jarvis-android-builds/jarvis-*.apk)
#   SERIAL=emulator-${EMU_PORT:-5592} ADB_PORT=5041 bash ...     (use a running emulator, do not boot or stop one)
#
# Own AVD copy "a6e" (console port 5592, own adb server on port 5041): other checks that use the shared adb server never see it.
# Checks: login, message + streamed reply, Board (projects, a need answered), Tasks (add), conversation list (Chats), photo upload
# (system picker -> POST /api/m/upload -> text carries the server path), file receive (JE_FILE -> download -> Downloads/Jarvis),
# push over the HTTPS long poll while the app is in the background (POST /fake/push -> a REAL notification, tap opens the app,
# the chat), the service text is neutral, the frame time of the app (paint microseconds per frame, <= 16 ms is the 60 Hz budget),
# voice output without a speech engine (status text, no crash), no crash.
# Nothing is guessed from the app itself: the fake server's log is the proof for what the app SENT, `dumpsys notification` for what
# the system SHOWS, tesseract (words with positions) for what is on the screen and where to touch it.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SDK=${SDK:-/root/android-sdk}
APK=${1:-$(ls -t /root/jarvis-android-builds/jarvis-*.apk 2>/dev/null | head -1)}
PKG=org.fleitec.jarvis
ACT=$PKG/android.app.NativeActivity
OUT=${OUT:-$(mktemp -d)}
PASS=0; FAIL=0
check() { # name got want
    if [ "$2" = "$3" ]; then echo "  OK    $1"; PASS=$((PASS+1))
    else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi
}
note() { echo "  NOTE  $*"; }
[ -f "$APK" ] || { echo "no apk: $APK"; exit 2; }
for t in tesseract python3; do command -v $t >/dev/null || { echo "skip: $t missing"; exit 0; }; done
python3 -c "import websockets, PIL" 2>/dev/null || { echo "skip: python3 websockets/PIL missing"; exit 0; }
W=$(mktemp -d)
EMU=""; SRV=""
ADB_PORT=${ADB_PORT:-5041}
AVD=${AVD:-a6e}
ADB0="$SDK/platform-tools/adb -P $ADB_PORT"
cleanup() {
    [ -n "$SRV" ] && kill $SRV 2>/dev/null
    cp "$SLOG" "$OUT/server.log" 2>/dev/null
    [ -n "$EMU" ] && { $ADB emu kill >/dev/null 2>&1; sleep 2; kill $EMU 2>/dev/null; $ADB0 kill-server >/dev/null 2>&1; }
    rm -rf "$W"
}
trap cleanup EXIT

if [ -z "${SERIAL:-}" ]; then
    AVDHOME=${ANDROID_AVD_HOME:-$HOME/.android/avd}
    if [ ! -d "$AVDHOME/$AVD.avd" ]; then # own copy: only the config, a fresh data partition
        mkdir -p "$AVDHOME/$AVD.avd"; sed "s/firn/$AVD/g" "$AVDHOME/firn.avd/config.ini" >"$AVDHOME/$AVD.avd/config.ini"
        printf 'avd.ini.encoding=UTF-8\npath=%s/%s.avd\npath.rel=avd/%s.avd\ntarget=android-35\n' "$AVDHOME" "$AVD" "$AVD" >"$AVDHOME/$AVD.ini"
    fi
    $ADB0 start-server >/dev/null 2>&1
    ANDROID_ADB_SERVER_PORT=$ADB_PORT ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=$AVDHOME "$SDK/emulator/emulator" -avd $AVD -port ${EMU_PORT:-5592} \
        -no-window -no-audio -no-boot-anim -no-snapshot -wipe-data -gpu swiftshader_indirect >"$W/emu.log" 2>&1 & EMU=$!
    SERIAL=emulator-${EMU_PORT:-5592}
fi
ADB="$ADB0 -s $SERIAL"
for _ in $(seq 150); do [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break; sleep 3; done
[ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] || { echo "the emulator did not boot"; exit 1; }
mkdir -p "$OUT"

# the fake server; a short poll cap so a quiet poll comes back fast, events appear at once
FAKE_POLL_MAX=${FAKE_POLL_MAX:-6} python3 "$ROOT/tools/mini/fake_jarvis.py" "$W/server.log" >"$W/out" 2>&1 & SRV=$!
for _ in $(seq 50); do grep -q '^PORT' "$W/out" && break; sleep 0.1; done
PORT=$(awk '/^PORT/{print $2}' "$W/out")
SLOG=$W/server.log

read -r SW SH < <($ADB shell wm size | tr -d '\r' | sed -n 's/.*: *\([0-9]*\)x\([0-9]*\).*/\1 \2/p' | tail -1)
shot() { $ADB exec-out screencap -p >"$OUT/$1.png"; }
ocr() { tesseract "$OUT/$1.png" stdout 2>/dev/null | tr -s '\n ' ' '; }
# the centre (pixels) of the n-th word matching a regex in the screenshot $1; empty when there is none
word_at() { python3 -I - "$OUT/$1.png" "$2" "${3:-1}" <<'EOF'
import re, subprocess, sys
png, rx, nth = sys.argv[1], sys.argv[2], int(sys.argv[3])
tsv = subprocess.run(["tesseract", png, "stdout", "--psm", "11", "tsv"], capture_output=True, text=True).stdout.splitlines()
hit = 0
for ln in tsv[1:]:
    f = ln.split("\t")
    if len(f) < 12 or not f[11].strip():
        continue
    if re.fullmatch(rx, f[11].strip()):
        hit += 1
        if hit == nth:
            print(int(f[6]) + int(f[8]) // 2, int(f[7]) + int(f[9]) // 2)
            break
EOF
}
# touch the n-th word matching a regex ($1); fails (returns 1) when the word is not on the screen
tap_word() { local p; shot _w; p=$(word_at _w "$1" "${2:-1}"); [ -n "$p" ] || { echo "  (no word $1 on the screen)"; return 1; }
    $ADB shell input tap $p; sleep "${3:-1.5}"; }
wait_ocr() { local i; for i in $(seq ${OCR_TRIES:-12}); do shot "$1"; ocr "$1" | grep -q "$2" && break; sleep 1.5; done; ocr "$1" | grep -c "$2"; }
wait_log() { local i; for i in $(seq ${2:-20}); do grep -q "$1" "$SLOG" && break; sleep 1; done; grep -c "$1" "$SLOG"; }
# a focused button (blue border) defeats full-page OCR: crop the region, scale x3, read one line
ocr_crop() { python3 -I -c "import sys
from PIL import Image
im=Image.open(sys.argv[1]).convert('L').crop(tuple(map(int,sys.argv[2:6])))
im.resize((im.width*3,im.height*3)).save(sys.argv[6])" "$OUT/$1.png" "$2" "$3" "$4" "$5" "$OUT/$1_crop.png" && tesseract "$OUT/$1_crop.png" stdout --psm 7 2>/dev/null | tr -s '\n ' ' '; }
kbd() { $ADB shell dumpsys input_method | grep -c 'mInputShown=true'; }
hide_kbd() { $ADB shell input keyevent KEYCODE_BACK; sleep 1; }
# type into the field whose hint is $1, then leave the keyboard
type_into() { tap_word "$1" 1 1.5 || return 1; $ADB shell input text "$2"; sleep 1; hide_kbd; }
send_text() { type_into "Message" "$1" && tap_word "Send" 1 1; }
notes() { $ADB shell dumpsys notification --noredact; }
# title / text lines of the notifications of the app (not the service one)
note_lines() { notes | awk -v p="$PKG" '/NotificationRecord\(/ { inrec = ($0 ~ "pkg=" p " ") }
    inrec && /android.title=String \(/ { t=$0; sub(/.*android.title=String \(/,"",t); sub(/\)$/,"",t) }
    inrec && /android.text=String \(/ { x=$0; sub(/.*android.text=String \(/,"",x); sub(/\)$/,"",x); print t " | " x }'; }
ui() { local i=0; while [ $i -lt 5 ]; do $ADB shell uiautomator dump /sdcard/ui.xml 2>&1 | grep -q dumped && break; sleep 1; i=$((i+1)); done
    $ADB shell cat /sdcard/ui.xml | tr '>' '\n'; }
mid() { echo "$1" | tr '[],' '   ' | awk '{print int(($1+$3)/2), int(($2+$4)/2)}'; }
# touch the first element of the system UI (uiautomator) whose text matches the regex
tap_ui() { local b; b=$(ui | grep -E "(text|content-desc)=\"$1" | grep -oP 'bounds="\K[^"]+' | head -1); [ -n "$b" ] || return 1
    $ADB shell input tap $(mid "$b"); sleep 2; }
app_log() { # the app's own log file (appkit.log), newest first
    $ADB shell "cat \$(ls -t \$(find /data/data/$PKG /sdcard/Android/data/$PKG -name 'app.log' 2>/dev/null) 2>/dev/null | head -1) 2>/dev/null"; }

# ---- install ---------------------------------------------------------------------------------------------------------------
$ADB root >/dev/null 2>&1; sleep 1
$ADB uninstall $PKG >/dev/null 2>&1
$ADB install -r "$APK" >/dev/null 2>&1
check "install" "$($ADB shell pm path $PKG | head -c 8)" "package:"
$ADB shell mkdir -p /sdcard/Android/data/$PKG/files
printf 'http://10.0.2.2:%s\n' "$PORT" >"$W/args.txt"
$ADB push "$W/args.txt" /sdcard/Android/data/$PKG/files/args.txt >/dev/null
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS >/dev/null 2>&1
$ADB shell pm grant $PKG android.permission.RECORD_AUDIO >/dev/null 2>&1
# a photo in the gallery for the upload (dated 2030, so it is the first of "Recent" in the picker)
python3 -I -c "
from PIL import Image
im = Image.new('RGB', (1600, 1200), (30, 90, 200))
for i in range(0, 1200, 40):
    for j in range(0, 1600, 40):
        im.putpixel((j, i), (255, 255, 255))
im.save('$W/photo.jpg', quality=90)"
$ADB push "$W/photo.jpg" /sdcard/Pictures/jarvis_e2e.jpg >/dev/null
$ADB shell touch -t 203001011200 /sdcard/Pictures/jarvis_e2e.jpg
$ADB shell am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d file:///sdcard/Pictures/jarvis_e2e.jpg >/dev/null
$ADB logcat -c
sleep 15
$ADB shell am start -n $ACT >/dev/null
for _ in $(seq 40); do [ -n "$($ADB shell pidof $PKG)" ] && break; sleep 1; done
check "login screen shows" "$(OCR_TRIES=40 wait_ocr 01_login 'Sign in')" 1

# ---- 1. login + chat ---------------------------------------------------------------------------------------------------------
echo "== 1. login, message, streamed reply"
type_into "Username" justin; type_into "Password" pw
tap_word "Sign" 1 1
check "the notification notice page shows after the first sign-in" "$(OCR_TRIES=20 wait_ocr 01b_notice 'Continue')" 1
tap_word "Continue" 1 3
check "chat screen after login" "$(OCR_TRIES=30 wait_ocr 02_chat 'Chats')" 1
check "login request reached the server" "$(grep -c '"type": "login"' "$SLOG")" 1
send_text hello
check "message 'hello' reached the server" "$(wait_log '"text": "hello"' 15 | awk '{print ($1>0)?"yes":"no"}')" yes
check "streamed reply visible" "$(OCR_TRIES=15 wait_ocr 03_reply 'Hallo hello')" 1

# ---- 2. Board: projects, a need answered; Tasks: add ---------------------------------------------------------------------
echo "== 2. Board and Tasks"
tap_word "Board" 1 4
check "the mobile routes were asked for (ping)" "$(wait_log '"path": "/api/m/ping"' 15 | awk '{print ($1>0)?"yes":"no"}')" yes
check "Board lists the project" "$(OCR_TRIES=15 wait_ocr 04_board 'Jarvis')" 1
shot 04_board
check "Board shows the roadmap stand" "$(ocr 04_board | grep -ciE '50 ?%|Needs')" 1
tap_word "Needs" 1 3; shot 05_needs
check "Needs list shows the question" "$(OCR_TRIES=10 wait_ocr 05_needs 'Merge')" 1
tap_word 'Merge\?' 1 2; shot 05b_need_answers
tap_word "Yes" 1 3
check "need answered over /api/m/needs/answer" "$(wait_log '"path": "/api/m/needs/answer".*"answer": "Yes"' 10)" 1
tap_word "Tasks" 1 4
check "Tasks list shows the task" "$(OCR_TRIES=15 wait_ocr 06_tasks 'milk')" 1
type_into "Task|New|Add|Text" "callmum" && tap_word "Add" 1 3
check "task added over /api/m/tasks" "$(wait_log '"path": "/api/m/tasks".*callmum' 10 | awk '{print ($1>0)?"yes":"no"}')" yes
tap_word "Chat" 1 2

# ---- 3. conversation list ----------------------------------------------------------------------------------------------------
echo "== 3. conversation list"
tap_word "Chats" 1 3; shot 07_convs
check "conversation list asked over the mobile route" "$(wait_log '"path": "/api/m/chats' 10 | awk '{print ($1>0)?"yes":"no"}')" yes
check "list shows a chat" "$(OCR_TRIES=10 wait_ocr 07_convs 'hello\|worker\|hi')" 1

# ---- 4. photo upload ---------------------------------------------------------------------------------------------------------
echo "== 4. photo upload"
tap_word "New" 1 2
shot _p; p=$(word_at _p 'Message'); [ -n "$p" ] && $ADB shell input tap $((141 * SW / 1080)) ${p#* }; sleep 4; U=$(ui)
check "the system's picker opens" "$(echo "$U" | grep -cE '(text|content-desc)="(Media picker|Photos|Files|Gallery|Camera|Recent|Photo taken)' | awk '{print ($1>0)?"yes":"no"}')" yes
P0=$($ADB shell pidof $PKG)
# the chooser (Camera / Media picker) may come first; then the picker lists the photo as a content description
tap_ui "Media picker" || true
for _ in 1 2 3 4 5 6; do tap_ui "Photo taken" && break; sleep 3; done || note "no photo in the picker"
sleep 10
check "the app survives the picker (same process)" "$($ADB shell pidof $PKG)" "$P0"
check "the photo waits in the input line" "$(OCR_TRIES=15 wait_ocr 08_photo 'photo ')" 1
send_text "see"
check "upload reached the server (POST /api/m/upload)" "$(wait_log '"path": "/api/m/upload' 20 | awk '{print ($1>0)?"yes":"no"}')" yes
check "the text carries the server path of the photo" "$(wait_log '"text": ".*/srv/uploads/' 15 | awk '{print ($1>0)?"yes":"no"}')" yes

# ---- 5. file receive ---------------------------------------------------------------------------------------------------------
echo "== 5. file receive"
send_text file
check "the file came down to Downloads/Jarvis" "$(for _ in $(seq 20); do $ADB shell cat /sdcard/Download/Jarvis/report.txt 2>/dev/null | grep -q 'hello report' && echo yes && break; sleep 1; done)" yes
$ADB shell am start -n $ACT >/dev/null; sleep 2

# ---- 6. push in the background ----------------------------------------------------------------------------------------------
echo "== 6. push over the HTTPS long poll, app in the background"
P1=$($ADB shell pidof $PKG)
push() { python3 -I -c "
import json, sys, urllib.request
r = urllib.request.Request('http://127.0.0.1:$PORT/fake/push', json.dumps({'title': sys.argv[1], 'body': sys.argv[2], 'cid': 'c1'}).encode(), {'Content-Type': 'application/json'})
print(urllib.request.urlopen(r).read().decode())" "$1" "$2" >/dev/null; }
sleep 8 # the first poll only learns the last event id
tap_word "New" 1 2 # an empty chat: the history of the event's chat must show up after the tap
$ADB shell input keyevent KEYCODE_HOME; sleep 3
check "the app is in the background" "$($ADB shell dumpsys activity activities | grep -c "topResumedActivity=.*$PKG")" 0
push "Worker done" "Merge finished äö"
for _ in $(seq 20); do note_lines | grep -q 'Worker done' && break; sleep 1; done
check "a real notification: title | text" "$(note_lines | grep 'Worker done' | head -1)" "Worker done | Merge finished äö"
check "the process lives on (foreground service)" "$($ADB shell pidof $PKG)" "$P1"
check "the service text is neutral (no 'Relay')" "$(notes | grep -A25 "pkg=$PKG" | grep -ci 'relay')" 0
push "Second" "another line"
for _ in $(seq 20); do note_lines | grep -q 'Second' && break; sleep 1; done
check "a second event: a second notification" "$(note_lines | grep -c 'Second')" 1
echo "  tap the notification"
$ADB shell cmd statusbar expand-notifications; sleep 3
b=$(ui | grep -E 'id/(notification_)?title"' | grep 'text="Worker done' | grep -oP 'bounds="\K[^"]+' | head -1)
if [ -n "$b" ]; then xy=$(mid "$b"); $ADB shell input tap $(( ${xy% *} + 200 )) ${xy#* }; sleep 5; else note "notification not found in the shade"; fi
$ADB shell cmd statusbar collapse
check "tap: the app is in front" "$($ADB shell dumpsys activity activities | grep -c "topResumedActivity=.*$PKG")" 1
shot 09_after_tap
check "tap: the chat of the event is on screen (its history 'hi / hello')" "$(OCR_TRIES=10 wait_ocr 09_after_tap 'You: hi\|JARVIS: hello')" 1

# ---- 7. frame time -----------------------------------------------------------------------------------------------------------
echo "== 7. frame time"
# scroll the chat and open tabs so that frames are painted, then read the app's own frame lines
for _ in 1 2 3; do $ADB shell input swipe 540 900 540 1500 150; $ADB shell input swipe 540 1500 540 900 150; done
tap_word "Board" 1 3; tap_word "Chat" 1 3
sleep 7
FL=$(app_log | grep ' frames: ' | tail -20)
check "frame lines in the app log" "$(echo "$FL" | grep -c 'n=' | awk '{print ($1>0)?"yes":"no"}')" yes
echo "$FL" | tail -3 | sed 's/^/  frames /'
# the weighted figures over all lines: frames, mean paint time, slowest, frames over 16 ms
echo "$FL" | python3 -I -c "
import re, sys
n = s = m = o = 0
for ln in sys.stdin:
    r = re.search(r'n=(\d+) avg_us=(\d+) max_us=(\d+) over16ms=(\d+)', ln)
    if r:
        a, b, c, d = map(int, r.groups()); n += a; s += a * b; m = max(m, c); o += d
if n:
    print('  FRAMES %d painted, mean %.2f ms, slowest %.2f ms, over 16 ms: %d (%.1f %%)' % (n, s / n / 1000, m / 1000, o, 100.0 * o / n))
    print('  FRAMETIME_MEAN_MS=%.2f' % (s / n / 1000))"

# ---- 8. voice output without an engine -------------------------------------------------------------------------------------
echo "== 8. voice output"
tap_word "Voice" 1 2; shot 10_voice
check "Voice switch shows 'Voice on'" "$(ocr_crop 10_voice 600 330 860 450 | grep -ci 'oice on')" 1
send_text "voicetest"
check "the app lives after a spoken answer" "$(OCR_TRIES=12 wait_ocr 11_say 'Hallo voicetest')" 1
check "app alive after voice output" "$([ -n "$($ADB shell pidof $PKG)" ] && echo yes)" yes

check "no crash in logcat" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|ANR in '$PKG)" 0
$ADB shell am force-stop $PKG
echo "JARVIS E2E: $PASS passed, $FAIL failed. Pictures in $OUT"
[ $FAIL -eq 0 ]
