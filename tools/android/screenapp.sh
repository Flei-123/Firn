#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/screenapp.sh -- A fUi SCREEN APP (examples/fui_apps/<app>) AS AN APK.
#
#   bash tools/android/screenapp.sh <app> [build.sh options]
#
# Inter (lib/fui/fonts) and the app assets (examples/fui_apps/data) travel in the APK's assets/;
# plat.uifont and plat.assetio read them through the AssetManager. Default output:
# /root/jarvis-android-builds/fui-screen/<app>-<abi>.apk, ABI x86_64 (emulator); `--abi arm64` for a phone.
set -euo pipefail
cd "$(dirname "$0")/../.."
APP=${1:?app}; shift
ABI=x86_64
ARGS=()
while [ $# -gt 0 ]; do
    case "$1" in
        --abi) ABI=$2; shift 2 ;;
        *) ARGS+=("$1"); shift ;;
    esac
done
STAGE=$(mktemp -d "${TMPDIR:-/tmp}/screenapp.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT
cp lib/fui/fonts/Inter-*.ttf examples/fui_apps/data/* "$STAGE/"
OUTDIR=${OUTDIR:-/root/jarvis-android-builds/fui-screen}
mkdir -p "$OUTDIR"
bash tools/android/build.sh "examples/fui_apps/$APP/main.fi" --name "fUi $APP" --package "com.fleitec.fui.$APP" \
    --abi "$ABI" --assets "$STAGE" --out "$OUTDIR/$APP-$ABI.apk" ${ARGS[@]+"${ARGS[@]}"}
