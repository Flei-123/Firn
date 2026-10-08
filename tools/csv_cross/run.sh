#!/usr/bin/env bash
# tools/csv_cross/run.sh -- std.csv against Python's csv module, and a fuzz run.
#
#   bash tools/csv_cross/run.sh            (CSV_CASES=6000 CSV_FUZZ=60000 by default)
#   CSV_CASES=20000 CSV_FUZZ=1000000 CSV_SEEDS="1 2 3" bash tools/csv_cross/run.sh
#
# 1. CROSS-CHECK. tools/csv_cross/gen.py lets Python's csv.reader / csv.writer
#    answer CSV_CASES random read cases (hostile bytes: delimiter, quote, CR, LF,
#    NUL, 0xFF, backslash; valid tables damaged by a few random edits; strict and
#    non-strict; doublequote on/off; dialects with ; TAB | : and even a letter as
#    delimiter) and CSV_CASES/2 write cases (terminators CRLF, LF, CR, "", "xy").
#    tools/csv_cross/check.fi asks lib/std/csv.fi the same and compares byte for
#    byte: rows, error kind and error LINE (= reader.line_num), writer output,
#    and the read back of what it wrote. Every read case runs over memory and
#    four times streamed (read size 1, 3, 64, 65536). Built in all four build
#    stages of test.sh.
# 2. FUZZ. tools/csv_cross/fuzz.fi, built with --opt-level=release-safe (every
#    index and every overflow is checked): random and damaged inputs, memory
#    against stream, ROUND TRIP of random tables, resident memory flat. A hang is
#    killed by `timeout`.
# 3. COUNTER-CHECKS. Without them 1 and 2 would pass with a harness that checks
#    nothing: a library with a deliberate bug (a doubled quote that is not
#    unescaped; a writer that does not double quotes) has to be CAUGHT by the
#    cross-check resp. the fuzz run.
#
# Needs python3. Exit 0 = no difference, no failure.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
CASES=${CSV_CASES:-6000}
FUZZ=${CSV_FUZZ:-60000}
SEEDS=${CSV_SEEDS:-"1 2"}
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-csv-cross.XXXXXX")
trap 'rm -rf "$W"' EXIT
ERRORS=0
report() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }
export FIRNLIB="$ROOT/lib"
mkdir -p "$W/work"

TOTAL_CASES=0
TOTAL_DIFFS=0
TOTAL_FUZZ=0
for seed in $SEEDS; do
    python3 tools/csv_cross/gen.py "$seed" "$CASES" "$W/cases.$seed" > "$W/gen.log" || { report "gen.py failed"; cat "$W/gen.log"; exit 1; }
    cat "$W/gen.log" | sed 's/^/  seed '"$seed"': /'
done

