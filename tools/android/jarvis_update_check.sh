#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_update_check.sh -- the JARVIS app's update line (channel, versions, release notes, channel switch)
# on an emulator, against a LOCAL throw-away store. Nothing here touches the live store or its key.
#
#   bash tools/android/jarvis_update_check.sh        (own AVD "a8u": console port 5596, own adb server on port 5045)
#   SERIAL=emulator-5596 ADB_PORT=5045 OUT=dir ...   (use a running emulator, keep pictures in dir)
#
# What runs: `store init` makes a throw-away store with its own key; tools/android/jarvis_release.sh stages APKs into it
# (signed entry, channel "test", release notes out of a notes file); tools/appkit/teststore_server.py serves it on the host
# (the emulator sees it as 10.0.2.2); the app is built twice for that store (FIRN_APP_STORE / FIRN_APP_STORE_KEY, x86_64 only):
# 0.2.2 is installed, 0.2.3 is what the store offers. The screen is read with tesseract; the store server's request log is the
# proof for what the app asked.
# Checks: (1) store with the channel "test" only: Settings shows "Channel: test", the installed version, "update 0.2.3 available",
# the notes of 0.2.3, and NO channel switch; (2) the store also gets 0.2.2 in channel "stabil": a check shows the switch; it
# changes to stable ("up to date", notes of 0.2.2) and back to test; the choice survives an app restart.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SDK=${SDK:-/root/android-sdk}
PKG=org.fleitec.jarvis
ACT=$PKG/android.app.NativeActivity
OUT=${OUT:-$(mktemp -d)}
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  OK    $1"; PASS=$((PASS+1)); else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
for t in tesseract python3; do command -v $t >/dev/null || { echo "skip: $t missing"; exit 0; }; done
python3 -c "import websockets, PIL, cryptography" 2>/dev/null || { echo "skip: python3 websockets/PIL/cryptography missing"; exit 0; }
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNC
TOOL=${ORIENTSTORE_TOOL:-/root/orientstore/werkzeug/store}
[ -f "$TOOL" ] || { echo "skip: no store tool $TOOL"; exit 0; }
W=$(mktemp -d)
EMU=""; SRV=""; STORESRV=""
ADB_PORT=${ADB_PORT:-5045}
ADB0="$SDK/platform-tools/adb -P $ADB_PORT"
cleanup() {
    [ -n "$SRV" ] && kill $SRV 2>/dev/null
    [ -n "$STORESRV" ] && kill $STORESRV 2>/dev/null
    cp "$W/store.log" "$OUT/store-requests.log" 2>/dev/null
    [ -n "$EMU" ] && { $ADB emu kill >/dev/null 2>&1; sleep 2; kill $EMU 2>/dev/null; $ADB0 kill-server >/dev/null 2>&1; }
    rm -rf "$W"
}
trap cleanup EXIT
mkdir -p "$OUT"

# ---- 1. the throw-away store ---------------------------------------------------------------------------------------------------
echo "== 1. throw-away store + builds"
REPO=$W/repo
mkdir -p "$REPO"
# english: ok -- the store tool's own German interface names (env var + key file)
export ORIENTSTORE_REPO=$REPO "ORIENTSTORE_SCHLUESSEL=$REPO/schluessel.geheim"
python3 "$TOOL" init --name "JARVIS update check" --adresse "http://10.0.2.2/" >/dev/null || { echo "store init failed"; exit 1; }
KEY=$(python3 "$TOOL" key)
python3 -I "$ROOT/tools/appkit/teststore_server.py" --root "$REPO" --log "$W/store.log" >"$W/store.out" 2>&1 & STORESRV=$!
for _ in $(seq 50); do grep -q '^PORT' "$W/store.out" && break; sleep 0.1; done
SPORT=$(awk '/^PORT/{print $2}' "$W/store.out")
[ -n "$SPORT" ] || { echo "no store server"; exit 1; }
export FIRN_APP_STORE=http://10.0.2.2:$SPORT/ FIRN_APP_STORE_KEY=$KEY
for v in 0.2.2 0.2.3; do
    bash "$ROOT/tools/android/jarvis_build.sh" --version $v --abi x86_64 --out "$W/apk" >"$W/build-$v.log" 2>&1 \
        || { echo "build $v failed"; tail -15 "$W/build-$v.log"; exit 1; }
