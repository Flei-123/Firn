#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-$PWD/compiler/target/release/firnc}
LEVEL=${LEVEL:-release-fast}
export FIRNLIB="$PWD/lib"
OUT=$(mktemp -d)
trap 'rm -rf "$OUT"' EXIT
count_instr() {
    valgrind --tool=callgrind --callgrind-out-file=/dev/null "$1" >"$2.out" 2>"$2.err"
    grep -oP 'Collected\s*:\s*\K[0-9]+' "$2.err"
}
for name in dot blur dot_raw blur_raw; do
    "$FIRNC" --opt-level="$LEVEL" "bench/simd/$name.fi" -o "$OUT/$name.firn" >"$OUT/$name.log" 2>&1 || { echo "$name BUILD_FAIL"; cat "$OUT/$name.log" | head -5; continue; }
    gcc -O2 -static "bench/simd/${name%_raw}_scalar.c" -o "$OUT/$name.cs"
    gcc -O2 -static "bench/simd/${name%_raw}_sse.c" -o "$OUT/$name.cx"
    f=$(count_instr "$OUT/$name.firn" "$OUT/$name.f")
    s=$(count_instr "$OUT/$name.cs" "$OUT/$name.s")
    x=$(count_instr "$OUT/$name.cx" "$OUT/$name.x")
    same=same
    cmp -s "$OUT/$name.f.out" "$OUT/$name.s.out" || same=DIFF_SCALAR
    cmp -s "$OUT/$name.f.out" "$OUT/$name.x.out" || same="$same+DIFF_SSE"
    printf '%-6s firn=%-12s c_scalar=%-12s c_sse2=%-12s firn/c_scalar=%.3f firn/c_sse2=%.3f %s\n' "$name" "$f" "$s" "$x" \
        "$(awk -v a="$f" -v b="$s" 'BEGIN{print a/b}')" "$(awk -v a="$f" -v b="$x" 'BEGIN{print a/b}')" "$same"
done
