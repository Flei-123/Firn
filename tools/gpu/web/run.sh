#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/gpu/web/run.sh -- gate 131: the WebGL2 back end (lib/gpu/glweb.fi) in headless Chromium (docs/WEBGL2.md).
#   1. builds tools/gpu/web/web_main.fi for wasm32-browser (mc_raster + gpu.glweb in one module) and
#      native_main.fi (mc_raster only), the native one writes the reference frames
#   2. tools/gpu/web/check.py: loads demos/mcweb into Chromium (WebGL2 = SwiftShader here), checks the in-module compare
#      (WebGL2 vs mc_raster, thresholds of tools/gpu/dump.fi) and the canvas snapshots vs the native frames (PSNR >= 34 dB)
# SKIP (exit 0) without chromium / python3 PIL+numpy. NOT covered: a real GPU, Firefox, Safari, mobile browsers.
# Usage: bash tools/gpu/web/run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
command -v chromium >/dev/null 2>&1 || { echo "   SKIP: no chromium"; exit 0; }
python3 -c 'import numpy, PIL' 2>/dev/null || { echo "   SKIP: python3 numpy / PIL missing"; exit 0; }
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
cp demos/mcweb/index.html demos/mcweb/glweb.js "$D/"
"$FIRNC" --opt-level=release-safe --target=wasm32-browser -o "$D/mcscene.wasm" tools/gpu/web/web_main.fi 2>"$D/err" \
    || { echo "   FAIL: web_main.fi does not compile for wasm32-browser"; head -5 "$D/err"; exit 1; }
"$FIRNC" -o "$D/native" tools/gpu/web/native_main.fi 2>"$D/err" || { echo "   FAIL: native_main.fi does not compile"; head -5 "$D/err"; exit 1; }
mkdir "$D/ref" && "$D/native" "$D/ref" >/dev/null || { echo "   FAIL: native reference run"; exit 1; }
python3 -I tools/gpu/web/check.py "$D" "$D/ref"
