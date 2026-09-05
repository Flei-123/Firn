#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# The big number run of the module `str` (SPEC 8.4, Z5/Z6).
#
#   f64 -> text (lib/num/dtoa.fi, in Firn)  ->  f64 (lib/num/strtod.fi, in Firn)
#
# it is checked TWICE:
#   1. the Firn program itself compares the conversion back bit by bit,
#   2. the Rust tool tools/dtoa_vectors/gen.rs compares every line with
#      the shortest representation of Rust and additionally converts it back
#      with Rust's own strtod.
#
# Rust is the YARDSTICK here, not a dependency: the compiler itself uses
# neither gen.rs nor any foreign crate.
#
# Usage:  bash tools/dtoa_vectors/run.sh [COUNT] [SEED]
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

echo "== 3. produce and convert $N doubles =="
"$WORK/gen" bits "$N" "$SEED" > "$WORK/vectors.bin"
START=$(date +%s.%N)
if "$WORK/dtoa_stream" < "$WORK/vectors.bin" > "$WORK/text.out" 2> "$WORK/summary.txt"; then
    RC=0
else
    RC=$?
fi
END=$(date +%s.%N)
read -r TOTAL BAD < "$WORK/summary.txt"
echo "   Firn itself: $TOTAL converted, $BAD conversions back wrong"
echo "   Dauer: $(echo "$END $START" | awk '{printf "%.1f s", $1-$2}')"

echo "== 4. counter-check with Rust =="
"$WORK/gen" check "$N" "$SEED" < "$WORK/text.out"
CHECKRC=$?

if [ "$RC" -ne 0 ] || [ "$CHECKRC" -ne 0 ] || [ "$BAD" != "0" ]; then
    echo "FAILED"
    exit 1
fi
echo "OK: $N/$N bit-identical on the way back, $N/$N shortest form like Rust"
