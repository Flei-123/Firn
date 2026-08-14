#!/usr/bin/env bash
# Durchsatz des Firn-Tokenizers auf einem Eingabekorpus, danach — wenn
# vorhanden — html5ever (rustc -O bzw. cargo --release) auf DEMSELBEN Korpus.
#
# Der Korpus entsteht aus den Eingaben der html5lib-Faelle, vielfach
# aneinandergehaengt (tools/tokenizer/korpus.py), damit die Messung nicht vom
# Prozessstart dominiert wird.
#
# Aufruf:  bash tools/tokenizer/durchsatz.sh <tokenizer-binary>
set -euo pipefail
cd "$(dirname "$0")/../.."
BIN="${1:-.tokenizer-work/tokenize}"
WORK=".tokenizer-work"
mkdir -p "$WORK"

if [ ! -f "$WORK/korpus.html" ]; then
    python3 tools/tokenizer/korpus.py "$WORK/korpus.html" "$WORK/korpus.auftrag"
fi
GROESSE=$(stat -c%s "$WORK/korpus.html")

START=$(date +%s.%N)
"$BIN" < "$WORK/korpus.auftrag" > "$WORK/korpus.out"
ENDE=$(date +%s.%N)
if grep -q 'NICHT-UNTERSTUETZT' "$WORK/korpus.out"; then
    echo "   ACHTUNG: der Tokenizer hat den Korpus NICHT vollstaendig verarbeitet"
    echo "            (Zustand nicht umgesetzt) — die MB/s sind daher kein"
    echo "            vergleichbarer Wert und werden nur nachrichtlich gezeigt."
fi
awk -v a="$START" -v b="$ENDE" -v n="$GROESSE" \
    'BEGIN{printf "   Firn      : %8.2f MB/s  (%.3f s fuer %.2f MB)\n", n/(b-a)/1048576, b-a, n/1048576}'

if [ -x bench/tokenizer/target/release/html5ever_bench ]; then
    START=$(date +%s.%N)
    bench/tokenizer/target/release/html5ever_bench "$WORK/korpus.html" > /dev/null
    ENDE=$(date +%s.%N)
    awk -v a="$START" -v b="$ENDE" -v n="$GROESSE" \
        'BEGIN{printf "   html5ever : %8.2f MB/s  (%.3f s)\n", n/(b-a)/1048576, b-a}'
else
    echo "   html5ever : nicht gebaut (bench/tokenizer/) — siehe bench/RESULTS.md"
fi
