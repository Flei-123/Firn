#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/gc_soak/run.sh -- ACCEPTANCE ITEM 2, the two halves that were open:
# THE LONG RUN and FRAGMENTATION WITH CHANGING OBJECT SIZES (round 95).
#
# `tools/dom_soak/run.sh` (round 3) runs 116 s with ONE set of objects,
# `tools/gc_frag/run.sh` (round 85) changes the sizes but runs 15 to 40
# minutes. ACCEPTANCE.md item 2 asks for both at once and for 24 hours.
#
# What this script does:
#
#   1. build in three build stages, short runs compared -- a memory model
#      that only holds with the optimiser switched on is worthless
#   2. mode 0 (the working set stays constant, the heap MUST stay bounded)
#      and mode 1 (nothing is released, the heap MUST grow -- without this
#      counter-check the flat curve of mode 0 would prove nothing)
#   3. the evaluation, with a verdict
#   4. the LONG series in tools/gc_soak/longrun/ is evaluated as well:
#      the endurance run of the round, with its REAL duration
#   5. the rescan A/B (round 91 / commit 47690a8a: the roots are scanned a
#      second time before the sweep). Two compilers, one with the two calls
#      removed, the same load -- so that the price of the fix is a number
#      and not an opinion.
#
# Environment:
#   SOAK_SEC       seconds for mode 0 (default 120)
#   SOAK_LEAK_SEC  seconds for the counter-check (default 60)
#   SOAK_SAMPLE_MS milliseconds per data line (default 2000)
#   SOAK_LEAK_MB   hard memory brake for the counter-check (default 3072)
#   SOAK_MIN_MS    below this mode 0 is still warming up (default 60000)
#   SOAK_LONG_MIN_MS  the same for the long series (default 600000)
#
# Usage:  bash tools/gc_soak/run.sh [--ab SECONDS]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)

FIRNC=compiler/target/release/firnc
WORK=.soak-work
OUT=tools/gc_soak
LONG=tools/gc_soak/longrun
SECS=${SOAK_SEC:-120}
LEAK_SECS=${SOAK_LEAK_SEC:-60}
SAMPLE_MS=${SOAK_SAMPLE_MS:-2000}
LEAK_MB=${SOAK_LEAK_MB:-3072}
MIN_MS=${SOAK_MIN_MS:-60000}
AB_SECS=0
if [ "${1:-}" = "--ab" ]; then
    AB_SECS=${2:-1800}
fi

export FIRNLIB="$ROOT/lib"

if [ ! -x "$FIRNC" ]; then
    echo "ERROR: $FIRNC is missing -- run 'cargo build --release' in compiler/ first."
    exit 1
fi
[ -f "$OUT/soak.fi" ] || { echo "ERROR: $OUT/soak.fi is missing."; exit 1; }
mkdir -p "$WORK" "$LONG"

# $1 target  $2 budget ms  $3 sample ms  $4 mode
adjust() {
    sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $2  // SOAK_BUDGET_MS|" \
        -e "s|^const SAMPLE_MS: i64 = .*$|const SAMPLE_MS: i64 = $3  // SOAK_SAMPLE_MS|" \
        -e "s|^const MODE: i64 = .*$|const MODE: i64 = $4  // SOAK_MODE|" \
        "$OUT/soak.fi" > "$1"
    for pair in "BUDGET_MS: i64 = $2 " "SAMPLE_MS: i64 = $3 " "MODE: i64 = $4 "; do
        grep -q "const $pair" "$1" || { echo "ERROR: '$pair' could not be replaced in $1."; exit 1; }
    done
}

echo "== GC endurance run with changing object sizes (acceptance item 2) =="
echo "   budget: ${SECS}s (mode 0), ${LEAK_SECS}s (counter-check), one line per ${SAMPLE_MS} ms"

