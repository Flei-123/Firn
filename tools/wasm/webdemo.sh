#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/wasm/webdemo.sh -- THE WEB DEMO, BUILT AND PROVEN (round WASM, step 6).
#
#   1. builds demos/webdemo/gallery9.wasm out of tools/fui/gallery9_web.fi
#      (the page of tools/fui/gallery9_main.fi plus lib/plat/web.fi), in
#      two build levels, and requires the two to paint the same pixels
#   2. paints the native reference: `sh tools/fui/run.sh --images` in a
#      working directory of its own (skipped when REF names a directory
#      that already holds its belege/)
#   3. loads the demo into a headless Chromium and compares, pixel for
#      pixel, four pictures with the four PNGs of run.sh for the same page,
#      then operates it with mouse, wheel and keyboard
#      (tools/wasm/webcheck.py)
#   4. loads it again with WebGL2 (SwiftShader) and holds the pictures fUi's
#      GPU backend draws against the ones it paints in memory
#      (tools/wasm/gpucheck.py)
#
# Needs: chromium, python3 with PIL, numpy and websocket-client.
# Usage: bash tools/wasm/webdemo.sh      (exit 0 = all of it held)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC=${FIRNC:-compiler/target/release/firnc}
W=${W:-/tmp/firn-webdemo}
mkdir -p "$W"
fail=0

echo "== 1. the page as WebAssembly =="
"$FIRNC" --opt-level=release-safe --target=wasm32-browser --stats \
    -o demos/webdemo/gallery9.wasm tools/fui/gallery9_web.fi 2> "$W/build.log" || { cat "$W/build.log"; exit 1; }
grep '^wasm32:' "$W/build.log" | sed 's/^/   release-safe: /'
"$FIRNC" --opt-level=dev --target=wasm32-browser -o "$W/gallery9-dev.wasm" tools/fui/gallery9_web.fi || exit 1
wasm-validate demos/webdemo/gallery9.wasm 2>/dev/null && echo "   wasm-validate: valid" || { command -v wasm-validate >/dev/null && { echo "   wasm-validate: INVALID"; fail=1; }; }
# The font of the native reference, octet for octet (see demos/webdemo/FONT.md).
if ! cmp -s demos/webdemo/DejaVuSans.ttf /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf; then
    echo "   NOTE: /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf differs from the demo's copy;"
    echo "         the native reference is painted with the system font, so pixels may differ."
fi

echo "== 2. the native reference (tools/fui/run.sh --images) =="
REF=${REF:-}
if [ -z "$REF" ]; then
    REF="$W/fui"
    rm -rf "$REF"
    if W="$REF" sh tools/fui/run.sh --images > "$W/fui.log" 2>&1; then
        echo "   $(tail -1 "$W/fui.log")"
    else
        tail -20 "$W/fui.log" | sed 's/^/   /'
        echo "   tools/fui/run.sh FAILED -- no reference"
        exit 1
    fi
fi
ls "$REF"/belege/fui-deklarativ-*.png | sed 's/^/   /'

echo "== 3. headless Chromium: the same pixels, and the page operated =="
W="$W/shots" python3 tools/wasm/webcheck.py demos/webdemo "$REF/belege" || fail=1
# The dev build (no optimizer) has to paint the same pixels as the
# release-safe build: the optimizer does not reorder floating point
# (SPEC 13), so the picture may not depend on it.
echo "   -- the same four pictures from the build without optimizer (--opt-level=dev):"
cp "$W/gallery9-dev.wasm" demos/webdemo/.gallery9-dev.wasm
W="$W/shots-dev" WASM=.gallery9-dev.wasm PICTURES_ONLY=1 \
    python3 tools/wasm/webcheck.py demos/webdemo "$REF/belege" | grep -E 'px differ|over all' || fail=1
rm -f demos/webdemo/.gallery9-dev.wasm

echo "== 4. the same page on the GPU (lib/fui/gpu.fi, WebGL2 on SwiftShader) =="
# fUi tempo, stage 2: the four pictures once in memory (?gl=0) and once
# drawn by the GPU backend (?gl=1), within bounds (tools/wasm/gpucheck.py:
# at most 5 per mille of the pixels over 32 levels apart, a mean under 1),
# idle, a lost and restored WebGL context, a wheel notch and back.
# gpuzoo: every way fUi draws that gallery9 does not (pictures, turned
# shapes, shadows, glass, text effects) -- built only for this check
"$FIRNC" --opt-level=release-safe --target=wasm32-browser \
    -o demos/webdemo/gpuzoo.wasm tools/fui/gpuzoo_web.fi || fail=1
W="$W/gpu" python3 tools/wasm/gpucheck.py demos/webdemo > "$W/gpu.log" 2>&1 || fail=1
rm -f demos/webdemo/gpuzoo.wasm
grep -E 'over 32|lost|wheel|idle|missed|FAILED|BOUNDS' "$W/gpu.log" | sed 's/^/ /'

if [ "$fail" = 0 ]; then
    echo "WEBDEMO PASSED"
else
    echo "WEBDEMO FAILED"
fi
exit $fail
