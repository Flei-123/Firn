#!/usr/bin/env bash
# tools/html/run.sh — HTML-Baumkonstruktion + DOM-Kern (Runde 54).
#
#   1. lib/browser/parse_main.fi in DREI Baustufen uebersetzen
#      (opt / --no-opt / dev-fast) — alle muessen dieselbe Quote liefern
#   2. tools/html/harness_baum.py gegen tools/html/cases/*.dat
#   3. die BEKANNTEN LUECKEN getrennt ausweisen (tools/html/luecken/)
#   4. Robustheit auf echten Seiten (testdata/realweb/): kein Abbruch, und
#      alle drei Baustufen liefern denselben Baum, Byte fuer Byte
#   5. Dauerlauf mit Gegenprobe (tools/html/gc_tree.sh)
#   6. Regressionsschranke aus tools/html/mindestquote_baum.txt
#
# Nicht bestandene Faelle zaehlen als FEHLSCHLAG. Es wird nichts gefiltert.
#
# Aufruf:  bash tools/html/run.sh [--schnell]
set -euo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".baum-work"
SCHNELL=0
[ "${1:-}" = "--schnell" ] && SCHNELL=1

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 1. Baumaufbau uebersetzen (Firn) =="
"$FIRNC" -o "$WORK/parse" lib/browser/parse_main.fi
echo "   opt      : $WORK/parse"
if [ "$SCHNELL" -eq 0 ]; then
    "$FIRNC" --no-opt -o "$WORK/parse.noopt" lib/browser/parse_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/parse.devfast" lib/browser/parse_main.fi
    echo "   noopt    : $WORK/parse.noopt"
    echo "   dev-fast : $WORK/parse.devfast"
fi

echo
echo "== 2. Eigene Faelle (tools/html/cases/*.dat) =="
python3 tools/html/harness_baum.py "$WORK/parse" \
        --json "$WORK/bilanz.json" --zeige 5 | tee "$WORK/bilanz.txt"
QUOTE=$(python3 -c "import json;print(json.load(open('$WORK/bilanz.json'))['passed'])")
GESAMT=$(python3 -c "import json;print(json.load(open('$WORK/bilanz.json'))['total'])")

if [ "$SCHNELL" -eq 0 ]; then
    echo
    echo "== 2a. Gleiche Quote in allen drei Baustufen =="
    for m in noopt devfast; do
        python3 tools/html/harness_baum.py "$WORK/parse.$m" --json "$WORK/bilanz.$m.json" >/dev/null || true
        Q=$(python3 -c "import json;print(json.load(open('$WORK/bilanz.$m.json'))['passed'])")
        if [ "$Q" != "$QUOTE" ]; then
            echo "   FEHLER: $m liefert $Q statt $QUOTE bestandene Faelle"
            exit 1
        fi
        echo "   $m: $Q — gleich"
    done
fi

echo
echo "== 3. Bekannte Luecken (tools/html/luecken/) — muessen fehlschlagen =="
echo "   Erwartete Baeume sind die RICHTIGEN; sie zeigen, was Runde 54 nicht kann."
set +e
python3 tools/html/harness_baum.py "$WORK/parse" --luecken \
        --json "$WORK/luecken.json" > "$WORK/luecken.txt"
set -e
tail -4 "$WORK/luecken.txt" | sed 's/^/   /'
LQ=$(python3 -c "import json;print(json.load(open('$WORK/luecken.json'))['passed'])")
LG=$(python3 -c "import json;print(json.load(open('$WORK/luecken.json'))['total'])")
echo "   $LQ von $LG bekannten Luecken bereits geschlossen"

echo
echo "== 4. Echte Seiten (testdata/realweb/) =="
python3 tools/html/realweb.py "$WORK/parse" > "$WORK/realweb.txt"
sed 's/^/   /' "$WORK/realweb.txt"
if [ "$SCHNELL" -eq 0 ]; then
    for m in noopt devfast; do
        python3 tools/html/realweb.py "$WORK/parse.$m" > "$WORK/realweb.$m.txt"
        if ! cmp -s "$WORK/realweb.txt" "$WORK/realweb.$m.txt"; then
            echo "   FEHLER: $m liefert auf echten Seiten einen anderen Baum"
            exit 1
        fi
        echo "   $m: gleiche Baeume, Byte fuer Byte"
    done
fi

echo
echo "== 5. Dauerlauf: Baeume aufbauen und verwerfen, ohne zu wachsen =="
if [ "$SCHNELL" -eq 1 ]; then
    BAUM_RUNDEN=${BAUM_RUNDEN:-4000} BAUM_MS=${BAUM_MS:-3000} \
      BAUM_LECK_RUNDEN=${BAUM_LECK_RUNDEN:-3000} bash tools/html/gc_tree.sh | sed 's/^/   /'
else
    bash tools/html/gc_tree.sh | sed 's/^/   /'
fi

echo
echo "== 6. Regressionsschranke =="
MIN=$(cat tools/html/mindestquote_baum.txt)
echo "   Baumkonstruktion: $QUOTE / $GESAMT   (Schranke: $MIN)"
if [ "$QUOTE" -lt "$MIN" ]; then
    echo "   FEHLGESCHLAGEN: die Quote ist unter die eingetragene Schranke gefallen."
    exit 1
fi
echo "OK: $QUOTE / $GESAMT eigene Faelle bestanden"
