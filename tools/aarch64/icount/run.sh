#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
OUT=${OUT:?set OUT to a work directory}
NAMES=${NAMES:-}
LEVEL=${LEVEL:-release-fast}
QEMU=${QEMU:-qemu-aarch64}
HEADER_DIR=${QEMU_PLUGIN_DIR:-$OUT}
export FIRNLIB="$ROOT/lib"
mkdir -p "$OUT"
if [ ! -f "$HEADER_DIR/qemu-plugin.h" ]; then
    curl -sL -o "$HEADER_DIR/qemu-plugin.h" https://raw.githubusercontent.com/qemu/qemu/v7.2.0/include/qemu/qemu-plugin.h || exit 1
fi
if [ ! -f "$OUT/icount.so" ]; then
    gcc -O2 -shared -fPIC -I"$HEADER_DIR" $(pkg-config --cflags glib-2.0) -o "$OUT/icount.so" tools/aarch64/icount/icount.c $(pkg-config --libs glib-2.0) || exit 1
fi
count() {
    "$QEMU" -plugin "$OUT/icount.so" -d plugin "$1" 2>"$2.err" >"$2.out"
    awk '/^INSNS/ {print $2}' "$2.err"
}
list=$NAMES
if [ -z "$list" ]; then
    list=$(for f in bench/firn_vs_c/firn/*.fi; do n=$(basename "$f" .fi); [ -f "bench/firn_vs_c/c/$n.c" ] && echo "$n"; done)
fi
for n in $list; do
    "$FIRNC" --opt-level="$LEVEL" --target=aarch64-linux "bench/firn_vs_c/firn/$n.fi" -o "$OUT/$n.firn" >"$OUT/$n.build.log" 2>&1 || { echo "$n BUILD_FAIL_FIRN"; continue; }
    if [ ! -f "$OUT/$n.c" ]; then
        aarch64-linux-gnu-gcc -static -O2 -fno-strict-aliasing -ffp-contract=off "bench/firn_vs_c/c/$n.c" -o "$OUT/$n.c" -lm || { echo "$n BUILD_FAIL_C"; continue; }
    fi
    fi_n=$(count "$OUT/$n.firn" "$OUT/$n.firn.run")
    c_n=$(count "$OUT/$n.c" "$OUT/$n.c.run")
    same=same
    cmp -s "$OUT/$n.firn.run.out" "$OUT/$n.c.run.out" || same=DIFF
    echo "$n $fi_n $c_n $same" | awk '{printf "%-14s firn=%-14s c=%-14s ratio=%.3f %s\n", $1, $2, $3, ($3>0 ? $2/$3 : 0), $4}'
done
