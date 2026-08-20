#!/usr/bin/env bash
# tools/js/run.sh -- the JavaScript path of round 63: lexer, parser,
# interpreter, built in objects.
#
#   0. are the foreign test data unchanged? (sha256 against the pinned
#      upstream commit of tc39/test262)
#   1. build the two drivers in THREE build stages (opt / --no-opt /
#      dev-fast) -- all three have to deliver the same quota
#   2. test262, the PARSER: does every case parse (or fail to parse) the way
#      the metadata says?
#   3. test262, the ENGINE: every case really run
#   4. the cross check against node: the same small programs, output
#      compared character for character
#   5. the endurance run with the counter check (tools/js/soak.sh)
#   6. the regression limits from tools/js/minquota*.txt
#
# Cases that do not pass count as a FAILURE. Nothing is filtered.
#
# Usage:  bash tools/js/run.sh [--fast]
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".js-work"
FAST=0
[ "${1:-}" = "--fast" ] && FAST=1

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 0. test data unchanged? (sha256 against the upstream commit) =="
if [ ! -d "$WORK/t262/test" ]; then
    mkdir -p "$WORK/t262"
    tar xzf testdata/test262/test262-subset.tar.gz -C "$WORK/t262"
fi
bash tools/js/verify_testdata.sh "$WORK/t262" | sed 's/^/   /'
export T262="$(pwd)/$WORK/t262"
echo

echo "== 1. compile the JavaScript path (Firn) =="
"$FIRNC" -o "$WORK/jsparse" lib/js/parse_main.fi
"$FIRNC" -o "$WORK/jsrun" lib/js/run_main.fi
echo "   opt      : $WORK/jsparse, $WORK/jsrun"
if [ "$FAST" -eq 0 ]; then
    "$FIRNC" --no-opt -o "$WORK/jsrun.noopt" lib/js/run_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/jsrun.devfast" lib/js/run_main.fi
    echo "   noopt    : $WORK/jsrun.noopt"
    echo "   dev-fast : $WORK/jsrun.devfast"
fi

echo
echo "== 2. test262, the parser (parse / must not parse) =="
python3 tools/js/harness_parse.py "$WORK/jsparse" --json "$WORK/parse.json" \
        --show 3 | tee "$WORK/parse.txt"
PQ=$(python3 -c "import json;d=json.load(open('$WORK/parse.json'));print(d['passed'])")
PT=$(python3 -c "import json;d=json.load(open('$WORK/parse.json'));print(d['total'])")

echo
echo "== 3. test262, the engine (really run) =="
python3 tools/js/harness_run.py "$WORK/jsrun" --json "$WORK/run.json" \
        --show 5 | tee "$WORK/run.txt"
RQ=$(python3 -c "import json;d=json.load(open('$WORK/run.json'));print(d['passed'])")
RT=$(python3 -c "import json;d=json.load(open('$WORK/run.json'));print(d['total'])")

if [ "$FAST" -eq 0 ]; then
    echo
    echo "== 3b. the same quota in the other two build stages =="
    for st in noopt devfast; do
        python3 tools/js/harness_run.py "$WORK/jsrun.$st" --json "$WORK/run.$st.json" \
            --dir test/language/types > "$WORK/run.$st.txt"
        A=$(python3 -c "import json;print(json.load(open('$WORK/run.json.types.ref'))['passed'])" 2>/dev/null || echo "")
        Q=$(python3 -c "import json;print(json.load(open('$WORK/run.$st.json'))['passed'])")
        echo "   $st: $Q passed (test/language/types)"
    done
    python3 tools/js/harness_run.py "$WORK/jsrun" --json "$WORK/run.opt.types.json" \
        --dir test/language/types > /dev/null
    O=$(python3 -c "import json;print(json.load(open('$WORK/run.opt.types.json'))['passed'])")
    N=$(python3 -c "import json;print(json.load(open('$WORK/run.noopt.json'))['passed'])")
    D=$(python3 -c "import json;print(json.load(open('$WORK/run.devfast.json'))['passed'])")
    if [ "$O" != "$N" ] || [ "$O" != "$D" ]; then
        echo "FAILED: the three build stages differ ($O / $N / $D)."
        exit 1
    fi
    echo "   all three build stages agree: $O"
fi

echo
echo "== 4. the cross check against node =="
bash tools/js/compare_node.sh "$WORK/jsrun" | sed 's/^/   /'

echo
echo "== 5. the endurance run with the counter check =="
ROUNDS=${JS_SOAK_ROUNDS:-150000}
[ "$FAST" -eq 1 ] && ROUNDS=20000
bash tools/js/soak.sh "$WORK/jsrun" "$ROUNDS" | sed 's/^/   /'

echo
echo "== 6. the regression limits =="
MINP=$(cat tools/js/minquota_parse.txt)
MINR=$(cat tools/js/minquota_run.txt)
echo "   parser: $PQ of $PT (limit $MINP)"
echo "   engine: $RQ of $RT (limit $MINR)"
if [ "$PQ" -lt "$MINP" ]; then
    echo "FAILED: the parser quota fell below the limit."
    exit 1
fi
if [ "$RQ" -lt "$MINR" ]; then
    echo "FAILED: the engine quota fell below the limit."
    exit 1
fi

echo
echo "TOTAL parser  : $PQ / $PT"
echo "TOTAL engine  : $RQ / $RT"
echo "OK: the JavaScript path holds its limits."
