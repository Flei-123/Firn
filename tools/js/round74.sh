#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/js/round74.sh -- the FEATURES OF ROUND 74 against test262, per
# group, plus the endurance run for the objects the round adds.
#
# Section 9d (tools/js/run.sh) measures the JavaScript path as a WHOLE and
# section 27 measures round 66. This script measures exactly the groups
# this round is about, so that a regression in one of them cannot hide
# behind the total:
#
#   1. builtins    built-ins/{Object,Math,Number,String,Symbol,Set,Map,
#                  Error,Array} -- the long tail of the built in objects
#   2. regexp      the three directories of the suite that really use a
#                  pattern: expressions/{assignment,tagged-template} carry
#                  none, so the group is the literal itself plus the six
#                  String methods, measured over built-ins/String and
#                  language/literals/regexp
#   3. dates       built-ins/Object plus the cases that need `Date`; there
#                  is no Date directory in the pinned subset, so the group
#                  is measured through the two programs of the round
#                  (tests/1502) and through language/literals
#   4. memory      tools/js/r74soak.sh: a compiled pattern, an iterator, a
#                  Date and the weak collections must not grow the RSS,
#                  and the counter check MUST see a leak.
#
# The limits stand in tools/js/minquota_r74.txt (`--fast`, what test.sh
# runs every time) resp. tools/js/minquota_r74_full.txt (`--full`, the
# numbers of docs/ROUND74.md) -- one line per group, "name passed".
# Falling below one of them is a FAILURE.
#
# NOTHING IS FILTERED: a case that uses a feature this engine does not have
# counts as a failure like every other one.
#
# Usage:  bash tools/js/round74.sh [--fast]
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".js-work"
MODE="${1:---full}"
mkdir -p "$WORK/r74"

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
if [ ! -d "$WORK/t262/test" ]; then
    mkdir -p "$WORK/t262"
    tar xzf testdata/test262/test262-subset.tar.gz -C "$WORK/t262"
fi
export T262="$(pwd)/$WORK/t262"

# Always built anew: a stale binary would measure the round before this one.
"$FIRNC" -o "$WORK/jsrun.r74" lib/js/run_main.fi || exit 1
ENGINE="$WORK/jsrun.r74"

BUILTIN_DIRS="built-ins/Object built-ins/Math built-ins/Number \
built-ins/Symbol built-ins/Set built-ins/Map built-ins/Error"
TEXT_DIRS="built-ins/String language/literals"
if [ "$MODE" = "--fast" ]; then
    BUILTIN_DIRS="built-ins/Math built-ins/Number built-ins/Symbol \
built-ins/Set built-ins/Map built-ins/Error"
    TEXT_DIRS="language/literals"
fi

LIMITS=tools/js/minquota_r74_full.txt
if [ "$MODE" = "--fast" ]; then
    LIMITS=tools/js/minquota_r74.txt
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
            --json "$WORK/r74/$n.json" > "$WORK/r74/$n.txt" 2>&1
        local p t
        p=$(python3 -c "import json;print(json.load(open('$WORK/r74/$n.json'))['passed'])")
        t=$(python3 -c "import json;print(json.load(open('$WORK/r74/$n.json'))['total'])")
        pass=$((pass + p))
        tot=$((tot + t))
    done
    local q
    q=$(python3 -c "print('%.2f%%' % (100.0*$pass/$tot if $tot else 0))")
    printf "   %-10s %6d / %-6d %s" "$name" "$pass" "$tot" "$q"
    local limit
    limit=$(awk -v k="$name" '$1==k {print $2}' "$LIMITS")
    if [ -z "$limit" ]; then
        limit=0
    fi
    if [ "$pass" -lt "$limit" ]; then
        printf "   BELOW THE LIMIT (%s)\n" "$limit"
        FAIL=$((FAIL + 1))
    else
        printf "   (limit %s)\n" "$limit"
    fi
    echo "$name $pass $tot" >> "$WORK/r74/summary.txt"
}

: > "$WORK/r74/summary.txt"
echo "== the features of round 74 against test262 =="
group builtins $BUILTIN_DIRS
group text $TEXT_DIRS

echo
echo "== the pattern engine against node, character for character =="
bash tools/js/regexp_compare.sh "$ENGINE" | sed 's/^/   /'
RC=$?
if [ "$RC" -ne 0 ]; then
    FAIL=$((FAIL + 1))
fi

echo
echo "== the endurance run for the objects of round 74 =="
ROUNDS=${JS_SOAK_ROUNDS:-30000}
[ "$MODE" = "--fast" ] && ROUNDS=${JS_SOAK_ROUNDS:-6000}
bash tools/js/r74soak.sh "$ENGINE" "$ROUNDS" > "$WORK/r74/soak.txt" 2>&1
SRC=$?
grep -E 'growth|^OK:|^FAILED' "$WORK/r74/soak.txt" | sed 's/^/   /'
if [ "$SRC" -ne 0 ]; then
    echo "   FAILED: the endurance run struck (see $WORK/r74/soak.txt)"
    FAIL=$((FAIL + 1))
fi

echo
if [ "$FAIL" -ne 0 ]; then
    echo "FAILED: $FAIL group(s) of round 74 below the limit."
    exit 1
fi
echo "OK: the features of round 74 hold their limits."
