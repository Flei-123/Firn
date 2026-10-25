#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/barcode/run.sh -- lib/barcode held against python-barcode, zxing-cpp and libdmtx.
#   BARCODE_PYDEPS=DIR (pip install --target) or an active venv with python-barcode zxing-cpp pylibdmtx Pillow numpy; else SKIP.
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT
[ -n "$BARCODE_PYDEPS" ] && export PYTHONPATH="$BARCODE_PYDEPS${PYTHONPATH:+:$PYTHONPATH}"
"$FIRNC" --opt-level=release-fast -o "$W/bccli" tools/barcode/bccli_main.fi
"$FIRNC" --opt-level=release-fast -o "$W/dmcli" tools/barcode/dmcli_main.fi
if ! python3 -c 'import barcode, zxingcpp, PIL, numpy' 2>/dev/null; then
    echo "  SKIP: python3 needs python-barcode zxing-cpp Pillow numpy"
    exit 0
fi
cd tools/barcode
python3 check_linear.py "$W/bccli" 1 10
python3 check_dm_matrix.py "$W/dmcli" 1 10
echo "BARCODE PASSED"
