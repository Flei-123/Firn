#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/layout/run.sh -- the layout path of rounds 61 and 67: box model,
# block flow, inline flow, floats, positioning with `fixed` and `sticky`,
# the paint order with `z-index`, and the full flexbox.
#
#   0. build the measuring font (tools/layout/make_font.py)
#   1. compile lib/layout/layout_main.fi in THREE build stages
#      (opt / --no-opt / dev-fast) -- all three have to yield the same
#      boxes, to the bit
#   2. the own cases: the box tree against the frozen expectation
#      (tools/layout/cases/*.expected), text against text
#   3. the CROSS-CHECK against Chromium: the same cases, box against box
#      out of `getBoundingClientRect()`
#   3a. the PAINT ORDER against Chromium: who lies on top, asked with
#      `document.elementFromPoint` (tools/layout/stack.py)
#   4. the cross-check on the REAL pages of testdata/realweb/
#   5. soak run with a counter check (tools/layout/gc_layout.sh)
#   6. regression limits from tools/layout/minquota*.txt
#
# Cases that do not pass count as a FAILURE. Nothing is filtered.
#
# ROUND 78 -- THE BROWSER IS NOT A DEPENDENCY ANY MORE.
# Sections 3, 3a and 4 used to START a Chromium, and a passing suite
# therefore required a 200 MB foreign program to be installed. Firn is
# supposed to stand on its own, so the browser is asked ONCE and its
# answer lives in the repository as data:
#
#     tools/layout/reference/cases.json     boxes of tools/layout/cases
#     tools/layout/reference/stack.json     probe points + topmost element
#     tools/layout/reference/realweb.json   boxes of testdata/realweb
#
# Every file names the browser version, the date and the layout viewport it
# was measured with. The comparison itself did not get weaker: it is still
# box against box against a foreign engine written by other people from the
# specification. Only the moment of asking moved -- and a deviation now has
# exactly ONE possible cause (this engine), because the other side can no
# longer silently become a different browser version between two runs.
#
# Usage:
#   bash tools/layout/run.sh [--fast]        against the frozen reference
#                                            (the default; no browser needed)
#   bash tools/layout/run.sh --live-chromium  against a live browser, for a
#                                            cross-check against a newer one
#   bash tools/layout/run.sh --refresh-reference
#                                            measure live AND rewrite the
#                                            frozen files under
#                                            tools/layout/reference/
#
# `--live-chromium` and `--refresh-reference` must never be called from
# test.sh: the acceptance has to run without foreign software.
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".layout-work"
FAST=0
MODE=frozen
for a in "$@"; do
    case "$a" in
        --fast)               FAST=1 ;;
        --live-chromium|--live) MODE=live ;;
        --refresh-reference)  MODE=refresh ;;
        *) echo "unknown option: $a"; exit 2 ;;
    esac
done

# The browser argument the three cross-checks are called with, and whether a
# deviation stops the run. While REFRESHING, a deviation must not stop it:
# the point of that run is to write all three reference files, and the
# numbers it prints are the live ones the caller wanted to see.
case "$MODE" in
    frozen)  CHARG="--reference"       ; HARD=1 ; WHO="the frozen reference" ;;
    live)    CHARG=""                  ; HARD=1 ; WHO="a LIVE Chromium" ;;
    refresh) CHARG="--write-reference" ; HARD=0 ; WHO="a LIVE Chromium (rewriting the reference)" ;;
esac

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 0. the measuring font =="
# A layout engine cannot be compared against a browser as long as the
# width of a letter is unknown. The font is BUILT, not downloaded, so the
# numbers stand in the source and not in a binary.
python3 tools/layout/make_font.py | sed 's/^/   /'

echo
echo "== 1. compile the layout path (Firn) =="
"$FIRNC" -o "$WORK/layout" lib/layout/layout_main.fi
"$FIRNC" -o "$WORK/layoutbench" lib/layout/bench_main.fi
echo "   opt      : $WORK/layout, $WORK/layoutbench"
if [ "$FAST" -eq 0 ]; then
    "$FIRNC" --no-opt -o "$WORK/layout.noopt" lib/layout/layout_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/layout.devfast" lib/layout/layout_main.fi
    echo "   noopt    : $WORK/layout.noopt"
    echo "   dev-fast : $WORK/layout.devfast"
fi

echo
echo "== 2. own cases: the box tree against the frozen expectation =="
python3 tools/layout/harness.py "$WORK/layout" --no-chrome \
        --json "$WORK/expected.json" --show 5 | tee "$WORK/expected.txt" || true