done
unset FIRN_APP_STORE FIRN_APP_STORE_KEY
cat >"$W/notes.md" <<'NOTES'
## 0.2.3
- Faster chat list scrolling.
- Fixed the upload dialog.

## 0.2.2
- First stable build.
NOTES
REL="bash $ROOT/tools/android/jarvis_release.sh --staging $REPO --notes $W/notes.md"
$REL --version 0.2.3 --apk "$W/apk/jarvis-0.2.3.apk" --channel test >"$W/rel1.log" 2>&1 || { cat "$W/rel1.log"; exit 1; }
check "release script: 0.2.3 staged in channel test" "$(grep -c "to channel 'test'" "$W/rel1.log")" 1
check "release script: notes stored in the signed catalog" "$(python3 -I -c "
import json
d = json.load(open('$REPO/index.json'))['pakete']['org.fleitec.jarvis']
print('Faster chat list' in d['builds'][0]['aenderungen'] and list(d['kanaele']) == ['test'])")" True
check "release script: verify has no real error" "$(grep -c 'verify found' "$W/rel1.log")" 0

# ---- 2. the emulator, the fake server -------------------------------------------------------------------------------------------
echo "== 2. emulator + install"
if [ -z "${SERIAL:-}" ]; then
    AVDHOME=${ANDROID_AVD_HOME:-$HOME/.android/avd}
    if [ ! -d "$AVDHOME/a8u.avd" ]; then
        mkdir -p "$AVDHOME/a8u.avd"; sed 's/firn/a8u/g' "$AVDHOME/firn.avd/config.ini" >"$AVDHOME/a8u.avd/config.ini"
        printf 'avd.ini.encoding=UTF-8\npath=%s/a8u.avd\npath.rel=avd/a8u.avd\ntarget=android-35\n' "$AVDHOME" >"$AVDHOME/a8u.ini"
    fi
    $ADB0 start-server >/dev/null 2>&1
    ANDROID_ADB_SERVER_PORT=$ADB_PORT ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=$AVDHOME "$SDK/emulator/emulator" -avd a8u -port 5596 \
        -no-window -no-audio -no-boot-anim -no-snapshot -wipe-data -gpu swiftshader_indirect >"$W/emu.log" 2>&1 & EMU=$!
    SERIAL=emulator-5596
