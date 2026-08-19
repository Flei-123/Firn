#!/usr/bin/env bash
# tools/html/gc_tree.sh — Dauerlauf: echte DOM-Baeume, kein Wachstum.
#
# Ein DOM-Baum ist die Zyklenart, an der ein Zaehlverweis scheitert (jeder
# Knoten haelt Eltern UND Kinder stark). Dieses Skript baut in einer Schleife
# vollstaendige Baeume aus echtem HTML (lib/browser/soak_tree.fi) und prueft,
# dass der Speicherverbrauch des Prozesses FLACH bleibt.
#
# GEGENPROBE: derselbe Lauf mit `leck=1` haelt jeden Baum fest. Er MUSS
# wachsen — sonst kann die Messung gar kein Leck anzeigen und ist wertlos.
# Bleibt die Gegenprobe flach, bricht dieses Skript ab.
#
# Umgebung:
#   BAUM_RUNDEN     Runden im Normallauf (Standard 20000)
#   BAUM_MS         Zeitbudget im Normallauf in ms (Standard 8000)
#   BAUM_LECK_RUNDEN Runden der Gegenprobe (Standard 4000)
#   BAUM_LECK_MB    harte Speicherbremse der Gegenprobe in MiB (Standard 1024)
#   BAUM_DRIFT_KIB  erlaubter RSS-Zuwachs im Normallauf (Standard 256)
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
