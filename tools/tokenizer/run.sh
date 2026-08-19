#!/usr/bin/env bash
# Builds the HTML5 tokenizer from lib/html/ (in Firn), drives it against the
# official html5lib test suite and prints the balance.
#
#   1. build the compiler (if needed)
#   2. compile lib/html/tokenize_main.fi (three build stages: opt/noopt/dev-fast
#      have to yield the same balance)
#   3. tools/tokenizer/harness.py: 6,810 cases, a balance per .test file
#      (the 4 xmlViolationTests run in XML mode, counter-check without it);
#      TWO quotas: only the token stream and additionally `--mit-fehlern`, which
#      compares the 'errors' lists (WHATWG code name, line, column) exactly
#   4. throughput in MB/s on the test corpus; if bench/tokenizer/ is built,
#      html5ever next to it on THE SAME corpus
#
# Cases that are not supported count as a FAILURE. Nothing is filtered.
#
# Usage:  bash tools/tokenizer/run.sh [--fast]
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

echo "== 0. Testdaten unveraendert? (sha256 gegen den Upstream-Commit) =="
bash tools/tokenizer/verifiziere_testdaten.sh | sed 's/^/   /'
echo

echo "== 1. Tokenizer uebersetzen (Firn) =="
"$FIRNC" -o "$WORK/tokenize" lib/html/tokenize_main.fi
# Measuring version: it only counts tokens (a fair comparison with html5ever, which
# also only counts). See the head of tools/tokenizer/throughput.sh.
"$FIRNC" -o "$WORK/tokenize_bench" lib/html/tokenize_bench.fi
echo "   opt      : $WORK/tokenize"
if [ "$SCHNELL" -eq 0 ]; then
    "$FIRNC" --no-opt -o "$WORK/tokenize.noopt" lib/html/tokenize_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/tokenize.devfast" lib/html/tokenize_main.fi
    echo "   noopt    : $WORK/tokenize.noopt"
    echo "   dev-fast : $WORK/tokenize.devfast"
fi

echo
echo "== 1b. Modulnachweis Zeichenreferenzen (lib/html/entities.fi) =="
"$FIRNC" -o "$WORK/entities_probe" lib/html/entities_probe.fi
python3 tools/tokenizer/check_entities.py "$WORK/entities_probe"

echo
echo "== 1c. Namenstabelle: keine feste Adresse, Ausfall wird gemeldet =="
"$FIRNC" -o "$WORK/entities_ausfall" lib/html/entities_ausfall.fi
"$WORK/entities_ausfall" > "$WORK/ausfall1.txt"
"$WORK/entities_ausfall" > "$WORK/ausfall2.txt"
sed 's/^/   /' "$WORK/ausfall1.txt"
A1=$(head -1 "$WORK/ausfall1.txt")
A2=$(head -1 "$WORK/ausfall2.txt")
if [ "$A1" = "$A2" ]; then
    echo "   FEHLER: zwei Laeufe melden dieselbe Tabellenadresse ($A1) —"
    echo "           das deutet auf eine feste Adresse hin (MAP_FIXED)."
    exit 1
fi
echo "   zweiter Lauf: $A2 — andere Adresse, also vom Kern gewaehlt (kein MAP_FIXED)"

echo
echo "== 2. html5lib-Testsuite (testdata/html5lib-tokenizer, 6.810 Faelle) =="
python3 tools/tokenizer/harness.py "$WORK/tokenize" \
        --json "$WORK/bilanz.json" --zeige 10 | tee "$WORK/bilanz.txt"

QUOTE=$(python3 -c "import json;d=json.load(open('$WORK/bilanz.json'));print(d['passed'])")
GESAMT=$(python3 -c "import json;d=json.load(open('$WORK/bilanz.json'));print(d['total'])")
QUOTE_FEHLER=$(python3 -c "import json;d=json.load(open('$WORK/bilanz.json'));print(d['passed_mit_fehlern'])")

echo
echo "== 2a. Zweite Bilanz MIT Vergleich der Parse-Fehlercodes (--mit-fehlern) =="
echo "   Verglichen wird zusaetzlich die 'errors'-Liste jedes Falles"
echo "   (WHATWG-Codename, Zeile, Spalte, in der Reihenfolge der Erwartung)."
python3 tools/tokenizer/harness.py "$WORK/tokenize" --mit-fehlern \
        --json "$WORK/bilanz.fehler.json" --zeige 5 | tail -6

echo
echo "== 2b. Gegenprobe ohne XML-Anpassung (--ohne-xml-modus) =="
echo "   Die 4 Faelle aus xmlViolation.test erwarten die XML-Anpassung; ohne"
echo "   die Auftragsflagge muessen 3 davon fehlschlagen."
python3 tools/tokenizer/harness.py "$WORK/tokenize" \
        --ohne-xml-modus --json "$WORK/bilanz.ohnexml.json" | tail -2

if [ "$SCHNELL" -eq 0 ]; then
    echo
    echo "== 3. Gleiche Bilanz in allen drei Baustufen =="
    for m in noopt devfast; do
        python3 tools/tokenizer/harness.py "$WORK/tokenize.$m" --json "$WORK/bilanz.$m.json" >/dev/null
        Q=$(python3 -c "import json;print(json.load(open('$WORK/bilanz.$m.json'))['passed'])")
        QF=$(python3 -c "import json;print(json.load(open('$WORK/bilanz.$m.json'))['passed_mit_fehlern'])")
        if [ "$Q" != "$QUOTE" ] || [ "$QF" != "$QUOTE_FEHLER" ]; then
            echo "   FEHLER: $m liefert $Q / $QF statt $QUOTE / $QUOTE_FEHLER bestandene Faelle"
            exit 1
        fi
        echo "   $m: $Q ohne / $QF mit Fehlercodes — gleich"
    done
fi

echo
echo "== 4. Durchsatz =="
bash tools/tokenizer/throughput.sh "$WORK/tokenize" || true

echo
echo "== 5. Regressionsschranke =="
MIN=$(cat tools/tokenizer/mindestquote.txt)
MINF=$(cat tools/tokenizer/minquota_errors.txt)
echo "   ohne Fehlercodes: $QUOTE / $GESAMT   (Schranke: $MIN)"
echo "   mit  Fehlercodes: $QUOTE_FEHLER / $GESAMT   (Schranke: $MINF)"
if [ "$QUOTE" -lt "$MIN" ] || [ "$QUOTE_FEHLER" -lt "$MINF" ]; then
    echo "   FEHLGESCHLAGEN: eine Quote ist unter die eingetragene Schranke gefallen."
    exit 1
fi
echo "OK: $QUOTE / $GESAMT ohne, $QUOTE_FEHLER / $GESAMT mit Fehlercodes bestanden"
