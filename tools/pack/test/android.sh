#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/pack/test/android.sh -- a project from newapp.sh becomes a signed APK with its launcher icons.
# Needs the Android SDK/NDK (as tools/android/build.sh); SKIPs (exit 0) without. Run: PACK_ANDROID=1 run.sh
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
SDK=${SDK:-/root/android-sdk}; BT=${BUILDTOOLS:-$SDK/build-tools/35.0.0}
[ -x "$BT/aapt2" ] && [ -d "${NDK_ROOT:-/root/android-ndk-min}" ] || { echo "SKIP: no Android SDK/NDK"; exit 0; }
W=$(mktemp -d "${TMPDIR:-/tmp}/pack-and.XXXXXX"); trap 'rm -rf "$W"' EXIT
PASS=0; FAIL=0
check() { if eval "$2"; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); echo "FAIL $1"; fi; }
bash "$ROOT/tools/newapp.sh" Hello hello --dir "$W/app" --store-key "$(printf 'ab%.0s' $(seq 1 32))" --version 1.2.3 \
    --android-id de.example.hello >/dev/null 2>&1
bash "$ROOT/tools/pack/all.sh" "$W/app" 1.2.3 --platforms android --out "$W/dist" > "$W/all.log" 2>&1
APK=$W/dist/hello-1.2.3-android.apk
check "the APK is built" "[ -f $APK ]"
if [ -f "$APK" ]; then
    B=$("$BT/aapt2" dump badging "$APK")
    check "package id and version code (1.2.3 -> 10203)" "echo \"\$B\" | grep -q \"package: name='de.example.hello' versionCode='10203' versionName='1.2.3'\""
    check "the launcher icon is the adaptive icon" "echo \"\$B\" | grep -q \"application-icon-.*ic_launcher.xml\""
    check "the label is the app's name" "echo \"\$B\" | grep -q \"application-label:'Hello'\""
    check "the signature verifies (v3)" "$BT/apksigner verify --verbose $APK 2>&1 | grep -q 'v3 scheme.*: true'"
    L=$(unzip -l "$APK")
    for d in mdpi hdpi xhdpi xxhdpi xxxhdpi; do check "mipmap-$d has the three icons" "echo \"\$L\" | grep -q \"mipmap-$d-v4/ic_launcher_round.png\" && echo \"\$L\" | grep -q \"mipmap-$d-v4/ic_launcher_foreground.png\""; done
    check "the native library is in (arm64-v8a and x86_64)" "echo \"\$L\" | grep -q 'lib/arm64-v8a/' && echo \"\$L\" | grep -q 'lib/x86_64/'"
    check "no .idsig is left" "[ ! -f $APK.idsig ]"
fi
echo "android.sh: $((PASS+FAIL)) checks, $FAIL failed"
[ $FAIL -eq 0 ]
