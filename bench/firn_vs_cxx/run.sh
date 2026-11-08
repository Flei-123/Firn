#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-$PWD/compiler/target/release/firnc}
OUT=${OUT:?set OUT to a work directory}
LEVEL=${LEVEL:-release-fast}
export FIRNLIB="$PWD/lib"
mkdir -p "$OUT"
count_instr() {
    valgrind --tool=callgrind --callgrind-out-file=/dev/null "$1" >"$2.out" 2>"$2.err"
    grep -oP 'Collected\s*:\s*\K[0-9]+' "$2.err"
}
logsum=0
pairs=0
for src in bench/firn_vs_cxx/firn/*.fi; do
    name=$(basename "$src" .fi)
    "$FIRNC" --opt-level="$LEVEL" "$src" -o "$OUT/$name.firn" >"$OUT/$name.build.log" 2>&1 || { echo "$name BUILD_FAIL_FIRN"; continue; }
    g++ -O2 -static -std=c++17 "bench/firn_vs_cxx/cxx/$(basename "$src" .fi).cpp" -o "$OUT/$name.cxx" || { echo "$name BUILD_FAIL_CXX"; continue; }
    firn_n=$(count_instr "$OUT/$name.firn" "$OUT/$name.firn.run")
    cxx_n=$(count_instr "$OUT/$name.cxx" "$OUT/$name.cxx.run")
    same=same
    cmp -s "$OUT/$name.firn.run.out" "$OUT/$name.cxx.run.out" || same=DIFF
    ratio=$(awk -v a="$firn_n" -v b="$cxx_n" 'BEGIN { printf "%.3f", (b > 0 ? a / b : 0) }')
    printf '%-12s firn=%-14s cxx=%-14s ratio=%s %s\n' "$name" "$firn_n" "$cxx_n" "$ratio" "$same"
    logsum=$(awk -v s="$logsum" -v r="$ratio" 'BEGIN { printf "%.6f", s + log(r) }')
    pairs=$((pairs + 1))
done
[ "$pairs" -gt 0 ] && awk -v s="$logsum" -v n="$pairs" 'BEGIN { printf "geomean(%d pairs)=%.3f\n", n, exp(s / n) }'
