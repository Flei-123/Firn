#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/kitmd/run.sh -- gate for lib/fui/kitmd.fi: checks, 20000 fuzz runs, streaming bench, golden images.
#     tools/kitmd/run.sh            compare against tools/kitmd/golden
#     tools/kitmd/run.sh --update   take the new images as goldens
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-/root/firn/compiler/target/release/firnc}"
D="${FONT_DIR:-/usr/share/fonts/truetype/dejavu}"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/img"
"$FIRNC" --opt-level=release-fast -o "$T/kitmd" tools/kitmd/kitmd_main.fi
"$T/kitmd" check x light tools/kitmd/fixtures.txt "$D/DejaVuSans.ttf" "$D/DejaVuSansMono.ttf" 20000
for s in light dark; do
    "$T/kitmd" render "$T/img/kitmd-$s.png" $s tools/kitmd/fixtures.txt "$D/DejaVuSans.ttf" "$D/DejaVuSansMono.ttf"
done
if [ "$1" = "--update" ]; then
    python3 -I tools/fui/refdiff.py update "$T/img" tools/kitmd/golden
else
    python3 -I tools/fui/refdiff.py compare "$T/img" tools/kitmd/golden --max "${KITMD_MAX:-0.5}"
fi
