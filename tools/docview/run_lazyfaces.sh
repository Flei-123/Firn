#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
rc=0
python3 tools/docview/gen_facepack.py lib/docview/pdf/pdffonts_data.fi "$W/pack" > /dev/null
cmp -s "$W/pack" demos/webdemo/pdffonts.pack || { echo "  FAIL demos/webdemo/pdffonts.pack is stale (run tools/docview/gen_facepack.py)"; rc=1; }
python3 tools/docview/mk_face_fixtures.py "$W" || exit 1
"$FIRNC" -o "$W/native" tools/libmvp/webfaces_native.fi > "$W/native.log" 2>&1 || { echo "  FAIL native build"; grep -v RWX "$W/native.log" | head -5; exit 1; }
PDF=$("$W/native" "$W/a.pdf") || { echo "  FAIL native pdf"; exit 1; }
DOCX=$("$W/native" "$W/a.docx") || { echo "  FAIL native docx"; exit 1; }
"$FIRNC" --target=wasm32-browser --opt-level=release-fast -o "$W/lazy.wasm" tools/libmvp/webfaces_probe.fi > "$W/lazy.log" 2>&1 || { echo "  FAIL lazy build"; grep -v RWX "$W/lazy.log" | head -5; exit 1; }
bash tools/docview/embedded_toolchain.sh "$W/emb"
FIRNLIB="$W/emb/lib" "$W/emb/compiler/target/release/firnc" --target=wasm32-browser --opt-level=release-fast -o "$W/embedded.wasm" tools/libmvp/webfaces_probe.fi > "$W/emb.log" 2>&1 || { echo "  FAIL embedded build"; grep -v RWX "$W/emb.log" | head -5; exit 1; }
echo "wasm release-fast: embedded fonts $(stat -c %s "$W/embedded.wasm") B, lazy fonts $(stat -c %s "$W/lazy.wasm") B"
PLAYWRIGHT="${PW:-playwright}" node tools/libmvp/check_webfaces.cjs "$W/lazy.wasm" "$W/embedded.wasm" "$W" "$PDF" "$DOCX" || rc=1
exit $rc
