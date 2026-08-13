#!/usr/bin/env bash
# Grosser Zahlenlauf des Moduls `str` (SPEC §8.4, Z5/Z6).
#
#   f64 -> Text (lib/num/dtoa.fi, in Firn)  ->  f64 (lib/num/strtod.fi, in Firn)
#
# geprueft wird ZWEIFACH:
#   1. das Firn-Programm selbst vergleicht die Rueckwandlung bitgenau,
#   2. das Rust-Werkzeug tools/dtoa_vectors/gen.rs vergleicht jede Zeile mit
#      der kuerzesten Darstellung von Rust und wandelt sie zusaetzlich mit
#      Rusts eigenem strtod zurueck.
#
# Rust ist hier MESSLATTE, nicht Abhaengigkeit: der Compiler selbst benutzt
# weder gen.rs noch irgendeine fremde Kiste.
#
# Aufruf:  bash tools/dtoa_vectors/run.sh [ANZAHL] [SAAT]
set -euo pipefail

cd "$(dirname "$0")/../.."
N=${1:-100000}
SEED=${2:-12345}
WORK=".dtoa-work"
FIRNC="compiler/target/release/firnc"

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 1. Werkbank uebersetzen (rustc -O, nur std) =="
rustc -O -o "$WORK/gen" tools/dtoa_vectors/gen.rs 2>/dev/null

echo "== 2. Firn-Programm uebersetzen =="
python3 tools/strlib/expand.py --all >/dev/null
"$FIRNC" -o "$WORK/dtoa_stream" tools/dtoa_vectors/dtoa_stream.fi

echo "== 3. $N Doubles erzeugen und wandeln =="
"$WORK/gen" bits "$N" "$SEED" > "$WORK/vektoren.bin"
START=$(date +%s.%N)
if "$WORK/dtoa_stream" < "$WORK/vektoren.bin" > "$WORK/text.out" 2> "$WORK/summe.txt"; then
    RC=0
else
    RC=$?
fi
END=$(date +%s.%N)
read -r GESAMT SCHLECHT < "$WORK/summe.txt"
echo "   Firn selbst: $GESAMT gewandelt, $SCHLECHT Rueckwandlungen falsch"
echo "   Dauer: $(echo "$END $START" | awk '{printf "%.1f s", $1-$2}')"

echo "== 4. Gegenpruefung mit Rust =="
"$WORK/gen" check "$N" "$SEED" < "$WORK/text.out"
CHECKRC=$?

if [ "$RC" -ne 0 ] || [ "$CHECKRC" -ne 0 ] || [ "$SCHLECHT" != "0" ]; then
    echo "FEHLGESCHLAGEN"
    exit 1
fi
echo "OK: $N/$N bitgleich zurueck, $N/$N kuerzeste Darstellung wie Rust"
