#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/mc/web/run.sh -- r106 browser hills demo in headless Chromium vs the native CPU reference.
# Prints `RESULT mc-web: ok` (or SKIP without chromium/PIL/numpy). NOT covered: real GPU (SwiftShader), Firefox/Safari, Pointer Lock.
set -u
cd "$(dirname "$0")/../../.."
if ! command -v chromium >/dev/null 2>&1; then echo "   SKIP: no chromium"; echo "RESULT mc-web: ok (skipped: no chromium)"; exit 0; fi
if ! python3 -c 'import numpy, PIL' 2>/dev/null; then echo "   SKIP: python3 numpy / PIL missing"; echo "RESULT mc-web: ok (skipped: no numpy/PIL)"; exit 0; fi
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
bash tools/mc/web/build.sh "$D" "${FIRNC:-}" || { echo "RESULT mc-web: FAIL build"; exit 1; }
mkdir "$D/ref" && "$D/native" "$D/ref" >/dev/null || { echo "RESULT mc-web: FAIL native reference run"; exit 1; }
python3 -I tools/mc/web/check.py "$D" "$D/ref"