fi
ADB="$ADB0 -s $SERIAL"
python3 -I "$ROOT/tools/mini/fake_jarvis.py" "$W/server.log" >"$W/out" 2>&1 & SRV=$!
for _ in $(seq 50); do grep -q '^PORT' "$W/out" && break; sleep 0.1; done
PORT=$(awk '/^PORT/{print $2}' "$W/out")
for _ in $(seq 150); do [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break; sleep 3; done
[ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] || { echo "the emulator did not boot"; exit 1; }

read -r SW SH < <($ADB shell wm size | tr -d '\r' | sed -n 's/.*: *\([0-9]*\)x\([0-9]*\).*/\1 \2/p' | tail -1)
shot() { $ADB exec-out screencap -p >"$OUT/$1.png"; }
ocr() { tesseract "$OUT/$1.png" stdout 2>/dev/null | tr -s '\n ' ' '; }
word_at() { python3 -I - "$OUT/$1.png" "$2" "${3:-1}" <<'PY'
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
PY
}
# the channel switch: a focused button defeats OCR of its own text, so it is found by position: the button row sits a fixed
# distance above "Notifications" (an anchor that is never focused), the switch is the right-hand button (x about 0.66 of the width)
tap_switch() { local p x y; shot _w; p=$(word_at _w "Notifications"); [ -n "$p" ] || { echo "  (no anchor on the screen)"; return 1; }
    x=$(( SW * 66 / 100 )); y=$(( ${p#* } - 73 * SH / 2340 )); $ADB shell input tap $x $y; sleep "${1:-2}"; }
tap_word() { local p; shot _w; p=$(word_at _w "$1" "${2:-1}"); [ -n "$p" ] || { echo "  (no word $1 on the screen)"; return 1; }
    $ADB shell input tap $p; sleep "${3:-1.5}"; }
wait_ocr() { local i; for i in $(seq ${OCR_TRIES:-12}); do shot "$1"; ocr "$1" | grep -qE "$2" && break; sleep 1.5; done; ocr "$1" | grep -cE "$2"; }
hide_kbd() { $ADB shell input keyevent KEYCODE_BACK; sleep 1; }
type_into() { tap_word "$1" 1 1.5 || return 1; $ADB shell input text "$2"; sleep 1; hide_kbd; }
store_gets() { grep -c "$1" "$W/store.log" 2>/dev/null; }

$ADB root >/dev/null 2>&1; sleep 1
$ADB uninstall $PKG >/dev/null 2>&1
$ADB install -r "$W/apk/jarvis-0.2.2.apk" >/dev/null 2>&1
check "install 0.2.2" "$($ADB shell pm path $PKG | head -c 8)" "package:"
$ADB shell mkdir -p /sdcard/Android/data/$PKG/files
printf 'http://10.0.2.2:%s\n' "$PORT" >"$W/args.txt"
$ADB push "$W/args.txt" /sdcard/Android/data/$PKG/files/args.txt >/dev/null
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS >/dev/null 2>&1
$ADB logcat -c
sleep 15
$ADB shell am start -n $ACT >/dev/null
for _ in $(seq 40); do [ -n "$($ADB shell pidof $PKG)" ] && break; sleep 1; done
check "login screen shows" "$(OCR_TRIES=40 wait_ocr 01_login 'Sign in')" 1
type_into "User" justin; type_into "Password" pw
tap_word "Sign" 1 1
check "chat screen after login" "$(OCR_TRIES=30 wait_ocr 02_chat 'Chats')" 1

# ---- 3. store knows the channel test only ------------------------------------------------------------------------------------------
echo "== 3. channel test only"
tap_word "Prefs" 1 2
check "Settings shows the channel and the installed version" "$(OCR_TRIES=15 wait_ocr 03_prefs 'Channel: test.*installed 0\.2\.2')" 1
check "available version shown" "$(OCR_TRIES=25 wait_ocr 03_prefs 'update 0\.2\.3 available')" 1
check "release notes of the offered build shown" "$(wait_ocr 03_prefs 'Faster chat')" 1
check "the app asked the store (entry.json + index.json)" "$([ "$(store_gets 'GET /entry.json')" -ge 1 ] && [ "$(store_gets 'GET /index.json')" -ge 1 ] && echo yes)" yes
check "no channel switch while the store knows one channel" "$(ocr 03_prefs | grep -ciE 'Channel: stable')" 0

# ---- 4. the store also has stabil ----------------------------------------------------------------------------------------------------
echo "== 4. channels stabil + test"
$REL --version 0.2.2 --apk "$W/apk/jarvis-0.2.2.apk" --channel stabil >"$W/rel2.log" 2>&1 || { cat "$W/rel2.log"; exit 1; }
tap_word "Check" 1 2
check "switch appears (offers stable)" "$(OCR_TRIES=25 wait_ocr 04_two 'Channel: stable')" 1
tap_switch 2
check "after the switch: channel stable" "$(OCR_TRIES=15 wait_ocr 05_stable 'Channel: stable.*installed 0\.2\.2')" 1
check "stable channel: up to date" "$(OCR_TRIES=25 wait_ocr 05_stable 'up to date')" 1
check "stable channel: notes of 0.2.2" "$(wait_ocr 05_stable 'First stable')" 1
tap_switch 2
check "back on test: update offered again" "$(OCR_TRIES=25 wait_ocr 06_back 'update 0\.2\.3 available')" 1
tap_switch 2
check "stable again" "$(OCR_TRIES=15 wait_ocr 07_again 'up to date')" 1
$ADB shell am force-stop $PKG
sleep 2
$ADB shell am start -n $ACT >/dev/null
check "login screen after the restart" "$(OCR_TRIES=30 wait_ocr 08_login 'Sign in')" 1
type_into "User" justin; type_into "Password" pw
tap_word "Sign" 1 1
check "chat screen after the second login" "$(OCR_TRIES=30 wait_ocr 08_chat 'Chats')" 1
tap_word "Prefs" 1 2
check "the channel choice survives a restart" "$(OCR_TRIES=15 wait_ocr 08_restart 'Channel: stable.*installed 0\.2\.2')" 1
check "no crash in logcat" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|ANR in '$PKG)" 0
$ADB shell am force-stop $PKG
echo "JARVIS update check: $PASS passed, $FAIL failed. Pictures in $OUT"
[ $FAIL -eq 0 ]