# ---------------------------------------------------------- 1. build stages
echo
echo "-- 1. build in three build stages, short run compared --"
STAGES=("release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast")
errs=0
for mode in 0 1; do
    adjust "$WORK/short_$mode.fi" 3000 1000 "$mode"
    for st in "${STAGES[@]}"; do
        name=${st%%:*}
        opt=${st#*:}
        # shellcheck disable=SC2086
        if ! "$FIRNC" "$WORK/short_$mode.fi" -o "$WORK/short_${mode}_$name" $opt 2> "$WORK/build_${mode}_$name.err"; then
            echo "   ERROR: build mode $mode / $name failed:"
            grep -v RWX "$WORK/build_${mode}_$name.err" | head -5
            errs=1
            continue
        fi
        ( ulimit -v $((LEAK_MB * 1024)); exec "$WORK/short_${mode}_$name" ) > "$WORK/short_${mode}_$name.tsv" 2>&1
        rc=$?
        if [ $rc -ne 0 ] || grep -q '^# error' "$WORK/short_${mode}_$name.tsv"; then
            echo "   ERROR: short run mode $mode / $name ended with $rc:"
            grep '^# error' "$WORK/short_${mode}_$name.tsv" | head -2
            errs=1
            continue
        fi
        printf '   mode %s  %-12s %s data lines, %s\n' "$mode" "$name" \
            "$(grep -c '^[0-9]' "$WORK/short_${mode}_$name.tsv")" \
            "$(grep -o 'rounds=[0-9]*' "$WORK/short_${mode}_$name.tsv" | head -1)"
    done
done
[ $errs -eq 0 ] || { echo "ABORT: the build stages do not agree."; exit 1; }

# ------------------------------------------------------------ 2. the runs
echo
echo "-- 2. the measuring runs --"
for mode in 0 1; do
    budget=$((SECS * 1000))
    [ "$mode" = 1 ] && budget=$((LEAK_SECS * 1000))
    adjust "$WORK/soak_$mode.fi" "$budget" "$SAMPLE_MS" "$mode"
    if ! "$FIRNC" "$WORK/soak_$mode.fi" -o "$WORK/soak_$mode" 2> "$WORK/build_$mode.err"; then
        echo "   ERROR: build of the measuring version $mode failed:"
        grep -v RWX "$WORK/build_$mode.err" | head -5
        exit 1
    fi
    start=$(date +%s)
    ( ulimit -v $((LEAK_MB * 1024)); exec "$WORK/soak_$mode" ) > "$OUT/measurement-$mode.tsv"
    rc=$?
    seconds=$(( $(date +%s) - start ))
    if [ $rc -ne 0 ] && [ "$mode" = 0 ]; then
        echo "   ERROR: run $mode ended with $rc:"
        grep '^# error' "$OUT/measurement-$mode.tsv" | head -2
        exit 1
    fi
    echo "   mode $mode: $(grep '^# done' "$OUT/measurement-$mode.tsv" | cut -c1-100) (${seconds}s wall clock, exit $rc)"
done

# ---------------------------------------------------------- 3. the A/B run
if [ "$AB_SECS" != 0 ]; then
    echo
    echo "-- the rescan A/B: ${AB_SECS}s each, side by side --"
    cp lib/gc/gc.fi "$WORK/gc.fi.orig"
    python3 - <<'PY'
import re
s = open('lib/gc/gc.fi', encoding='utf-8').read()
out, n = re.subn(r'(?m)^([ \t]+)__gc_roots_rescan\(\)[ \t]*$',
                 r'\1// ABLATION: the rescan is removed here for the A/B', s)
if n != 2:
    raise SystemExit('expected 2 call sites of __gc_roots_rescan, found %d' % n)
open('lib/gc/gc.fi', 'w', encoding='utf-8').write(out)
print('   the two call sites of __gc_roots_rescan are removed for the B side')
PY
    rc=$?
    if [ $rc -ne 0 ]; then cp "$WORK/gc.fi.orig" lib/gc/gc.fi; exit 1; fi
    CARGO_TARGET_DIR="$ROOT/$WORK/tgt-nr" cargo build --release \
        --manifest-path compiler/Cargo.toml > "$WORK/cargo-nr.log" 2>&1
    rc=$?
    cp "$WORK/tgt-nr/release/firnc" "$WORK/firnc-norescan" 2>/dev/null
    cp "$WORK/gc.fi.orig" lib/gc/gc.fi
    rm -rf "$WORK/tgt-nr"
    if [ $rc -ne 0 ]; then tail -5 "$WORK/cargo-nr.log"; exit 1; fi
    cmp -s "$WORK/gc.fi.orig" lib/gc/gc.fi || { echo "ERROR: gc.fi was not restored"; exit 1; }
    adjust "$WORK/ab.fi" $((AB_SECS * 1000)) 5000 0
    "$FIRNC" "$WORK/ab.fi" -o "$WORK/ab_on" 2> /dev/null || exit 1
    "$WORK/firnc-norescan" "$WORK/ab.fi" -o "$WORK/ab_off" 2> /dev/null || exit 1
    "$WORK/ab_on" > "$LONG/rescan-on.tsv" 2>&1 &
    p1=$!
    "$WORK/ab_off" > "$LONG/rescan-off.tsv" 2>&1 &
    p2=$!
    wait $p1
    wait $p2
    echo "   both sides done"
fi

# ------------------------------------------------------------ 4. evaluation
echo
echo "-- 3. evaluation --"
python3 tools/gc_soak/evaluate.py "$OUT/measurement-0.tsv" "$OUT/measurement-1.tsv" "$MIN_MS"
rc=$?

if [ -f "$LONG/soak-24h-mode0.tsv" ]; then
    echo
    echo "-- 4. the endurance run of the round (tools/gc_soak/longrun/) --"
    python3 tools/gc_soak/evaluate.py --single "$LONG/soak-24h-mode0.tsv" \
        "${SOAK_LONG_MIN_MS:-600000}" || rc=1
fi
if [ -f "$LONG/rescan-on.tsv" ] && [ -f "$LONG/rescan-off.tsv" ]; then
    echo
    echo "-- 5. what the rescan of the roots costs (round 91, commit 47690a8a) --"
    python3 tools/gc_soak/evaluate.py --ab "$LONG/rescan-on.tsv" "$LONG/rescan-off.tsv" || rc=1
fi

echo
if [ $rc -eq 0 ]; then
    echo "OK: endurance run with changing object sizes -- passed"
else
    echo "ERROR: endurance run NOT passed."
fi
exit $rc
