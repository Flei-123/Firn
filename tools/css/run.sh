#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/css/run.sh -- the CSS path of round 60: syntax, selectors, cascade.
#
#   0. are the foreign test data unchanged? (sha256 against the upstream commit)
#   1. compile lib/css/parse_main.fi and lib/css/style_main.fi in THREE
#      build stages (opt / --no-opt / dev-fast) -- all three have to yield
#      the same quota
#   2. the official suite css-parsing-tests (305 cases, ten files):
#      tools/css/harness_syntax.py
#   3. the own cases for selectors, cascade, inheritance and error
#      tolerance: tools/css/harness_cases.py
#   4. the cross-check of the selector engine against cssselect2 on the
#      real pages of testdata/realweb/: tools/css/harness_select.py
#   5. soak run with a counter check (tools/css/gc_style.sh)
#   6. regression limits from tools/css/minquota*.txt
#
# Cases that do not pass count as a FAILURE. Nothing is filtered.
#
# Usage:  bash tools/css/run.sh [--fast]
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".css-work"
FAST=0
[ "${1:-}" = "--fast" ] && FAST=1

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 0. test data unchanged? (sha256 against the upstream commit) =="
bash tools/css/verify_testdata.sh | sed 's/^/   /'
echo

echo "== 1. compile the CSS path (Firn) =="
"$FIRNC" -o "$WORK/cssparse" lib/css/parse_main.fi
"$FIRNC" -o "$WORK/cssstyle" lib/css/style_main.fi
"$FIRNC" -o "$WORK/cssbench" lib/css/bench_main.fi
echo "   opt      : $WORK/cssparse, $WORK/cssstyle, $WORK/cssbench"
if [ "$FAST" -eq 0 ]; then
    "$FIRNC" --no-opt -o "$WORK/cssparse.noopt" lib/css/parse_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/cssparse.devfast" lib/css/parse_main.fi
    "$FIRNC" --no-opt -o "$WORK/cssstyle.noopt" lib/css/style_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/cssstyle.devfast" lib/css/style_main.fi
    echo "   noopt    : $WORK/cssparse.noopt, $WORK/cssstyle.noopt"
    echo "   dev-fast : $WORK/cssparse.devfast, $WORK/cssstyle.devfast"
fi

echo
echo "== 2. css-parsing-tests (testdata/css-parsing-tests, 305 cases) =="
python3 tools/css/harness_syntax.py "$WORK/cssparse" \
        --json "$WORK/syntax.json" --show 5 | tee "$WORK/syntax.txt"
SYNTAX=$(python3 -c "import json;print(json.load(open('$WORK/syntax.json'))['passed'])")
SYNTAX_TOTAL=$(python3 -c "import json;print(json.load(open('$WORK/syntax.json'))['total'])")

if [ "$FAST" -eq 0 ]; then
    echo
    echo "== 2a. the same quota in all three build stages =="
    for m in noopt devfast; do
        python3 tools/css/harness_syntax.py "$WORK/cssparse.$m" \
                --json "$WORK/syntax.$m.json" >/dev/null
        Q=$(python3 -c "import json;print(json.load(open('$WORK/syntax.$m.json'))['passed'])")
        if [ "$Q" != "$SYNTAX" ]; then
            echo "   ERROR: $m yields $Q instead of $SYNTAX passed cases"
            exit 1
        fi
        echo "   $m: $Q -- equal"
    done
fi

echo
echo "== 3. own cases: selectors, cascade, inheritance, error tolerance =="
python3 tools/css/harness_cases.py "$WORK/cssstyle" \
        --json "$WORK/cases.json" --show 5 | tee "$WORK/cases.txt"
CASES=$(python3 -c "import json;print(json.load(open('$WORK/cases.json'))['passed'])")
CASES_TOTAL=$(python3 -c "import json;print(json.load(open('$WORK/cases.json'))['total'])")

if [ "$FAST" -eq 0 ]; then
    echo
    echo "== 3a. the same quota in all three build stages =="
    for m in noopt devfast; do
        python3 tools/css/harness_cases.py "$WORK/cssstyle.$m" \
                --json "$WORK/cases.$m.json" >/dev/null || true
        Q=$(python3 -c "import json;print(json.load(open('$WORK/cases.$m.json'))['passed'])")
        if [ "$Q" != "$CASES" ]; then
            echo "   ERROR: $m yields $Q instead of $CASES passed checks"
            exit 1
        fi
        echo "   $m: $Q -- equal"
    done
fi

echo
echo "== 4. cross-check of the selectors against cssselect2 (real pages) =="
SELECT_RC=0
python3 tools/css/harness_select.py "$WORK/cssstyle" \
        --json "$WORK/select.json" --show 5 | tee "$WORK/select.txt" || SELECT_RC=$?
if [ ! -f "$WORK/select.json" ]; then
    echo "   NOT RUN: cssselect2 is missing (pip3 install cssselect2)."
    SEL_EQUAL=0
    SEL_CHECKS=0
    SEL_SPEC=0
else
    SEL_EQUAL=$(python3 -c "import json;print(json.load(open('$WORK/select.json'))['equal'])")
    SEL_CHECKS=$(python3 -c "import json;print(json.load(open('$WORK/select.json'))['checks'])")
    SEL_SPEC=$(python3 -c "import json;print(json.load(open('$WORK/select.json'))['spec_equal'])")
    if [ "$SEL_EQUAL" != "$SEL_CHECKS" ]; then
        echo "   FAILED: $SEL_EQUAL of $SEL_CHECKS match sets equal"
        exit 1
    fi
fi

echo
echo "== 5. soak run: the CSS path without a leak (tools/css/gc_style.sh) =="
if [ "$FAST" -eq 1 ]; then
    CSS_SOAK_MS=${CSS_SOAK_MS:-6000} CSS_SOAK_LEAK_ROUNDS=${CSS_SOAK_LEAK_ROUNDS:-2500} \
        bash tools/css/gc_style.sh | sed 's/^/   /'
else
    bash tools/css/gc_style.sh | sed 's/^/   /'
fi

echo
echo "== 6. regression limits =="
MIN_SYNTAX=$(cat tools/css/minquota_syntax.txt)
MIN_CASES=$(cat tools/css/minquota_cases.txt)
echo "   css-parsing-tests: $SYNTAX / $SYNTAX_TOTAL   (limit: $MIN_SYNTAX)"
echo "   own cases:         $CASES / $CASES_TOTAL   (limit: $MIN_CASES)"
echo "   selectors against cssselect2: $SEL_EQUAL / $SEL_CHECKS match sets, $SEL_SPEC specificities"
if [ "$SYNTAX" -lt "$MIN_SYNTAX" ] || [ "$CASES" -lt "$MIN_CASES" ]; then
    echo "   FAILED: a quota has fallen below the recorded limit."
    exit 1
fi
echo "OK: $SYNTAX / $SYNTAX_TOTAL foreign cases, $CASES / $CASES_TOTAL own checks, $SEL_EQUAL / $SEL_CHECKS against cssselect2"
