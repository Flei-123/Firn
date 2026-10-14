#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/gpu/web/run_loss.sh -- gate 141: WebGL2 context loss + restore in headless Chromium (SwiftShader). The page takes the context
# away (WEBGL_lose_context) and gives it back; the frame after the restore must be byte-equal to the frame before the loss.
# SKIP without chromium. NOT covered: a real GPU reset / driver crash, Firefox, Safari, mobile (they fire the same two events, untested).
# Usage: bash tools/gpu/web/run_loss.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
command -v chromium >/dev/null 2>&1 || { echo "   SKIP: no chromium"; exit 0; }
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
cp tools/gpu/web/loss.html "$D/index.html"; cp demos/mcweb/glweb.js "$D/"
"$FIRNC" --opt-level=release-safe --target=wasm32-browser -o "$D/loss.wasm" tools/gpu/web/web_loss_main.fi 2>"$D/err" \
    || { echo "   FAIL: web_loss_main.fi does not compile for wasm32-browser"; head -5 "$D/err"; exit 1; }
python3 -I tools/gpu/web/check_loss.py "$D"
