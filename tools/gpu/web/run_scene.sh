#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/gpu/web/run_scene.sh -- gate 133: the WebGL2 scene layer (lib/gpu/glweb_scene.fi: sky, tint, entities, HUD) in headless Chromium
# against mc_raster + mc_sky + mc_hud (PSNR >= 34 dB, in-module and canvas-vs-native), cull on/off byte-equal. SKIP without chromium/PIL/numpy.
# NOT covered: real GPU, Firefox, Safari, mobile, timer queries (not implemented). Usage: bash tools/gpu/web/run_scene.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
command -v chromium >/dev/null 2>&1 || { echo "   SKIP: no chromium"; exit 0; }
python3 -c 'import numpy, PIL' 2>/dev/null || { echo "   SKIP: python3 numpy / PIL missing"; exit 0; }
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
cp demos/mcweb/index.html demos/mcweb/glweb.js "$D/"
"$FIRNC" --opt-level=release-safe --target=wasm32-browser -o "$D/mcscene2.wasm" tools/gpu/web/web_scene_main.fi 2>"$D/err" \
    || { echo "   FAIL: web_scene_main.fi does not compile for wasm32-browser"; head -5 "$D/err"; exit 1; }
"$FIRNC" -o "$D/native" tools/gpu/web/native_scene_main.fi 2>"$D/err" || { echo "   FAIL: native_scene_main.fi does not compile"; head -5 "$D/err"; exit 1; }
mkdir "$D/ref" && "$D/native" "$D/ref" >/dev/null || { echo "   FAIL: native reference run"; exit 1; }
GATE_WASM=mcscene2.wasm GATE_CLEAR=102030 GATE_MIN_SCENES=8 GATE_LABEL=webgl2-scene GATE_CULL=1 python3 -I tools/gpu/web/check.py "$D" "$D/ref"
