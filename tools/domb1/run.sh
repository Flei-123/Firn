#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/domb1/run.sh -- HTML tree construction against the OFFICIAL html5lib
# tests, plus the DOM and the style tree (ROUND B1).
#
#   1. compile the two drivers in THREE build stages (opt / --no-opt /
#      dev-fast) -- all three have to yield the same result
#   2. tools/domb1/harness.py against tests/data/html5lib/*.dat
#      (the official tree-construction suite, 1936 cases, nothing filtered)
#   3. the same quota in all three build stages
#   4. tools/domb1/dom_harness.py: the DOM and the style tree
#      (getElementById, querySelector, textContent, innerHTML, cascade,
#      inheritance, specificity, origins) -- here EVERY case has to pass
#   5. the regression limit from tools/domb1/minquota.txt
#
# Nothing is filtered and nothing is skipped: a case that does not pass is
# a failure and counts against the quota.
#
# Usage:  bash tools/domb1/run.sh [--fast]
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".b1-work"
FAST=0
[ "${1:-}" = "--fast" ] && FAST=1

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 1. compile the two drivers (Firn) =="
"$FIRNC" -o "$WORK/b1parse" lib/browser/b1_main.fi
"$FIRNC" -o "$WORK/b1dom" lib/dom/b1_dom_main.fi
echo "   opt      : $WORK/b1parse, $WORK/b1dom"
if [ "$FAST" -eq 0 ]; then
    "$FIRNC" --no-opt -o "$WORK/b1parse.noopt" lib/browser/b1_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/b1parse.devfast" lib/browser/b1_main.fi
    "$FIRNC" --no-opt -o "$WORK/b1dom.noopt" lib/dom/b1_dom_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/b1dom.devfast" lib/dom/b1_dom_main.fi
    echo "   noopt    : done"
    echo "   dev-fast : done"
fi

echo
echo "== 2. the official html5lib tree construction tests =="
python3 tools/domb1/harness.py "$WORK/b1parse" --json "$WORK/tree.json" \
    | tee "$WORK/tree.txt"
QUOTA=$(python3 -c "import json;print(json.load(open('$WORK/tree.json'))['passed'])")
TOTAL=$(python3 -c "import json;print(json.load(open('$WORK/tree.json'))['total'])")

if [ "$FAST" -eq 0 ]; then
    echo
    echo "== 3. the same quota in all three build stages =="
    for m in noopt devfast; do
        python3 tools/domb1/harness.py "$WORK/b1parse.$m" \
            --json "$WORK/tree.$m.json" > /dev/null || true
        Q=$(python3 -c "import json;print(json.load(open('$WORK/tree.$m.json'))['passed'])")
        if [ "$Q" != "$QUOTA" ]; then
            echo "   ERROR: $m yields $Q instead of $QUOTA passed cases"
            exit 1
        fi
        echo "   $m: $Q -- equal"
    done
fi

echo
echo "== 4. the DOM and the style tree (tools/domb1/cases/) =="
python3 tools/domb1/dom_harness.py "$WORK/b1dom" --show 4 \
    --json "$WORK/dom.json" | tee "$WORK/dom.txt"
DOMGOOD=$(python3 -c "import json;print(json.load(open('$WORK/dom.json'))['passed'])")
DOMALL=$(python3 -c "import json;print(json.load(open('$WORK/dom.json'))['total'])")
if [ "$DOMGOOD" != "$DOMALL" ]; then
    echo "   FAILED: $DOMGOOD of $DOMALL DOM cases -- every one of them has to pass."
    exit 1
fi
if [ "$FAST" -eq 0 ]; then
    for m in noopt devfast; do
        python3 tools/domb1/dom_harness.py "$WORK/b1dom.$m" \
            --json "$WORK/dom.$m.json" > /dev/null
        D=$(python3 -c "import json;print(json.load(open('$WORK/dom.$m.json'))['passed'])")
        if [ "$D" != "$DOMGOOD" ]; then
            echo "   ERROR: $m yields $D instead of $DOMGOOD DOM cases"
            exit 1
        fi
        echo "   $m: $D -- equal"
    done
fi

echo
echo "== 5. regression limit =="
MIN=$(cat tools/domb1/minquota.txt)
PERCENT=$(python3 -c "print('%.2f' % (100.0 * $QUOTA / $TOTAL))")
echo "   tree construction: $QUOTA / $TOTAL  ($PERCENT %)   (limit: $MIN)"
echo "   DOM and style:     $DOMGOOD / $DOMALL"
if [ "$QUOTA" -lt "$MIN" ]; then
    echo "   FAILED: the quota has fallen below the recorded limit."
    exit 1
fi
echo "B1: $QUOTA / $TOTAL html5lib ($PERCENT %), $DOMGOOD / $DOMALL DOM and style cases"
