#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/voice_check.sh -- tools/android/voicedemo on a device/emulator: RECORD_AUDIO permission flow,
# AAudio capture -> WAV in Downloads, VIEW / SEND / URL intents, clipboard, TTS (error path when no engine).
#
#   bash tools/android/voice_check.sh [apk]     (ADB, SERIAL from env)
#
# Build the APK first:
#   bash tools/android/build.sh tools/android/voicedemo/main.fi --name "Firn Voice" \
#        --package org.firn.voicedemo --speech --tts --abi x86_64
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PKG=org.firn.voicedemo
APK=${1:-$ROOT/build/android/firn_voice/firn_voice.apk}
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  OK    $1 (got $2)"; PASS=$((PASS+1));
          else echo "  FAIL  $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
atleast() { if [ "$2" -ge "$3" ] 2>/dev/null; then echo "  OK    $1 (got $2)"; PASS=$((PASS+1));
            else echo "  FAIL  $1 (got '$2', want >= $3)"; FAIL=$((FAIL+1)); fi; }
OUT=/sdcard/Android/data/$PKG/files/stdout.txt

$ADB root >/dev/null 2>&1; sleep 1
$ADB shell am force-stop $PKG
$ADB install -r "$APK" >/dev/null || { echo "install failed"; exit 1; }
$ADB shell pm revoke $PKG android.permission.RECORD_AUDIO 2>/dev/null
$ADB shell rm -f /sdcard/Download/Voice/voice01.wav "$OUT"
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null; sleep 6
DLG=$($ADB shell dumpsys activity activities | grep -c 'GrantPermissionsActivity')
atleast "permission dialog (GrantPermissionsActivity) visible" "$DLG" 1
$ADB shell pm grant $PKG android.permission.RECORD_AUDIO
sleep 40
S=$($ADB shell cat "$OUT" 2>/dev/null | tr -d '\r')
echo "$S" | head -20
OCT=$(echo "$S" | sed -n 's/^recorded octets=\([0-9]*\).*/\1/p')
atleast "recorded octets" "${OCT:-0}" 20000
check "WAV saved to Downloads" "$(echo "$S" | grep -c '^saved=1')" 1
check "file exists" "$($ADB shell ls /sdcard/Download/Voice/voice01.wav 2>/dev/null | grep -c voice01.wav)" 1
check "VIEW intent started" "$(echo "$S" | grep -c '^view=1')" 1
check "SEND(file) chooser started" "$(echo "$S" | grep -c '^send_last=1')" 1
check "SEND(text) chooser started" "$(echo "$S" | grep -c '^send_text=1')" 1
check "open URL started" "$(echo "$S" | grep -c '^open_url=1')" 1
check "clipboard written" "$(echo "$S" | grep -c 'clip written=1 read=11')" 1
check "no crash" "$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|panic|ANR in '$PKG)" 0
echo "ANDROID VOICE: $PASS passed, $FAIL failed"
[ $FAIL -eq 0 ]
