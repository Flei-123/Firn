#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/jarvis_build.sh -- build the JARVIS Android app (examples/jarvis_android) as an APK (arm64 + x86_64).
#
#   bash tools/android/jarvis_build.sh [--version X.Y.Z] [--out DIR]
#
# Output: /root/jarvis-android-builds/jarvis-<version>.apk (default). NEVER published: the Orient store (test channel) is
# fed by hand by the boss. App id de.fleitec.jarvis, package org.fleitec.jarvis, update channel "test".
# Heavy: run through `/root/jarvis/bin/heavy bash tools/android/jarvis_build.sh`.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
VERSION=0.1.0
OUT=/root/jarvis-android-builds
while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION=$2; shift 2 ;;
        --out) OUT=$2; shift 2 ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
done
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNC FIRNLIB=$ROOT/lib
export FIRN_APP_VERSION=$VERSION FIRN_APP_CHANNEL=${CHANNEL:-test}
IFS=. read -r MA MI PA _ <<<"${VERSION%%-*}"
VCODE=$(( ${MA:-0} * 10000 + ${MI:-0} * 100 + ${PA:-0} ))
mkdir -p "$OUT"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
APK="$OUT/jarvis-$VERSION.apk"
bash "$ROOT/tools/android/build.sh" "$ROOT/examples/jarvis_android/main.fi" --name "JARVIS" --package org.fleitec.jarvis \
    --version-code "$VCODE" --version-name "$VERSION" \
    --permission android.permission.INTERNET \
    --permission android.permission.REQUEST_INSTALL_PACKAGES \
    --permission android.permission.UPDATE_PACKAGES_WITHOUT_USER_ACTION \
    --install --push --pick --speech --tts --out "$APK"
echo "built $APK (versionCode $VCODE, channel $FIRN_APP_CHANNEL)"
