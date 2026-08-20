#!/usr/bin/env bash
# tools/layout/run.sh -- the layout path of round 61: box model, block
# flow, inline flow, floats, positioning, flexbox.
#
#   0. build the measuring font (tools/layout/make_font.py)
#   1. compile lib/layout/layout_main.fi in THREE build stages
#      (opt / --no-opt / dev-fast) -- all three have to yield the same
#      boxes, to the bit
#   2. the own cases: the box tree against the frozen expectation
#      (tools/layout/cases/*.expected), text against text
#   3. the CROSS-CHECK against Chromium: the same cases through a real
#      browser, `getBoundingClientRect()` against `getBoundingClientRect()`
#   4. the cross-check on the REAL pages of testdata/realweb/
#   5. soak run with a counter check (tools/layout/gc_layout.sh)
#   6. regression limits from tools/layout/minquota*.txt
#
# Cases that do not pass count as a FAILURE. Nothing is filtered.
#
# Usage:  bash tools/layout/run.sh [--fast]
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".layout-work"
FAST=0
[ "${1:-}" = "--fast" ] && FAST=1

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
echo "== 3. cross-check against Chromium (the same cases in a real browser) =="
CHROME_RC=0
python3 tools/layout/harness.py "$WORK/layout" \
        --json "$WORK/chrome.json" --show 5 | tee "$WORK/chrome.txt" || CHROME_RC=$?
if [ ! -f "$WORK/chrome.json" ]; then
    echo "   NOT RUN: no Chromium found (set FIRN_CHROMIUM)."
    CH_OK=0
    CH_TOTAL=0
    CH_RATE=100
else
    CH_OK=$(python3 -c "import json;print(json.load(open('$WORK/chrome.json'))['chrome_ok'])")
    CH_TOTAL=$(python3 -c "import json;print(json.load(open('$WORK/chrome.json'))['chrome_total'])")
    CH_RATE=$(python3 -c "import json;d=json.load(open('$WORK/chrome.json'));print('%.2f'%d.get('chrome_deviation_percent',100.0))")
    if [ "$CH_OK" != "$CH_TOTAL" ]; then
        echo "   FAILED: $CH_OK of $CH_TOTAL boxes equal to Chromium ($CH_RATE %% off)"
        exit 1
    fi
fi

if [ "$FAST" -eq 0 ]; then
    echo
    echo "== 4. cross-check on the REAL pages of testdata/realweb/ =="
    # This number is NOT a pass criterion and is not meant to be one: the
    # pages need tables, replaced elements with an intrinsic size and
    # presentational attributes, and none of that exists yet. It is
    # reported so that the gap is a MEASUREMENT and not an opinion.
    python3 tools/layout/realweb.py "$WORK/layout" \
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
echo "   own cases:            $EXP_OK / $EXP_TOTAL boxes in $CASES cases   (limit: $MIN_CASES)"
echo "   against Chromium:     $CH_OK / $CH_TOTAL boxes, deviation $CH_RATE %   (limit: $MIN_CHROME)"
if [ "$EXP_OK" -lt "$MIN_CASES" ] || [ "$CH_OK" -lt "$MIN_CHROME" ]; then
    echo "   FAILED: a quota has fallen below the recorded limit."
    exit 1
fi
echo "LAYOUT OK: $EXP_OK / $EXP_TOTAL own boxes, $CH_OK / $CH_TOTAL equal to Chromium (deviation $CH_RATE %)"
