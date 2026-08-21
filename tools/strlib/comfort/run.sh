#!/usr/bin/env bash
# tools/strlib/comfort/run.sh -- the acceptance of the comfort layer
# (round 69, docs/ROUND69.md).
#
# TWO THINGS ARE PROVEN HERE, and both with a counter-check:
#
#  1. THE DEMO REALLY RUNS. demos/number_check.fi reads from the real
#     standard input and can therefore not be part of the test corpus
#     (test.sh runs its programs without a pipe of their own). So it is run
#     here with an input and its WHOLE output is held against the
#     expectation -- for a positive number, a negative one, zero and a text
#     that is not a number.
#
#  2. THE INPUT LAYER DOES NOT LEAK. tools/strlib/comfort/soak.fi reads
#     hundreds of thousands of lines with `io.read_text()` and releases
#     every `Text`; the resident memory (RSS, /proc/self/statm) has to stay
#     flat while doing so. THE COUNTER-CHECK is the same program with the
#     `free` left out -- its RSS HAS to climb, roughly one page per line. A
#     measurement that never strikes would prove nothing.
#
# Environment: COMFORT_LINES (default 200000), COMFORT_LEAK_LINES (40000),
# COMFORT_SLACK (pages the soak run may drift, default 64 = 256 KiB).
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC="$ROOT/compiler/target/release/firnc"

LINES=${COMFORT_LINES:-200000}
LEAK_LINES=${COMFORT_LEAK_LINES:-40000}
SLACK=${COMFORT_SLACK:-64}

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

FAIL=0
bad() { echo "FAIL: $*"; FAIL=$((FAIL + 1)); }

if [ ! -x "$FIRNC" ]; then
    echo "FAIL: the compiler is missing: $FIRNC"
    exit 1
fi

# ------------------------------------------------------------- 1. the demo
if ! "$FIRNC" -o "$WORK/number_check" demos/number_check.fi > "$WORK/c.log" 2>&1; then
    bad "demos/number_check.fi does not compile"
    sed 's/^/     /' "$WORK/c.log" | head -10
else
    demo_case() {          # $1 = input, $2 = expected exit code, rest = stdin of the expectation
        local input="$1" want_rc="$2"
        local got rc
        got=$(printf '%s\n' "$input" | "$WORK/number_check")
        rc=$?
        local want
        want=$(cat)
        if [ "$rc" -ne "$want_rc" ]; then
            bad "number_check '$input': exit code $rc, expected $want_rc"
        elif [ "$got" != "$want" ]; then
            bad "number_check '$input': output differs"
            diff <(printf '%s\n' "$want") <(printf '%s\n' "$got") | sed 's/^/     /'
        fi
    }

    demo_case 144 0 <<'EOF'
Enter a number: Number    : 144
Even      : true
Sign      : 1
Digits    : 3
Square    : 20736
Above 100 : true
Root      : 12
EOF

    demo_case "  -7  " 0 <<'EOF'
Enter a number: Number    : -7
Even      : false
Sign      : -1
Digits    : 1
Square    : 49
Above 100 : false
Root      : not real (number < 0)
EOF

    demo_case 0 0 <<'EOF'
Enter a number: Number    : 0
Even      : true
Sign      : 0
Digits    : 1
Square    : 0
Above 100 : false
Root      : 0
EOF

    demo_case abc 1 <<'EOF'
Enter a number: That is not a number.
EOF

    if [ "$FAIL" -eq 0 ]; then
        echo "   demo: demos/number_check.fi, 4 inputs, output identical"
    fi
fi

# -------------------------------------------------- 2. the endurance run
if ! "$FIRNC" -o "$WORK/soak" tools/strlib/comfort/soak.fi > "$WORK/s.log" 2>&1; then
    bad "tools/strlib/comfort/soak.fi does not compile"
    sed 's/^/     /' "$WORK/s.log" | head -10
    echo "comfort: $FAIL failures"
    exit 1
fi

{ echo free; seq 1 "$LINES" | sed 's/^/a line of input /'; } > "$WORK/in_free.txt"
{ echo keep; seq 1 "$LEAK_LINES" | sed 's/^/a line of input /'; } > "$WORK/in_keep.txt"

FREE_OUT=$("$WORK/soak" < "$WORK/in_free.txt")
KEEP_OUT=$("$WORK/soak" < "$WORK/in_keep.txt")

field() { echo "$1" | tr ' ' '\n' | grep -A1 "^$2\$" | tail -1; }

free_lines=$(field "$FREE_OUT" lines)
free_first=$(field "$FREE_OUT" rss_first)
free_last=$(field "$FREE_OUT" rss_last)
keep_lines=$(field "$KEEP_OUT" lines)
keep_first=$(field "$KEEP_OUT" rss_first)
keep_last=$(field "$KEEP_OUT" rss_last)

if [ "$free_lines" != "$LINES" ]; then
    bad "soak (free): $free_lines lines read, expected $LINES"
fi
if [ "$keep_lines" != "$LEAK_LINES" ]; then
    bad "soak (keep): $keep_lines lines read, expected $LEAK_LINES"
fi

drift=$((free_last - free_first))
if [ "$drift" -lt 0 ]; then drift=$((0 - drift)); fi
if [ "$drift" -gt "$SLACK" ]; then
    bad "soak (free): RSS drifted by $drift pages ($free_first -> $free_last), at most $SLACK allowed"
else
    echo "   soak: $free_lines lines, RSS $free_first -> $free_last pages (drift $drift, limit $SLACK)"
fi

# The counter-check has to strike: one page per line that is not released;
# half of that is asked for, so that the measurement stays robust.
want_growth=$((LEAK_LINES / 2))
growth=$((keep_last - keep_first))
if [ "$growth" -lt "$want_growth" ]; then
    bad "counter-check does NOT strike: RSS grew by only $growth pages ($keep_first -> $keep_last), at least $want_growth expected -- the measurement is broken"
else
    echo "   counter-check strikes: without free RSS $keep_first -> $keep_last pages (+$growth over $keep_lines lines)"
fi

echo
if [ "$FAIL" -eq 0 ]; then
    echo "comfort: PASSED (demo 4/4, soak flat, counter-check strikes)"
    exit 0
fi
echo "comfort: $FAIL failures"
exit 1
