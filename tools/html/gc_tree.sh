#!/usr/bin/env bash
# tools/html/gc_tree.sh -- soak run: real DOM trees, no growth.
#
# A DOM tree is the kind of cycle a reference count fails at (every
# node holds parent AND children strongly). This script builds complete
# trees from real HTML in a loop (lib/browser/soak_tree.fi) and checks
# that the memory consumption of the process stays FLAT.
#
# COUNTER-CHECK: the same run with `leck=1` keeps every tree. It MUST
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
ARBEIT=.tree-work
RUNDEN=${BAUM_RUNDEN:-20000}
MS=${BAUM_MS:-8000}
LECK_RUNDEN=${BAUM_LECK_RUNDEN:-4000}
LECK_MB=${BAUM_LECK_MB:-1024}
DRIFT=${BAUM_DRIFT_KIB:-256}

mkdir -p "$ARBEIT"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
"$FIRNC" -o "$ARBEIT/soak" lib/browser/soak_tree.fi || exit 1

auftrag() {   # $1 runden  $2 ms  $3 leck
    python3 -c "import struct,sys; sys.stdout.buffer.write(struct.pack('<III',$1,$2,$3))"
}

echo "== 1. Normallauf: Baum aufbauen, verwerfen, $RUNDEN Runden =="
auftrag "$RUNDEN" "$MS" 0 > "$ARBEIT/auftrag_a.bin"
if ! "$ARBEIT/soak" < "$ARBEIT/auftrag_a.bin" > "$ARBEIT/normal.tsv" 2>&1; then
    echo "   FEHLER: der Normallauf endete mit einem Fehler"
    tail -5 "$ARBEIT/normal.tsv"
    exit 1
fi
sed -n '3p;$p' "$ARBEIT/normal.tsv" | sed 's/^/   /'
grep '^# angelegt=' "$ARBEIT/normal.tsv" | sed 's/^/   /'

RSS0=$(awk '!/^#/{print $4; exit}' "$ARBEIT/normal.tsv")
RSS1=$(awk '!/^#/{v=$4} END{print v}' "$ARBEIT/normal.tsv")
RUND=$(awk '!/^#/{v=$2} END{print v}' "$ARBEIT/normal.tsv")
ZEILEN=$(grep -vc '^#' "$ARBEIT/normal.tsv")
ANGELEGT=$(sed -n 's/^# angelegt=\([0-9]*\).*/\1/p' "$ARBEIT/normal.tsv")
LEBEND=$(sed -n 's/^# angelegt=[0-9]* lebende=\([0-9]*\).*/\1/p' "$ARBEIT/normal.tsv")

if [ -z "$RSS0" ] || [ "$ZEILEN" -lt 5 ]; then
    echo "   FEHLER: zu wenige Messpunkte ($ZEILEN)"
    exit 1
fi
DELTA=$((RSS1 - RSS0))
echo "   RSS erste Stichprobe: ${RSS0} KiB, letzte: ${RSS1} KiB, Zuwachs: ${DELTA} KiB"
echo "   Runden: $RUND, angelegte GC-Objekte: $ANGELEGT, davon am Ende lebendig: $LEBEND"
if [ "$DELTA" -gt "$DRIFT" ]; then
    echo "   FEHLGESCHLAGEN: RSS waechst um $DELTA KiB (erlaubt: $DRIFT)"
    exit 1
fi

echo
echo "== 2. Gegenprobe: jeder Baum wird FESTGEHALTEN — muss wachsen =="
auftrag "$LECK_RUNDEN" "$MS" 1 > "$ARBEIT/auftrag_b.bin"
(
    ulimit -v $((LECK_MB * 1024))
    "$ARBEIT/soak" < "$ARBEIT/auftrag_b.bin" > "$ARBEIT/leck.tsv" 2>&1
)
sed -n '3p;$p' "$ARBEIT/leck.tsv" | sed 's/^/   /'
LRSS0=$(awk '!/^#/{print $4; exit}' "$ARBEIT/leck.tsv")
LRSS1=$(awk '!/^#/{v=$4} END{print v}' "$ARBEIT/leck.tsv")
if [ -z "$LRSS0" ] || [ -z "$LRSS1" ]; then
    echo "   FEHLER: die Gegenprobe lieferte keine Messpunkte"
    exit 1
fi
LDELTA=$((LRSS1 - LRSS0))
echo "   RSS erste Stichprobe: ${LRSS0} KiB, letzte: ${LRSS1} KiB, Zuwachs: ${LDELTA} KiB"
if [ "$LDELTA" -lt 4096 ]; then
    echo "   FEHLGESCHLAGEN: die Gegenprobe waechst nur um $LDELTA KiB —"
    echo "   die Messung koennte ein echtes Leck gar nicht anzeigen."
    exit 1
fi
echo "   Gegenprobe schlaegt an (+${LDELTA} KiB): die Messung kann ein Leck sehen."

echo
echo "BESTANDEN: $RUND Runden, $ANGELEGT GC-Objekte angelegt, RSS-Zuwachs ${DELTA} KiB"
exit 0
