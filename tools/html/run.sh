#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/html/run.sh -- HTML tree construction + DOM core (round 54).
#
#   1. compile lib/browser/parse_main.fi in THREE build stages
#      (opt / --no-opt / dev-fast) -- all of them have to yield the same quota
#   2. tools/html/harness_tree.py against tools/html/cases/*.dat
#   3. report the KNOWN GAPS separately (tools/html/gaps/)
#   4. robustness on real pages (testdata/realweb/): no abort, and
#      all three build stages yield the same tree, byte for byte
#   5. soak run with a counter-check (tools/html/gc_tree.sh)
#   6. regression limit from tools/html/minquota_tree.txt
#
# Cases that do not pass count as a FAILURE. Nothing is filtered.
#
# Usage:  bash tools/html/run.sh [--fast]
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".tree-work"
FAST=0
[ "${1:-}" = "--fast" ] && FAST=1

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 1. compile the tree builder (Firn) =="
"$FIRNC" -o "$WORK/parse" lib/browser/parse_main.fi
echo "   opt      : $WORK/parse"
if [ "$FAST" -eq 0 ]; then
    "$FIRNC" --no-opt -o "$WORK/parse.noopt" lib/browser/parse_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/parse.devfast" lib/browser/parse_main.fi
    echo "   noopt    : $WORK/parse.noopt"
    echo "   dev-fast : $WORK/parse.devfast"
fi

echo
echo "== 2. own cases (tools/html/cases/*.dat) =="
python3 tools/html/harness_tree.py "$WORK/parse" \
        --json "$WORK/balance.json" --show 5 | tee "$WORK/balance.txt"
QUOTA=$(python3 -c "import json;print(json.load(open('$WORK/balance.json'))['passed'])")
TOTAL=$(python3 -c "import json;print(json.load(open('$WORK/balance.json'))['total'])")

if [ "$FAST" -eq 0 ]; then
    echo
    echo "== 2a. the same quota in all three build stages =="
    for m in noopt devfast; do
        python3 tools/html/harness_tree.py "$WORK/parse.$m" --json "$WORK/balance.$m.json" >/dev/null || true
        Q=$(python3 -c "import json;print(json.load(open('$WORK/balance.$m.json'))['passed'])")
        if [ "$Q" != "$QUOTA" ]; then
            echo "   ERROR: $m yields $Q instead of $QUOTA passed cases"
            exit 1
        fi
        echo "   $m: $Q -- equal"
    done
fi

echo
echo "== 3. known gaps (tools/html/gaps/) -- they have to fail =="
echo "   The expected trees are the RIGHT ones; they show what round 54 cannot do."
set +e
python3 tools/html/harness_tree.py "$WORK/parse" --gaps \
        --json "$WORK/gaps.json" > "$WORK/gaps.txt"
set -e
tail -4 "$WORK/gaps.txt" | sed 's/^/   /'
GQ=$(python3 -c "import json;print(json.load(open('$WORK/gaps.json'))['passed'])")
GT=$(python3 -c "import json;print(json.load(open('$WORK/gaps.json'))['total'])")
echo "   $GQ of $GT known gaps already closed"

echo
echo "== 4. real pages (testdata/realweb/) =="
python3 tools/html/realweb.py "$WORK/parse" > "$WORK/realweb.txt"
sed 's/^/   /' "$WORK/realweb.txt"
if [ "$FAST" -eq 0 ]; then
    for m in noopt devfast; do
        python3 tools/html/realweb.py "$WORK/parse.$m" > "$WORK/realweb.$m.txt"
        if ! cmp -s "$WORK/realweb.txt" "$WORK/realweb.$m.txt"; then
            echo "   ERROR: $m yields a different tree on real pages"
            exit 1
        fi
        echo "   $m: the same trees, byte for byte"
    done
fi

echo
echo "== 5. soak run: build and discard trees without growing =="
if [ "$FAST" -eq 1 ]; then
    BAUM_RUNDEN=${BAUM_RUNDEN:-4000} BAUM_MS=${BAUM_MS:-3000} \
      BAUM_LECK_RUNDEN=${BAUM_LECK_RUNDEN:-3000} bash tools/html/gc_tree.sh | sed 's/^/   /'
else
    bash tools/html/gc_tree.sh | sed 's/^/   /'
fi

echo
echo "== 6. regression limit =="
MIN=$(cat tools/html/minquota_tree.txt)
echo "   tree construction: $QUOTA / $TOTAL   (limit: $MIN)"
if [ "$QUOTA" -lt "$MIN" ]; then
    echo "   FAILED: the quota has fallen below the recorded limit."
    exit 1
fi
echo "OK: $QUOTA / $TOTAL own cases passed"
