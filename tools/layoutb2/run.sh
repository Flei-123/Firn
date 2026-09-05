#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/layoutb2/run.sh -- ROUND B2: the layout engine against the OFFICIAL
# Web Platform Tests, and the proof that a second layout of the same tree
# gives the same answer.
#
#   1. compile lib/layout/b2_main.fi in THREE build stages
#      (opt / --no-opt / dev-fast)
#   2. the whole harvested corpus through the release build: the quota per
#      group (tests/data/wpt-css, see PROVENANCE.md there)
#   3. the same corpus through the other two builds -- the quota has to be
#      the SAME number, or the optimiser changed a layout
#   4. THE REFLOW: every document is laid out at 800, then at 400, then at
#      800 again, and the result has to be identical to the single layout.
#      That is the proof of the split between what the window width
#      decides and what it does not (docs/ROUNDB2.md)
#   5. the regression limits from tools/layoutb2/minquota.txt
#
# The comparison itself is `tools/layoutb2/harness.py`; it reads the
# `data-expected-*` attributes of the WPT tests and applies the tolerance
# of `resources/check-layout-th.js`. NO BROWSER IS STARTED and no socket is
# opened -- the expectations lie in the test files themselves.
# `tools/layoutb2/chrome_check.py` runs the same corpus through a real
# Chromium; that is a calibration for a person at a keyboard and must
# never be called from test.sh.
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".layoutb2-work"
mkdir -p "$WORK"

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 1. three build stages of lib/layout/b2_main.fi =="
"$FIRNC" -o "$WORK/b2_opt" lib/layout/b2_main.fi 2>"$WORK/build.log"
"$FIRNC" --no-opt -o "$WORK/b2_noopt" lib/layout/b2_main.fi 2>>"$WORK/build.log"
"$FIRNC" --opt-level=dev-fast -o "$WORK/b2_dev" lib/layout/b2_main.fi \
    2>>"$WORK/build.log"
echo "   opt, --no-opt and dev-fast built"

echo "== 2. the corpus (tests/data/wpt-css) through the release build =="
python3 tools/layoutb2/harness.py "$WORK/b2_opt" --json "$WORK/b2.json" \
    | tee "$WORK/opt.txt"
QUOTA=$(grep -o 'B2-WPT: [0-9]*' "$WORK/opt.txt" | awk '{print $2}')
TOTAL=$(grep -oE 'B2-WPT: [0-9]+ / [0-9]+' "$WORK/opt.txt" | awk '{print $4}')

echo "== 3. the same corpus in the other two build stages =="
for STAGE in noopt dev; do
    python3 tools/layoutb2/harness.py "$WORK/b2_$STAGE" --group b2 \
        > "$WORK/$STAGE.txt"
    Q=$(grep -o 'B2-WPT: [0-9]*' "$WORK/$STAGE.txt" | awk '{print $2}')
    if [ "$Q" != "$QUOTA" ]; then
        echo "   FAILED: stage $STAGE reaches $Q, the release build $QUOTA"
        exit 1
    fi
    echo "   $STAGE: $Q -- the same number"
done

echo "== 4. the reflow: 800 -> 400 -> 800 has to be the first layout again =="
python3 tools/layoutb2/harness.py "$WORK/b2_opt" --reflow-check \
    | tee "$WORK/reflow.txt"
if ! grep -q '^REFLOW: \([0-9]*\) of \1 documents identical' "$WORK/reflow.txt"
then
    echo "   FAILED: a second layout of the same tree is not the first one"
    exit 1
fi

echo "== 5. the regression limits (tools/layoutb2/minquota.txt) =="
LIMIT=$(awk '/^wpt_b2 /{print $2}' tools/layoutb2/minquota.txt)
REFLOW_LIMIT=$(awk '/^reflow /{print $2}' tools/layoutb2/minquota.txt)
REFLOW_OK=$(grep -o '^REFLOW: [0-9]*' "$WORK/reflow.txt" | awk '{print $2}')
echo "   corpus B2:  $QUOTA / $TOTAL   (limit: $LIMIT)"
echo "   reflow:     $REFLOW_OK documents identical   (limit: $REFLOW_LIMIT)"
if [ "$QUOTA" -lt "$LIMIT" ]; then
    echo "   FAILED: the quota fell below the limit"
    exit 1
fi
if [ "$REFLOW_OK" -lt "$REFLOW_LIMIT" ]; then
    echo "   FAILED: fewer documents survive a reflow than before"
    exit 1
fi

echo "B2 OK: $QUOTA / $TOTAL tests of corpus B2 against the official WPT" \
     "suite, reflow $REFLOW_OK / $REFLOW_OK documents identical"
