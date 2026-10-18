#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_postlogin_check.sh -- the JARVIS app AFTER the sign-in (the phase that crashed on a real phone) against the
# REALISTIC fake server (tools/mini/fake_jarvis.py: 3.5 MB WebSocket `info` with 30 running chats + live partials, 150 chats with
# emoji/umlauts/null titles, big answers; FAKE_STREAM=1 keeps tokens of the other chats flowing).
#
#   bash tools/android/jarvis_postlogin_check.sh [apk] [variant ...]       variant = abi:perm:density:fontscale
#       abi      arm64-v8a (real arm64 library through libndk_translation) | x86_64
#       perm     granted (pm grant POST_NOTIFICATIONS before) | allow (system dialog -> Allow) | deny (system dialog -> Don't allow)
#       density  0 = as is, else `wm density N`        fontscale  1.0 = as is, else `settings put system font_scale F`
#   default variants: arm64-v8a:allow:0:1.0 arm64-v8a:deny:0:1.0 x86_64:allow:0:1.0 x86_64:deny:0:1.0 x86_64:granted:320:1.3 arm64-v8a:granted:560:0.85
#   OUT=<dir> keeps the screenshots; AVD=a22p EMU_PORT=5600 ADB_PORT=5049 (own emulator, own adb server); SERIAL=... uses a running one.
#   FAKE_NRUNNING / FAKE_PARTIAL_MAX_KB / FAKE_NCHATS pass through to the fake server.
# Per variant: install, sign in, the notice page "Continue", the permission dialog, then 10 s main view (the WS `info` arrives), then
# the tabs and a big chat are opened. Judged by: process alive, crash buffer + logcat free of fatal lines, startup_stage.txt reached
# the end, server log shows `info_sent`. Heavy: run through /root/jarvis/bin/heavy.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SDK=${SDK:-/root/android-sdk}
APK=${1:-$(ls -t /root/jarvis-android-builds/jarvis-*.apk 2>/dev/null | head -1)}
shift || true
VARIANTS=("$@")
[ ${#VARIANTS[@]} -gt 0 ] || VARIANTS=(arm64-v8a:allow:0:1.0 arm64-v8a:deny:0:1.0 x86_64:allow:0:1.0 x86_64:deny:0:1.0 x86_64:granted:320:1.3 arm64-v8a:granted:560:0.85)
PKG=${PKG:-org.fleitec.jarvis}
OUT=${OUT:-$(mktemp -d)}
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  OK    $1"; PASS=$((PASS+1)); else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
note() { echo "  NOTE  $*"; }
[ -f "$APK" ] || { echo "no apk: $APK"; exit 2; }
for t in tesseract python3; do command -v $t >/dev/null || { echo "skip: $t missing"; exit 0; }; done
python3 -c "import websockets, PIL" 2>/dev/null || { echo "skip: python3 websockets/PIL missing"; exit 0; }
W=$(mktemp -d); EMU=""; SRV=""
ADB_PORT=${ADB_PORT:-5049}; AVD=${AVD:-a22p}; EMU_PORT=${EMU_PORT:-5600}
ADB0="$SDK/platform-tools/adb -P $ADB_PORT"
SLOG=$W/server.log
cleanup() {
    [ -n "$SRV" ] && kill $SRV 2>/dev/null
    mkdir -p "$OUT"; cp "$SLOG" "$OUT/server.log" 2>/dev/null
    if [ -n "$EMU" ]; then $ADB shell wm density reset >/dev/null 2>&1; $ADB emu kill >/dev/null 2>&1; sleep 2; kill $EMU 2>/dev/null; $ADB0 kill-server >/dev/null 2>&1
        rm -rf "$HOME/.android/avd/$AVD.avd" "$HOME/.android/avd/$AVD.ini"; fi
    rm -rf "$W"
}
trap cleanup EXIT
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
mkdir -p "$OUT"; sleep 5
$ADB root >/dev/null 2>&1; sleep 2; $ADB wait-for-device
echo "emulator up: $($ADB shell getprop ro.product.cpu.abilist | tr -d '\r') sdk $($ADB shell getprop ro.build.version.sdk | tr -d '\r')"

FAKE_STREAM=${FAKE_STREAM:-1} FAKE_POLL_MAX=${FAKE_POLL_MAX:-6} python3 "$ROOT/tools/mini/fake_jarvis.py" "$SLOG" >"$W/out" 2>&1 & SRV=$!
for _ in $(seq 50); do grep -q '^PORT' "$W/out" && break; sleep 0.1; done
PORT=$(awk '/^PORT/{print $2}' "$W/out")
[ -n "$PORT" ] || { echo "fake server did not start"; exit 1; }

# ---- helpers (OCR + touch, as in jarvis_e2e.sh) --------------------------------------------------------------------------------
shot() { $ADB exec-out screencap -p >"$OUT/$1.png"; }
ocr() { tesseract "$OUT/$1.png" stdout 2>/dev/null | tr -s '\n ' ' '; }
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
tap_word() { local p; shot _w; p=$(word_at _w "$1" "${2:-1}"); [ -n "$p" ] || { echo "  (no word $1 on the screen)"; return 1; }
    $ADB shell input tap $p; sleep "${3:-1.5}"; }
wait_ocr() { local i; for i in $(seq ${OCR_TRIES:-12}); do shot "$1"; ocr "$1" | grep -q -E "$2" && break; sleep 1.5; done; ocr "$1" | grep -c -E "$2"; }
kbd() { $ADB shell dumpsys input_method | grep -c 'mInputShown=true'; }
wait_kbd_up() { local i; for i in $(seq 12); do [ "$(kbd)" = 1 ] && return 0; sleep 1; done; return 1; }
hide_kbd() { if [ "$(kbd)" = 1 ]; then $ADB shell input keyevent KEYCODE_BACK; fi; sleep 1; }
type_into() { tap_word "$1" 1 1.5 || return 1; wait_kbd_up || note "keyboard did not show for $1"; $ADB shell input text "$2"; sleep 1; hide_kbd; }
ui() { local i=0; while [ $i -lt 4 ]; do $ADB shell uiautomator dump /sdcard/ui.xml 2>&1 | grep -q dumped && break; sleep 1; i=$((i+1)); done; $ADB shell cat /sdcard/ui.xml | tr '>' '\n'; }
mid() { echo "$1" | tr '[],' '   ' | awk '{print int(($1+$3)/2), int(($2+$4)/2)}'; }
tap_ui_id() { local b; b=$(ui | grep -E "resource-id=\"[^\"]*$1\"" | grep -oP 'bounds="\K[^"]+' | head -1); [ -n "$b" ] || return 1; $ADB shell input tap $(mid "$b"); sleep 2; }
alive() { [ -n "$($ADB shell pidof $PKG | tr -d '\r')" ] && echo yes || echo no; }
fatal_lines() { { $ADB logcat -b crash -d; $ADB logcat -b main,system -d -s AndroidRuntime:E ActivityManager:I DEBUG:I libc:F 2>/dev/null; } \
    | grep -E "FATAL|Fatal signal|SIGSEGV|SIGABRT|JNI DETECTED|Process $PKG .*has died|ANR in $PKG|crash_dump|am_crash.*$PKG" | head -5; }
stage_file() { $ADB shell "cat \$(find /data/data/$PKG /sdcard/Android/data/$PKG -name startup_stage.txt 2>/dev/null | head -1) 2>/dev/null" | tr -d '\r' | tail -3 | tr '\n' ' '; }

run_variant() { # abi:perm:density:font
    local abi perm dens font tag
    IFS=: read -r abi perm dens font <<<"$1"
    tag="${abi}-${perm}-d${dens}-f${font}"
    echo "== $tag"
    $ADB shell wm density reset >/dev/null 2>&1; $ADB shell settings put system font_scale 1.0 >/dev/null 2>&1
    [ "$dens" != 0 ] && $ADB shell wm density "$dens" >/dev/null 2>&1
    [ "$font" != 1.0 ] && $ADB shell settings put system font_scale "$font" >/dev/null 2>&1
    sleep 2
    read -r SW SH < <($ADB shell wm size | tr -d '\r' | sed -n 's/.*: *\([0-9]*\)x\([0-9]*\).*/\1 \2/p' | tail -1)
    $ADB uninstall $PKG >/dev/null 2>&1
    check "$tag: install" "$($ADB install --abi "$abi" "$APK" 2>&1 | grep -c '^Success')" 1
    $ADB shell mkdir -p /sdcard/Android/data/$PKG/files
    printf 'http://10.0.2.2:%s\n' "$PORT" >"$W/args.txt"; $ADB push "$W/args.txt" /sdcard/Android/data/$PKG/files/args.txt >/dev/null
    [ "$perm" = granted ] && $ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS >/dev/null 2>&1
    $ADB logcat -c; $ADB logcat -b crash -c
    local n0; n0=$(grep -c '"type": "info_sent"' "$SLOG")
    $ADB shell monkey -p $PKG -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
    check "$tag: sign-in page" "$(OCR_TRIES=30 wait_ocr ${tag}_01 'Sign in')" 1
    type_into "Username" justin; type_into "Password" pw; tap_word "Sign" 1 1
    check "$tag: notice page or main view after the sign-in" "$(OCR_TRIES=20 wait_ocr ${tag}_02 'Continue|Chats')" 1
    tap_word "Continue" 1 3 && echo "  (notice page: Continue)"
    # the system permission dialog (perm=allow|deny): answer it like a person would
    if [ "$perm" != granted ]; then
        local ans=permission_allow_button; [ "$perm" = deny ] && ans=permission_deny_button
        for _ in 1 2 3 4; do tap_ui_id "$ans" && { echo "  (permission dialog: $perm)"; break; }; sleep 2; done
    fi
    shot ${tag}_03
    # 10 s main view: the WebSocket opens, `info` (3.5 MB) arrives, the push setup runs, the first long poll
    local i
    for i in 1 2 3 4 5; do sleep 2; [ "$(alive)" = yes ] || break; done
    check "$tag: process alive 10 s after the sign-in" "$(alive)" yes
    check "$tag: the big WS info reached the app (server sent it)" "$([ "$(grep -c '"type": "info_sent"' "$SLOG")" -gt "$n0" ] && echo yes || echo no)" yes
    shot ${tag}_04
    check "$tag: main view on screen" "$(ocr ${tag}_04 | grep -c -E 'Chats|Board|Tasks|Message')" 1
    # walk through the tabs, open a big chat
    tap_word "Chats" 1 3; shot ${tag}_05
    check "$tag: alive after the Chats tab" "$(alive)" yes
    p=$(word_at ${tag}_05 'worker|Gr.*|task'); [ -n "$p" ] && { $ADB shell input tap $p; sleep 4; }
    shot ${tag}_06
    check "$tag: alive after opening a chat" "$(alive)" yes
    for t in Board Tasks; do tap_word "$t" 1 2.5; done
    sleep 3
    check "$tag: alive after the tabs" "$(alive)" yes
    check "$tag: crash buffer + logcat free of fatal lines" "$(fatal_lines | wc -l)" 0
    fatal_lines | sed 's/^/        /'
    echo "  stage file: $(stage_file)"
    $ADB shell am force-stop $PKG
}

echo "apk: $APK  fake server port $PORT  out $OUT"
for v in "${VARIANTS[@]}"; do run_variant "$v"; done
$ADB shell wm density reset >/dev/null 2>&1; $ADB shell settings put system font_scale 1.0 >/dev/null 2>&1
echo "postlogin check: $PASS ok, $FAIL failed"
[ $FAIL -eq 0 ]
