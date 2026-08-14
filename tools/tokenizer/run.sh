#!/usr/bin/env bash
# Baut den HTML5-Tokenizer aus lib/html/ (in Firn), faehrt ihn gegen die
# offizielle html5lib-Testsuite und gibt die Bilanz aus.
#
#   1. Compiler bauen (falls noetig)
#   2. lib/html/tokenize_main.fi uebersetzen (drei Baustufen: opt/noopt/dev-fast
#      muessen dieselbe Bilanz liefern)
#   3. tools/tokenizer/harness.py: 6.810 Faelle, Bilanz je .test-Datei
#   4. Durchsatz in MB/s auf dem Testkorpus; wenn bench/tokenizer/ gebaut ist,
#      daneben html5ever auf DEMSELBEN Korpus
#
# Nicht unterstuetzte Faelle zaehlen als FEHLSCHLAG. Es wird nichts gefiltert.
#
# Aufruf:  bash tools/tokenizer/run.sh [--schnell]
set -euo pipefail

cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
WORK=".tokenizer-work"
SCHNELL=0
[ "${1:-}" = "--schnell" ] && SCHNELL=1

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 1. Tokenizer uebersetzen (Firn) =="
"$FIRNC" -o "$WORK/tokenize" lib/html/tokenize_main.fi
echo "   opt      : $WORK/tokenize"
if [ "$SCHNELL" -eq 0 ]; then
    "$FIRNC" --no-opt -o "$WORK/tokenize.noopt" lib/html/tokenize_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/tokenize.devfast" lib/html/tokenize_main.fi
    echo "   noopt    : $WORK/tokenize.noopt"
    echo "   dev-fast : $WORK/tokenize.devfast"
fi

echo
echo "== 2. html5lib-Testsuite (testdata/html5lib-tokenizer, 6.810 Faelle) =="
python3 tools/tokenizer/harness.py "$WORK/tokenize" \
        --json "$WORK/bilanz.json" --zeige 10 | tee "$WORK/bilanz.txt"

QUOTE=$(python3 -c "import json;d=json.load(open('$WORK/bilanz.json'));print(d['passed'])")
GESAMT=$(python3 -c "import json;d=json.load(open('$WORK/bilanz.json'));print(d['total'])")

if [ "$SCHNELL" -eq 0 ]; then
    echo
    echo "== 3. Gleiche Bilanz in allen drei Baustufen =="
    for m in noopt devfast; do
        python3 tools/tokenizer/harness.py "$WORK/tokenize.$m" --json "$WORK/bilanz.$m.json" >/dev/null
        Q=$(python3 -c "import json;print(json.load(open('$WORK/bilanz.$m.json'))['passed'])")
        if [ "$Q" != "$QUOTE" ]; then
            echo "   FEHLER: $m liefert $Q statt $QUOTE bestandene Faelle"
            exit 1
        fi
        echo "   $m: $Q — gleich"
    done
fi

echo
echo "== 4. Durchsatz =="
bash tools/tokenizer/durchsatz.sh "$WORK/tokenize" || true

echo
echo "== 5. Regressionsschranke =="
MIN=$(cat tools/tokenizer/mindestquote.txt)
echo "   bestanden: $QUOTE / $GESAMT   (Schranke: $MIN)"
if [ "$QUOTE" -lt "$MIN" ]; then
    echo "   FEHLGESCHLAGEN: die Quote ist unter die eingetragene Schranke gefallen."
    exit 1
fi
echo "OK: $QUOTE / $GESAMT Faelle bestanden"
