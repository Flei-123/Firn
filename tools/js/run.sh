#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/js/run.sh -- the JavaScript path of round 63: lexer, parser,
# interpreter, built in objects.
#
#   0. are the foreign test data unchanged? (sha256 against the pinned
#      upstream commit of tc39/test262)
#   1. build the two drivers -- with --full additionally in the other two
#      build stages, which have to deliver the same quota
#   2. test262, the PARSER: does every case parse (or fail to parse) the way
#      its metadata says?
#   3. test262, the ENGINE: every case really run
#   4. the cross check against node: the same small programs, output
#      compared character for character
#   5. the endurance run with the counter check (tools/js/soak.sh)
#   6. the regression limits from tools/js/minquota*.txt
#
# Cases that do not pass count as a FAILURE. Nothing is filtered.
#
# `--fast` (what test.sh uses) runs steps 2 and 3 on a REPRESENTATIVE part
# of the suite instead of all of it, and shortens the endurance run. The
# full numbers of the round come from `bash tools/js/run.sh --full`, and
# they are in tools/js/RESULTS.md.
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".js-work"
MODE="${1:---fast}"

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
if [ "$MODE" = "--full" ]; then
    "$FIRNC" --no-opt -o "$WORK/jsrun.noopt" lib/js/run_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/jsrun.devfast" lib/js/run_main.fi
    echo "   noopt    : $WORK/jsrun.noopt"
    echo "   dev-fast : $WORK/jsrun.devfast"
fi

# The representative part for --fast: the type system, the lexical grammar,
# automatic semicolon insertion, the block scope and the smaller built ins.
SAMPLE_DIRS="language/types language/asi language/block-scope \
language/literals language/white-space language/line-terminators \
language/comments language/punctuators language/reserved-words \
language/keywords language/rest-parameters language/destructuring \
built-ins/Math built-ins/JSON built-ins/Boolean built-ins/NativeErrors"

echo
echo "== 2. test262, the parser (parse / must not parse) =="
if [ "$MODE" = "--full" ]; then
    python3 tools/js/harness_parse.py "$WORK/jsparse" --json "$WORK/parse.json" \
            --show 3 | tee "$WORK/parse.txt"
else
    rm -f "$WORK/parse.sample.json"
    python3 - "$WORK" $SAMPLE_DIRS <<'PY' | tee "$WORK/parse.txt"
import json, os, subprocess, sys
work, dirs = sys.argv[1], sys.argv[2:]
tot = passed = 0
for d in dirs:
    out = os.path.join(work, "p_%s.json" % d.replace("/", "_"))
    subprocess.run(["python3", "tools/js/harness_parse.py",
                    os.path.join(work, "jsparse"), "--list-dir", d,
                    "--json", out], stdout=subprocess.DEVNULL)
    j = json.load(open(out))
    tot += j["total"]; passed += j["passed"]
json.dump({"total": tot, "passed": passed, "failed": tot - passed},
          open(os.path.join(work, "parse.json"), "w"))
print("runs        : %d" % tot)
print("passed      : %d" % passed)
print("quota       : %.2f%%" % (100.0 * passed / tot if tot else 0))
PY
fi
PQ=$(python3 -c "import json;print(json.load(open('$WORK/parse.json'))['passed'])")
PT=$(python3 -c "import json;print(json.load(open('$WORK/parse.json'))['total'])")

echo
echo "== 3. test262, the engine (really run) =="
if [ "$MODE" = "--full" ]; then
    rm -rf "$WORK/per"
    JS_JOBS=${JS_JOBS:-6} JS_DIR_TIMEOUT=${JS_DIR_TIMEOUT:-9000} \
        bash tools/js/run_all.sh "$WORK/jsrun" | tee "$WORK/run.txt"
else
    python3 - "$WORK" $SAMPLE_DIRS <<'PY' | tee "$WORK/run.txt"
import json, os, subprocess, sys
work, dirs = sys.argv[1], sys.argv[2:]
tot = passed = 0
reasons = {}
for d in dirs:
    out = os.path.join(work, "r_%s.json" % d.replace("/", "_"))
    subprocess.run(["python3", "tools/js/harness_run.py",
                    os.path.join(work, "jsrun"), "--dir", "test/" + d,
                    "--json", out], stdout=subprocess.DEVNULL)
    j = json.load(open(out))
    tot += j["total"]; passed += j["passed"]
    for k, v in j.get("reasons", {}).items():
        reasons[k] = reasons.get(k, 0) + v
json.dump({"total": tot, "passed": passed, "failed": tot - passed,
           "reasons": reasons}, open(os.path.join(work, "run.json"), "w"))
print("runs        : %d" % tot)
print("passed      : %d" % passed)
print("quota       : %.2f%%" % (100.0 * passed / tot if tot else 0))
print("failures by cause:")
for k in sorted(reasons, key=lambda x: -reasons[x]):
    print("   %-22s %6d" % (k, reasons[k]))
PY
fi
RQ=$(python3 -c "import json;print(json.load(open('$WORK/run.json'))['passed'])")
RT=$(python3 -c "import json;print(json.load(open('$WORK/run.json'))['total'])")

if [ "$MODE" = "--full" ]; then
    echo
    echo "== 3b. the same quota in the other two build stages =="
    for st in opt noopt devfast; do
        exe="$WORK/jsrun"
        [ "$st" != "opt" ] && exe="$WORK/jsrun.$st"
        python3 tools/js/harness_run.py "$exe" --json "$WORK/stage.$st.json" \
            --dir test/language/types > /dev/null
    done
    O=$(python3 -c "import json;print(json.load(open('$WORK/stage.opt.json'))['passed'])")
    N=$(python3 -c "import json;print(json.load(open('$WORK/stage.noopt.json'))['passed'])")
    D=$(python3 -c "import json;print(json.load(open('$WORK/stage.devfast.json'))['passed'])")
    echo "   opt $O / no-opt $N / dev-fast $D  (test/language/types)"
    if [ "$O" != "$N" ] || [ "$O" != "$D" ]; then
        echo "FAILED: the three build stages differ."
        exit 1
    fi
    echo "   all three build stages agree"
fi

echo
echo "== 4. the cross check against node =="
bash tools/js/compare_node.sh "$WORK/jsrun" | sed 's/^/   /'

echo
echo "== 5. the endurance run with the counter check =="
ROUNDS=${JS_SOAK_ROUNDS:-150000}
[ "$MODE" != "--full" ] && ROUNDS=${JS_SOAK_ROUNDS:-20000}
bash tools/js/soak.sh "$WORK/jsrun" "$ROUNDS" | sed 's/^/   /'

echo
echo "== 6. the regression limits =="
MINP=$(cat tools/js/minquota_parse.txt)
MINR=$(cat tools/js/minquota_run.txt)
if [ "$MODE" = "--full" ]; then
    MINP=$(cat tools/js/minquota_parse_full.txt)
    MINR=$(cat tools/js/minquota_run_full.txt)
fi
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

if [ "$MODE" = "--full" ]; then
    python3 tools/js/report.py
fi
echo
echo "TOTAL parser  : $PQ / $PT"
echo "TOTAL engine  : $RQ / $RT"
echo "OK: the JavaScript path holds its limits."