for stage in "release-fast:--opt-level=release-fast" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast" "release-safe:--opt-level=release-safe"; do
    name=${stage%%:*}
    opt=${stage#*:}
    if ! $FIRNC $opt -o "$W/check.$name" tools/csv_cross/check.fi 2> "$W/err"; then
        report "$name: tools/csv_cross/check.fi does not compile"
        sed 's/^/        /' "$W/err" | head -6
        continue
    fi
    for seed in $SEEDS; do
        out=$("$W/check.$name" "$W/cases.$seed" "$W/work" 2>&1); rc=$?
        last=$(echo "$out" | tail -1)
        echo "  $name seed $seed: $last"
        if [ "$rc" -ne 0 ]; then
            report "$name seed $seed: cross-check differs"
            echo "$out" | head -30 | sed 's/^/        /'
        fi
        n=$(echo "$last" | sed -n 's/^csv cross-check: \([0-9]*\) cases.*/\1/p')
        d=$(echo "$last" | sed -n 's/.* \([0-9]*\) differences$/\1/p')
        TOTAL_CASES=$((TOTAL_CASES + ${n:-0}))
        TOTAL_DIFFS=$((TOTAL_DIFFS + ${d:-0}))
    done
done

# --- fuzz (release-safe, plus release-fast so that the optimiser is not the difference)
for stage in "release-safe:--opt-level=release-safe" "release-fast:--opt-level=release-fast"; do
    name=${stage%%:*}
    opt=${stage#*:}
    if ! $FIRNC $opt -o "$W/fuzz.$name" tools/csv_cross/fuzz.fi 2> "$W/err"; then
        report "$name: tools/csv_cross/fuzz.fi does not compile"
        sed 's/^/        /' "$W/err" | head -6
        continue
    fi
    for seed in $SEEDS; do
        out=$(timeout 900 "$W/fuzz.$name" "$FUZZ" "$seed" "$W/work" 2>&1); rc=$?
        echo "  fuzz $name seed $seed: $(echo "$out" | tail -1)"
        if [ "$rc" -eq 124 ]; then
            report "fuzz $name seed $seed: HANG (killed by timeout)"
        elif [ "$rc" -ne 0 ]; then
            report "fuzz $name seed $seed: failed (exit $rc)"
            echo "$out" | head -12 | sed 's/^/        /'
        else
            TOTAL_FUZZ=$((TOTAL_FUZZ + FUZZ))
        fi
    done
done

# --- counter-checks: a library with a deliberate bug has to be caught
mkdir -p "$W/badlib/std"
for f in "$ROOT"/lib/*; do [ "$(basename "$f")" = std ] || ln -s "$f" "$W/badlib/$(basename "$f")"; done
for f in "$ROOT"/lib/std/*; do [ "$(basename "$f")" = csv.fi ] || ln -s "$f" "$W/badlib/std/$(basename "$f")"; done
python3 - "$ROOT/lib/std/csv.fi" "$W/badlib/std/csv.fi" "$W/badlib/std/csv2.fi" <<'EOF'
import sys
src = open(sys.argv[1]).read()
# bug 1: a doubled quote inside a quoted field is dropped instead of kept as one quote
a = "            if c == (*r).quote {\n                rt.buf_push(&(*r).row, c)\n                state = ST_IN_QUOTED"
assert a in src
open(sys.argv[2], "w").write(src.replace(a, "            if c == (*r).quote {\n                state = ST_IN_QUOTED"))
# bug 2: the writer does not double a quote character inside a quoted field
b = "            if c == (*w).quote {\n                rt.buf_push((*w).out, c)\n            }\n"
assert b in src
open(sys.argv[3], "w").write(src.replace(b, ""))
EOF
FIRNLIB="$W/badlib" $FIRNC --opt-level=release-safe -o "$W/check.bad" tools/csv_cross/check.fi 2> "$W/err" || { report "counter-check: the bad library does not compile"; head -5 "$W/err"; }
seed1=${SEEDS%% *}
if [ -x "$W/check.bad" ]; then
    if "$W/check.bad" "$W/cases.$seed1" "$W/work" > "$W/bad.out" 2>&1; then
        report "counter-check: the cross-check did NOT catch a dropped doubled quote"
    else
        echo "  counter-check: dropped doubled quote caught ($(tail -1 "$W/bad.out" | sed 's/.*, //'))"
    fi
fi
# bug 2 is in the writer: swap the file and build the fuzz program
cp "$W/badlib/std/csv2.fi" "$W/badlib/std/csv.fi.tmp" && mv -f "$W/badlib/std/csv.fi.tmp" "$W/badlib/std/csv.fi"
FIRNLIB="$W/badlib" $FIRNC --opt-level=release-safe -o "$W/fuzz.bad" tools/csv_cross/fuzz.fi 2> "$W/err" || { report "counter-check: the bad writer does not compile"; head -5 "$W/err"; }
if [ -x "$W/fuzz.bad" ]; then
    if "$W/fuzz.bad" 3000 1 "$W/work" > "$W/bad.out" 2>&1; then
        report "counter-check: the fuzz run did NOT catch a writer that does not double quotes"
    else
        echo "  counter-check: writer without quote doubling caught by the fuzz round trip"
    fi
fi

DISTINCT=0
for seed in $SEEDS; do DISTINCT=$((DISTINCT + $(wc -l < "$W/cases.$seed"))); done
echo
echo "csv_cross: $DISTINCT cases (x4 build stages = $TOTAL_CASES comparisons), $TOTAL_DIFFS differences, $TOTAL_FUZZ fuzz iterations"
if [ "$ERRORS" -ne 0 ]; then
    echo "FAILED: $ERRORS"
    exit 1
fi
echo "ALL OK"
