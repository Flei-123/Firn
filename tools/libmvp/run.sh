#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/run.sh -- the libraries OpenPlan asked for (LIB-001..008),
# held against other implementations:
#   std.time  20,000 instants x 5 zones against Python's datetime/zoneinfo
#   lib/zip   Python's zipfile and Info-ZIP's unzip, both directions, and
#             the hostile archives (zip slip, duplicates, other methods)
#   lib/pdf   poppler (pdfinfo, pdffonts, pdftotext, pdftoppm) and pypdf
#   lib/regex Python's re on 20,000 random patterns plus a fixed corpus
#   lib/i18n  ICU (PyICU): plural rules, numbers and dates in 8 languages
# std.fs has no second implementation to compare with; tests/1910_std_fs.fi
# is its proof (against the kernel's own answers).
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
rc=0
for t in time_probe zip_probe pdf_probe regex_probe i18n_probe; do
    "$FIRNC" -o "$W/$t" "tools/libmvp/$t.fi" > "$W/$t.log" 2>&1 || { echo "  FAIL $t does not build"; grep -v RWX "$W/$t.log" | head -5; rc=1; }
done
[ $rc -eq 0 ] || exit 1
echo "-- std.time"
if [ -r /usr/share/zoneinfo/Europe/Vienna ]; then
    "$W/time_probe" | python3 tools/libmvp/check_time.py || rc=1
else
    echo "  skip: no /usr/share/zoneinfo"
fi
echo "-- lib/regex"
python3 tools/libmvp/check_regex.py "$W/regex_probe" 20000 || rc=1
echo "-- lib/i18n"
python3 tools/libmvp/check_i18n.py "$W/i18n_probe" || rc=1
echo "-- lib/zip"
mkdir -p "$W/zip"
if command -v unzip > /dev/null && command -v zip > /dev/null; then
    python3 tools/libmvp/check_zip.py "$W/zip_probe" "$W/zip" || rc=1
else
    echo "  skip: unzip/zip missing"
fi
echo "-- lib/pdf"
mkdir -p "$W/pdf"
if command -v pdftoppm > /dev/null && python3 -c 'import pypdf, PIL' 2> /dev/null; then
    python3 tools/libmvp/check_pdf.py "$W/pdf_probe" "$W/pdf" tests/data/fonts/FirnSans.ttf \
        /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf /usr/share/fonts/truetype/liberation2/LiberationSerif-Regular.ttf || rc=1
else
    echo "  skip: poppler-utils or pypdf/Pillow missing"
fi
if [ $rc -eq 0 ]; then echo "LIBMVP PASSED"; else echo "LIBMVP FAILED"; fi
exit $rc