EXP_OK=$(python3 -c "import json;print(json.load(open('$WORK/expected.json'))['expected_ok'])")
EXP_TOTAL=$(python3 -c "import json;print(json.load(open('$WORK/expected.json'))['expected_total'])")
CASES=$(ls tools/layout/cases/*.html | wc -l)
if [ "$EXP_OK" != "$EXP_TOTAL" ]; then
    echo "   FAILED: $EXP_OK of $EXP_TOTAL boxes match the expectation"
    exit 1
fi

if [ "$FAST" -eq 0 ]; then
    echo
    echo "== 2a. the same boxes in all three build stages =="
    for m in noopt devfast; do
        python3 tools/layout/harness.py "$WORK/layout.$m" --no-chrome \
                --json "$WORK/expected.$m.json" --show 0 >/dev/null || true
        Q=$(python3 -c "import json;print(json.load(open('$WORK/expected.$m.json'))['expected_ok'])")
        if [ "$Q" != "$EXP_OK" ]; then
            echo "   ERROR: $m yields $Q instead of $EXP_OK matching boxes"
            exit 1
        fi
        echo "   $m: $Q -- equal"
    done
fi

echo
echo "== 3. cross-check against Chromium -- $WHO =="
rm -f "$WORK/chrome.json"
CHROME_RC=0
python3 tools/layout/harness.py "$WORK/layout" $CHARG \
        --json "$WORK/chrome.json" --show 5 | tee "$WORK/chrome.txt" || CHROME_RC=$?
if [ ! -f "$WORK/chrome.json" ]; then
    # In every mode this is a FAILURE. Before round 78 a missing browser
    # silently turned the section off and the run still said OK -- which is
    # how a cross-check quietly stops being one.
    echo "   FAILED: no measurement (mode $MODE)."
    exit 1
fi
CH_OK=$(python3 -c "import json;print(json.load(open('$WORK/chrome.json'))['chrome_ok'])")
CH_TOTAL=$(python3 -c "import json;print(json.load(open('$WORK/chrome.json'))['chrome_total'])")
CH_RATE=$(python3 -c "import json;d=json.load(open('$WORK/chrome.json'));print('%.2f'%d.get('chrome_deviation_percent',100.0))")
if [ "$CH_OK" != "$CH_TOTAL" ]; then
    echo "   FAILED: $CH_OK of $CH_TOTAL boxes equal to Chromium ($CH_RATE % off)"
    if [ "$HARD" -eq 1 ]; then exit 1; fi
fi

echo
echo "== 3a. the paint order against Chromium (tools/layout/stack.py) =="
# A layout can be proven with rectangles. A paint order cannot: no browser
# has a `getPaintOrder()`. But `document.elementFromPoint(x, y)` answers
# exactly the question the order decides -- which element is on top at
# this point -- and that answer can be compared. The probe points are
# frozen together with the answers (round 78): a point is a fixed place on
# the page, so the QUESTION stays the same even when the engine moves a box.
rm -f "$WORK/stack.json"
STACK_RC=0
python3 tools/layout/stack.py "$WORK/layout" $CHARG \
        --json "$WORK/stack.json" --show 5 | tee "$WORK/stack.txt" \
        || STACK_RC=$?
if [ ! -f "$WORK/stack.json" ]; then
    echo "   FAILED: no measurement (mode $MODE)."
    exit 1
fi
ST_OK=$(python3 -c "import json;print(json.load(open('$WORK/stack.json'))['points_ok'])")
ST_TOTAL=$(python3 -c "import json;print(json.load(open('$WORK/stack.json'))['points_total'])")
if [ "$ST_OK" != "$ST_TOTAL" ]; then
    echo "   FAILED: $ST_OK of $ST_TOTAL probe points equal to Chromium"
    if [ "$HARD" -eq 1 ]; then exit 1; fi
fi

if [ "$FAST" -eq 0 ] || [ "$MODE" = refresh ]; then
    echo
    echo "== 4. cross-check on the REAL pages of testdata/realweb/ =="
    # This number is NOT a pass criterion and is not meant to be one: the
    # pages need tables, replaced elements with an intrinsic size and
    # presentational attributes, and none of that exists yet. It is
    # reported so that the gap is a MEASUREMENT and not an opinion.
    python3 tools/layout/realweb.py "$WORK/layout" $CHARG \
            --json "$WORK/realweb.json" | tee "$WORK/realweb.txt" || true
fi

echo
echo "== 5. soak run: the box tree without a leak (tools/layout/gc_layout.sh) =="
if [ "$FAST" -eq 1 ]; then
    LAYOUT_SOAK_MS=${LAYOUT_SOAK_MS:-6000} \
    LAYOUT_SOAK_LEAK_ROUNDS=${LAYOUT_SOAK_LEAK_ROUNDS:-2500} \
        bash tools/layout/gc_layout.sh | sed 's/^/   /'
else
    bash tools/layout/gc_layout.sh | sed 's/^/   /'
fi

echo
echo "== 6. regression limits =="
MIN_CASES=$(cat tools/layout/minquota_cases.txt)
MIN_CHROME=$(cat tools/layout/minquota_chrome.txt)
MIN_STACK=$(cat tools/layout/minquota_stack.txt)
echo "   own cases:            $EXP_OK / $EXP_TOTAL boxes in $CASES cases   (limit: $MIN_CASES)"
echo "   against Chromium:     $CH_OK / $CH_TOTAL boxes, deviation $CH_RATE %   (limit: $MIN_CHROME)"
echo "   paint order:          $ST_OK / $ST_TOTAL probe points   (limit: $MIN_STACK)"
if [ "$EXP_OK" -lt "$MIN_CASES" ] || [ "$CH_OK" -lt "$MIN_CHROME" ] ||
   [ "$ST_OK" -lt "$MIN_STACK" ]; then
    echo "   FAILED: a quota has fallen below the recorded limit."
    if [ "$HARD" -eq 1 ]; then exit 1; fi
fi
echo "LAYOUT OK: $EXP_OK / $EXP_TOTAL own boxes, $CH_OK / $CH_TOTAL equal to Chromium (deviation $CH_RATE %), paint order $ST_OK / $ST_TOTAL -- $WHO"
