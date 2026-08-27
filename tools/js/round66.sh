#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/js/round66.sh -- the FEATURES OF ROUND 66 against test262, per
# feature, plus the endurance run for the objects the round adds.
#
# Section 9d (tools/js/run.sh) measures the JavaScript path as a WHOLE.
# This script measures exactly the four groups this round is about, so that
# a regression in one of them cannot hide behind the total:
#
#   1. generators           language/{statements,expressions}/generators
#   2. async and await      .../async-function, .../async-generator,
#                           .../async-arrow-function, expressions/await,
#                           statements/for-await-of
#   3. classes              language/{statements,expressions}/class
#                           (the private elements, the fields, the static
#                            blocks -- there is no directory of their own)
#   4. the endurance run    tools/js/soak.sh: an abandoned generator, a
#                           promise and a BigInt must not grow the RSS,
#                           and the counter-check MUST see a leak.
#
# The limits stand in tools/js/minquota_r66.txt (one line per group,
# "name passed total"). Falling below one of them is a FAILURE.
#
# NOTHING IS FILTERED: a case that uses a feature this engine does not have
# counts as a failure like every other one.
#
# Usage:  bash tools/js/round66.sh [--fast]
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".js-work"
MODE="${1:---full}"
mkdir -p "$WORK/r66"

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
if [ ! -d "$WORK/t262/test" ]; then
    mkdir -p "$WORK/t262"
    tar xzf testdata/test262/test262-subset.tar.gz -C "$WORK/t262"
fi
export T262="$(pwd)/$WORK/t262"

# Always built anew: a stale binary would measure the round before this one.
"$FIRNC" -o "$WORK/jsrun.r66" lib/js/run_main.fi || exit 1
ENGINE="$WORK/jsrun.r66"

GEN_DIRS="language/statements/generators language/expressions/generators"
ASYNC_DIRS="language/statements/async-function language/expressions/async-function \
language/statements/async-generator language/expressions/async-generator \
language/expressions/async-arrow-function language/expressions/await \
language/statements/for-await-of"
CLASS_DIRS="language/statements/class language/expressions/class"
if [ "$MODE" = "--fast" ]; then
    # The representative part: the same features, without the two large
    # class directories, which the full run in docs/ROUND66.md covers.
    CLASS_DIRS="language/statements/class"
fi

FAIL=0

group() {
    local name="$1"
    shift
    local tot=0
    local pass=0
    for d in "$@"; do
        local n
        n=$(echo "$d" | tr '/' '_')
        python3 tools/js/harness_run.py "$ENGINE" --dir "test/$d" \
            --json "$WORK/r66/$n.json" > "$WORK/r66/$n.txt" 2>&1
        local p t
        p=$(python3 -c "import json;print(json.load(open('$WORK/r66/$n.json'))['passed'])")
        t=$(python3 -c "import json;print(json.load(open('$WORK/r66/$n.json'))['total'])")
        pass=$((pass + p))
        tot=$((tot + t))
    done
    local q
    q=$(python3 -c "print('%.2f%%' % (100.0*$pass/$tot if $tot else 0))")
    printf "   %-10s %6d / %-6d %s" "$name" "$pass" "$tot" "$q"
    local limit
    limit=$(awk -v k="$name" '$1==k {print $2}' tools/js/minquota_r66.txt)
    if [ -z "$limit" ]; then
        limit=0
    fi
    if [ "$pass" -lt "$limit" ]; then
        printf "   BELOW THE LIMIT (%s)\n" "$limit"
        FAIL=$((FAIL + 1))
    else
        printf "   (limit %s)\n" "$limit"
    fi
    echo "$name $pass $tot" >> "$WORK/r66/summary.txt"
}

: > "$WORK/r66/summary.txt"
echo "== the features of round 66 against test262 =="
group generators $GEN_DIRS
group async $ASYNC_DIRS
group classes $CLASS_DIRS

echo
echo "== the endurance run for the new objects (tools/js/soak.sh) =="
ROUNDS=${JS_SOAK_ROUNDS:-40000}
[ "$MODE" = "--fast" ] && ROUNDS=${JS_SOAK_ROUNDS:-8000}
bash tools/js/soak.sh "$ENGINE" "$ROUNDS" > "$WORK/r66/soak.txt" 2>&1
SRC=$?
grep -E '^(gen|genleak|jobs|clean|leak) +growth|^OK:|^FAILED' "$WORK/r66/soak.txt" | sed 's/^/   /'
if [ "$SRC" -ne 0 ]; then
    echo "   FAILED: the endurance run struck (see $WORK/r66/soak.txt)"
    FAIL=$((FAIL + 1))
fi

echo
if [ "$FAIL" -ne 0 ]; then
    echo "FAILED: $FAIL group(s) of round 66 below the limit."
    exit 1
fi
echo "OK: the features of round 66 hold their limits."
