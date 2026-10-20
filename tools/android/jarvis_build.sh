#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_build.sh -- build the JARVIS Android app (examples/jarvis_android) as an APK (arm64 + x86_64).
#
#   bash tools/android/jarvis_build.sh [--version X.Y.Z] [--out DIR] [--abi arm64|x86_64|both] [--plain-activity] [--variant bN]
#
#   --plain-activity   launcher = plain android.app.NativeActivity (no org.firn.FirnMain / alias)
#   --variant bN       PARALLEL bisect build next to the real app: package org.fleitec.jarvis.bN, label "JARVIS bN", own state
#                      directory, update check pointed at nothing; file <out>/jarvis-bN.apk (tools/android/jarvis_bisect.sh)
#
# Output: /root/jarvis-android-builds/jarvis-<version>.apk (default). NEVER published: the Orient store (test channel) is
# fed by hand by the boss (staged locally by tools/android/jarvis_release.sh). App id de.fleitec.jarvis, package org.fleitec.jarvis, update channel "test".
# Launcher icon: examples/jarvis_android/res (adaptive + monochrome + legacy PNGs; regenerate with tools/android/gen_jarvis_icon.py);
# --variant builds use the same icon.
# Heavy: run through `/root/jarvis/bin/heavy bash tools/android/jarvis_build.sh`.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
VERSION=0.1.0
OUT=/root/jarvis-android-builds
ABI=both
PLAIN=""
VARIANT=""
while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION=$2; shift 2 ;;
        --out) OUT=$2; shift 2 ;;
        --abi) ABI=$2; shift 2 ;;
        --plain-activity) PLAIN="--plain-activity"; shift ;;
        --variant) VARIANT=$2; shift 2 ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
done
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNC FIRNLIB=$ROOT/lib
export SOFT_INPUT=adjustResize # composer above the keyboard (build.sh writes android:windowSoftInputMode)
export FIRN_APP_VERSION=$VERSION FIRN_APP_CHANNEL=${CHANNEL:-test}
IFS=. read -r MA MI PA _ <<<"${VERSION%%-*}"
VCODE=$(( ${MA:-0} * 10000 + ${MI:-0} * 100 + ${PA:-0} ))
mkdir -p "$OUT"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
APK="$OUT/jarvis-$VERSION.apk"
PKGNAME=org.fleitec.jarvis
LABEL=JARVIS
if [ -n "$VARIANT" ]; then
    case "$VARIANT" in [a-z][a-z0-9]*) ;; *) echo "bad --variant $VARIANT" >&2; exit 2 ;; esac
    PKGNAME=org.fleitec.jarvis.$VARIANT
    LABEL="JARVIS $VARIANT"
    APK="$OUT/jarvis-$VARIANT.apk"
    export FIRN_APP_NAME="$LABEL" FIRN_APP_ID=de.fleitec.jarvis.$VARIANT FIRN_APP_ANDROID_ID=$PKGNAME
    export FIRN_APP_STORE=https://store.invalid/ FIRN_APP_CHANNEL=bisect
fi
bash "$ROOT/tools/android/build.sh" "$ROOT/examples/jarvis_android/main.fi" --name "$LABEL" --package "$PKGNAME" $PLAIN \
    --version-code "$VCODE" --version-name "$VERSION" \
    --permission android.permission.INTERNET \
    --permission android.permission.ACCESS_NETWORK_STATE \
    --permission android.permission.REQUEST_INSTALL_PACKAGES \
    --permission android.permission.UPDATE_PACKAGES_WITHOUT_USER_ACTION \
    --abi "$ABI" --install --push --pick --speech --tts \
    --icon-res "$ROOT/examples/jarvis_android/res" --out "$APK"
echo "built $APK (versionCode $VCODE, channel $FIRN_APP_CHANNEL)"
