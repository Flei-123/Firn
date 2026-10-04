#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/qr/run.sh -- lib/qr held against implementations nobody here wrote.
#
#   encoder  check_qr.py: random data, every version 1..40, every level, every
#            mask, automatic mask and version, level boost -- the module matrix
#            must be IDENTICAL with Project Nayuki's reference encoder
#            (qrcodegen) and, for a forced version and mask, with python-qrcode;
#            every code is read back by ZXing-C++.
#   decoder  check_decode.py: codes drawn by python-qrcode and segno as pictures
#            that get worse (clean, rotated, perspective, blurred + noisy + dim,
#            damaged, cluttered); every payload read must be the one encoded;
#            ZXing-C++ reads the same pictures as the yardstick.
#
# Needs python3 with qrcodegen, qrcode, segno, zxing-cpp, Pillow, numpy
# (`pip install --target DIR ...` and QR_PYDEPS=DIR). Without them: SKIP.
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT
[ -n "$QR_PYDEPS" ] && export PYTHONPATH="$QR_PYDEPS${PYTHONPATH:+:$PYTHONPATH}"

"$FIRNC" --opt-level=release-fast -o "$W/qrcli" tools/qr/qrcli_main.fi
"$FIRNC" --opt-level=release-fast -o "$W/qrdeccli" tools/qr/qrdeccli_main.fi

if ! python3 -c 'import qrcodegen, qrcode, segno, zxingcpp, PIL, numpy' 2>/dev/null; then
    echo "  SKIP: python3 needs qrcodegen qrcode segno zxing-cpp Pillow numpy (QR_PYDEPS=DIR)"
    exit 0
fi

echo "== encoder against qrcodegen / python-qrcode / ZXing-C++ =="
python3 tools/qr/check_qr.py "$W/qrcli" 1 150
python3 tools/qr/check_qr.py "$W/qrcli" 2 150

echo "== decoder on pictures that get worse (ZXing-C++ as the yardstick) =="
python3 tools/qr/check_decode.py "$W/qrdeccli" 1 30 --floor
python3 tools/qr/check_decode.py "$W/qrdeccli" 2 30 --floor
echo "QR PASSED"
