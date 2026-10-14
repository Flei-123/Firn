#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/mc/web/build.sh -- builds the browser hills demo (r106) into <outdir>: hills.wasm (WebGL2), hills.html + glweb.js and
# <outdir>/native (CPU reference, tools/gpu/web/hills_native.fi). Serve <outdir> over http and open hills.html.
# Usage: bash tools/mc/web/build.sh <outdir> [firnc]      FIRNC must be a build that knows --target=wasm32-browser (compiler of /root/firn).
set -u
cd "$(dirname "$0")/../../.."
OUT=${1:?usage: build.sh <outdir> [firnc]}
FIRNC=${2:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
mkdir -p "$OUT"
cp demos/mcweb/hills.html demos/mcweb/glweb.js "$OUT/"
E=$(mktemp)
trap 'rm -f "$E"' EXIT
# older firnc builds (e.g. /root/firn-w-r5) cannot emit wasm32-browser (".asciz"): retry with the compiler of the main checkout
"$FIRNC" --opt-level=release-safe --target=wasm32-browser -o "$OUT/hills.wasm" tools/gpu/web/hills_web.fi 2>"$E" \
    || "${FIRNC_WASM:-/root/firn/compiler/target/release/firnc}" --opt-level=release-safe --target=wasm32-browser -o "$OUT/hills.wasm" tools/gpu/web/hills_web.fi 2>"$E" \
    || { echo "   FAIL: hills_web.fi does not compile for wasm32-browser"; head -5 "$E"; exit 1; }
"$FIRNC" -o "$OUT/native" tools/gpu/web/hills_native.fi 2>"$E" \
    || { echo "   FAIL: hills_native.fi does not compile"; head -5 "$E"; exit 1; }
echo "   built $OUT/hills.wasm ($(stat -c %s "$OUT/hills.wasm") bytes), $OUT/native"
