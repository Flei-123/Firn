#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/html/gc_tree.sh -- soak run: real DOM trees, no growth.
#
# A DOM tree is the kind of cycle a reference count fails at (every
# node holds parent AND children strongly). This script builds complete
# trees from real HTML in a loop (lib/browser/soak_tree.fi) and checks
# that the memory consumption of the process stays FLAT.
#
# COUNTER-CHECK: the same run with `leak=1` keeps every tree. It MUST
# grow -- otherwise the measurement cannot show a leak at all and is worthless.
# If the counter-check stays flat, this script aborts.
#
# Environment:
#   BAUM_RUNDEN      rounds in the normal run (default 20000)
#   BAUM_MS          time budget of the normal run in ms (default 8000)
#   BAUM_LECK_RUNDEN rounds of the counter-check (default 4000)
#   BAUM_LECK_MB     hard memory brake of the counter-check in MiB (default 1024)
#   BAUM_DRIFT_KIB   allowed RSS increase in the normal run (default 256)
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
WORK=.tree-work
ROUNDS=${BAUM_RUNDEN:-20000}
MS=${BAUM_MS:-8000}
LEAK_ROUNDS=${BAUM_LECK_RUNDEN:-4000}
LEAK_MB=${BAUM_LECK_MB:-1024}
DRIFT=${BAUM_DRIFT_KIB:-256}

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
"$FIRNC" -o "$WORK/soak" lib/browser/soak_tree.fi || exit 1

job() {   # $1 rounds  $2 ms  $3 leak
    python3 -c "import struct,sys; sys.stdout.buffer.write(struct.pack('<III',$1,$2,$3))"
}

echo "== 1. normal run: build a tree, discard it, $ROUNDS rounds =="
job "$ROUNDS" "$MS" 0 > "$WORK/auftrag_a.bin"
if ! "$WORK/soak" < "$WORK/auftrag_a.bin" > "$WORK/normal.tsv" 2>&1; then
    echo "   ERROR: the normal run ended with an error"
    tail -5 "$WORK/normal.tsv"
    exit 1
fi
sed -n '3p;$p' "$WORK/normal.tsv" | sed 's/^/   /'
grep '^# created=' "$WORK/normal.tsv" | sed 's/^/   /'

RSS0=$(awk '!/^#/{print $4; exit}' "$WORK/normal.tsv")
RSS1=$(awk '!/^#/{v=$4} END{print v}' "$WORK/normal.tsv")
ROUND=$(awk '!/^#/{v=$2} END{print v}' "$WORK/normal.tsv")
LINES=$(grep -vc '^#' "$WORK/normal.tsv")
CREATED=$(sed -n 's/^# created=\([0-9]*\).*/\1/p' "$WORK/normal.tsv")
LIVE=$(sed -n 's/^# created=[0-9]* live=\([0-9]*\).*/\1/p' "$WORK/normal.tsv")

if [ -z "$RSS0" ] || [ "$LINES" -lt 5 ]; then
    echo "   ERROR: too few measuring points ($LINES)"
    exit 1
fi
DELTA=$((RSS1 - RSS0))
echo "   RSS first sample: ${RSS0} KiB, last: ${RSS1} KiB, growth: ${DELTA} KiB"
echo "   rounds: $ROUND, GC objects created: $CREATED, of them alive at the end: $LIVE"
if [ "$DELTA" -gt "$DRIFT" ]; then
    echo "   FAILED: the RSS grows by $DELTA KiB (allowed: $DRIFT)"
    exit 1
fi

echo
echo "== 2. counter-check: every tree is HELD ON TO -- it has to grow =="
job "$LEAK_ROUNDS" "$MS" 1 > "$WORK/auftrag_b.bin"
(
    ulimit -v $((LEAK_MB * 1024))
    "$WORK/soak" < "$WORK/auftrag_b.bin" > "$WORK/leak.tsv" 2>&1
)
sed -n '3p;$p' "$WORK/leak.tsv" | sed 's/^/   /'
LRSS0=$(awk '!/^#/{print $4; exit}' "$WORK/leak.tsv")
LRSS1=$(awk '!/^#/{v=$4} END{print v}' "$WORK/leak.tsv")
if [ -z "$LRSS0" ] || [ -z "$LRSS1" ]; then
    echo "   ERROR: the counter-check yielded no measuring points"
    exit 1
fi
LDELTA=$((LRSS1 - LRSS0))
echo "   RSS first sample: ${LRSS0} KiB, last: ${LRSS1} KiB, growth: ${LDELTA} KiB"
if [ "$LDELTA" -lt 4096 ]; then
    echo "   FAILED: the counter-check only grows by $LDELTA KiB --"
    echo "   the measurement could not show a real leak at all."
    exit 1
fi
echo "   counter-check strikes (+${LDELTA} KiB): the measurement can see a leak."

echo
echo "PASSED: $ROUND rounds, $CREATED GC objects created, RSS growth ${DELTA} KiB"
exit 0
