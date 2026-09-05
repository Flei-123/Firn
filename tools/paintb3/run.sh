#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/paintb3/run.sh -- ROUND B3: the browser becomes VISIBLE.
#
#   1. compile the three root files of the round in THREE build stages
#      (opt / --no-opt / dev-fast)
#   2. the TrueType reader and the glyph rasteriser against fontTools and
#      against a second, independent rasteriser (tools/paintb3/font_check.py)
#   3. PNG in both directions against Pillow (tools/paintb3/png_check.py)
#   4. the seven own cases: the same picture in all three build stages,
#      against a frozen hash, and never empty (tools/paintb3/cases.py)
#   5. the metrics really flow back: the letters fit the boxes the layout
#      made for them, WITH a counter-check that fails when they do not
#      (tools/paintb3/textfit.py)
#   6. the OFFICIAL REFERENCE TESTS of the Web Platform Tests: 541 pairs
#      of documents that have to look the same (tools/paintb3/reftest.py,
#      tests/data/wpt-ref/PROVENANCE.md)
#   7. the regression limits from tools/paintb3/minquota.txt
#
# NO BROWSER IS STARTED and no socket is opened. The reference tests, the
# two fonts and every stylesheet they need lie in the repository;
# `tools/paintb3/harvest.py` fetched them once and is never called from
# here.
#
# WHY A REFERENCE TEST NEEDS A GUARD. An engine that draws NOTHING passes
# every reference test there is: both sides come out white and white
# equals white. `reftest.py` therefore counts a pair only if the picture
# is also not empty, and prints the empty matches separately as `vacuous`.
# The same guard, in its smallest form, is in section 4: every own case
# carries the number of octets it painted and the number of pixels its
# glyphs set, and both have to stay within half of the frozen value. That
# is the lesson of round K7B in the kernel, where a screen was 87 per cent
# right and every single letter was missing.
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".paintb3-work"
mkdir -p "$WORK"

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 1. three build stages of the round's three root files =="
for SRC in lib/paint/b3_main.fi lib/font/font_main.fi lib/paint/png_main.fi; do
    BASE=$(basename "$SRC" .fi)
    "$FIRNC" -o "$WORK/${BASE}_opt" "$SRC" 2>"$WORK/build.log"
    "$FIRNC" --no-opt -o "$WORK/${BASE}_noopt" "$SRC" 2>>"$WORK/build.log"
    "$FIRNC" --opt-level=dev-fast -o "$WORK/${BASE}_dev" "$SRC" \
        2>>"$WORK/build.log"
done
echo "   opt, --no-opt and dev-fast built for paint, font and png"

echo "== 2. the TrueType reader and the glyph rasteriser =="
python3 tools/paintb3/font_check.py "$WORK/font_main_opt" \
    tests/data/fonts/FirnSans.ttf | tee "$WORK/font_sans.txt"
python3 tools/paintb3/font_check.py "$WORK/font_main_opt" \
    tests/data/fonts/Ahem.ttf | tee "$WORK/font_ahem.txt"

echo "== 3. PNG, both directions, against Pillow =="
python3 tools/paintb3/png_check.py "$WORK/b3_main_opt" "$WORK/png_main_opt" \
    | tee "$WORK/png.txt"

echo "== 4. the own cases, in three build stages, against a frozen picture =="
python3 tools/paintb3/cases.py "$WORK/b3_main_opt" "$WORK/b3_main_noopt" \
    "$WORK/b3_main_dev" | tee "$WORK/cases.txt"

echo "== 5. do the letters fit the boxes the layout made for them? =="
python3 tools/paintb3/textfit.py "$WORK/b3_main_opt" | tee "$WORK/textfit.txt"

echo "== 6. the official reference tests (tests/data/wpt-ref) =="
python3 tools/paintb3/reftest.py "$WORK/b3_main_opt" \
    --json "$WORK/ref.json" | tee "$WORK/ref.txt"
QUOTA=$(grep -o '^B3-REF: [0-9]*' "$WORK/ref.txt" | awk '{print $2}')
TOTAL=$(grep -oE '^B3-REF: [0-9]+ / [0-9]+' "$WORK/ref.txt" | awk '{print $4}')

echo "== 7. the regression limits (tools/paintb3/minquota.txt) =="
LIMIT=$(awk '/^wpt_ref /{print $2}' tools/paintb3/minquota.txt)
GLIMIT=$(awk '/^glyphs /{print $2}' tools/paintb3/minquota.txt)
GOOD_SANS=$(grep -o '^FONT: [0-9]*' "$WORK/font_sans.txt" | awk '{print $2}')
GOOD_AHEM=$(grep -o '^FONT: [0-9]*' "$WORK/font_ahem.txt" | awk '{print $2}')
GLYPHS=$((GOOD_SANS + GOOD_AHEM))
echo "   reference tests: $QUOTA / $TOTAL   (limit: $LIMIT)"
echo "   glyphs:          $GLYPHS correct against the second rasteriser" \
     "  (limit: $GLIMIT)"
FAILED=0
if [ "$QUOTA" -lt "$LIMIT" ]; then
    echo "   FAILED: the reference quota fell below the limit"
    FAILED=1
fi
if [ "$GLYPHS" -lt "$GLIMIT" ]; then
    echo "   FAILED: fewer glyphs are drawn correctly than before"
    FAILED=1
fi
if ! grep -q '^PNG: OK' "$WORK/png.txt"; then
    echo "   FAILED: PNG does not survive the round trip"
    FAILED=1
fi
if grep -q 'FAILED' "$WORK/cases.txt"; then
    echo "   FAILED: an own case changed or is empty"
    FAILED=1
fi
if ! grep -q '^TEXTFIT: OK' "$WORK/textfit.txt"; then
    echo "   FAILED: the layout and the painter do not agree about text"
    FAILED=1
fi
if grep -qE '^FONT: .*[1-9][0-9]* metric deviations' "$WORK/font_sans.txt" \
    "$WORK/font_ahem.txt"; then
    echo "   FAILED: the TrueType reader disagrees with fontTools"
    FAILED=1
fi
if [ "$FAILED" -ne 0 ]; then
    exit 1
fi

echo "B3 OK: $QUOTA / $TOTAL official reference tests, $GLYPHS glyphs" \
     "against an independent rasteriser, PNG both ways, text inside its" \
     "boxes, 7 / 7 own cases identical in three build stages"
