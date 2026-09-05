#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/layout/gc_layout.sh -- soak run: box trees without growth.
#
# Every round builds the WHOLE layout path out of one document and one
# stylesheet -- DOM, component values, rules, style table, box tree, line
# boxes, inline items, fragments -- and drops it again
# (lib/layout/soak_layout.fi). The memory consumption of the process has
# to stay FLAT.
#
# COUNTER CHECK: the same run with `leak=1` holds on to every box tree. It
# MUST grow -- otherwise the measurement cannot show a leak at all and is
# worthless. If the counter check stays flat, this script aborts.
#
# Environment:
#   LAYOUT_SOAK_ROUNDS   rounds of the normal run (default 200000)
#   LAYOUT_SOAK_MS       time budget of the normal run in ms (default 60000)
#   LAYOUT_SOAK_LEAK_ROUNDS  rounds of the counter check (default 6000)
#   LAYOUT_SOAK_LEAK_MB  hard memory brake of the counter check in MiB (1024)
#   LAYOUT_SOAK_DRIFT_KIB  allowed RSS growth of the normal run (default 256)
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
WORK=.layout-work
ROUNDS=${LAYOUT_SOAK_ROUNDS:-200000}
MS=${LAYOUT_SOAK_MS:-60000}
LEAK_ROUNDS=${LAYOUT_SOAK_LEAK_ROUNDS:-6000}
LEAK_MB=${LAYOUT_SOAK_LEAK_MB:-1024}
DRIFT=${LAYOUT_SOAK_DRIFT_KIB:-256}

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
"$FIRNC" -o "$WORK/layoutsoak" lib/layout/soak_layout.fi 2>/dev/null || exit 1

job() {   # $1 rounds  $2 ms  $3 leak
    python3 -c "import struct,sys; sys.stdout.buffer.write(struct.pack('<III',$1,$2,$3))"
}

echo "== 1. normal run: build the box tree, drop it, ${MS} ms =="
job "$ROUNDS" "$MS" 0 > "$WORK/soak_a.bin"
if ! "$WORK/layoutsoak" < "$WORK/soak_a.bin" > "$WORK/soak.tsv" 2>&1; then
    echo "   ERROR: the normal run ended with an error"
    tail -5 "$WORK/soak.tsv"
    exit 1
fi
sed -n '3p;$p' "$WORK/soak.tsv" | sed 's/^/   /'
grep '^# created=' "$WORK/soak.tsv" | sed 's/^/   /'

RSS0=$(awk '!/^#/{print $4; exit}' "$WORK/soak.tsv")
RSS1=$(awk '!/^#/{v=$4} END{print v}' "$WORK/soak.tsv")
ROUND=$(awk '!/^#/{v=$2} END{print v}' "$WORK/soak.tsv")
BOXES=$(awk '!/^#/{v=$3} END{print v}' "$WORK/soak.tsv")
SECONDS_RUN=$(awk '!/^#/{v=$1} END{print int(v/1000)}' "$WORK/soak.tsv")
LINES=$(grep -vc '^#' "$WORK/soak.tsv")
CREATED=$(sed -n 's/^# created=\([0-9]*\).*/\1/p' "$WORK/soak.tsv")
LIVE=$(sed -n 's/^# created=[0-9]* live=\([0-9]*\).*/\1/p' "$WORK/soak.tsv")

if [ -z "$RSS0" ] || [ "$LINES" -lt 5 ]; then
    echo "   ERROR: too few measuring points ($LINES)"
    exit 1
fi
DELTA=$((RSS1 - RSS0))
echo "   RSS first sample: ${RSS0} KiB, last: ${RSS1} KiB, growth: ${DELTA} KiB"
echo "   rounds: $ROUND in ${SECONDS_RUN} s, $BOXES boxes per round,"
echo "   GC objects created: $CREATED, alive at the end: $LIVE"
if [ "$DELTA" -gt "$DRIFT" ]; then
    echo "   FAILED: the RSS grows by $DELTA KiB (allowed: $DRIFT)"
    exit 1
fi

echo
echo "== 2. counter check: every box tree is HELD ON TO -- it has to grow =="
job "$LEAK_ROUNDS" "$MS" 1 > "$WORK/soak_b.bin"
(
    ulimit -v $((LEAK_MB * 1024))
    "$WORK/layoutsoak" < "$WORK/soak_b.bin" > "$WORK/leak.tsv" 2>&1
)
sed -n '3p;$p' "$WORK/leak.tsv" | sed 's/^/   /'
LRSS0=$(awk '!/^#/{print $4; exit}' "$WORK/leak.tsv")
LRSS1=$(awk '!/^#/{v=$4} END{print v}' "$WORK/leak.tsv")
if [ -z "$LRSS0" ] || [ -z "$LRSS1" ]; then
    echo "   ERROR: the counter check yielded no measuring points"
    exit 1
fi
LDELTA=$((LRSS1 - LRSS0))
echo "   RSS first sample: ${LRSS0} KiB, last: ${LRSS1} KiB, growth: ${LDELTA} KiB"
if [ "$LDELTA" -lt 4096 ]; then
    echo "   FAILED: the counter check grows by only $LDELTA KiB --"
    echo "   the measurement could not show a real leak at all."
    exit 1
fi
echo "   counter check strikes (+${LDELTA} KiB): the measurement can see a leak."

echo
echo "PASSED: $ROUND rounds in ${SECONDS_RUN} s, $CREATED GC objects created, RSS growth ${DELTA} KiB"
exit 0
